//! The backend abstraction the lifecycle manager drives.
//!
//! A `ModelBackend` knows how to bring one kind of model into memory (llama.cpp
//! is the first impl, Phase 15). A `LoadedInstance` is the running result — the
//! manager health-checks it, reads its measured footprint, and shuts it down.
//!
//! Detail specific to a runtime lives behind this trait and nowhere else
//! (`CLAUDE.md` Article I; `ARCHITECTURE.md` §2).

use std::any::Any;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::contracts::generation::{GenerationEvent, SamplingParams, StopReason};
use crate::contracts::model::RegisteredModel;
use crate::ipc::AppResult;

/// Everything a backend needs to load one model.
#[derive(Debug, Clone)]
pub struct LoadRequest {
    /// The registry row (metadata + confined path + on-disk availability).
    pub model: RegisteredModel,
}

/// Loads models of one backend family into memory.
#[async_trait]
pub trait ModelBackend: Send + Sync {
    /// Bring the model into memory. Long-running (seconds to minutes). Must
    /// observe `cancel`: on cancellation, tear down any partial state and return
    /// [`crate::ipc::AppError::Cancelled`].
    ///
    /// # Errors
    /// Any failure to load — the manager releases the reservation and applies
    /// its retry policy. [`crate::ipc::AppError::Cancelled`] is treated
    /// specially (no retry).
    async fn load(
        &self,
        req: &LoadRequest,
        cancel: CancellationToken,
    ) -> AppResult<Box<dyn LoadedInstance>>;
}

/// A model that is resident in memory.
#[async_trait]
pub trait LoadedInstance: Send + Sync {
    /// A cheap liveness check. `Err` means the instance is dead and the manager
    /// should move it to `Failed` and release its reservation.
    ///
    /// # Errors
    /// [`crate::ipc::AppError::BackendUnavailable`] /
    /// [`crate::ipc::AppError::WorkerCrashed`] when the instance is not
    /// answering.
    async fn health(&self) -> AppResult<()>;

    /// The VRAM the instance actually occupies, in MB, if the backend can
    /// measure it. `None` → the manager keeps its estimate as the committed
    /// figure.
    fn measured_vram_mb(&self) -> Option<u32>;

    /// Stop the instance and free its resources. Best-effort; must not panic.
    async fn shutdown(&self);

    /// The instance's LLM generation capability, if it is a language model.
    /// Non-LLM instances (STT / TTS / image, later phases) return `None`.
    fn as_llm(&self) -> Option<&dyn LlmInstance> {
        None
    }

    /// Escape hatch for a future capability trait not modelled here. Every impl
    /// is `{ self }`.
    fn as_any(&self) -> &(dyn Any + Send + Sync);
}

/// The result of a non-streaming generation.
#[derive(Debug, Clone)]
pub struct Completion {
    /// The generated text.
    pub text: String,
    /// Tokens generated.
    pub tokens: u32,
    /// Why generation stopped.
    pub stop_reason: StopReason,
}

/// LLM generation on top of a [`LoadedInstance`]. Reached via
/// [`LoadedInstance::as_llm`].
#[async_trait]
pub trait LlmInstance: Send + Sync {
    /// One non-streaming generation.
    ///
    /// # Errors
    /// Transport / timeout / cancellation / backend errors as [`crate::ipc::AppError`].
    async fn generate(
        &self,
        prompt: String,
        params: SamplingParams,
        cancel: CancellationToken,
    ) -> AppResult<Completion>;

    /// Streaming generation — events land in `tx` (ordered `TokenDelta`s then one
    /// terminal: `Done` / `Cancelled` / `Error`). Returns once the terminal has
    /// been sent.
    async fn stream(
        &self,
        prompt: String,
        params: SamplingParams,
        tx: mpsc::Sender<GenerationEvent>,
        cancel: CancellationToken,
    );
}

#[cfg(test)]
pub(crate) use fake::FakeBackend;

#[cfg(test)]
mod fake {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use super::{LoadRequest, LoadedInstance, ModelBackend};
    use crate::ipc::{AppError, AppResult};

    /// A controllable backend for the lifecycle tests. No real process.
    pub(crate) struct FakeBackend {
        load_delay: Duration,
        /// Number of upcoming `load` calls that will fail before one succeeds.
        fail_countdown: AtomicU32,
        loads: AtomicU32,
        shutdowns: Arc<AtomicU32>,
        health_checks: Arc<AtomicU32>,
        /// Shared with every instance this backend hands out.
        healthy: Arc<AtomicBool>,
        measured_vram_mb: Option<u32>,
    }

    impl FakeBackend {
        pub(crate) fn new(load_delay: Duration) -> Self {
            Self {
                load_delay,
                fail_countdown: AtomicU32::new(0),
                loads: AtomicU32::new(0),
                shutdowns: Arc::new(AtomicU32::new(0)),
                health_checks: Arc::new(AtomicU32::new(0)),
                healthy: Arc::new(AtomicBool::new(true)),
                measured_vram_mb: Some(4_096),
            }
        }

        /// The next `n` `load` calls fail with `BackendUnavailable`.
        pub(crate) fn fail_next(&self, n: u32) {
            self.fail_countdown.store(n, Ordering::SeqCst);
        }

        /// Flip the health of every instance this backend has handed out.
        pub(crate) fn set_healthy(&self, healthy: bool) {
            self.healthy.store(healthy, Ordering::SeqCst);
        }

        pub(crate) fn loads(&self) -> u32 {
            self.loads.load(Ordering::SeqCst)
        }

        pub(crate) fn shutdowns(&self) -> u32 {
            self.shutdowns.load(Ordering::SeqCst)
        }

        pub(crate) fn health_checks(&self) -> u32 {
            self.health_checks.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl ModelBackend for FakeBackend {
        async fn load(
            &self,
            _req: &LoadRequest,
            cancel: CancellationToken,
        ) -> AppResult<Box<dyn LoadedInstance>> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            tokio::select! {
                () = cancel.cancelled() => return Err(AppError::Cancelled),
                () = tokio::time::sleep(self.load_delay) => {}
            }
            if self
                .fail_countdown
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
            {
                return Err(AppError::BackendUnavailable("fake load failure".to_owned()));
            }
            self.healthy.store(true, Ordering::SeqCst);
            Ok(Box::new(FakeInstance {
                healthy: Arc::clone(&self.healthy),
                health_checks: Arc::clone(&self.health_checks),
                shutdowns: Arc::clone(&self.shutdowns),
                measured_vram_mb: self.measured_vram_mb,
            }))
        }
    }

    struct FakeInstance {
        healthy: Arc<AtomicBool>,
        health_checks: Arc<AtomicU32>,
        shutdowns: Arc<AtomicU32>,
        measured_vram_mb: Option<u32>,
    }

    #[async_trait]
    impl LoadedInstance for FakeInstance {
        async fn health(&self) -> AppResult<()> {
            self.health_checks.fetch_add(1, Ordering::SeqCst);
            if self.healthy.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err(AppError::WorkerCrashed(
                    "fake instance is unhealthy".to_owned(),
                ))
            }
        }

        fn measured_vram_mb(&self) -> Option<u32> {
            self.measured_vram_mb
        }

        async fn shutdown(&self) {
            self.shutdowns.fetch_add(1, Ordering::SeqCst);
        }

        fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
            self
        }
    }
}
