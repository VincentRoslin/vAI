//! Spawning + supervising the image sidecar child.
//!
//! Transport (ADR-0013): a diffusers/uvicorn HTTP server with no named-pipe
//! mode, so it binds `127.0.0.1:<free port>` and every request carries a
//! per-launch bearer token. A Windows Job Object owns the child so it dies with
//! us. Free-port selection + token minting are shared with `llm::server`.

use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::{Child, Command};

pub use crate::llm::server::{bearer_token, pick_free_port};

use crate::ipc::{AppError, AppResult};
use crate::job::JobObject;

/// The knobs the adapter turns when launching the sidecar.
#[derive(Debug, Clone)]
pub struct SidecarArgs {
    /// The Python interpreter (shared dev venv, ADR-0018; a sibling of the exe
    /// when shipped).
    pub python: PathBuf,
    /// Absolute path to `server.py` (or `server_fake.py` in tests).
    pub script: PathBuf,
    /// Loopback port to bind.
    pub port: u16,
    /// Per-launch bearer token.
    pub token: String,
    /// The Krea 2 model directory (diffusers layout). `None` for the fake.
    pub model_path: Option<PathBuf>,
    /// The NF4 quant-cache directory. `None` for the fake.
    pub quant_cache: Option<PathBuf>,
    /// The confined LoRA directory. `None` for the fake.
    pub loras_dir: Option<PathBuf>,
    /// Extra argv appended after the standard flags (tests only).
    pub extra: Vec<String>,
}

impl SidecarArgs {
    /// The argv after the interpreter + script.
    #[must_use]
    pub fn to_argv(&self) -> Vec<String> {
        let mut argv = vec![
            self.script.display().to_string(),
            "--port".into(),
            self.port.to_string(),
            "--token".into(),
            self.token.clone(),
        ];
        for (flag, path) in [
            ("--model-path", &self.model_path),
            ("--quant-cache", &self.quant_cache),
            ("--loras-dir", &self.loras_dir),
        ] {
            if let Some(p) = path {
                argv.push(flag.into());
                argv.push(p.display().to_string());
            }
        }
        argv.extend(self.extra.iter().cloned());
        argv
    }

    /// `http://127.0.0.1:<port>`.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

/// A spawned image sidecar, kept alive by its [`JobObject`].
pub struct SidecarProcess {
    child: Child,
    _job: JobObject,
}

impl SidecarProcess {
    /// Spawn `python script …args`, assigned to a fresh kill-on-close job.
    ///
    /// # Errors
    /// [`AppError::BackendUnavailable`] if the interpreter or script is missing
    /// or the process will not start; [`AppError::internal`] on job assignment.
    pub fn spawn(args: &SidecarArgs) -> AppResult<Self> {
        if !args.python.is_file() {
            return Err(AppError::BackendUnavailable(format!(
                "Python interpreter not found at {}",
                args.python.display()
            )));
        }
        if !args.script.is_file() {
            return Err(AppError::BackendUnavailable(format!(
                "image sidecar script not found at {}",
                args.script.display()
            )));
        }
        let job = JobObject::new().map_err(|e| AppError::internal("create job object", e))?;

        let mut cmd = Command::new(&args.python);
        cmd.args(args.to_argv())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        crate::worker::env::apply(&mut cmd);
        crate::job::hide_console(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| {
            AppError::BackendUnavailable(format!("failed to start image sidecar: {e}"))
        })?;

        if let Some(handle) = raw_handle(&child) {
            if let Err(err) = job.assign(handle) {
                let _ = child.start_kill();
                return Err(AppError::internal(
                    "assign image sidecar to job object",
                    err,
                ));
            }
        }
        Ok(Self { child, _job: job })
    }

    /// `Some(code)` once the child has exited.
    pub fn exit_status(&mut self) -> Option<i32> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.code().unwrap_or(-1)),
            _ => None,
        }
    }

    /// Ask the child to stop, wait briefly, then force-kill.
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
