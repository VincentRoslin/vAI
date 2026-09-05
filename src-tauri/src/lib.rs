//! LocalAI — Rust application core.
//!
//! The authoritative nucleus (`CLAUDE.md` Article I): owns application state,
//! persistence, model lifecycle, resource management, scheduling, process
//! supervision, and IPC. The frontend is presentation only and talks to this
//! crate exclusively through typed Tauri IPC (`ipc` module).
//!
//! Bootstrap scope (Phase 6): the Tauri builder, structured logging, and one
//! typed round-trip command. Subsystem modules (`config`, `db`, `models`,
//! `resources`, `scheduler`, `conversation`, …) are added by their own phases,
//! each registering itself in `ARCHITECTURE.md`.

pub mod contracts;
pub mod ipc;
pub mod logging;

/// Build and run the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "LocalAI core starting");

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ipc::commands::app_ready,
            ipc::commands::app_ping,
            ipc::commands::frontend_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LocalAI");
}
