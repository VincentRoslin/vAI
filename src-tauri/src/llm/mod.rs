//! llama.cpp adapter — the first LLM backend (ADR-0003 / ADR-0004 / ADR-0013).
//!
//! `LlamaBackend` implements [`crate::lifecycle::backend::ModelBackend`]: its
//! `load` spawns a supervised `llama-server` child (loopback HTTP + per-launch
//! bearer token) and waits for `/health`. The loaded [`LlamaServer`] implements
//! both `LoadedInstance` (health / shutdown, for Phase 14) and [`LlmInstance`]
//! (generate / stream, for Phase 16).
//!
//! **Nothing outside this module references llama.cpp** (`ARCHITECTURE.md` §2).

pub mod client;
pub mod job;
pub mod protocol;
pub mod server;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod live_tests;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::contracts::generation::{GenerationEvent, SamplingParams};
use crate::ipc::{AppError, AppResult};
use crate::lifecycle::backend::{LoadRequest, LoadedInstance, ModelBackend};
use client::{Completion, LlamaClient};
use protocol::CompletionRequest;
use server::{bearer_token, pick_free_port, ServerArgs, ServerProcess};

/// The backend key the lifecycle manager registers this adapter under — matches
/// the `backend` string `acquisition` writes for a GGUF LLM.
pub const BACKEND_KEY: &str = "llama.cpp";

const DEFAULT_CTX_TOKENS: u32 = 4_096;
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(250);
const DEFAULT_LOAD_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_CALL_DEADLINE: Duration = Duration::from_secs(300);

/// A generation capability on top of a [`LoadedInstance`]. Phase 16 reaches it
/// with [`as_llm`].
#[async_trait]
pub trait LlmInstance: Send + Sync {
    /// One non-streaming generation.
    ///
    /// # Errors
    /// Transport / timeout / cancellation / server errors, mapped to [`AppError`].
    async fn generate(
        &self,
        prompt: String,
        params: SamplingParams,
        cancel: CancellationToken,
    ) -> AppResult<Completion>;

    /// Streaming generation — events land in `tx` (ordered `TokenDelta`s then one
    /// terminal). Returns once the terminal has been sent.
    async fn stream(
        &self,
        prompt: String,
        params: SamplingParams,
        tx: mpsc::Sender<GenerationEvent>,
        cancel: CancellationToken,
    );
}

/// Downcast a lifecycle-manager instance to its LLM capability, if it has one.
#[must_use]
pub fn as_llm(instance: &Arc<dyn LoadedInstance>) -> Option<&dyn LlmInstance> {
    instance
        .as_any()
        .downcast_ref::<LlamaServer>()
        .map(|s| s as &dyn LlmInstance)
}

/// Spawns `llama-server` children. One per managed GGUF model.
pub struct LlamaBackend {
    binary: PathBuf,
    load_timeout: Duration,
    call_deadline: Duration,
}

impl LlamaBackend {
    /// `binary` is the path to `llama-server(.exe)`.
    #[must_use]
    pub fn new(binary: PathBuf) -> Self {
        Self {
            binary,
            load_timeout: DEFAULT_LOAD_TIMEOUT,
            call_deadline: DEFAULT_CALL_DEADLINE,
        }
    }
}

#[async_trait]
impl ModelBackend for LlamaBackend {
    async fn load(
        &self,
        req: &LoadRequest,
        cancel: CancellationToken,
    ) -> AppResult<Box<dyn LoadedInstance>> {
        let model = &req.model;
        let args = ServerArgs {
            model_path: PathBuf::from(&model.path),
            port: pick_free_port()?,
            token: bearer_token(),
            n_gpu_layers: -1, // offload everything; Phase 23 does smarter placement
            ctx_size: model
                .metadata
                .capabilities
                .context_tokens
                .unwrap_or(DEFAULT_CTX_TOKENS),
        };
        let base_url = args.base_url();
        let estimated_vram_mb = model.metadata.estimated_vram_mb;

        let mut process = ServerProcess::spawn(&self.binary, &args)?;
        let client = LlamaClient::new(base_url.clone(), args.token.clone(), self.call_deadline);

        // Poll /health until ready, the child exits, we time out, or we're cancelled.
        let deadline = tokio::time::Instant::now() + self.load_timeout;
        loop {
            if cancel.is_cancelled() {
                process.shutdown().await;
                return Err(AppError::Cancelled);
            }
            if let Some(code) = process.exit_status() {
                return Err(AppError::BackendUnavailable(format!(
                    "llama-server exited with code {code} before becoming ready"
                )));
            }
            if client.health().await.is_ok() {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                process.shutdown().await;
                return Err(AppError::Timeout(
                    "llama-server did not become ready".to_owned(),
                ));
            }
            tokio::select! {
                () = cancel.cancelled() => {
                    process.shutdown().await;
                    return Err(AppError::Cancelled);
                }
                () = tokio::time::sleep(HEALTH_POLL_INTERVAL) => {}
            }
        }

        tracing::info!(model = %model.metadata.id, %base_url, "llama-server ready");
        Ok(Box::new(LlamaServer {
            process: Mutex::new(Some(process)),
            client,
            estimated_vram_mb,
        }))
    }
}

/// A running `llama-server` + its HTTP client.
pub struct LlamaServer {
    process: Mutex<Option<ServerProcess>>,
    client: LlamaClient,
    estimated_vram_mb: Option<u32>,
}

impl LlamaServer {
    /// Test-only constructor: wrap an already-running server (the stub).
    #[cfg(test)]
    fn attached(client: LlamaClient, estimated_vram_mb: Option<u32>) -> Self {
        Self {
            process: Mutex::new(None),
            client,
            estimated_vram_mb,
        }
    }

    /// The `llama-server` child's OS process id, for the live gate tests.
    #[cfg(test)]
    pub(crate) async fn child_id(&self) -> Option<u32> {
        self.process
            .lock()
            .await
            .as_ref()
            .and_then(ServerProcess::child_id)
    }
}

#[async_trait]
impl LoadedInstance for LlamaServer {
    async fn health(&self) -> AppResult<()> {
        if let Some(process) = self.process.lock().await.as_mut() {
            if let Some(code) = process.exit_status() {
                return Err(AppError::WorkerCrashed(format!(
                    "llama-server exited with code {code}"
                )));
            }
        }
        self.client.health().await
    }

    fn measured_vram_mb(&self) -> Option<u32> {
        // llama-server does not report its footprint; pass the load estimate
        // through so the resource ledger has a figure to commit.
        self.estimated_vram_mb
    }

    async fn shutdown(&self) {
        if let Some(process) = self.process.lock().await.take() {
            process.shutdown().await;
        }
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

#[async_trait]
impl LlmInstance for LlamaServer {
    async fn generate(
        &self,
        prompt: String,
        params: SamplingParams,
        cancel: CancellationToken,
    ) -> AppResult<Completion> {
        params.validate()?;
        let req = CompletionRequest::from_params(prompt, &params, false);
        self.client.complete(&req, &cancel).await
    }

    async fn stream(
        &self,
        prompt: String,
        params: SamplingParams,
        tx: mpsc::Sender<GenerationEvent>,
        cancel: CancellationToken,
    ) {
        if let Err(err) = params.validate() {
            let _ = tx.send(GenerationEvent::Error { error: err }).await;
            return;
        }
        let req = CompletionRequest::from_params(prompt, &params, true);
        self.client.stream(&req, &tx, &cancel).await;
    }
}
