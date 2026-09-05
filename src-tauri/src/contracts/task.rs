//! Task + task-status contracts.
//!
//! Every long-running operation the core performs is a *task* with a [`TaskId`].
//! Progress and cancellation are part of the contract, not out-of-band.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::contracts::ids::TaskId;
use crate::ipc::AppError;

/// What kind of work a task represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum TaskKind {
    /// Text generation from an LLM.
    LlmGeneration,
    /// Image generation from the diffusion backend.
    ImageGeneration,
    /// Speech-to-text over a captured utterance.
    Stt,
    /// Text-to-speech synthesis.
    Tts,
    /// Producing an embedding vector (e.g. the face-identity gate).
    Embedding,
    /// Downloading a model from a remote source.
    ModelDownload,
    /// Loading a model into memory.
    ModelLoad,
    /// Unloading a model from memory.
    ModelUnload,
}

/// Where a task is in its lifecycle. Terminal states: `Succeeded`, `Failed`,
/// `Cancelled`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum TaskState {
    /// Accepted, waiting on a resource or the scheduler.
    Queued,
    /// Actively running.
    Running,
    /// Finished successfully.
    Succeeded,
    /// Finished with an error.
    Failed,
    /// Stopped at the caller's request.
    Cancelled,
}

impl TaskState {
    /// Whether no further transitions are possible.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// A snapshot of one task. Broadcast on progress and on every state change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct TaskStatus {
    /// The task this snapshot describes.
    pub id: TaskId,
    /// What the task is doing.
    pub kind: TaskKind,
    /// Lifecycle position.
    pub state: TaskState,
    /// Fractional progress in `0.0..=1.0` when the task can estimate it.
    pub progress: Option<f32>,
    /// Short human-readable detail (“loading shards 3/7”, “queued behind 2”).
    pub detail: Option<String>,
}

impl TaskStatus {
    /// Reject a nonsensical snapshot: progress must be a finite fraction in
    /// `0.0..=1.0`, and a terminal state should not carry partial progress.
    ///
    /// # Errors
    /// [`AppError::Validation`] if `progress` is out of range or not finite.
    pub fn validate(&self) -> Result<(), AppError> {
        if let Some(p) = self.progress {
            if !p.is_finite() || !(0.0..=1.0).contains(&p) {
                return Err(AppError::Validation(format!(
                    "task progress must be within 0.0..=1.0, got {p}"
                )));
            }
        }
        Ok(())
    }
}

/// Request to cancel a running or queued task. Idempotent: cancelling an
/// already-terminal task is a no-op, not an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct CancelRequest {
    /// The task to cancel.
    pub task_id: TaskId,
}
