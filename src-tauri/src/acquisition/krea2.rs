//! Krea 2 Turbo acquisition (Phase 22.B, ADR-0019).
//!
//! Unlike every other acquisition path this **downloads nothing at runtime**:
//!
//! - the bf16 weights are the user's existing HuggingFace cache — LocalAI never
//!   re-pulls them (~34 GB; `CLAUDE.md` "always ask first"). If they are absent
//!   this returns [`AppError::NotFound`] rather than fetching.
//! - the NF4 quant cache is either already staged into `<models.dir>/image/
//!   quant_cache/krea2/` or built **once** here by `image_gen/quantize.py`
//!   running in `.venv-image` (the sidecar never quantizes — ADR-0006).
//!
//! On success Krea 2 is in the model registry under
//! `backend = "krea2-diffusers"` ([`crate::image::BACKEND_KEY`]); the registry
//! `path` is a marker dir (the weights live in the shared HF cache, outside the
//! model dir), and `config` carries `{ model_id, hf_snapshot, quant_cache_dir }`.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::config::ConfigManager;
use crate::contracts::ids::ModelId;
use crate::contracts::model::{Device, ModelBackend, ModelKind, Quant};
use crate::image::{image_venv_python, BACKEND_KEY, KREA2_MODEL_ID};
use crate::ipc::{AppError, AppResult};
use crate::models::{ModelDraft, ModelRegistry};

/// Loaded VRAM estimate for Krea 2 NF4 with CPU offload. Refined from the
/// measured peak at the 22.C live gate.
const KREA2_ESTIMATED_VRAM_MB: u32 = 11_750;

/// Verify / prepare Krea 2 and register it. Idempotent — a present, registered
/// model with a valid quant cache makes this a no-op.
///
/// # Errors
/// [`AppError::NotFound`] if the bf16 weights are not in the HF cache;
/// [`AppError::BackendUnavailable`] if the one-time quantize is needed but the
/// image venv / script is missing or `quantize.py` fails; a persistence error
/// from the registry.
pub(super) async fn acquire_image(
    config: &ConfigManager,
    registry: &ModelRegistry,
    allow_quantize: bool,
) -> AppResult<ModelId> {
    let eff = config.effective();
    let models_dir = eff.models.dir.clone();
    let image_dir = models_dir.join("image");
    let quant_cache = eff
        .image
        .quant_cache_dir
        .clone()
        .unwrap_or_else(|| image_dir.join("quant_cache").join("krea2"));

    // 1. bf16 weights — must already be in the HF cache. Never pull.
    let snapshot = hf_cache_snapshot(KREA2_MODEL_ID).ok_or_else(|| {
        AppError::NotFound(format!(
            "Krea 2 bf16 weights ({KREA2_MODEL_ID}) are not in the HuggingFace cache. \
             LocalAI does not download them (~34 GB) — place them there first."
        ))
    })?;

    // 2. NF4 quant cache — build once if the directory layout is absent /
    //    incomplete. A cache that is present but fingerprint-stale (a library
    //    version moved) is caught by the sidecar on load — it returns a typed
    //    503 that tells the user to re-run setup — not re-checked here.
    match quant_cache_state(&quant_cache) {
        QuantCacheState::Ok => {
            tracing::info!(target: "acquisition", dir = %quant_cache.display(), "NF4 quant cache present");
        }
        state if !allow_quantize => {
            // Startup path: never quantize silently (needs .venv-image + ~26 GB
            // RAM). The explicit `acquire_image_model` IPC passes allow_quantize.
            return Err(AppError::NotFound(format!(
                "the Krea 2 NF4 quant cache is {state:?} at {} — run image-model setup",
                quant_cache.display()
            )));
        }
        state => {
            tracing::info!(target: "acquisition", ?state, dir = %quant_cache.display(), "building NF4 quant cache (one-time)");
            run_quantize(
                &eff.workers.python,
                &eff.workers.dir,
                &snapshot,
                &quant_cache,
            )
            .await?;
        }
    }

    // 3. Register (idempotent).
    if let Some(id) = existing_krea2(registry).await? {
        tracing::info!(target: "acquisition", %id, "Krea 2 already registered");
        return Ok(id);
    }

    let marker = image_dir.join("krea2");
    tokio::fs::create_dir_all(&marker)
        .await
        .map_err(|e| AppError::internal("create image model marker dir", e))?;
    tokio::fs::write(
        marker.join("SOURCE.txt"),
        format!(
            "Krea 2 Turbo — weights load from the HuggingFace cache ({KREA2_MODEL_ID}).\n\
             hf_snapshot: {}\nquant_cache: {}\n",
            snapshot.display(),
            quant_cache.display(),
        ),
    )
    .await
    .map_err(|e| AppError::internal("write image model marker", e))?;

    let draft = ModelDraft {
        display_name: "Krea 2 Turbo".to_owned(),
        kind: ModelKind::Image,
        backend: ModelBackend(BACKEND_KEY.to_owned()),
        quant: Some(Quant("nf4".to_owned())),
        path: marker,
        streaming: false,
        context_tokens: None,
        estimated_vram_mb: Some(KREA2_ESTIMATED_VRAM_MB),
        devices: vec![Device::Cuda],
        config: json!({
            "model_id": KREA2_MODEL_ID,
            "hf_snapshot": snapshot.display().to_string(),
            "quant_cache_dir": quant_cache.display().to_string(),
        }),
    };
    registry.register(draft, &models_dir).await
}

async fn existing_krea2(registry: &ModelRegistry) -> AppResult<Option<ModelId>> {
    Ok(registry
        .list_by_kind(ModelKind::Image)
        .await?
        .into_iter()
        .find(|m| m.metadata.backend.0 == BACKEND_KEY)
        .map(|m| m.metadata.id))
}

// ---------------------------------------------------------------- HF cache

/// The one snapshot dir under `models--<org>--<name>/snapshots/` that carries a
/// `model_index.json` (a complete diffusers snapshot), or `None`.
fn hf_cache_snapshot(repo_id: &str) -> Option<PathBuf> {
    let folder = format!("models--{}", repo_id.replace('/', "--"));
    let snapshots = hf_hub_dir()?.join(folder).join("snapshots");
    let mut found = None;
    for entry in std::fs::read_dir(&snapshots).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join("model_index.json").exists() {
            found = Some(path);
        }
    }
    found
}

/// `$HF_HUB_CACHE`, else `$HF_HOME/hub`, else `~/.cache/huggingface/hub`.
fn hf_hub_dir() -> Option<PathBuf> {
    let non_empty = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    if let Some(v) = non_empty("HF_HUB_CACHE") {
        return Some(PathBuf::from(v));
    }
    if let Some(v) = non_empty("HF_HOME") {
        return Some(PathBuf::from(v).join("hub"));
    }
    let home = non_empty("USERPROFILE").or_else(|| non_empty("HOME"))?;
    Some(
        PathBuf::from(home)
            .join(".cache")
            .join("huggingface")
            .join("hub"),
    )
}

// ---------------------------------------------------------------- quant cache

#[derive(Debug)]
enum QuantCacheState {
    Ok,
    Absent,
    Incomplete,
}

fn quant_cache_state(dir: &Path) -> QuantCacheState {
    if !dir.exists() {
        return QuantCacheState::Absent;
    }
    if dir.join("transformer").is_dir()
        && dir.join("text_encoder").is_dir()
        && dir.join("fingerprint.json").is_file()
    {
        QuantCacheState::Ok
    } else {
        QuantCacheState::Incomplete
    }
}

/// Run `image_gen/quantize.py` in `.venv-image`. Streams its `PROGRESS <phase>`
/// lines to the log. `workers_python` / `workers_dir` are the dev config values;
/// the image interpreter is derived by swapping the `.venv` component.
async fn run_quantize(
    workers_python: &Path,
    workers_dir: &Path,
    snapshot: &Path,
    out: &Path,
) -> AppResult<()> {
    let python = image_venv_python(workers_python);
    if !python.is_file() {
        return Err(AppError::BackendUnavailable(format!(
            "image venv interpreter not found at {} — run `node scripts/setup-venv.mjs image`",
            python.display()
        )));
    }
    let script = workers_dir
        .parent()
        .map(|p| p.join("image_gen").join("quantize.py"))
        .filter(|p| p.is_file())
        .ok_or_else(|| AppError::NotFound("image_gen/quantize.py".to_owned()))?;

    tokio::fs::create_dir_all(out)
        .await
        .map_err(|e| AppError::internal("create quant cache dir", e))?;

    let mut cmd = tokio::process::Command::new(&python);
    cmd.arg(&script)
        .arg("--model-path")
        .arg(snapshot)
        .arg("--model-id")
        .arg(KREA2_MODEL_ID)
        .arg("--out")
        .arg(out)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    crate::worker::env::apply(&mut cmd);

    let output = cmd
        .output()
        .await
        .map_err(|e| AppError::internal("spawn quantize.py", e))?;

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(phase) = line.strip_prefix("PROGRESS ") {
            tracing::info!(target: "acquisition", phase, "quantize");
        }
    }
    if !output.status.success() {
        return Err(AppError::BackendUnavailable(format!(
            "quantize.py failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `std::env::set_var` is process-global; serialise the env-touching tests.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn quant_cache_state_reads_the_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("krea2");
        assert!(matches!(quant_cache_state(&dir), QuantCacheState::Absent));

        std::fs::create_dir_all(dir.join("transformer")).unwrap();
        std::fs::create_dir_all(dir.join("text_encoder")).unwrap();
        assert!(matches!(
            quant_cache_state(&dir),
            QuantCacheState::Incomplete
        ));

        std::fs::write(dir.join("fingerprint.json"), "{}").unwrap();
        assert!(matches!(quant_cache_state(&dir), QuantCacheState::Ok));
    }

    #[test]
    fn hf_cache_snapshot_needs_model_index() {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("HF_HUB_CACHE", tmp.path());
        let snap = tmp
            .path()
            .join("models--unsloth--Krea-2-Turbo")
            .join("snapshots")
            .join("abc123");
        std::fs::create_dir_all(&snap).unwrap();
        assert!(hf_cache_snapshot("unsloth/Krea-2-Turbo").is_none());

        std::fs::write(snap.join("model_index.json"), "{}").unwrap();
        assert_eq!(
            hf_cache_snapshot("unsloth/Krea-2-Turbo"),
            Some(snap.clone())
        );
        std::env::remove_var("HF_HUB_CACHE");
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)] // test-only env guard; contention is fine
    async fn acquire_errors_when_weights_absent() {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("HF_HUB_CACHE", tmp.path().join("empty-hub"));
        let db = std::sync::Arc::new(crate::db::Db::open(&tmp.path().join("d.db")).await.unwrap());
        let registry = ModelRegistry::new(db);
        let cfg = ConfigManager::load(tmp.path(), tmp.path()).unwrap();
        let err = acquire_image(&cfg, &registry, true).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err:?}");
        std::env::remove_var("HF_HUB_CACHE");
    }
}
