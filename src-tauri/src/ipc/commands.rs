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

// ---------------------------------------------------------------- resources

use crate::contracts::resource::ResourceSnapshot;
use crate::resources::ResourceManager;

/// The resource manager's current view: last GPU / RAM measurement + what the
/// reservation ledger holds (ADR-0007).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn resources_snapshot(
    resources: State<'_, Arc<ResourceManager>>,
) -> AppResult<ResourceSnapshot> {
    Ok(resources.snapshot().await)
}

// ---------------------------------------------------------------- lifecycle

use crate::contracts::model::LifecycleStatus;
use crate::lifecycle::LifecycleManager;

/// Every model the lifecycle manager is tracking, with its runtime state
/// (Phase 14).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn lifecycle_status(
    lifecycle: State<'_, Arc<LifecycleManager>>,
) -> AppResult<Vec<LifecycleStatus>> {
    Ok(lifecycle.statuses().await)
}

/// Register a GGUF already present in the model directory (no download).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn model_register_local(
    acquisition: State<'_, AcquisitionService>,
    filename: String,
) -> AppResult<ModelId> {
    acquisition.register_local_gguf(&filename).await
}

/// Load a registered model into memory (Phase 14).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn model_load(lifecycle: State<'_, Arc<LifecycleManager>>, id: ModelId) -> AppResult<()> {
    lifecycle.load(&id).await
}

/// Unload a model.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn model_unload(
    lifecycle: State<'_, Arc<LifecycleManager>>,
    id: ModelId,
) -> AppResult<()> {
    lifecycle.unload(&id).await
}

// ---------------------------------------------------------------- chat (Phase 16 / 17)

use crate::contracts::conversation::{Conversation, GenerationState, Message};
use crate::contracts::generation::GenerationEvent;
use crate::contracts::ids::{ConversationId, TaskId};
use crate::conversation::ConversationEngine;

/// Start a new conversation.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn conversation_create(
    chat: State<'_, Arc<ConversationEngine>>,
) -> AppResult<Conversation> {
    chat.create().await
}

/// Every conversation, newest activity first.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn conversation_list(
    chat: State<'_, Arc<ConversationEngine>>,
) -> AppResult<Vec<Conversation>> {
    chat.list().await
}

/// A conversation's messages, in order.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn conversation_messages(
    chat: State<'_, Arc<ConversationEngine>>,
    id: ConversationId,
) -> AppResult<Vec<Message>> {
    chat.messages(&id).await
}

/// Request body for [`chat_send`].
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ChatSendRequest {
    /// Which conversation.
    pub conversation_id: ConversationId,
    /// Which loaded model to generate with.
    pub model_id: ModelId,
    /// The user's message.
    pub text: String,
}

/// Send a user message and stream the assistant reply over `events`. Returns the
/// generation's task id (for [`chat_cancel`]).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn chat_send(
    chat: State<'_, Arc<ConversationEngine>>,
    req: ChatSendRequest,
    events: Channel<GenerationEvent>,
) -> AppResult<TaskId> {
    let ChatSendRequest {
        conversation_id,
        model_id,
        text,
    } = req;
    chat.send(conversation_id, model_id, text, move |ev| {
        let _ = events.send(ev);
    })
    .await
}

/// Generate a reply over the conversation's existing history (no new user turn).
/// Used after a voice turn was added by the STT pipeline. Streams over `events`.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn chat_generate(
    chat: State<'_, Arc<ConversationEngine>>,
    conversation_id: ConversationId,
    model_id: ModelId,
    events: Channel<GenerationEvent>,
) -> AppResult<TaskId> {
    chat.generate(conversation_id, model_id, move |ev| {
        let _ = events.send(ev);
    })
    .await
}

/// The engine's streaming state — whether a generation is running and, if so,
/// its task + conversation.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn chat_state(chat: State<'_, Arc<ConversationEngine>>) -> AppResult<GenerationState> {
    Ok(chat.generation_state().await)
}

/// Cancel an in-flight generation.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn chat_cancel(
    chat: State<'_, Arc<ConversationEngine>>,
    task_id: TaskId,
) -> AppResult<()> {
    chat.cancel(&task_id).await
}

// ---------------------------------------------------------------- voice (Phase 18)

use crate::voice::capture::InputDevice;
use crate::voice::{VoiceInput, VoiceState};

/// List the machine's audio input devices.
#[must_use]
#[tauri::command]
pub fn voice_input_devices() -> Vec<InputDevice> {
    crate::voice::capture::list_input_devices()
}

/// Start push-to-talk listening on `conversation_id`; `events` receives every
/// [`VoiceState`] change until the session ends.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn voice_start(
    voice: State<'_, Arc<VoiceInput>>,
    conversation_id: ConversationId,
    events: Channel<VoiceState>,
) -> AppResult<()> {
    let mut rx = voice.subscribe();
    let _ = events.send(*rx.borrow_and_update());
    tokio::spawn(async move {
        while rx.changed().await.is_ok() {
            if events.send(*rx.borrow_and_update()).is_err() {
                break;
            }
        }
    });
    voice.start_listening(conversation_id).await
}

/// Release push-to-talk — transcribe an utterance in progress, then stop.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn voice_stop(voice: State<'_, Arc<VoiceInput>>) -> AppResult<()> {
    voice.stop_listening().await;
    Ok(())
}

/// The current voice state (poll fallback for the channel).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn voice_state(voice: State<'_, Arc<VoiceInput>>) -> AppResult<VoiceState> {
    Ok(voice.state())
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
