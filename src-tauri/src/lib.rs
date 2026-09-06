//! LocalAI — Rust application core.
//!
//! The authoritative nucleus (`CLAUDE.md` Article I): owns application state,
//! persistence, model lifecycle, resource management, scheduling, process
//! supervision, and IPC. The frontend is presentation only and talks to this
//! crate exclusively through typed Tauri IPC (`ipc` module).
//!
//! Modules land phase by phase (`config` P8, `db` P9, `models` P11, …), each
//! registering itself in `src-tauri/README.md` and `ARCHITECTURE.md`.

pub mod acquisition;
pub mod config;
pub mod context;
pub mod contracts;
pub mod conversation;
pub mod db;
pub mod diag;
pub mod ipc;
pub mod job;
pub mod lifecycle;
pub mod llm;
pub mod logging;
pub mod memory;
pub mod models;
pub mod resources;
pub mod voice;
pub mod worker;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{Emitter, Manager, RunEvent};

/// Build and run the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "LocalAI core starting");

    let app = tauri::Builder::default()
        .setup(|app| setup(app).map_err(Into::into))
        .invoke_handler(tauri::generate_handler![
            ipc::commands::app_ready,
            ipc::commands::app_ping,
            ipc::commands::frontend_log,
            ipc::commands::config_get,
            ipc::commands::config_set,
            ipc::commands::config_keys,
            ipc::commands::models_list,
            ipc::commands::model_delete,
            ipc::commands::hf_search,
            ipc::commands::hf_list_files,
            ipc::commands::download_start,
            ipc::commands::download_pause,
            ipc::commands::download_resume,
            ipc::commands::download_cancel,
            ipc::commands::downloads_list,
            ipc::commands::acquire_fixed,
            ipc::commands::resources_snapshot,
            ipc::commands::lifecycle_status,
            ipc::commands::model_register_local,
            ipc::commands::model_load,
            ipc::commands::model_unload,
            ipc::commands::conversation_create,
            ipc::commands::conversation_set_persona,
            ipc::commands::conversation_list,
            ipc::commands::conversation_messages,
            ipc::commands::chat_send,
            ipc::commands::chat_generate,
            ipc::commands::chat_state,
            ipc::commands::chat_cancel,
            ipc::commands::chat_prompt_preview,
            ipc::commands::persona_list,
            ipc::commands::persona_get,
            ipc::commands::persona_create,
            ipc::commands::persona_update,
            ipc::commands::persona_delete,
            ipc::commands::memory_list,
            ipc::commands::memory_delete,
            ipc::commands::voice_input_devices,
            ipc::commands::voice_output_devices,
            ipc::commands::voice_start,
            ipc::commands::voice_stop,
            ipc::commands::voice_state,
            ipc::commands::diag_snapshot,
            ipc::commands::diag_export,
        ])
        .build(tauri::generate_context!())
        .expect("error while building LocalAI");

    app.run(|handle, event| {
        if let RunEvent::ExitRequested { .. } = event {
            // Cancel any in-flight generation, unload models (the Job Object also
            // kills llama-server), then checkpoint the WAL.
            if let Some(voice) = handle.try_state::<Arc<voice::VoiceInput>>() {
                tauri::async_runtime::block_on(voice.cancel_listening());
            }
            if let Some(chat) = handle.try_state::<Arc<conversation::ConversationEngine>>() {
                tauri::async_runtime::block_on(chat.shutdown());
            }
            if let Some(lifecycle) = handle.try_state::<Arc<lifecycle::LifecycleManager>>() {
                tauri::async_runtime::block_on(lifecycle.unload_all());
            }
            if let Some(database) = handle.try_state::<Arc<db::Db>>() {
                tauri::async_runtime::block_on(database.checkpoint_and_optimize());
            }
        }
    });
}

/// Assemble every subsystem into Tauri managed state. Runs once, on the main
/// thread, before the window shows.
#[allow(clippy::too_many_lines)]
fn setup(app: &mut tauri::App) -> Result<(), String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|err| format!("resolve app config dir: {err}"))?;
    let data_root = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("resolve app data dir: {err}"))?;

    let config_started = Instant::now();
    let manager = config::ConfigManager::load(&config_dir, &data_root)
        .map_err(|err| format!("load configuration: {err}"))?;
    let effective = manager.effective();
    if let Err(err) = logging::set_level(&effective.logging.level) {
        tracing::warn!(%err, "could not apply configured logging.level");
    }
    // Persistent rotating file log (Phase 18.5) — so a session we did not watch
    // is still inspectable. Redacted by the same writer as stdout.
    if let Err(err) = logging::enable_file_sink(&data_root.join("logs")) {
        tracing::warn!(%err, "could not enable the log file sink");
    }
    tracing::info!(
        models_dir = %effective.models.dir.display(),
        runtimes_dir = %effective.runtimes.dir.display(),
        logging_level = %effective.logging.level,
        recovered = manager.recovered(),
        load_ms = config_started.elapsed().as_secs_f64() * 1000.0,
        "configuration loaded"
    );
    if manager.recovered() {
        let _ = app.emit("config://recovered", ());
    }
    let manager = Arc::new(manager);
    app.manage(Arc::clone(&manager));

    let db_path = data_root.join("localai.db");
    let db_started = Instant::now();
    let (database, schema_version) = tauri::async_runtime::block_on(async {
        let database = db::Db::open(&db_path).await?;
        let version = database.migrate().await?;
        Ok::<_, db::DbError>((database, version))
    })
    .map_err(|err| format!("open database: {err}"))?;
    tracing::info!(
        schema_version,
        open_ms = db_started.elapsed().as_secs_f64() * 1000.0,
        "database ready"
    );
    let database = Arc::new(database);
    let registry = Arc::new(models::ModelRegistry::new(Arc::clone(&database)));
    let acquisition = acquisition::AcquisitionService::new(
        Arc::clone(&database),
        Arc::clone(&registry),
        Arc::clone(&manager),
    );
    let reconciled = tauri::async_runtime::block_on(acquisition.reconcile_on_start()).unwrap_or(0);
    if reconciled > 0 {
        tracing::info!(reconciled, "paused interrupted downloads from a prior run");
    }
    app.manage(acquisition);
    app.manage(Arc::clone(&registry));
    app.manage(Arc::clone(&database));

    let resources = start_resource_manager(effective.resources.vram_safety_margin_mb);
    app.manage(Arc::clone(&resources));
    let lifecycle =
        start_lifecycle_manager(Arc::clone(&registry), resources, &effective.runtimes.dir);
    app.manage(Arc::clone(&lifecycle));

    // The one conversation engine (Phase 16 flow, Phase 17 formalized). Holds
    // the persona repo + the one context builder (Phase 20).
    let engine = Arc::new(conversation::ConversationEngine::new(
        database,
        Arc::clone(&registry),
        lifecycle,
    ));
    app.manage(Arc::clone(&engine));

    // Voice (Phase 18 in + Phase 19 out): STT + TTS worker supervisors +
    // capture / VAD / playback / barge-in service.
    app.manage(start_voice(&engine, &effective, &data_root));

    Ok(())
}

/// Wire the STT + TTS worker supervisors + [`voice::VoiceInput`] (Phases 18/19,
/// ADR-0005 / 0013 / 0018). Nothing is spawned until the first `voice_start`.
fn start_voice(
    engine: &Arc<conversation::ConversationEngine>,
    effective: &config::AppConfig,
    data_root: &std::path::Path,
) -> Arc<voice::VoiceInput> {
    let workers = || {
        worker::WorkerLayout::from_config(
            effective.workers.python.clone(),
            effective.workers.dir.clone(),
        )
    };
    let stt = Arc::new(
        worker::WorkerSupervisor::new(workers(), contracts::worker::WorkerKind::Stt).with_env([(
            "LOCALAI_STT_MODEL_DIR".to_owned(),
            effective.models.dir.join("stt").display().to_string(),
        )]),
    );
    let tts_worker = Arc::new(
        worker::WorkerSupervisor::new(workers(), contracts::worker::WorkerKind::Tts).with_env([(
            "LOCALAI_TTS_MODEL_DIR".to_owned(),
            effective.models.dir.join("tts").display().to_string(),
        )]),
    );
    let temp_dir = data_root.join("voice-cache");
    let tts = Arc::new(voice::tts::TtsOutput::new(
        tts_worker,
        temp_dir.clone(),
        effective.voice.output_device.clone(),
    ));
    let cfg = voice::VoiceConfig {
        vad_model: effective.models.dir.join("vad").join("silero_vad.onnx"),
        temp_dir,
        input_device: effective.voice.input_device.clone(),
        output_device: effective.voice.output_device.clone(),
        vad: voice::vad::VadConfig {
            min_silence_ms: effective.voice.end_of_speech_ms,
            ..voice::vad::VadConfig::default()
        },
        pre_roll_ms: 300,
        playback_duck: 0.2,
    };
    voice::VoiceInput::new(Arc::clone(engine), stt, tts, cfg)
}

/// Build the resource manager (Phase 13, ADR-0007): whole-GPU NVML + `sysinfo`
/// behind one probe, our own reservation ledger on top. NVML init failure is
/// non-fatal — the probe reports unavailable and loads that need VRAM
/// accounting are refused with a clear message. Takes the first measurement and
/// spawns the observation loop.
fn start_resource_manager(vram_safety_margin_mb: u32) -> Arc<resources::ResourceManager> {
    let probe = Arc::new(resources::probe::NvmlProbe::new());
    let manager = Arc::new(resources::ResourceManager::new(
        probe,
        vram_safety_margin_mb,
    ));
    let first = tauri::async_runtime::block_on(manager.observe());
    tracing::info!(
        gpu = ?first.gpu,
        ram = ?first.ram,
        vram_safety_margin_mb,
        "resource manager ready"
    );
    let handle = Arc::clone(&manager);
    tauri::async_runtime::spawn(async move { observe_loop(handle).await });
    manager
}

/// Poll the hardware probe and reconcile the ledger: ~1.5 s while any
/// reservation is outstanding, ~10 s when the ledger is empty.
async fn observe_loop(manager: Arc<resources::ResourceManager>) {
    const BUSY: Duration = Duration::from_millis(1500);
    const IDLE: Duration = Duration::from_secs(10);
    loop {
        let snapshot = manager.observe().await;
        let idle = snapshot.reserved_gpu_mb == 0 && snapshot.reserved_ram_mb == 0;
        if !idle {
            manager.reconcile().await;
        }
        tokio::time::sleep(if idle { IDLE } else { BUSY }).await;
    }
}

/// Build the model lifecycle manager (Phase 14) and register the backends it
/// knows about. The llama.cpp adapter (Phase 15) is registered when a
/// `llama-server` binary is present in `runtimes.dir` (config, default
/// `<app_data>/runtimes`); until then LLM loads are unavailable. Spawns the
/// liveness monitor.
fn start_lifecycle_manager(
    registry: Arc<models::ModelRegistry>,
    resources: Arc<resources::ResourceManager>,
    runtimes_dir: &Path,
) -> Arc<lifecycle::LifecycleManager> {
    let manager = Arc::new(lifecycle::LifecycleManager::new(
        registry,
        resources,
        lifecycle::RetryPolicy::default(),
    ));

    let binary = runtimes_dir.join(if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    });
    if binary.is_file() {
        manager.register_backend(
            llm::BACKEND_KEY,
            Arc::new(llm::LlamaBackend::new(binary.clone())),
        );
        tracing::info!(binary = %binary.display(), "llama.cpp backend registered");
    } else {
        tracing::info!(
            expected = %binary.display(),
            "no llama-server binary — LLM loads unavailable until it is installed (plan 15.D)"
        );
    }

    let handle = Arc::clone(&manager);
    tauri::async_runtime::spawn(async move { liveness_loop(handle).await });
    manager
}

/// Health-check every loaded model every ~2 s; a dead backend is moved to
/// `Failed` and its reservation released (Phase 14).
async fn liveness_loop(manager: Arc<lifecycle::LifecycleManager>) {
    const INTERVAL: Duration = Duration::from_secs(2);
    loop {
        let failed = manager.check_liveness().await;
        if failed > 0 {
            tracing::warn!(failed, "liveness monitor moved models to Failed");
        }
        tokio::time::sleep(INTERVAL).await;
    }
}
