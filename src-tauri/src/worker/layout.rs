//! Where the Python interpreter and the worker scripts live.
//!
//! **Dev:** `<repo>/.venv/Scripts/python.exe` + `<repo>/workers/` — from config
//! (`workers.python`, `workers.dir`, schema v6, ADR-0018).
//! **Shipped:** siblings of the executable, resolved from `current_exe()`, never
//! PATH (ADR-0014). The packaged build ignores the config keys.

use std::path::PathBuf;

use crate::contracts::worker::WorkerKind;
use crate::ipc::{AppError, AppResult};

/// Resolved paths for launching a worker.
#[derive(Debug, Clone)]
pub struct WorkerLayout {
    /// The Python interpreter.
    pub python: PathBuf,
    /// The directory holding `stt.py`, `tts.py`, …
    pub workers_dir: PathBuf,
}

impl WorkerLayout {
    /// Build from the config values (already absolute — `config::validate`
    /// checks that).
    #[must_use]
    pub fn from_config(python: PathBuf, workers_dir: PathBuf) -> Self {
        Self {
            python,
            workers_dir,
        }
    }

    /// The script path for one worker kind, verified to exist.
    ///
    /// # Errors
    /// [`AppError::BackendUnavailable`] if the interpreter or the script is
    /// missing (a clear, explicit error — never a silent fallback).
    pub fn script_for(&self, kind: WorkerKind) -> AppResult<PathBuf> {
        let name = match kind {
            WorkerKind::Stt => "stt.py",
            WorkerKind::Tts => "tts.py",
            WorkerKind::Embed => "embedder.py",
        };
        let script = self.workers_dir.join(name);
        if !self.python.is_file() {
            return Err(AppError::BackendUnavailable(format!(
                "Python interpreter not found at {} — run scripts/setup-venv.mjs",
                self.python.display()
            )));
        }
        if !script.is_file() {
            return Err(AppError::BackendUnavailable(format!(
                "worker script not found at {}",
                script.display()
            )));
        }
        Ok(script)
    }

    /// A layout pointing at a test `workers/` dir (tests — the fake worker).
    #[cfg(test)]
    #[must_use]
    pub fn for_test(python: impl Into<PathBuf>, workers_dir: impl Into<PathBuf>) -> Self {
        Self {
            python: python.into(),
            workers_dir: workers_dir.into(),
        }
    }
}
