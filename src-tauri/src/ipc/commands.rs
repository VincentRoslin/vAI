//! IPC commands. Each is thin: validate input → call a service → return a DTO.
//! No business logic lives here (`CLAUDE.md` Article I).

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use super::error::{AppError, AppResult};
use crate::config::{AppConfig, ConfigKeyInfo, ConfigManager, ConfigSet};

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

// ---------------------------------------------------------------- config
//
// Tauri injects `State` by value into command handlers; `needless_pass_by_value`
// is expected here and allowed per command.

/// The effective configuration (defaults ← file ← session overrides).
#[must_use]
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn config_get(config: State<'_, std::sync::Arc<ConfigManager>>) -> AppConfig {
    config.effective()
}

/// Change one configuration value — persisted to the file when `persist`, else a
/// session-only override.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn config_set(
    config: State<'_, std::sync::Arc<ConfigManager>>,
    req: ConfigSet,
) -> AppResult<()> {
    let ConfigSet {
        key,
        value,
        persist,
    } = req;
    if persist {
        config.set_user(key, &value)
    } else {
        config.set_session(key, &value)
    }
}

/// Every overridable key with its current effective value.
#[must_use]
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn config_keys(config: State<'_, std::sync::Arc<ConfigManager>>) -> Vec<ConfigKeyInfo> {
    config.keys()
}

// ---------------------------------------------------------------- models + acquisition
//
// `State` is injected by value (Tauri) → `needless_pass_by_value` allowed per fn.

use std::sync::Arc;

use tauri::ipc::Channel;

use crate::acquisition::{AcquisitionService, FixedModel};
use crate::contracts::acquisition::{DownloadInfo, DownloadProgress, HfGgufFile, HfModelSummary};
use crate::contracts::ids::{DownloadId, ModelId};
use crate::contracts::model::RegisteredModel;
use crate::models::ModelRegistry;

/// Every registered model.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn models_list(
    registry: State<'_, Arc<ModelRegistry>>,
) -> AppResult<Vec<RegisteredModel>> {
    registry.list().await
}

/// Delete a model — its file, its registry row, and any download row.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn model_delete(
    registry: State<'_, Arc<ModelRegistry>>,
    db: State<'_, Arc<crate::db::Db>>,
    id: ModelId,
) -> AppResult<()> {
    crate::acquisition::delete_model(&registry, &db, &id).await
}

/// Search HuggingFace for GGUF models.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn hf_search(
    acquisition: State<'_, AcquisitionService>,
    query: String,
    limit: u32,
) -> AppResult<Vec<HfModelSummary>> {
    acquisition.search(&query, limit).await
}

/// List a repo's `.gguf` files (with header-derived quant / context).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn hf_list_files(
    acquisition: State<'_, AcquisitionService>,
    repo: String,
) -> AppResult<Vec<HfGgufFile>> {
    acquisition.list_files(&repo).await
}

/// Request body for [`download_start`].
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DownloadRequest {
    /// `owner/name`.
    pub repo: String,
    /// File within the repo.
    pub filename: String,
    /// Size in bytes from the file listing (for the budget check).
    pub size: Option<u64>,
    /// SHA-256 from the file listing (for verification).
    pub sha256: Option<String>,
}

/// Start downloading one GGUF file. `progress` receives byte ticks.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn download_start(
    acquisition: State<'_, AcquisitionService>,
    req: DownloadRequest,
    progress: Channel<DownloadProgress>,
) -> AppResult<DownloadId> {
    let DownloadRequest {
        repo,
        filename,
        size,
        sha256,
    } = req;
    acquisition
        .download_gguf(&repo, &filename, size, sha256, Some(progress))
        .await
}

/// Pause a running download.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn download_pause(
    acquisition: State<'_, AcquisitionService>,
    id: DownloadId,
) -> AppResult<()> {
    acquisition.pause(&id).await
}

/// Resume a paused download.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn download_resume(
    acquisition: State<'_, AcquisitionService>,
    id: DownloadId,
) -> AppResult<()> {
    acquisition.resume(&id).await
}

/// Cancel a download (deletes the `.part` + row).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn download_cancel(
    acquisition: State<'_, AcquisitionService>,
    id: DownloadId,
) -> AppResult<()> {
    acquisition.cancel(&id).await
}

/// Every download row.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn downloads_list(
    acquisition: State<'_, AcquisitionService>,
) -> AppResult<Vec<DownloadInfo>> {
    acquisition.downloads().await
}

/// Which fixed model bundle to acquire.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum FixedModelKind {
    /// faster-whisper (STT).
    Stt,
    /// Chatterbox Turbo (TTS).
    Tts,
}

/// Acquire the pinned faster-whisper or Chatterbox model bundle.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn acquire_fixed(
    acquisition: State<'_, AcquisitionService>,
    which: FixedModelKind,
) -> AppResult<Vec<DownloadId>> {
    let which = match which {
        FixedModelKind::Stt => FixedModel::Stt,
        FixedModelKind::Tts => FixedModel::Tts,
    };
    acquisition.acquire_fixed(which).await
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
