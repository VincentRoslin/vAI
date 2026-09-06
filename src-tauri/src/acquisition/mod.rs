//! Model acquisition (Phase 12, ADR-0008): an in-app HuggingFace picker +
//! one-shot **resumable** download for LLM GGUF, and the same download/verify
//! path for the pinned faster-whisper + Chatterbox models. Rust owns the network
//! calls, the transfer, verification, and the filesystem writes (confined to the
//! model dir). Fully skippable; everything acquired works offline afterwards
//! (`CLAUDE.md` Art. II).

pub mod budget;
pub mod download;
pub mod gguf;
pub mod hf;
mod krea2;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::ipc::Channel;

use crate::acquisition::download::{DownloadEngine, DownloadSpec, RegisterPlan};
use crate::config::ConfigManager;
use crate::contracts::acquisition::{DownloadInfo, DownloadProgress, HfGgufFile, HfModelSummary};
use crate::contracts::ids::{DownloadId, ModelId};
use crate::contracts::model::{Device, ModelBackend, ModelKind};
use crate::db::Db;
use crate::ipc::{AppError, AppResult};
use crate::models::{validate_model_path, ModelDraft, ModelRegistry};

/// The `llama.cpp` backend id used for downloaded GGUF models.
pub const LLAMA_BACKEND: &str = "llama.cpp";

/// One of the two fixed (no-picker) model bundles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedModel {
    /// faster-whisper (STT).
    Stt,
    /// Chatterbox Turbo (TTS).
    Tts,
}

/// Orchestrates the HF client, the download engine, and the registry.
#[derive(Debug)]
pub struct AcquisitionService {
    hf: hf::HfClient,
    engine: Arc<DownloadEngine>,
    config: Arc<ConfigManager>,
}

impl AcquisitionService {
    /// Build the service.
    #[must_use]
    pub fn new(db: Arc<Db>, registry: Arc<ModelRegistry>, config: Arc<ConfigManager>) -> Self {
        Self {
            hf: hf::HfClient::default(),
            engine: Arc::new(DownloadEngine::new(db, registry)),
            config,
        }
    }

    /// Point the HF client at a different base URL (tests).
    #[cfg(test)]
    #[must_use]
    pub fn with_hf_base(mut self, base: &str) -> Self {
        self.hf = hf::HfClient::new(base);
        self
    }

    #[cfg(test)]
    #[must_use]
    pub fn engine(&self) -> &Arc<DownloadEngine> {
        &self.engine
    }

    /// Search HuggingFace for GGUF models.
    ///
    /// # Errors
    /// [`AppError::BackendUnavailable`] when offline.
    pub async fn search(&self, query: &str, limit: u32) -> AppResult<Vec<HfModelSummary>> {
        self.hf.search_models(query, limit).await
    }

    /// List a repo's `.gguf` files with header metadata.
    ///
    /// # Errors
    /// [`AppError::BackendUnavailable`] when offline.
    pub async fn list_files(&self, repo: &str) -> AppResult<Vec<HfGgufFile>> {
        self.hf.list_gguf_files(repo).await
    }

    /// Start downloading one GGUF file. Budget-checked before any byte moves;
    /// registered as an LLM on verified completion.
    ///
    /// # Errors
    /// [`AppError::ResourceExhausted`] (budget), [`AppError::Validation`] (path),
    /// or a persistence error.
    pub async fn download_gguf(
        &self,
        repo: &str,
        filename: &str,
        expected_size: Option<u64>,
        sha256: Option<String>,
        channel: Option<Channel<DownloadProgress>>,
    ) -> AppResult<DownloadId> {
        let models = self.config.effective().models;
        let dest = self.resolve_dest(&models.dir, repo, filename)?;

        if let Some(size) = expected_size {
            budget::check_budget(size, &models.dir, models.budget_gb, models.min_free_gb)?;
        }

        let spec = DownloadSpec {
            url: download::hf_resolve_url(repo, "main", filename),
            repo: repo.to_owned(),
            revision: "main".to_owned(),
            filename: filename.to_owned(),
            kind: ModelKind::Llm,
            dest_path: dest,
            models_dir: models.dir.clone(),
            expected_size,
            sha256_expected: sha256,
            register: RegisterPlan::GgufLlm {
                backend: LLAMA_BACKEND.to_owned(),
            },
        };
        self.engine.start(spec, channel).await
    }

    /// Acquire a fixed model bundle (all files) via the same download path.
    ///
    /// # Errors
    /// As [`AcquisitionService::download_gguf`].
    pub async fn acquire_fixed(&self, which: FixedModel) -> AppResult<Vec<DownloadId>> {
        let models = self.config.effective().models;
        let spec = fixed_spec(which);
        let base_dir = models.dir.join(spec.dir);

        let mut ids = Vec::new();
        for (i, file) in spec.files.iter().enumerate() {
            let dest = base_dir.join(file);
            // The last file triggers registration once all files are in place.
            let register = if i + 1 == spec.files.len() {
                RegisterPlan::Fixed(Box::new(spec.draft(base_dir.join(spec.primary_file))))
            } else {
                RegisterPlan::None
            };
            let dl = DownloadSpec {
                url: download::hf_resolve_url(spec.repo, spec.revision, file),
                repo: spec.repo.to_owned(),
                revision: spec.revision.to_owned(),
                filename: (*file).to_owned(),
                kind: spec.kind,
                dest_path: dest,
                models_dir: models.dir.clone(),
                expected_size: None,
                sha256_expected: None,
                register,
            };
            ids.push(self.engine.start(dl, None).await?);
        }
        Ok(ids)
    }

    /// Acquire the Krea 2 Turbo image model (Phase 22.B, ADR-0019): verify the
    /// bf16 weights are already in the HF cache (never pulled), build the NF4
    /// quant cache once if it is absent, and register the model. Idempotent.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if the bf16 weights are missing;
    /// [`AppError::BackendUnavailable`] if a needed one-time quantize cannot run;
    /// a persistence error from the registry.
    pub async fn acquire_image(&self) -> AppResult<ModelId> {
        krea2::acquire_image(&self.config, self.engine.registry(), true).await
    }

    /// Register Krea 2 **only if** the NF4 quant cache is already built — never
    /// quantizes. Called on startup so a staged model just works; a machine
    /// without the cache stays unregistered until `acquire_image` is called.
    ///
    /// # Errors
    /// [`AppError::NotFound`] when the weights or the quant cache are absent.
    pub async fn register_image_if_ready(&self) -> AppResult<ModelId> {
        krea2::acquire_image(&self.config, self.engine.registry(), false).await
    }

    /// Every download row.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn downloads(&self) -> AppResult<Vec<DownloadInfo>> {
        self.engine.list().await
    }

    /// Pause a running download.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn pause(&self, id: &DownloadId) -> AppResult<()> {
        self.engine.pause(id).await
    }

    /// Resume a paused download.
    ///
    /// # Errors
    /// As [`DownloadEngine::resume`].
    pub async fn resume(&self, id: &DownloadId) -> AppResult<()> {
        self.engine.resume(id).await
    }

    /// Cancel a download (deletes the `.part` + row).
    ///
    /// # Errors
    /// A persistence error.
    pub async fn cancel(&self, id: &DownloadId) -> AppResult<()> {
        self.engine.cancel(id).await
    }

    /// On startup: interrupted rows → `Paused`.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn reconcile_on_start(&self) -> AppResult<usize> {
        self.engine.reconcile_on_start().await
    }

    /// Register a GGUF the user dropped into `<models_dir>/llm/` (no download).
    /// `filename` is a bare file name — no path separators.
    ///
    /// # Errors
    /// [`AppError::Validation`] for a bad name / non-file / truncated GGUF;
    /// [`AppError::NotFound`] if the file is missing; a persistence error.
    pub async fn register_local_gguf(&self, filename: &str) -> AppResult<ModelId> {
        if filename.is_empty()
            || std::path::Path::new(filename).components().count() != 1
            || filename.contains(['/', '\\'])
        {
            return Err(AppError::Validation(format!(
                "expected a bare file name, got {filename:?}"
            )));
        }
        let models_dir = self.config.effective().models.dir;
        // Canonical location is `<models_dir>/llm/`; fall back to the models dir
        // root so files placed there before the `llm/` convention still work.
        let candidate = {
            let in_llm = models_dir.join("llm").join(filename);
            if in_llm.is_file() {
                in_llm
            } else {
                models_dir.join(filename)
            }
        };
        self.register_gguf_path(&candidate, &models_dir).await
    }

    /// Scan `<models_dir>/llm/` for `*.gguf` files that aren't in the registry
    /// yet and register each as an LLM. Returns how many were added. Called on
    /// startup and by `models_rescan`.
    ///
    /// # Errors
    /// A persistence error reading the registry.
    pub async fn scan_llm_models(&self) -> AppResult<usize> {
        let models_dir = self.config.effective().models.dir;
        let llm_dir = models_dir.join("llm");
        if !llm_dir.is_dir() {
            return Ok(0);
        }

        let known: std::collections::HashSet<PathBuf> = self
            .engine
            .registry()
            .list()
            .await?
            .into_iter()
            .filter_map(|m| std::path::Path::new(&m.path).canonicalize().ok())
            .collect();

        let mut added = 0usize;
        let mut entries = match tokio::fs::read_dir(&llm_dir).await {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(target: "acquisition", %err, dir = %llm_dir.display(), "cannot read llm dir");
                return Ok(0);
            }
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
            {
                continue;
            }
            if path.canonicalize().is_ok_and(|c| known.contains(&c)) {
                continue;
            }
            match self.register_gguf_path(&path, &models_dir).await {
                Ok(id) => {
                    added += 1;
                    tracing::info!(target: "acquisition", %id, file = %path.display(), "registered a local GGUF");
                }
                Err(err) => {
                    tracing::warn!(target: "acquisition", %err, file = %path.display(), "skipped a GGUF during scan");
                }
            }
        }
        Ok(added)
    }

    /// Parse a GGUF at `path` and register it as an LLM on the llama.cpp backend.
    async fn register_gguf_path(
        &self,
        path: &std::path::Path,
        models_dir: &std::path::Path,
    ) -> AppResult<ModelId> {
        let path = validate_model_path(path, models_dir)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("model")
            .to_owned();

        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| AppError::internal("read local gguf", e))?;
        let header = match gguf::parse_header(&bytes)? {
            gguf::GgufParse::Header(h) => h,
            gguf::GgufParse::Incomplete => {
                return Err(AppError::Validation(format!("{name} is a truncated GGUF")))
            }
        };
        let size_mb = u32::try_from(bytes.len() / (1024 * 1024)).unwrap_or(u32::MAX);
        let draft = ModelDraft {
            display_name: name,
            kind: ModelKind::Llm,
            backend: ModelBackend(LLAMA_BACKEND.to_owned()),
            quant: header.quant.map(crate::contracts::model::Quant),
            path,
            streaming: true,
            context_tokens: header.context_length,
            estimated_vram_mb: Some(size_mb.saturating_add(1024)),
            devices: vec![Device::Cuda, Device::Cpu],
            config: serde_json::json!({}),
        };
        self.engine.registry().register(draft, models_dir).await
    }

    #[allow(clippy::unused_self)]
    fn resolve_dest(
        &self,
        models_dir: &std::path::Path,
        repo: &str,
        filename: &str,
    ) -> AppResult<PathBuf> {
        if filename.contains("..") || repo.contains("..") {
            return Err(AppError::Validation(
                "repo/filename must not contain '..'".to_owned(),
            ));
        }
        let dest = models_dir.join(sanitize(repo)).join(filename);
        // The parent must resolve inside the model dir; the file itself does not
        // exist yet, so validate the directory it will live in.
        let parent = dest.parent().unwrap_or(models_dir);
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::internal("create model subdir", e))?;
        // Re-use the registry's confinement check on the parent.
        validate_model_path(parent, models_dir)?;
        Ok(dest)
    }
}

fn sanitize(repo: &str) -> String {
    repo.replace(['/', '\\'], "__")
}

// ---------------------------------------------------------------- fixed specs

struct FixedSpec {
    repo: &'static str,
    revision: &'static str,
    /// Short subdirectory under the model dir (`stt` / `tts`).
    dir: &'static str,
    files: &'static [&'static str],
    /// The file the registry entry points at.
    primary_file: &'static str,
    kind: ModelKind,
}

impl FixedSpec {
    fn draft(&self, primary_path: PathBuf) -> ModelDraft {
        let (name, ctx, vram) = match self.kind {
            ModelKind::Stt => ("faster-whisper large-v3", None, 3000),
            ModelKind::Tts => ("Chatterbox Turbo", None, 3000),
            _ => ("fixed model", None, 2000),
        };
        ModelDraft {
            display_name: name.to_owned(),
            kind: self.kind,
            backend: ModelBackend(match self.kind {
                ModelKind::Stt => "faster-whisper".to_owned(),
                ModelKind::Tts => "chatterbox".to_owned(),
                _ => "unknown".to_owned(),
            }),
            quant: None,
            path: primary_path,
            streaming: false,
            context_tokens: ctx,
            estimated_vram_mb: Some(vram),
            devices: vec![Device::Cuda],
            config: serde_json::json!({}),
        }
    }
}

/// Pinned bundles (repos + file lists owner-confirmed 2026-09-06, both MIT).
/// Each lands in a short subdir of the model dir (`stt` / `tts`).
/// faster-whisper size is nominally a Phase 18 call — `large-v3` CT2 is the
/// Phase 12 default. Chatterbox is the **Turbo** distill the owner uses (350M,
/// one-step decoder, English, paralinguistic tags).
fn fixed_spec(which: FixedModel) -> FixedSpec {
    match which {
        FixedModel::Stt => FixedSpec {
            repo: "Systran/faster-whisper-large-v3",
            revision: "main",
            dir: "stt",
            files: &[
                "config.json",
                "preprocessor_config.json",
                "tokenizer.json",
                "vocabulary.json",
                "model.bin",
            ],
            primary_file: "model.bin",
            kind: ModelKind::Stt,
        },
        FixedModel::Tts => FixedSpec {
            repo: "ResembleAI/chatterbox-turbo",
            revision: "main",
            dir: "tts",
            files: &[
                "t3_turbo_v1.yaml",
                "tokenizer_config.json",
                "vocab.json",
                "merges.txt",
                "added_tokens.json",
                "special_tokens_map.json",
                "conds.pt",
                "ve.safetensors",
                "s3gen.safetensors",
                "s3gen_meanflow.safetensors",
                "t3_turbo_v1.safetensors",
            ],
            primary_file: "t3_turbo_v1.safetensors",
            kind: ModelKind::Tts,
        },
    }
}

/// Remove a model: its file, its registry row, and any download row.
///
/// # Errors
/// A persistence error.
pub async fn delete_model(registry: &ModelRegistry, db: &Db, id: &ModelId) -> AppResult<()> {
    let model = registry.get(id).await?;
    let _ = tokio::fs::remove_file(&model.path).await;
    registry.delete(id).await?;
    let path = model.path.clone();
    db.write(move |tx| {
        tx.execute("DELETE FROM model_downloads WHERE dest_path = ?1", [path])?;
        Ok(())
    })
    .await
    .map_err(AppError::from)
}
