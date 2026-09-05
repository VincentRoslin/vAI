//! Structured logging for the core (`docs/decisions/` — observability is
//! formalized in Phase 10; this is the Phase 6 seed).
//!
//! - JSON lines, levels, timestamps, a `target` field.
//! - Level filter from the `LOCALAI_LOG` env var (default `info`).
//! - No network sink, ever. No telemetry.
//! - Frontend log lines are forwarded via the `frontend_log` command and land
//!   in this same stream (target `frontend`).

use std::sync::Once;

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

static INIT: Once = Once::new();

/// Initialize the global subscriber. Idempotent — safe to call from tests.
pub fn init() {
    INIT.call_once(|| {
        let filter =
            EnvFilter::try_from_env("LOCALAI_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

        let layer = fmt::layer()
            .json()
            .with_timer(fmt::time::UtcTime::rfc_3339())
            .with_target(true)
            .with_current_span(true);

        tracing_subscriber::registry()
            .with(filter)
            .with(layer)
            .init();
    });
}
