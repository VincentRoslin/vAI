//! IPC commands. Each is thin: validate input → call a service → return a DTO.
//! No business logic lives here (`CLAUDE.md` Article I).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::error::{AppError, AppResult};

/// Response for [`app_ready`] — the startup handshake.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct AppReady {
    /// Core crate version.
    pub version: String,
}

/// Startup handshake. The frontend calls this once the webview is live to
/// confirm the core is up and learn its version. This is a command rather than
/// an event because an event emitted during `.setup()` fires before the webview
/// can subscribe.
#[must_use]
#[tauri::command]
pub fn app_ready() -> AppReady {
    AppReady {
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Response for [`app_ping`].
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Pong {
    /// Echoes the request's nonce so the caller can correlate.
    pub nonce: String,
    /// Core crate version.
    pub version: String,
}

/// A no-op round-trip used to verify the IPC path end to end.
#[tauri::command]
pub fn app_ping(nonce: String) -> AppResult<Pong> {
    if nonce.trim().is_empty() {
        return Err(AppError::Validation("nonce must not be empty".into()));
    }
    tracing::debug!(%nonce, "app_ping");
    Ok(Pong {
        nonce,
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Log levels the frontend may forward. Mirrors `tracing::Level`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum FrontendLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<FrontendLogLevel> for tracing::Level {
    fn from(level: FrontendLogLevel) -> Self {
        match level {
            FrontendLogLevel::Error => Self::ERROR,
            FrontendLogLevel::Warn => Self::WARN,
            FrontendLogLevel::Info => Self::INFO,
            FrontendLogLevel::Debug => Self::DEBUG,
            FrontendLogLevel::Trace => Self::TRACE,
        }
    }
}

/// A log line forwarded from the frontend. The frontend never writes logs
/// directly — it routes them here so everything lands in one structured stream.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct FrontendLog {
    pub level: FrontendLogLevel,
    pub target: String,
    pub message: String,
}

#[tauri::command]
pub fn frontend_log(entry: FrontendLog) {
    let FrontendLog {
        level,
        target,
        message,
    } = entry;
    let source = format!("frontend::{target}");
    match tracing::Level::from(level) {
        tracing::Level::ERROR => tracing::error!(target: "frontend", source, "{message}"),
        tracing::Level::WARN => tracing::warn!(target: "frontend", source, "{message}"),
        tracing::Level::INFO => tracing::info!(target: "frontend", source, "{message}"),
        tracing::Level::DEBUG => tracing::debug!(target: "frontend", source, "{message}"),
        tracing::Level::TRACE => tracing::trace!(target: "frontend", source, "{message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_ping_echoes_nonce() {
        let pong = app_ping("abc123".to_string()).expect("ok");
        assert_eq!(pong.nonce, "abc123");
        assert_eq!(pong.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn app_ping_rejects_empty_nonce() {
        let err = app_ping("   ".to_string()).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn app_ready_reports_version() {
        assert_eq!(app_ready().version, env!("CARGO_PKG_VERSION"));
    }

    /// Regenerates the TypeScript bindings from the Rust types. The check script
    /// runs this then fails if `git` reports a diff — keeping `src/bindings/` in
    /// sync with the contract.
    #[test]
    fn export_bindings() {
        use ts_rs::TS;
        AppReady::export_all().unwrap();
        Pong::export_all().unwrap();
        FrontendLog::export_all().unwrap();
        FrontendLogLevel::export_all().unwrap();
        AppError::export_all().unwrap();
    }
}
