//! Model lifecycle manager — the **only** component that loads and unloads
//! managed models (`CLAUDE.md` Article I, `ARCHITECTURE.md` §3).
//!
//! It owns *runtime* model state (`ModelState`); the registry owns metadata,
//! the resource manager owns accounting. An explicit state machine:
//!
//! ```text
//! Unloaded ──load──▶ Loading ──ok──▶ Loaded ⇄ Busy
//!                       │  │            │
//!                cancel │  │ error      └─unload─▶ Unloading ─▶ Unloaded
//!                       ▼  ▼
//!                 Unloaded  Failed ──load(recover)──▶ Loading
//! ```
//!
//! Rules:
//! - Concurrent `load` for the same model **coalesce** onto one backend load.
//! - A Phase 13 reservation is acquired **before** the backend load and released
//!   on every exit path — success-then-unload, failure, cancellation, crash.
//! - One `tokio::sync::Mutex` serializes every state transition; backend calls
//!   (`load` / `shutdown` / `health`) happen outside it (ADR-0010: the
//!   transition holder does the whole transition, nothing re-enters the lock).

pub mod backend;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

use crate::contracts::ids::{ModelId, ReservationId};
use crate::contracts::model::{LifecycleStatus, ModelState, RegistryAvailability};
use crate::ipc::{AppError, AppResult};
use crate::models::ModelRegistry;
use crate::resources::{ResourceManager, ResourceRequest};
use backend::{LoadRequest, LoadedInstance, ModelBackend};

/// Used as the reservation estimate when a model carries no
/// `estimated_vram_mb` — conservative so a guess never over-commits into an OOM.
const FALLBACK_VRAM_ESTIMATE_MB: u32 = 6_144;

/// How the manager retries a failing backend load within one `load` call.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Total backend-load attempts before the model parks in `Failed`.
    pub max_attempts: u32,
    /// First backoff; doubles each attempt (`base`, `2·base`, …).
    pub backoff_base: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff_base: Duration::from_millis(500),
        }
    }
}

struct Entry {
    state: ModelState,
    reservation: Option<ReservationId>,
    instance: Option<Arc<dyn LoadedInstance>>,
    /// Present while `Loading` — tripping it cancels the in-flight load.
    cancel: Option<CancellationToken>,
    /// Woken on every transition into a settled state.
    notify: Arc<Notify>,
    last_error: Option<String>,
}

impl Entry {
    fn new() -> Self {
        Self {
            state: ModelState::Unloaded,
            reservation: None,
            instance: None,
            cancel: None,
            notify: Arc::new(Notify::new()),
            last_error: None,
        }
    }
}

/// The lifecycle manager. Held in Tauri managed state as
/// `Arc<LifecycleManager>`.
pub struct LifecycleManager {
    registry: Arc<ModelRegistry>,
    resources: Arc<ResourceManager>,
    backends: std::sync::Mutex<HashMap<String, Arc<dyn ModelBackend>>>,
    retry: RetryPolicy,
    entries: Mutex<HashMap<ModelId, Entry>>,
}

impl LifecycleManager {
    /// Build a manager over the registry + resource manager.
    #[must_use]
    pub fn new(
        registry: Arc<ModelRegistry>,
        resources: Arc<ResourceManager>,
        retry: RetryPolicy,
    ) -> Self {
        Self {
            registry,
            resources,
            backends: std::sync::Mutex::new(HashMap::new()),
            retry,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Register the backend that handles models whose `backend` metadata equals
    /// `key` (e.g. `"llama.cpp"`, Phase 15). Call at startup.
    pub fn register_backend(&self, key: impl Into<String>, backend: Arc<dyn ModelBackend>) {
        self.backends
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key.into(), backend);
    }

    fn backend_for(&self, key: &str) -> AppResult<Arc<dyn ModelBackend>> {
        self.backends
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            .cloned()
            .ok_or_else(|| AppError::Validation(format!("no backend registered for {key:?}")))
    }

    /// Current runtime state of `id` (`Unloaded` if the manager has never seen
    /// it).
    pub async fn state(&self, id: &ModelId) -> ModelState {
        self.entries
            .lock()
            .await
            .get(id)
            .map_or(ModelState::Unloaded, |e| e.state)
    }

    /// The loaded instance for `id`, if it is `Loaded` or `Busy`. Callers that
    /// need a capability (e.g. `llm::as_llm`) downcast via
    /// [`backend::LoadedInstance::as_any`].
    pub async fn instance(&self, id: &ModelId) -> Option<Arc<dyn LoadedInstance>> {
        let entries = self.entries.lock().await;
        entries.get(id).and_then(|e| match e.state {
            ModelState::Loaded | ModelState::Busy => e.instance.clone(),
            _ => None,
        })
    }

    /// A snapshot of every model the manager is tracking.
    pub async fn statuses(&self) -> Vec<LifecycleStatus> {
        self.entries
            .lock()
            .await
            .iter()
            .map(|(id, e)| LifecycleStatus {
                id: id.clone(),
                state: e.state,
                vram_mb: e.instance.as_ref().and_then(|i| i.measured_vram_mb()),
                error: e.last_error.clone(),
            })
            .collect()
    }

    /// Load `id` into memory. Idempotent: a `Loaded`/`Busy` model returns `Ok`
    /// immediately; a concurrent call for a `Loading` model waits for that load
    /// rather than starting a second one.
    ///
    /// # Errors
    /// - [`AppError::NotFound`] — no such registry row, or its file is missing.
    /// - [`AppError::Validation`] — no backend registered for the model.
    /// - [`AppError::ResourceExhausted`] — not enough VRAM (nothing is spawned;
    ///   the model stays `Unloaded`).
    /// - [`AppError::Cancelled`] — a concurrent `cancel_load`.
    /// - the backend's own error after `retry.max_attempts` failed attempts (the
    ///   model parks in `Failed`).
    pub async fn load(&self, id: &ModelId) -> AppResult<()> {
        // Claim the load, or coalesce onto one already running.
        loop {
            let mut entries = self.entries.lock().await;
            let entry = entries.entry(id.clone()).or_insert_with(Entry::new);
            match entry.state {
                ModelState::Loaded | ModelState::Busy => return Ok(()),
                ModelState::Loading | ModelState::Unloading => {
                    let notified = entry.notify.clone();
                    let fut = notified.notified();
                    tokio::pin!(fut);
                    fut.as_mut().enable(); // register before releasing the lock
                    drop(entries);
                    fut.await;
                }
                ModelState::Unloaded | ModelState::Failed => {
                    entry.state = ModelState::Loading;
                    entry.cancel = Some(CancellationToken::new());
                    entry.last_error = None;
                    break;
                }
            }
        }

        let outcome = self.run_load(id).await;

        let mut entries = self.entries.lock().await;
        let entry = entries.entry(id.clone()).or_insert_with(Entry::new);
        entry.cancel = None;
        match &outcome {
            Ok((reservation, instance)) => {
                entry.state = ModelState::Loaded;
                entry.reservation = Some(reservation.clone());
                entry.instance = Some(Arc::clone(instance));
            }
            Err(err) => {
                entry.reservation = None;
                entry.instance = None;
                entry.state = match err {
                    // Not the model's fault — a transient shortage, a bad
                    // request, or a deliberate cancel. Back to Unloaded.
                    AppError::ResourceExhausted(_)
                    | AppError::Cancelled
                    | AppError::NotFound(_)
                    | AppError::Validation(_) => ModelState::Unloaded,
                    // A backend that tried and failed → Failed, needs recovery.
                    _ => {
                        entry.last_error = Some(err.to_string());
                        ModelState::Failed
                    }
                };
            }
        }
        entry.notify.notify_waiters();
        outcome.map(|_| ())
    }

    /// The load itself: fetch the row, then up to `retry.max_attempts` rounds of
    /// reserve → backend load, releasing the reservation on every failed round.
    async fn run_load(&self, id: &ModelId) -> AppResult<(ReservationId, Arc<dyn LoadedInstance>)> {
        let model = self.registry.get(id).await?;
        if model.availability != RegistryAvailability::Ready {
            return Err(AppError::NotFound(format!(
                "model {id} file is missing at {}",
                model.path
            )));
        }
        let backend = self.backend_for(&model.metadata.backend.0)?;
        let estimate = model
            .metadata
            .estimated_vram_mb
            .unwrap_or(FALLBACK_VRAM_ESTIMATE_MB);
        let cancel = {
            let entries = self.entries.lock().await;
            entries
                .get(id)
                .and_then(|e| e.cancel.clone())
                .unwrap_or_default()
        };

        let mut last_err = AppError::Internal;
        for attempt in 0..self.retry.max_attempts {
            if cancel.is_cancelled() {
                return Err(AppError::Cancelled);
            }
            let reservation = self
                .resources
                .request(
                    ResourceRequest::gpu(estimate)
                        .calibrated(id.as_str(), model.metadata.backend.0.clone()),
                )
                .await?;

            let req = LoadRequest {
                model: model.clone(),
            };
            match backend.load(&req, cancel.clone()).await {
                Ok(instance) => {
                    let instance: Arc<dyn LoadedInstance> = Arc::from(instance);
                    let measured = instance.measured_vram_mb().unwrap_or(estimate);
                    if let Err(err) = self.resources.commit(&reservation.id, measured).await {
                        err.log("commit model-load reservation");
                    }
                    tracing::info!(model = %id, attempt, measured_vram_mb = measured, "model loaded");
                    return Ok((reservation.id, instance));
                }
                Err(AppError::Cancelled) => {
                    self.resources.release(&reservation.id).await;
                    return Err(AppError::Cancelled);
                }
                Err(err) => {
                    self.resources.release(&reservation.id).await;
                    tracing::warn!(model = %id, attempt, %err, "model load attempt failed");
                    last_err = err;
                    if attempt + 1 < self.retry.max_attempts {
                        let backoff = self.retry.backoff_base * 2u32.pow(attempt);
                        tokio::select! {
                            () = cancel.cancelled() => return Err(AppError::Cancelled),
                            () = tokio::time::sleep(backoff) => {}
                        }
                    }
                }
            }
        }
        Err(last_err)
    }

    /// Cancel an in-flight load. No-op unless the model is `Loading`.
    ///
    /// # Errors
    /// [`AppError::Conflict`] if the model is not currently loading.
    pub async fn cancel_load(&self, id: &ModelId) -> AppResult<()> {
        let entries = self.entries.lock().await;
        let entry = entries
            .get(id)
            .ok_or_else(|| AppError::NotFound(format!("model {id}")))?;
        if entry.state != ModelState::Loading {
            return Err(AppError::Conflict(format!(
                "model {id} is {:?}, not loading",
                entry.state
            )));
        }
        if let Some(cancel) = &entry.cancel {
            cancel.cancel();
        }
        Ok(())
    }

    /// Unload `id`. Idempotent for an already-unloaded model.
    ///
    /// # Errors
    /// [`AppError::Conflict`] if the model is `Busy` or mid-transition.
    pub async fn unload(&self, id: &ModelId) -> AppResult<()> {
        let (instance, reservation) = {
            let mut entries = self.entries.lock().await;
            let Some(entry) = entries.get_mut(id) else {
                return Ok(());
            };
            match entry.state {
                ModelState::Unloaded => return Ok(()),
                ModelState::Failed => {
                    // A failed model holds no reservation (run_load releases on
                    // every failure path) — just clear it.
                    let reservation = entry.reservation.take();
                    entry.instance = None;
                    entry.state = ModelState::Unloaded;
                    entry.notify.notify_waiters();
                    (None, reservation)
                }
                ModelState::Busy => {
                    return Err(AppError::Conflict(format!(
                        "model {id} is serving a request"
                    )))
                }
                ModelState::Loading | ModelState::Unloading => {
                    return Err(AppError::Conflict(format!(
                        "model {id} is {:?}",
                        entry.state
                    )))
                }
                ModelState::Loaded => {
                    entry.state = ModelState::Unloading;
                    (entry.instance.take(), entry.reservation.take())
                }
            }
        };

        if let Some(instance) = instance {
            instance.shutdown().await;
        }
        if let Some(reservation) = reservation {
            self.resources.release(&reservation).await;
        }

        let mut entries = self.entries.lock().await;
        if let Some(entry) = entries.get_mut(id) {
            entry.state = ModelState::Unloaded;
            entry.notify.notify_waiters();
        }
        tracing::info!(model = %id, "model unloaded");
        Ok(())
    }

    /// Mark `id` busy for the duration of the returned [`BusyGuard`]. Blocks
    /// `unload` until the guard drops.
    ///
    /// # Errors
    /// [`AppError::Conflict`] unless the model is `Loaded`;
    /// [`AppError::NotFound`] if the manager has never loaded it.
    pub async fn begin_use(self: &Arc<Self>, id: &ModelId) -> AppResult<BusyGuard> {
        let mut entries = self.entries.lock().await;
        let entry = entries
            .get_mut(id)
            .ok_or_else(|| AppError::NotFound(format!("model {id}")))?;
        match entry.state {
            ModelState::Loaded => {
                entry.state = ModelState::Busy;
                Ok(BusyGuard {
                    manager: Arc::clone(self),
                    id: id.clone(),
                    released: false,
                })
            }
            other => Err(AppError::Conflict(format!(
                "model {id} is {other:?}, not Loaded"
            ))),
        }
    }

    /// Unload every `Loaded`/`Failed` model (called on app exit). A `Busy` model
    /// is unloaded too — the process is going away regardless.
    pub async fn unload_all(&self) {
        let ids: Vec<ModelId> = {
            let entries = self.entries.lock().await;
            entries
                .iter()
                .filter(|(_, e)| e.instance.is_some() || e.reservation.is_some())
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in ids {
            // Force it out of Busy first so `unload` doesn't refuse.
            {
                let mut entries = self.entries.lock().await;
                if let Some(e) = entries.get_mut(&id) {
                    if e.state == ModelState::Busy {
                        e.state = ModelState::Loaded;
                    }
                }
            }
            if let Err(err) = self.unload(&id).await {
                err.log("unload model on exit");
            }
        }
    }

    /// Return a `Busy` model to `Loaded`. Called by [`BusyGuard`] on drop; safe
    /// to call directly.
    pub async fn end_use(&self, id: &ModelId) {
        let mut entries = self.entries.lock().await;
        if let Some(entry) = entries.get_mut(id) {
            if entry.state == ModelState::Busy {
                entry.state = ModelState::Loaded;
                entry.notify.notify_waiters();
            }
        }
    }

    /// Health-check every `Loaded`/`Busy` instance. A failed check → `Failed`,
    /// reservation released, waiters notified. Returns the number moved to
    /// `Failed`. The Phase 13 observe loop then sees the freed VRAM.
    pub async fn check_liveness(&self) -> usize {
        let to_check: Vec<(ModelId, Arc<dyn LoadedInstance>)> = {
            let entries = self.entries.lock().await;
            entries
                .iter()
                .filter(|(_, e)| matches!(e.state, ModelState::Loaded | ModelState::Busy))
                .filter_map(|(id, e)| e.instance.clone().map(|i| (id.clone(), i)))
                .collect()
        };

        let mut dead = Vec::new();
        for (id, instance) in to_check {
            if let Err(err) = instance.health().await {
                tracing::warn!(model = %id, %err, "backend liveness check failed");
                dead.push(id);
            }
        }
        if dead.is_empty() {
            return 0;
        }

        let mut to_release = Vec::new();
        {
            let mut entries = self.entries.lock().await;
            for id in &dead {
                if let Some(entry) = entries.get_mut(id) {
                    if matches!(entry.state, ModelState::Loaded | ModelState::Busy) {
                        entry.state = ModelState::Failed;
                        entry.last_error = Some("backend liveness check failed".to_owned());
                        entry.instance = None;
                        if let Some(reservation) = entry.reservation.take() {
                            to_release.push(reservation);
                        }
                        entry.notify.notify_waiters();
                    }
                }
            }
        }
        for reservation in to_release {
            self.resources.release(&reservation).await;
        }
        dead.len()
    }
}

/// Holds a model `Busy` until dropped. On drop it returns the model to `Loaded`
/// (best-effort — spawned on the current runtime).
pub struct BusyGuard {
    manager: Arc<LifecycleManager>,
    id: ModelId,
    released: bool,
}

impl std::fmt::Debug for BusyGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BusyGuard")
            .field("id", &self.id)
            .field("released", &self.released)
            .finish_non_exhaustive()
    }
}

impl BusyGuard {
    /// Explicitly end the busy period now. The drop becomes a no-op.
    pub async fn release(mut self) {
        self.released = true;
        self.manager.end_use(&self.id).await;
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        let manager = Arc::clone(&self.manager);
        let id = self.id.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move { manager.end_use(&id).await });
        } else {
            tracing::warn!(model = %id, "BusyGuard dropped outside a runtime — model stays Busy");
        }
    }
}
