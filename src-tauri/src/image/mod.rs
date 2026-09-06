//! Image adapter — the Krea 2 Turbo backend (Phase 22, ADR-0006 / ADR-0013).
//!
//! `Krea2Backend` implements [`crate::lifecycle::backend::ModelBackend`]: its
//! `load` spawns a supervised image sidecar child (loopback HTTP + per-launch
//! bearer token) and waits for the `ready` handshake. The loaded `Krea2Server`
//! implements [`LoadedInstance`] (health / shutdown, Phase 14) and
//! [`ImageInstance`] (generate, Phase 22).
//!
//! **Nothing outside this module references diffusers, Krea, or the sidecar's
//! HTTP shape** (`ARCHITECTURE.md` §2). The sidecar owns no VRAM and makes no
//! load/unload decision — that is the resource manager + (Phase 22 manual,
//! Phase 23 automated) orchestrator's job.

pub mod client;
pub mod orchestrator;
pub mod protocol;
pub mod repo;
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

use crate::ipc::{AppError, AppResult};
use crate::lifecycle::backend::{
    GeneratedPng, ImageGenerateArgs, ImageInstance, ImageStepProgress, LoadRequest, LoadedInstance,
    ModelBackend,
};
use client::ImageClient;
use protocol::{GenerateBody, LoraBody, PROTOCOL_VERSION};
use server::{bearer_token, pick_free_port, SidecarArgs, SidecarProcess};

/// The backend key the lifecycle manager registers this adapter under — matches
/// the `backend` string acquisition writes for the Krea 2 model.
pub const BACKEND_KEY: &str = "krea2-diffusers";

/// The diffusers-format repo id the sidecar resolves from the **offline** HF
/// cache (`HF_HUB_OFFLINE=1`, ADR-0015) when the registry path is a marker dir
/// rather than a full local diffusers layout. The `unsloth/*` mirror is ungated;
/// `krea/Krea-2-Turbo` is gated. Overridable in the sidecar via `KREA2_MODEL_ID`.
pub const KREA2_MODEL_ID: &str = "unsloth/Krea-2-Turbo";

/// Swap a `.venv` path component for `.venv-image` (ADR-0018 two-venv
/// amendment — the image sidecar + `quantize.py` run from their own venv).
/// Returns the input unchanged when there is no `.venv` component (packaged
/// build — ADR-0014 sibling layout).
#[must_use]
pub fn image_venv_python(workers_python: &std::path::Path) -> PathBuf {
    let mut out = PathBuf::new();
    let mut swapped = false;
    for comp in workers_python.components() {
        if comp.as_os_str() == ".venv" {
            out.push(".venv-image");
            swapped = true;
        } else {
            out.push(comp.as_os_str());
        }
    }
    if swapped {
        out
    } else {
        workers_python.to_path_buf()
    }
}

const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(250);
const PROGRESS_POLL_INTERVAL: Duration = Duration::from_millis(250);
const DEFAULT_LOAD_TIMEOUT: Duration = Duration::from_secs(180);
const DEFAULT_GENERATE_DEADLINE: Duration = Duration::from_secs(600);

/// Spawns image-sidecar children. One per managed Krea 2 model.
pub struct Krea2Backend {
    /// Python interpreter for the sidecar.
    python: PathBuf,
    /// Absolute path to `server.py` (or `server_fake.py` in tests).
    script: PathBuf,
    /// The NF4 quant-cache dir (`None` ⇒ the fake sidecar).
    quant_cache: Option<PathBuf>,
    /// The confined LoRA dir (`None` ⇒ the fake sidecar).
    loras_dir: Option<PathBuf>,
    /// Where per-call PNG exchange dirs are created.
    exchange_root: PathBuf,
    load_timeout: Duration,
    generate_deadline: Duration,
    /// Extra argv appended after the standard flags (tests only — e.g.
    /// `--protocol 99` to drive the handshake-mismatch path).
    extra_args: Vec<String>,
}

impl Krea2Backend {
    /// Build the backend. `exchange_root` is a scratch dir the core owns;
    /// `quant_cache` / `loras_dir` are `None` only for the fake sidecar.
    #[must_use]
    pub fn new(
        python: PathBuf,
        script: PathBuf,
        quant_cache: Option<PathBuf>,
        loras_dir: Option<PathBuf>,
        exchange_root: PathBuf,
    ) -> Self {
        Self {
            python,
            script,
            quant_cache,
            loras_dir,
            exchange_root,
            load_timeout: DEFAULT_LOAD_TIMEOUT,
            generate_deadline: DEFAULT_GENERATE_DEADLINE,
            extra_args: Vec::new(),
        }
    }

    /// Append extra sidecar argv (tests only).
    #[cfg(test)]
    #[must_use]
    fn with_extra_args(mut self, args: impl IntoIterator<Item = String>) -> Self {
        self.extra_args = args.into_iter().collect();
        self
    }
}

#[async_trait]
impl ModelBackend for Krea2Backend {
    async fn load(
        &self,
        req: &LoadRequest,
        cancel: CancellationToken,
    ) -> AppResult<Box<dyn LoadedInstance>> {
        let model = &req.model;
        // A full local diffusers layout (has `model_index.json`) is loaded
        // directly; a marker dir — what acquisition registers when the weights
        // live in the shared HF cache — falls back to the repo id the sidecar
        // resolves offline.
        let model_ref = {
            let p = PathBuf::from(&model.path);
            if p.join("model_index.json").is_file() {
                p.display().to_string()
            } else {
                KREA2_MODEL_ID.to_owned()
            }
        };
        let args = SidecarArgs {
            python: self.python.clone(),
            script: self.script.clone(),
            port: pick_free_port()?,
            token: bearer_token(),
            model_path: Some(PathBuf::from(model_ref)),
            quant_cache: self.quant_cache.clone(),
            loras_dir: self.loras_dir.clone(),
            extra: self.extra_args.clone(),
        };
        let base_url = args.base_url();
        let estimated_vram_mb = model.metadata.estimated_vram_mb;

        let mut process = SidecarProcess::spawn(&args)?;
        let client = Arc::new(ImageClient::new(
            base_url.clone(),
            args.token.clone(),
            self.generate_deadline,
        ));

        let deadline = tokio::time::Instant::now() + self.load_timeout;
        loop {
            if cancel.is_cancelled() {
                process.shutdown().await;
                return Err(AppError::Cancelled);
            }
            if let Some(code) = process.exit_status() {
                return Err(AppError::BackendUnavailable(format!(
                    "image sidecar exited with code {code} before becoming ready"
                )));
            }
            match client.health().await {
                Ok(h) if h.status == "ok" => {
                    if h.protocol != PROTOCOL_VERSION {
                        process.shutdown().await;
                        return Err(AppError::BackendUnavailable(format!(
                            "image sidecar protocol {} != expected {PROTOCOL_VERSION}",
                            h.protocol
                        )));
                    }
                    break;
                }
                _ => {}
            }
            if tokio::time::Instant::now() >= deadline {
                process.shutdown().await;
                return Err(AppError::Timeout(
                    "image sidecar did not become ready".to_owned(),
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

        tracing::info!(model = %model.metadata.id, %base_url, "image sidecar ready");
        Ok(Box::new(Krea2Server {
            process: Mutex::new(Some(process)),
            client,
            estimated_vram_mb,
            exchange_root: self.exchange_root.clone(),
        }))
    }
}

/// A running image sidecar + its HTTP client.
pub struct Krea2Server {
    process: Mutex<Option<SidecarProcess>>,
    client: Arc<ImageClient>,
    estimated_vram_mb: Option<u32>,
    exchange_root: PathBuf,
}

#[async_trait]
impl LoadedInstance for Krea2Server {
    async fn health(&self) -> AppResult<()> {
        if let Some(process) = self.process.lock().await.as_mut() {
            if let Some(code) = process.exit_status() {
                return Err(AppError::WorkerCrashed(format!(
                    "image sidecar exited with code {code}"
                )));
            }
        }
        self.client.health().await.map(|_| ())
    }

    fn measured_vram_mb(&self) -> Option<u32> {
        self.estimated_vram_mb
    }

    async fn shutdown(&self) {
        self.client.unload().await;
        if let Some(process) = self.process.lock().await.take() {
            process.shutdown().await;
        }
    }

    fn as_image(&self) -> Option<&dyn ImageInstance> {
        Some(self)
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

#[async_trait]
impl ImageInstance for Krea2Server {
    async fn generate(
        &self,
        args: ImageGenerateArgs,
        progress: mpsc::UnboundedSender<ImageStepProgress>,
        cancel: CancellationToken,
    ) -> AppResult<Vec<GeneratedPng>> {
        let call_dir = self
            .exchange_root
            .join(uuid::Uuid::new_v4().simple().to_string());
        tokio::fs::create_dir_all(&call_dir)
            .await
            .map_err(|e| AppError::internal("create image exchange dir", e))?;

        let body = GenerateBody {
            prompt: args.prompt,
            negative: args.negative,
            width: args.width,
            height: args.height,
            steps: args.steps,
            guidance: args.guidance,
            seed: args.seed,
            batch_count: args.batch_count,
            out_dir: call_dir.display().to_string(),
            lora: args.lora.map(|(file, weight)| LoraBody { file, weight }),
        };

        // Poll /progress in the background and forward step frames.
        let poll_stop = CancellationToken::new();
        let poller = {
            let client = Arc::clone(&self.client);
            let stop = poll_stop.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        () = stop.cancelled() => break,
                        () = tokio::time::sleep(PROGRESS_POLL_INTERVAL) => {}
                    }
                    if let Ok(p) = client.progress().await {
                        if p.active {
                            let _ = progress.send(ImageStepProgress {
                                image_index: p.image_index,
                                step: p.step,
                                total_steps: p.total_steps,
                            });
                        }
                    }
                }
            })
        };

        let result = self.client.generate(&body, &cancel).await;
        poll_stop.cancel();
        let _ = poller.await;

        let out = match result {
            Ok(resp) => {
                let mut pngs = Vec::with_capacity(resp.images.len());
                for entry in resp.images {
                    let bytes = tokio::fs::read(&entry.path).await.map_err(|e| {
                        AppError::internal("read generated image", format!("{}: {e}", entry.path))
                    })?;
                    pngs.push(GeneratedPng {
                        bytes,
                        seed: entry.seed,
                    });
                }
                Ok(pngs)
            }
            Err(e) => Err(e),
        };

        let _ = tokio::fs::remove_dir_all(&call_dir).await;
        out
    }
}
