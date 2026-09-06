//! Resource manager — the single authority for "can this operation safely use
//! the GPU (and system RAM) right now?" (`CLAUDE.md` Article I, ADR-0007).
//!
//! It **accounts**; it does not load models (Phase 14) or order/evict work
//! (the scheduler, Phase 24). Per-process VRAM is unavailable on this driver,
//! so it pairs whole-device measurements ([`probe`]) with its own in-memory
//! **reservation ledger**.
//!
//! Lifecycle for one load:
//! `request(estimate)` → ledger entry `Held` → `commit(measured)` →
//! `observe()` poll → `release()`; `reconcile()` runs periodically to trust the
//! measurement over the ledger and to recover reservations a crashed caller
//! never released.
//!
//! One `tokio::sync::Mutex` serializes `request` / `commit` / `release` /
//! `reconcile` (ADR-0010 lock ordering — the holder performs the whole
//! transition; nothing re-enters it, nothing holds it across a probe call).

pub mod estimate;
pub mod probe;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::contracts::ids::{ReservationId, TaskId};
use crate::contracts::resource::{
    GpuMemory, RamInfo, Reservation, ReservationState, ResourceKind, ResourceSnapshot,
};
use crate::ipc::{AppError, AppResult};
use estimate::Calibration;
use probe::HardwareProbe;

/// A reservation with no `commit` this long after `request` is treated as a
/// crashed caller and recovered by [`ResourceManager::reconcile`].
const DEFAULT_STALE_TTL: Duration = Duration::from_secs(120);

/// VRAM (MB) by which measured `used` may exceed our committed total before
/// [`ResourceManager::reconcile`] logs an external-holder drift.
const DRIFT_SLACK_MB: u64 = 512;

/// What a caller wants to reserve. `estimate_mb` is the caller's raw estimate
/// (e.g. from [`estimate::estimate_llm_vram`]); the manager applies the learned
/// calibration factor and the safety margin itself.
#[derive(Debug, Clone)]
pub struct ResourceRequest {
    /// GPU VRAM or system RAM.
    pub kind: ResourceKind,
    /// The caller's raw estimate, in MB.
    pub estimate_mb: u32,
    /// The task this reservation serves, when tied to one. Two un-terminal
    /// reservations of the same `kind` for the same task are rejected.
    pub task_id: Option<TaskId>,
    /// Calibration key (model id) + fallback key (backend). `None` skips
    /// calibration for this request.
    pub calibration: Option<(String, String)>,
}

impl ResourceRequest {
    /// A GPU request for `estimate_mb`, no task, no calibration.
    #[must_use]
    pub fn gpu(estimate_mb: u32) -> Self {
        Self {
            kind: ResourceKind::Gpu,
            estimate_mb,
            task_id: None,
            calibration: None,
        }
    }

    /// A system-RAM request for `estimate_mb`.
    #[must_use]
    pub fn system_ram(estimate_mb: u32) -> Self {
        Self {
            kind: ResourceKind::SystemRam,
            estimate_mb,
            task_id: None,
            calibration: None,
        }
    }

    /// Builder: attach a task id.
    #[must_use]
    pub fn for_task(mut self, task_id: TaskId) -> Self {
        self.task_id = Some(task_id);
        self
    }

    /// Builder: attach calibration keys (`model_id`, `backend`).
    #[must_use]
    pub fn calibrated(mut self, model_id: impl Into<String>, backend: impl Into<String>) -> Self {
        self.calibration = Some((model_id.into(), backend.into()));
        self
    }
}

#[derive(Debug)]
struct LedgerEntry {
    id: ReservationId,
    kind: ResourceKind,
    /// Raw caller estimate.
    raw_mb: u32,
    /// Effective reservation = `raw_mb` × calibration factor. Counted against the
    /// pool until `committed_mb` replaces it.
    estimate_mb: u32,
    /// First real measurement, once `commit` is called.
    committed_mb: Option<u32>,
    task_id: Option<TaskId>,
    calibration: Option<(String, String)>,
    created_at: Instant,
}

impl LedgerEntry {
    /// What this entry currently costs the pool.
    fn charged_mb(&self) -> u32 {
        self.committed_mb.unwrap_or(self.estimate_mb)
    }

    /// The wire form. A ledger entry is always `Held` — `Requested` / `Denied`
    /// are transient (a denied request leaves no entry) and `Released` means the
    /// entry is gone.
    fn to_contract(&self) -> Reservation {
        Reservation {
            id: self.id.clone(),
            kind: self.kind,
            amount_mb: self.charged_mb(),
            task_id: self.task_id.clone(),
            state: ReservationState::Held,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Cached {
    gpu: Option<GpuMemory>,
    ram: Option<RamInfo>,
}

struct Inner {
    ledger: Vec<LedgerEntry>,
    cached: Cached,
    calibration: Calibration,
}

impl Inner {
    /// Sum of what every outstanding reservation of `kind` costs the pool.
    fn outstanding_mb(&self, kind: ResourceKind) -> u64 {
        self.ledger
            .iter()
            .filter(|e| e.kind == kind)
            .map(|e| u64::from(e.charged_mb()))
            .sum()
    }
}

/// The resource manager. Held in Tauri managed state as `Arc<ResourceManager>`.
pub struct ResourceManager {
    probe: Arc<dyn HardwareProbe>,
    vram_safety_margin_mb: u32,
    stale_ttl: Duration,
    inner: Mutex<Inner>,
}

impl ResourceManager {
    /// Build a manager over `probe`, holding back `vram_safety_margin_mb` on
    /// every GPU grant.
    #[must_use]
    pub fn new(probe: Arc<dyn HardwareProbe>, vram_safety_margin_mb: u32) -> Self {
        Self {
            probe,
            vram_safety_margin_mb,
            stale_ttl: DEFAULT_STALE_TTL,
            inner: Mutex::new(Inner {
                ledger: Vec::new(),
                cached: Cached::default(),
                calibration: Calibration::new(),
            }),
        }
    }

    /// Test hook: shorten the stale-reservation TTL.
    #[cfg(test)]
    fn with_stale_ttl(mut self, ttl: Duration) -> Self {
        self.stale_ttl = ttl;
        self
    }

    /// Refresh the cached measurement from the hardware probe and return the
    /// snapshot. This is the only method that touches the probe; the observation
    /// loop calls it on a poll. A failed probe leaves that half of the cache as
    /// it was and logs at `debug`.
    pub async fn observe(&self) -> ResourceSnapshot {
        let gpu = self.probe.gpu().map_err(|e| e.to_string());
        let ram = self.probe.ram().map_err(|e| e.to_string());

        let mut inner = self.inner.lock().await;
        match gpu {
            Ok(g) => inner.cached.gpu = Some(g),
            Err(err) => tracing::debug!(%err, "GPU probe unavailable this tick"),
        }
        match ram {
            Ok(r) => inner.cached.ram = Some(r),
            Err(err) => tracing::debug!(%err, "RAM probe unavailable this tick"),
        }
        Self::snapshot_locked(&inner)
    }

    /// The manager's current view without touching the probe.
    pub async fn snapshot(&self) -> ResourceSnapshot {
        let inner = self.inner.lock().await;
        Self::snapshot_locked(&inner)
    }

    fn snapshot_locked(inner: &Inner) -> ResourceSnapshot {
        ResourceSnapshot {
            gpu: inner.cached.gpu,
            ram: inner.cached.ram,
            reserved_gpu_mb: clamp_u32(inner.outstanding_mb(ResourceKind::Gpu)),
            reserved_ram_mb: clamp_u32(inner.outstanding_mb(ResourceKind::SystemRam)),
        }
    }

    /// Reserve resources for an operation. Uses the **cached** measurement plus
    /// the ledger — it never blocks on the probe.
    ///
    /// # Errors
    /// - [`AppError::BackendUnavailable`] if there is no measurement for `kind`.
    /// - [`AppError::Conflict`] if the same task already holds a reservation of
    ///   this `kind`.
    /// - [`AppError::ResourceExhausted`] if the pool cannot satisfy the request
    ///   after the safety margin and outstanding reservations. No ledger entry
    ///   is created in that case.
    pub async fn request(&self, req: ResourceRequest) -> AppResult<Reservation> {
        let mut inner = self.inner.lock().await;

        if let Some(task_id) = &req.task_id {
            if inner
                .ledger
                .iter()
                .any(|e| e.kind == req.kind && e.task_id.as_ref() == Some(task_id))
            {
                return Err(AppError::Conflict(format!(
                    "task {task_id} already holds a {:?} reservation",
                    req.kind
                )));
            }
        }

        let (total_mb, free_mb, margin_mb) = match req.kind {
            ResourceKind::Gpu => {
                let gpu = inner.cached.gpu.ok_or_else(|| {
                    AppError::BackendUnavailable(
                        "GPU VRAM has not been measured; cannot account a reservation".to_owned(),
                    )
                })?;
                (
                    gpu.total_mb,
                    gpu.free_mb,
                    u64::from(self.vram_safety_margin_mb),
                )
            }
            ResourceKind::SystemRam => {
                let ram = inner.cached.ram.ok_or_else(|| {
                    AppError::BackendUnavailable(
                        "system RAM has not been measured; cannot account a reservation".to_owned(),
                    )
                })?;
                (ram.total_mb, ram.available_mb, 0)
            }
        };

        let effective_estimate = match &req.calibration {
            Some((key, fallback)) => inner.calibration.apply(req.estimate_mb, key, fallback),
            None => req.estimate_mb,
        };

        let outstanding = inner.outstanding_mb(req.kind);
        let budget = free_mb
            .saturating_sub(margin_mb)
            .saturating_sub(outstanding);

        if u64::from(effective_estimate) > budget {
            return Err(AppError::ResourceExhausted(format!(
                "{:?}: need {effective_estimate} MB, {budget} MB available \
                 (free {free_mb}, margin {margin_mb}, reserved {outstanding}, total {total_mb})",
                req.kind
            )));
        }

        let entry = LedgerEntry {
            id: new_reservation_id(),
            kind: req.kind,
            raw_mb: req.estimate_mb,
            estimate_mb: effective_estimate,
            committed_mb: None,
            task_id: req.task_id,
            calibration: req.calibration,
            created_at: Instant::now(),
        };
        let reservation = entry.to_contract();
        tracing::info!(
            reservation = %entry.id,
            kind = ?entry.kind,
            estimate_mb = entry.estimate_mb,
            "resource reserved"
        );
        inner.ledger.push(entry);
        Ok(reservation)
    }

    /// Replace a reservation's estimate with the first real measurement. Feeds
    /// the calibration table.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if `id` is not an outstanding reservation.
    pub async fn commit(&self, id: &ReservationId, measured_mb: u32) -> AppResult<()> {
        let mut inner = self.inner.lock().await;
        let idx = inner
            .ledger
            .iter()
            .position(|e| &e.id == id)
            .ok_or_else(|| AppError::NotFound(format!("reservation {id}")))?;

        let (raw_mb, calibration) = {
            let entry = &mut inner.ledger[idx];
            entry.committed_mb = Some(measured_mb);
            (entry.raw_mb, entry.calibration.clone())
        };
        if let Some((key, _fallback)) = calibration {
            inner.calibration.record(&key, measured_mb, raw_mb);
        }
        tracing::info!(reservation = %id, measured_mb, "resource reservation committed");
        Ok(())
    }

    /// Give a reservation back to the pool. Idempotent — releasing an unknown or
    /// already-released id is a no-op.
    pub async fn release(&self, id: &ReservationId) {
        let mut inner = self.inner.lock().await;
        let before = inner.ledger.len();
        inner.ledger.retain(|e| &e.id != id);
        if inner.ledger.len() != before {
            tracing::info!(reservation = %id, "resource reservation released");
        }
    }

    /// Periodic housekeeping:
    /// - recover reservations `Held` past the stale TTL with no `commit`
    ///   (a caller that crashed mid-load);
    /// - when measured `used` exceeds our committed total by more than the
    ///   slack, log an external-holder drift (we trust the measurement — every
    ///   `request` already works off measured `free`).
    ///
    /// Returns the number of stale reservations recovered.
    pub async fn reconcile(&self) -> usize {
        let started = Instant::now();
        let mut inner = self.inner.lock().await;

        let ttl = self.stale_ttl;
        let mut recovered = 0;
        inner.ledger.retain(|e| {
            let stale = e.committed_mb.is_none() && e.created_at.elapsed() > ttl;
            if stale {
                recovered += 1;
                tracing::warn!(
                    reservation = %e.id,
                    kind = ?e.kind,
                    age_s = e.created_at.elapsed().as_secs(),
                    "recovering a stale reservation — caller never committed or released"
                );
            }
            !stale
        });

        if let Some(gpu) = inner.cached.gpu {
            let committed: u64 = inner
                .ledger
                .iter()
                .filter(|e| e.kind == ResourceKind::Gpu)
                .filter_map(|e| e.committed_mb.map(u64::from))
                .sum();
            if gpu.used_mb > committed + DRIFT_SLACK_MB {
                tracing::warn!(
                    measured_used_mb = gpu.used_mb,
                    our_committed_mb = committed,
                    "VRAM in use exceeds our reservations — an external process holds GPU memory; \
                     trusting the measurement"
                );
            }
        }

        drop(inner);
        tracing::debug!(
            recovered,
            elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
            "resource ledger reconciled"
        );
        recovered
    }
}

/// Mint a fresh reservation id (UUIDv4, ADR-0017).
fn new_reservation_id() -> ReservationId {
    ReservationId::from_trusted(uuid::Uuid::new_v4().hyphenated().to_string())
}

fn clamp_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
