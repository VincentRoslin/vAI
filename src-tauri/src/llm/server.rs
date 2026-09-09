//! Spawning + supervising the `llama-server` child.
//!
//! Transport (ADR-0013): `llama-server` is an upstream HTTP server with no
//! named-pipe mode, so it binds `127.0.0.1:<free port>` and every request must
//! carry a per-launch bearer token (`--api-key`). A Windows Job Object owns the
//! child so it dies with us.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::{Child, Command};

use crate::ipc::{AppError, AppResult};
use crate::job::JobObject;

/// A random 64-hex-char bearer token, minted per launch. Never logged (the
/// logging layer also redacts `Bearer` / `api-key`).
#[must_use]
pub fn bearer_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Grab a free TCP port on the loopback interface (bind `:0`, read it back,
/// drop). A small spawn race remains — accepted (ADR-0013).
///
/// # Errors
/// [`AppError::internal`] if no loopback port can be bound.
pub fn pick_free_port() -> AppResult<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| AppError::internal("bind a free loopback port", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| AppError::internal("read the bound port", e))?
        .port();
    drop(listener);
    Ok(port)
}

/// The knobs the adapter turns when launching `llama-server`.
#[derive(Debug, Clone)]
pub struct ServerArgs {
    /// Absolute path to the `.gguf` weights.
    pub model_path: PathBuf,
    /// Loopback port to bind.
    pub port: u16,
    /// Per-launch bearer token.
    pub token: String,
    /// GPU layer count; `-1` offloads everything.
    pub n_gpu_layers: i32,
    /// Context window in tokens.
    pub ctx_size: u32,
}

impl ServerArgs {
    /// The full argv (after the binary itself).
    #[must_use]
    pub fn to_argv(&self) -> Vec<String> {
        vec![
            "--model".into(),
            self.model_path.display().to_string(),
            "--host".into(),
            "127.0.0.1".into(),
            "--port".into(),
            self.port.to_string(),
            "--api-key".into(),
            self.token.clone(),
            "--n-gpu-layers".into(),
            self.n_gpu_layers.to_string(),
            "--ctx-size".into(),
            self.ctx_size.to_string(),
            // Flash Attention: default 'auto' (enabled where the model supports
            // it) — do not force it, older/edge models fail with it 'on'.
            //
            // `--no-warmup` was dropped (2026-09): it skipped llama.cpp's own
            // warm-up pass, so the *first* prompt after every load paid an
            // extra one-time kernel/graph-build latency spike on top of the
            // model load itself.
            "--no-webui".into(),
        ]
    }

    /// `http://127.0.0.1:<port>` — the base URL for the HTTP client.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

/// A spawned `llama-server`, kept alive by its [`JobObject`].
pub struct ServerProcess {
    child: Child,
    _job: JobObject,
}

impl ServerProcess {
    /// Spawn `binary` with `args`, assign it to a fresh kill-on-close job.
    ///
    /// # Errors
    /// [`AppError::BackendUnavailable`] if the binary is missing or won't start;
    /// [`AppError::internal`] on a job-assignment failure.
    pub fn spawn(binary: &Path, args: &ServerArgs) -> AppResult<Self> {
        if !binary.is_file() {
            return Err(AppError::BackendUnavailable(format!(
                "llama-server binary not found at {}",
                binary.display()
            )));
        }
        let job = JobObject::new().map_err(|e| AppError::internal("create job object", e))?;

        let mut cmd = Command::new(binary);
        cmd.args(args.to_argv())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        crate::job::hide_console(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| {
            AppError::BackendUnavailable(format!("failed to start llama-server: {e}"))
        })?;

        if let Some(handle) = raw_handle(&child) {
            if let Err(err) = job.assign(handle) {
                let _ = child.start_kill();
                return Err(AppError::internal("assign llama-server to job object", err));
            }
        }

        Ok(Self { child, _job: job })
    }

    /// The child's OS process id, while it is running.
    #[cfg(test)]
    #[must_use]
    pub fn child_id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Has the child already exited? (`Some(code)` when it has.)
    pub fn exit_status(&mut self) -> Option<i32> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.code().unwrap_or(-1)),
            _ => None,
        }
    }

    /// Ask the child to stop, then wait briefly, then force-kill.
    pub async fn shutdown(mut self) {
        let _ = self.child.start_kill();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), self.child.wait()).await;
    }
}

#[cfg(windows)]
fn raw_handle(child: &Child) -> Option<isize> {
    child.raw_handle().map(|h| h as isize)
}

#[cfg(not(windows))]
fn raw_handle(_child: &Child) -> Option<isize> {
    None
}
