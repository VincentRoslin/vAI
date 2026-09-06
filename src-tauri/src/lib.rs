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
pub mod contracts;
pub mod db;
pub mod ipc;
pub mod logging;
pub mod models;
pub mod resources;

use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{Emitter, Manager, RunEvent};

/// Build and run the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "LocalAI core starting");

    let app = tauri::Builder::default()
        .setup(|app| {
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
            tracing::info!(
                models_dir = %effective.models.dir.display(),
                models_budget_gb = effective.models.budget_gb,
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
            let reconciled =
                tauri::async_runtime::block_on(acquisition.reconcile_on_start()).unwrap_or(0);
            if reconciled > 0 {
                tracing::info!(reconciled, "paused interrupted downloads from a prior run");
            }
            app.manage(acquisition);
            app.manage(registry);
            app.manage(database);

            app.manage(start_resource_manager(
                effective.resources.vram_safety_margin_mb,
            ));

            Ok(())
        })
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
        ])
        .build(tauri::generate_context!())
        .expect("error while building LocalAI");

    app.run(|handle, event| {
        if let RunEvent::ExitRequested { .. } = event {
            if let Some(database) = handle.try_state::<Arc<db::Db>>() {
                tauri::async_runtime::block_on(database.checkpoint_and_optimize());
            }
        }
    });
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
