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

/// Acquire the Krea 2 Turbo image model (Phase 22.B): verify the bf16 weights
/// are in the HuggingFace cache, build the NF4 quant cache once if absent, and
/// register it. Downloads nothing. Returns the registry id.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn acquire_image_model(acquisition: State<'_, AcquisitionService>) -> AppResult<ModelId> {
    acquisition.acquire_image().await
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

/// Register a GGUF already present in `<models_dir>/llm/` (no download).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn model_register_local(
    acquisition: State<'_, AcquisitionService>,
    filename: String,
) -> AppResult<ModelId> {
    acquisition.register_local_gguf(&filename).await
}

/// Scan `<models_dir>/llm/` for GGUFs not yet in the registry and register
/// them. Returns how many were newly added.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn models_rescan(acquisition: State<'_, AcquisitionService>) -> AppResult<u32> {
    let added = acquisition.scan_llm_models().await?;
    Ok(u32::try_from(added).unwrap_or(u32::MAX))
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

/// Start a new conversation, optionally bound to a Persona (Phase 20).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn conversation_create(
    chat: State<'_, Arc<ConversationEngine>>,
    persona_id: Option<crate::contracts::ids::PersonaId>,
) -> AppResult<Conversation> {
    chat.create_with_persona(persona_id).await
}

/// Bind (or clear, with `null`) the Persona for a conversation. Rejected once
/// the conversation has a turn (FR-17).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn conversation_set_persona(
    chat: State<'_, Arc<ConversationEngine>>,
    conversation_id: ConversationId,
    persona_id: Option<crate::contracts::ids::PersonaId>,
) -> AppResult<()> {
    chat.set_persona(&conversation_id, persona_id).await
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

// ---------------------------------------------------------------- personas + prompt preview (Phase 20)

use crate::context::builder::Provenance;
use crate::context::persona::{Persona, PersonaDraft};
use crate::contracts::ids::PersonaId;

/// The exact assembled prompt for the next generation on a conversation, plus
/// its [`Provenance`] — the FR-34 "show prompt" surface.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct PromptPreview {
    /// The full string the LLM adapter would receive.
    pub prompt: String,
    /// What went into it (token budget, persona inclusion, history kept/dropped).
    pub provenance: Provenance,
}

/// Every persona, newest first.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn persona_list(chat: State<'_, Arc<ConversationEngine>>) -> AppResult<Vec<Persona>> {
    chat.personas().list().await
}

/// One persona.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn persona_get(
    chat: State<'_, Arc<ConversationEngine>>,
    id: PersonaId,
) -> AppResult<Persona> {
    chat.personas().get(&id).await
}

/// Create a persona.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn persona_create(
    chat: State<'_, Arc<ConversationEngine>>,
    draft: PersonaDraft,
) -> AppResult<PersonaId> {
    chat.personas().create(draft).await
}

/// Update a persona in place.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn persona_update(
    chat: State<'_, Arc<ConversationEngine>>,
    id: PersonaId,
    draft: PersonaDraft,
) -> AppResult<()> {
    chat.personas().update(&id, draft).await
}

/// Delete a persona. Conversations bound to it fall back to the default
/// assistant.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn persona_delete(
    chat: State<'_, Arc<ConversationEngine>>,
    id: PersonaId,
) -> AppResult<()> {
    chat.personas().delete(&id).await
}

/// Assemble and return the exact prompt the next generation on `conversation_id`
/// with `model_id` would receive (FR-34).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn chat_prompt_preview(
    chat: State<'_, Arc<ConversationEngine>>,
    conversation_id: ConversationId,
    model_id: ModelId,
) -> AppResult<PromptPreview> {
    let built = chat.preview_prompt(&conversation_id, &model_id).await?;
    Ok(PromptPreview {
        prompt: built.text,
        provenance: built.provenance,
    })
}

// ---------------------------------------------------------------- memory (Phase 21)

use crate::contracts::ids::MemoryId;
use crate::contracts::memory::Memory;
use crate::memory::MemoryScope;

/// Every memory a Persona holds, newest first (FR-52).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn memory_list(
    chat: State<'_, Arc<ConversationEngine>>,
    persona_id: PersonaId,
) -> AppResult<Vec<Memory>> {
    chat.memory().list(&MemoryScope::Persona(persona_id)).await
}

/// Delete one memory (FR-53).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn memory_delete(
    chat: State<'_, Arc<ConversationEngine>>,
    id: MemoryId,
) -> AppResult<()> {
    chat.memory().delete(&id).await
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

/// List the machine's audio output devices (Phase 19).
#[must_use]
#[tauri::command]
pub fn voice_output_devices() -> Vec<crate::voice::playback::OutputDevice> {
    crate::voice::playback::list_output_devices()
}

/// Start a voice session on `conversation_id`. `model_id = Some` runs the full
/// listen → think → speak loop with barge-in (Phase 19); `None` transcribes one
/// utterance (Phase 18). `events` receives every [`VoiceState`] change.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn voice_start(
    voice: State<'_, Arc<VoiceInput>>,
    conversation_id: ConversationId,
    model_id: Option<ModelId>,
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
    voice.start_listening(conversation_id, model_id).await
}

/// Release push-to-talk / stop the session — transcribe an utterance in
/// progress, or barge-in on the assistant, then stop.
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

// ---------------------------------------------------------------- diagnostics (Phase 18.5)

use tauri::{AppHandle, Manager as _};

use crate::diag::DiagSnapshot;

/// A live diagnostics snapshot (build + config + registry + resources +
/// lifecycle + conversation metadata + recent logs + host facts). All local,
/// no conversation content (`SECURITY.md`, ADR-0015).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn diag_snapshot(
    config: State<'_, Arc<ConfigManager>>,
    registry: State<'_, Arc<ModelRegistry>>,
    resources: State<'_, Arc<ResourceManager>>,
    lifecycle: State<'_, Arc<LifecycleManager>>,
    chat: State<'_, Arc<ConversationEngine>>,
) -> AppResult<DiagSnapshot> {
    crate::diag::collect(&config, &registry, &resources, &lifecycle, &chat).await
}

/// Write a diagnostics snapshot to `<app_data>/diagnostics/diag-<ts>.json` and
/// return the file path (for the owner to share with the agent).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn diag_export(
    app: AppHandle,
    config: State<'_, Arc<ConfigManager>>,
    registry: State<'_, Arc<ModelRegistry>>,
    resources: State<'_, Arc<ResourceManager>>,
    lifecycle: State<'_, Arc<LifecycleManager>>,
    chat: State<'_, Arc<ConversationEngine>>,
) -> AppResult<String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::internal("resolve app data dir", e))?;
    crate::diag::export(&app_data, &config, &registry, &resources, &lifecycle, &chat).await
}

// ---------------------------------------------------------------- image generation (Phase 22)

use crate::blob::BlobStore;
use crate::contracts::ids::AssetId;
use crate::contracts::image::{
    GeneratedImageRow, ImageEvent, ImageLora, ImagePreset, ImageRequest,
};
use crate::image::orchestrator::ImageOrchestrator;

/// Generate one or more images with Krea 2 Turbo. The reply streams back over
/// `events` — progress frames then one terminal (`Done` / `Error` / `Cancelled`).
/// Returns the task id (for [`image_cancel`]).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn image_generate(
    image: State<'_, Arc<ImageOrchestrator>>,
    req: ImageRequest,
    events: Channel<ImageEvent>,
) -> AppResult<TaskId> {
    let orch = Arc::clone(&image);
    orch.generate(req, move |ev| {
        let _ = events.send(ev);
    })
    .await
}

/// Cancel the running image generation.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn image_cancel(
    image: State<'_, Arc<ImageOrchestrator>>,
    task_id: TaskId,
) -> AppResult<()> {
    image.cancel(&task_id).await
}

/// The realism LoRAs available to Krea 2.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn image_loras(image: State<'_, Arc<ImageOrchestrator>>) -> AppResult<Vec<ImageLora>> {
    image.list_loras().await
}

/// The saved image-generation presets.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn image_presets(
    image: State<'_, Arc<ImageOrchestrator>>,
) -> AppResult<Vec<ImagePreset>> {
    image.list_presets().await
}

/// Recent generations, newest first.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn image_history(
    image: State<'_, Arc<ImageOrchestrator>>,
    limit: u32,
) -> AppResult<Vec<GeneratedImageRow>> {
    image.history(limit.clamp(1, 200)).await
}

/// The PNG bytes of one stored image (rendered via a blob URL by the frontend —
/// a filesystem path never crosses the wire).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn image_bytes(blob: State<'_, Arc<BlobStore>>, asset: AssetId) -> AppResult<Vec<u8>> {
    blob.read(&asset)
}

/// The browsable folder every generated PNG is written to, as a display string.
#[must_use]
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn image_output_dir(image: State<'_, Arc<ImageOrchestrator>>) -> String {
    image.output_dir().display().to_string()
}

/// Open the image output folder in the OS file browser (creating it first).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn image_open_output_dir(image: State<'_, Arc<ImageOrchestrator>>) -> AppResult<()> {
    let dir = image.output_dir().to_path_buf();
    std::fs::create_dir_all(&dir).map_err(|e| AppError::internal("create image output dir", e))?;
    #[cfg(windows)]
    let spawned = std::process::Command::new("explorer").arg(&dir).spawn();
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("open").arg(&dir).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let spawned = std::process::Command::new("xdg-open").arg(&dir).spawn();
    // `explorer` exits non-zero even on success — spawning is enough.
    spawned
        .map(|_| ())
        .map_err(|e| AppError::internal("open file browser", e))
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
        PromptPreview::export_all().unwrap();
    }
}
