//! The single IPC boundary error type.
//!
//! Per-subsystem code uses its own `thiserror` enums; the IPC layer maps them
//! into [`AppError`] with an explicit `From`. The full error chain is logged at
//! the boundary — only the stable `kind` + a human `message` cross to the UI.

use serde::Serialize;
use ts_rs::TS;

/// Error returned across the IPC boundary. Serialized as
/// `{ "kind": "...", "message": "..." }` for the frontend to switch on.
#[derive(Debug, Clone, Serialize, TS, thiserror::Error)]
#[serde(tag = "kind", content = "message")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum AppError {
    /// A requested resource does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// Input failed validation.
    #[error("invalid input: {0}")]
    Validation(String),

    /// A subsystem/backend failed. The detail is safe to show; the chain is logged.
    #[error("{0}")]
    Backend(String),

    /// The operation was cancelled.
    #[error("cancelled")]
    Cancelled,

    /// An unexpected internal error. The real cause is in the logs.
    #[error("internal error")]
    Internal,
}

impl AppError {
    /// Log the full context and return an opaque [`AppError::Internal`].
    pub fn internal(context: &str, source: impl std::fmt::Display) -> Self {
        tracing::error!(context, %source, "internal error");
        Self::Internal
    }
}

/// Convenience alias for command results.
pub type AppResult<T> = Result<T, AppError>;
