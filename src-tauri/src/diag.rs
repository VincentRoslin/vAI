//! Local diagnostics snapshot (Phase 18.5). One JSON blob the owner can hand
//! the agent after a hands-on session it did not watch.
//!
//! **All local** (ADR-0015): assembled from in-process state + read-only host
//! commands, written to a file the owner chooses to share. Never transmitted.
//! **No conversation content** — metadata only (`SECURITY.md`).

use std::sync::Arc;

use serde::Serialize;
use ts_rs::TS;

use crate::config::ConfigManager;
use crate::conversation::ConversationEngine;
use crate::ipc::{AppError, AppResult};
use crate::lifecycle::LifecycleManager;
use crate::models::ModelRegistry;
use crate::resources::ResourceManager;

/// The build that produced this binary.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BuildInfo {
    /// Crate version (`Cargo.toml`).
    pub version: String,
    /// Short git SHA at build time (empty if unavailable).
    pub git_sha: String,
    /// `debug` or `release`.
    pub profile: String,
}

impl BuildInfo {
    #[must_use]
    pub fn current() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            git_sha: env!("LOCALAI_GIT_SHA").to_owned(),
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .to_owned(),
        }
    }
}

/// Conversation metadata — **never** content.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ConversationMeta {
    /// Conversation id.
    pub id: String,
    /// `Persona` / `Character`.
    pub kind: String,
    /// Number of messages.
    pub messages: u32,
    /// Last-updated timestamp (RFC-3339).
    pub updated_at: String,
}

/// Read-only host facts.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct HostInfo {
    /// OS description.
    pub os: String,
    /// `nvidia-smi` one-liner, if the tool is present.
    pub nvidia_smi: Option<String>,
    /// Where the rotating log files live.
    pub log_dir: Option<String>,
}

/// The full snapshot.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DiagSnapshot {
    /// When this snapshot was taken (RFC-3339).
    pub taken_at: String,
    /// Build provenance.
    pub build: BuildInfo,
    /// Effective config, redacted (carries no secret today; the guard stays).
    #[ts(type = "unknown")]
    pub config: serde_json::Value,
    /// Registered models (paths + metadata, no weights).
    #[ts(type = "unknown")]
    pub models: serde_json::Value,
    /// Resource-manager snapshot (GPU / RAM / reservations).
    #[ts(type = "unknown")]
    pub resources: serde_json::Value,
    /// Per-model lifecycle status.
    #[ts(type = "unknown")]
    pub lifecycle: serde_json::Value,
    /// Conversation metadata (no content).
    pub conversations: Vec<ConversationMeta>,
    /// Recent (already-redacted) log lines.
    pub recent_logs: Vec<String>,
    /// Host facts.
    pub host: HostInfo,
}

/// Assemble a snapshot from live state.
///
/// # Errors
/// A persistence error while reading the registry or conversations.
pub async fn collect(
    config: &ConfigManager,
    registry: &ModelRegistry,
    resources: &ResourceManager,
    lifecycle: &LifecycleManager,
    engine: &Arc<ConversationEngine>,
) -> AppResult<DiagSnapshot> {
    let config_value = redact_value(
        &serde_json::to_value(config.effective())
            .map_err(|e| AppError::internal("serialize config for diag", e))?,
    );
    let models = redact_value(
        &serde_json::to_value(registry.list().await?)
            .map_err(|e| AppError::internal("serialize models for diag", e))?,
    );
    let resources_value = serde_json::to_value(resources.snapshot().await)
        .map_err(|e| AppError::internal("serialize resources for diag", e))?;
    let lifecycle_value = serde_json::to_value(lifecycle.statuses().await)
        .map_err(|e| AppError::internal("serialize lifecycle for diag", e))?;

    let mut conversations = Vec::new();
    for convo in engine.list().await? {
        let messages = u32::try_from(engine.messages(&convo.id).await?.len()).unwrap_or(u32::MAX);
        conversations.push(ConversationMeta {
            id: convo.id.to_string(),
            kind: format!("{:?}", convo.kind),
            messages,
            updated_at: convo.updated_at,
        });
    }

    Ok(DiagSnapshot {
        taken_at: now_rfc3339(),
        build: BuildInfo::current(),
        config: config_value,
        models,
        resources: resources_value,
        lifecycle: lifecycle_value,
        conversations,
        recent_logs: crate::logging::recent_lines(),
        host: HostInfo {
            os: os_string(),
            nvidia_smi: nvidia_smi(),
            log_dir: crate::logging::log_dir().map(|p| p.display().to_string()),
        },
    })
}

/// Write a snapshot to `<app_data>/diagnostics/diag-<ts>.json`; returns the path.
///
/// # Errors
/// A collection error, or an I/O failure writing the file.
pub async fn export(
    app_data: &std::path::Path,
    config: &ConfigManager,
    registry: &ModelRegistry,
    resources: &ResourceManager,
    lifecycle: &LifecycleManager,
    engine: &Arc<ConversationEngine>,
) -> AppResult<String> {
    let snap = collect(config, registry, resources, lifecycle, engine).await?;
    let dir = app_data.join("diagnostics");
    std::fs::create_dir_all(&dir).map_err(|e| AppError::internal("create diagnostics dir", e))?;
    let stamp = snap.taken_at.replace([':', '.'], "-");
    let path = dir.join(format!("diag-{stamp}.json"));
    let json = serde_json::to_string_pretty(&snap)
        .map_err(|e| AppError::internal("serialize diag snapshot", e))?;
    std::fs::write(&path, json).map_err(|e| AppError::internal("write diag snapshot", e))?;
    tracing::info!(path = %path.display(), "diagnostics snapshot exported");
    Ok(path.display().to_string())
}

// ---------------------------------------------------------------- helpers

/// Redact secret shapes from a JSON value by round-tripping through the log
/// redactor (defence in depth — no field holds a secret today).
fn redact_value(value: &serde_json::Value) -> serde_json::Value {
    let raw = value.to_string();
    serde_json::from_str(&crate::logging::redact_line(&raw)).unwrap_or_else(|_| value.clone())
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_owned())
}

fn os_string() -> String {
    #[cfg(windows)]
    {
        {
            let mut cmd = std::process::Command::new("cmd");
            cmd.args(["/C", "ver"]);
            crate::job::hide_console_std(&mut cmd);
            cmd.output().ok()
        }
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Windows".to_owned())
    }
    #[cfg(not(windows))]
    {
        format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
    }
}

fn nvidia_smi() -> Option<String> {
    let mut cmd = std::process::Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=name,driver_version,memory.total,memory.used",
        "--format=csv,noheader",
    ]);
    crate::job::hide_console_std(&mut cmd);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_info_is_populated() {
        let b = BuildInfo::current();
        assert!(!b.version.is_empty());
        assert!(b.profile == "debug" || b.profile == "release");
        // git_sha may be empty in a non-git build — just assert it is a string.
        let _ = b.git_sha;
    }

    #[test]
    fn redact_value_scrubs_a_planted_secret() {
        let v = serde_json::json!({ "note": "x", "token": "hf_ABCDEFGHIJKLMNOPQRSTUVWX1234" });
        let out = redact_value(&v);
        assert_eq!(out["token"], "***");
        assert_eq!(out["note"], "x");
    }

    #[test]
    fn now_rfc3339_looks_like_a_timestamp() {
        let s = now_rfc3339();
        assert!(s.contains('T') && s.ends_with('Z'), "got {s}");
    }
}
