//! LocalAI — Rust application core.
//!
//! The authoritative nucleus (`CLAUDE.md` Article I): owns application state,
//! persistence, model lifecycle, resource management, scheduling, process
//! supervision, and IPC. The frontend is presentation only and talks to this
//! crate exclusively through typed Tauri IPC (`ipc` module).
//!
//! Modules land phase by phase (`config` P8, `db` P9, `models` P11, …), each
//! registering itself in `src-tauri/README.md` and `ARCHITECTURE.md`.

pub mod config;
pub mod contracts;
pub mod db;
pub mod ipc;
pub mod logging;

use std::time::Instant;

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
            tracing::info!(
                models_dir = %effective.models.dir.display(),
                models_budget_gb = effective.models.budget_gb,
                recovered = manager.recovered(),
                load_ms = config_started.elapsed().as_secs_f64() * 1000.0,
                "configuration loaded"
            );
            if manager.recovered() {
                let _ = app.emit("config://recovered", ());
            }
            app.manage(manager);

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
            app.manage(database);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::commands::app_ready,
            ipc::commands::app_ping,
            ipc::commands::frontend_log,
            ipc::commands::config_get,
            ipc::commands::config_set,
            ipc::commands::config_keys,
        ])
        .build(tauri::generate_context!())
        .expect("error while building LocalAI");

    app.run(|handle, event| {
        if let RunEvent::ExitRequested { .. } = event {
            if let Some(database) = handle.try_state::<db::Db>() {
                tauri::async_runtime::block_on(database.checkpoint_and_optimize());
            }
        }
    });
}
