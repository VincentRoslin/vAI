//! The single IPC boundary error type.
//!
//! Per-subsystem code uses its own `thiserror` enums; the IPC layer maps them
//! into [`AppError`] with an explicit `From`. The full error chain is logged at
//! the boundary — only the stable `kind` + a human `message` cross to the UI.
//!
//! `AppError` also travels the worker boundary inside
//! [`crate::contracts::worker::WorkerResult`] and
//! [`crate::contracts::generation::GenerationEvent`], so it derives
//! `Deserialize` too.
//!
//! ## `kind` taxonomy (stable, machine-readable)
//!
//! | `kind` | meaning | retriable? | typically raised by |
//! | ------ | ------- | ---------- | ------------------- |
//! | `NotFound` | a referenced entity does not exist | no | registry, repositories |
//! | `Validation` | input failed a schema / range / allow-list check | no | IPC commands, contract `validate()` |
//! | `Conflict` | the request contradicts current state (duplicate, wrong phase) | no | lifecycle, resource manager |
//! | `ResourceExhausted` | not enough VRAM / RAM / disk to proceed | yes, later | resource manager, acquisition |
//! | `Timeout` | an operation exceeded its deadline | yes | adapters, workers |
//! | `Cancelled` | the caller cancelled the task | no | anything cancellable |
//! | `BackendUnavailable` | a model server / worker is down or unreachable | yes | adapters, worker supervisor |
//! | `WorkerCrashed` | a subprocess exited unexpectedly mid-job | yes | worker supervisor |
//! | `Internal` | an unexpected bug; real cause is in the logs only | no | anywhere |
//!
//! Evolution: additive-only. New variants may be added (the enum is
//! `#[non_exhaustive]`); existing tag strings never change meaning. See
//! `docs/contracts.md`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::contracts::ids::TaskId;

/// Error returned across the IPC boundary. Serialized as
/// `{ "kind": "...", "message": "..." }` (the `message` key is absent for the
/// unit variants `Cancelled` and `Internal`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(tag = "kind", content = "message")]
#[ts(export, export_to = "../../src/bindings/")]
#[non_exhaustive]
pub enum AppError {
    /// A requested resource does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// Input failed validation.
    #[error("invalid input: {0}")]
    Validation(String),

    /// The request contradicts current state (duplicate, wrong lifecycle phase).
    #[error("conflict: {0}")]
    Conflict(String),

    /// Not enough VRAM / system RAM / disk to proceed.
    #[error("resource exhausted: {0}")]
    ResourceExhausted(String),

    /// An operation exceeded its deadline.
    #[error("timed out: {0}")]
    Timeout(String),

    /// The operation was cancelled.
    #[error("cancelled")]
    Cancelled,

    /// A model server or worker is down or unreachable.
    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),

    /// A subprocess exited unexpectedly mid-job.
    #[error("worker crashed: {0}")]
    WorkerCrashed(String),

    /// An unexpected internal error. The real cause is in the logs.
    #[error("internal error")]
    Internal,
}

impl AppError {
    /// The stable machine-readable discriminant, as a `&'static str`.
    #[must_use]
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "NotFound",
            Self::Validation(_) => "Validation",
            Self::Conflict(_) => "Conflict",
            Self::ResourceExhausted(_) => "ResourceExhausted",
            Self::Timeout(_) => "Timeout",
            Self::Cancelled => "Cancelled",
            Self::BackendUnavailable(_) => "BackendUnavailable",
            Self::WorkerCrashed(_) => "WorkerCrashed",
            Self::Internal => "Internal",
        }
    }

    /// Emit a structured `error!` line for this error with a `context` tag. Use
    /// at the point a fallible operation fails, before the error crosses the IPC
    /// boundary (`Internal`'s real cause is already logged at its source).
    pub fn log(&self, context: &str) {
        tracing::error!(kind = self.kind_str(), context, error = %self, "operation failed");
    }

    /// Log the full context and return an opaque [`AppError::Internal`].
    pub fn internal(context: &str, source: impl std::fmt::Display) -> Self {
        tracing::error!(context, %source, "internal error");
        Self::Internal
    }

    /// Attach a task id, producing an [`ErrorEnvelope`] for callers that need to
    /// correlate the failure with the task that caused it.
    #[must_use]
    pub fn with_task(self, task_id: TaskId) -> ErrorEnvelope {
        ErrorEnvelope {
            error: self,
            task_id: Some(task_id),
        }
    }
}

/// An [`AppError`] plus optional task correlation. Used where the `kind` alone
/// is not enough to route the failure (e.g. one of several concurrent tasks
/// failed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ErrorEnvelope {
    /// The underlying error.
    pub error: AppError,
    /// The task this failure belongs to, when there is one.
    pub task_id: Option<TaskId>,
}

/// Convenience alias for command results.
pub type AppResult<T> = Result<T, AppError>;
