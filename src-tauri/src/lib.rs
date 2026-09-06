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
pub mod blob;
pub mod config;
pub mod context;
pub mod contracts;
pub mod conversation;
pub mod db;
pub mod diag;
pub mod image;
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
            ipc::commands::acquire_image_model,
            ipc::commands::resources_snapshot,
            ipc::commands::lifecycle_status,
            ipc::commands::model_register_local,
            ipc::commands::models_rescan,
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
            ipc::commands::image_generate,
            ipc::commands::image_cancel,
            ipc::commands::image_loras,
            ipc::commands::image_presets,
            ipc::commands::image_history,
            ipc::commands::image_bytes,
            ipc::commands::image_output_dir,
            ipc::commands::image_open_output_dir,
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
    // `LOCALAI_DATA_DIR` (set by scripts/deploy-local.mjs) pins the config + data
    // root explicitly — `app_config_dir()` has proven unreliable for a
    // standalone release binary depending on the launching shell. Falls back to
    // the Tauri-resolved dirs when unset (e.g. `tauri dev`, a packaged install).
    let override_dir = std::env::var_os("LOCALAI_DATA_DIR").map(std::path::PathBuf::from);
    let config_dir = match &override_dir {
        Some(d) => d.clone(),
        None => app
            .path()
            .app_config_dir()
            .map_err(|err| format!("resolve app config dir: {err}"))?,
    };
    let data_root = match &override_dir {
        Some(d) => d.clone(),
        None => app
            .path()
            .app_data_dir()
            .map_err(|err| format!("resolve app data dir: {err}"))?,
    };

    tracing::info!(
        config_dir = %config_dir.display(),
        data_root = %data_root.display(),
        overridden = override_dir.is_some(),
        "resolved app dirs"
    );
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
    // Auto-register any GGUFs the user dropped into `<models_dir>/llm/`.
    let scanned = tauri::async_runtime::block_on(acquisition.scan_llm_models()).unwrap_or(0);
    if scanned > 0 {
        tracing::info!(scanned, "registered new GGUF models from models/llm");
    }
    // Register the Krea 2 image model if its assets are already staged (never
    // quantizes here — that is the explicit `acquire_image_model` IPC).
    match tauri::async_runtime::block_on(acquisition.register_image_if_ready()) {
        Ok(id) => tracing::info!(%id, "Krea 2 image model registered"),
        Err(err) => tracing::info!(%err, "no image model registered yet"),
    }
    app.manage(acquisition);
    app.manage(Arc::clone(&registry));
    app.manage(Arc::clone(&database));

    let resources = start_resource_manager(effective.resources.vram_safety_margin_mb);
    app.manage(Arc::clone(&resources));
    let lifecycle = start_lifecycle_manager(
        Arc::clone(&registry),
        Arc::clone(&resources),
        &effective.runtimes.dir,
    );
    app.manage(Arc::clone(&lifecycle));

    // The one conversation engine (Phase 16 flow, Phase 17 formalized). Holds
    // the persona repo + the one context builder (Phase 20).
    let engine = Arc::new(conversation::ConversationEngine::new(
        Arc::clone(&database),
        Arc::clone(&registry),
        Arc::clone(&lifecycle),
    ));
    app.manage(Arc::clone(&engine));

    // Voice (Phase 18 in + Phase 19 out): STT + TTS worker supervisors +
    // capture / VAD / playback / barge-in service.
    app.manage(start_voice(&engine, &effective, &data_root));

    // Image generation (Phase 22): the blob store + LoRA/preset registry + the
    // manual evict/restore orchestrator. The Krea 2 backend registers only when
    // its sidecar script is present (Phase 22.B) — until then image_generate
    // returns a clean "no image model registered".
    let (orchestrator, blob) = start_image(
        &database, &registry, &resources, &lifecycle, &engine, &effective, &data_root,
    );
    app.manage(orchestrator);
    app.manage(blob);

    Ok(())
}

/// Wire the image subsystem (Phase 22, ADR-0006 / ADR-0013): the blob store,
/// the LoRA + preset registry (seeded from the confined loras dir), the Krea 2
/// backend (only if its sidecar script exists — Phase 22.B), and the manual
/// evict/restore [`image::orchestrator::ImageOrchestrator`].
fn start_image(
    database: &Arc<db::Db>,
    registry: &Arc<models::ModelRegistry>,
    resources: &Arc<resources::ResourceManager>,
    lifecycle: &Arc<lifecycle::LifecycleManager>,
    engine: &Arc<conversation::ConversationEngine>,
    effective: &config::AppConfig,
    data_root: &Path,
) -> (
    Arc<image::orchestrator::ImageOrchestrator>,
    Arc<blob::BlobStore>,
) {
    let blob = Arc::new(blob::BlobStore::new(data_root.join("blobs")));

    let loras_dir = effective
        .image
        .loras_dir
        .clone()
        .unwrap_or_else(|| effective.models.dir.join("image").join("loras"));
    let quant_cache = effective.image.quant_cache_dir.clone().unwrap_or_else(|| {
        effective
            .models
            .dir
            .join("image")
            .join("quant_cache")
            .join("krea2")
    });

    let repo = image::repo::ImageRepo::new(Arc::clone(database));
    if let Err(err) = tauri::async_runtime::block_on(repo.seed(&loras_dir)) {
        tracing::warn!(%err, "image LoRA / preset registry seed failed");
    }

    // Dev: `<repo>/image_gen/server.py`. Shipped: a sibling of the executable.
    let image_dir = effective
        .workers
        .dir
        .parent()
        .map_or_else(|| data_root.join("image_gen"), |p| p.join("image_gen"));
    let script = image_dir.join("server.py");
    // The image sidecar runs from its own venv (ADR-0018 two-venv amendment):
    // dev `.venv/Scripts/python.exe` -> `.venv-image/Scripts/python.exe`. Falls
    // back to the workers interpreter when there is no `.venv` component to swap
    // (packaged build — a sibling layout ADR-0014 owns).
    let image_python = image::image_venv_python(&effective.workers.python);
    if script.is_file() {
        lifecycle.register_backend(
            image::BACKEND_KEY,
            Arc::new(image::Krea2Backend::new(
                image_python,
                script.clone(),
                Some(quant_cache),
                Some(loras_dir),
                data_root.join("image-exchange"),
            )),
        );
        tracing::info!(script = %script.display(), "Krea 2 image backend registered");
    } else {
        tracing::info!(
            expected = %script.display(),
            "no image sidecar script — image generation unavailable until Phase 22.B"
        );
    }

    let orchestrator = Arc::new(image::orchestrator::ImageOrchestrator::new(
        Arc::clone(lifecycle),
        Arc::clone(resources),
        Arc::clone(registry),
        Arc::clone(engine),
        Arc::clone(&blob),
        repo,
        // Browsable generated PNGs live in the project's model tree
        // (`<models.dir>/image/outputs/`), next to the LoRAs + quant cache —
        // not the app-data folder. The blob store stays under `data_root`.
        effective.models.dir.join("image").join("outputs"),
    ));
    (orchestrator, blob)
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
