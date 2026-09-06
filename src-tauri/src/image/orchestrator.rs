//! The manual evict → load → generate → restore sequence for one image
//! generation (Phase 22, ADR-0010 shape).
//!
//! **This is written to be deleted.** Phase 23 (hot-swap) replaces the manual
//! eviction with the scheduler; Phase 24 formalizes the queue. Until then this
//! is the *only* place that unloads the LLM to make room for the image model,
//! and the restore runs in a guard so it happens on success, error, and
//! cancellation alike. One image generation runs at a time.
//!
//! The orchestrator never calls the resource manager's mutating methods — the
//! lifecycle manager owns the VRAM reservation (acquired before the backend
//! load, released on every exit path, Phase 14). Here we only *read* the
//! resource snapshot for the WDDM settle-wait between unload and load.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use time::format_description::FormatItem;
use time::macros::format_description;
use time::OffsetDateTime;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use super::repo::{ImageRepo, NewGeneratedImage};
use crate::blob::BlobStore;
use crate::contracts::ids::{ImageLoraId, ModelId, TaskId};
use crate::contracts::image::{
    GeneratedImageRow, ImageEvent, ImagePhase, ImageProgress, ImageRequest,
};
use crate::contracts::model::{ModelKind, ModelState, RegistryAvailability};
use crate::conversation::ConversationEngine;
use crate::ipc::{AppError, AppResult};
use crate::lifecycle::backend::ImageGenerateArgs;
use crate::lifecycle::LifecycleManager;
use crate::models::ModelRegistry;
use crate::resources::ResourceManager;

/// Krea 2's model defaults (ADR-0006): 8 steps, `guidance_scale` 0.0.
const DEFAULT_STEPS: u32 = 8;
const DEFAULT_GUIDANCE: f32 = 0.0;
/// How long to wait for freed VRAM to settle before loading the image model.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(20);
const SETTLE_POLL: Duration = Duration::from_millis(400);

/// `20260906-221530` — local time, for the browsable PNG copies.
const STAMP: &[FormatItem<'_>] = format_description!("[year][month][day]-[hour][minute][second]");

struct Running {
    task_id: TaskId,
    cancel: CancellationToken,
}

/// Drives one image generation at a time through the manual eviction sequence.
pub struct ImageOrchestrator {
    lifecycle: Arc<LifecycleManager>,
    resources: Arc<ResourceManager>,
    registry: Arc<ModelRegistry>,
    engine: Arc<ConversationEngine>,
    blob: Arc<BlobStore>,
    repo: ImageRepo,
    /// A browsable folder every generated PNG is also written to (in addition to
    /// the content-addressed blob store). `<app_data>/images/`.
    output_dir: PathBuf,
    running: Mutex<Option<Running>>,
    settle_timeout: Duration,
}

impl ImageOrchestrator {
    #[must_use]
    pub fn new(
        lifecycle: Arc<LifecycleManager>,
        resources: Arc<ResourceManager>,
        registry: Arc<ModelRegistry>,
        engine: Arc<ConversationEngine>,
        blob: Arc<BlobStore>,
        repo: ImageRepo,
        output_dir: PathBuf,
    ) -> Self {
        Self {
            lifecycle,
            resources,
            registry,
            engine,
            blob,
            repo,
            output_dir,
            running: Mutex::new(None),
            settle_timeout: SETTLE_TIMEOUT,
        }
    }

    /// The browsable folder generated PNGs are written to.
    #[must_use]
    pub fn output_dir(&self) -> &std::path::Path {
        &self.output_dir
    }

    /// Start a generation. Streams [`ImageEvent`]s into `sink` — progress
    /// frames, then exactly one terminal (`Done` / `Error` / `Cancelled`).
    /// Returns the [`TaskId`] once the job has started.
    ///
    /// # Errors
    /// [`AppError::Validation`] if `req` is invalid; [`AppError::Conflict`] if
    /// an image generation is already running or an interactive chat
    /// generation is in flight.
    pub async fn generate<F>(self: &Arc<Self>, req: ImageRequest, sink: F) -> AppResult<TaskId>
    where
        F: Fn(ImageEvent) + Send + Sync + 'static,
    {
        req.validate()?;

        let mut guard = self.running.lock().await;
        if guard.is_some() {
            return Err(AppError::Conflict(
                "an image generation is already running".to_owned(),
            ));
        }
        if self.engine.is_generating().await {
            return Err(AppError::Conflict(
                "finish the current chat reply before generating an image".to_owned(),
            ));
        }
        let task_id = TaskId::from_trusted(uuid::Uuid::new_v4().hyphenated().to_string());
        let cancel = CancellationToken::new();
        *guard = Some(Running {
            task_id: task_id.clone(),
            cancel: cancel.clone(),
        });
        drop(guard);

        let this = Arc::clone(self);
        let tid = task_id.clone();
        tokio::spawn(async move {
            let terminal = match this.run(&req, &cancel, &sink).await {
                Ok(rows) => ImageEvent::Done { images: rows },
                Err(AppError::Cancelled) => ImageEvent::Cancelled,
                Err(error) => {
                    error.log("image generation");
                    ImageEvent::Error { error }
                }
            };
            sink(terminal);
            *this.running.lock().await = None;
            tracing::info!(task_id = %tid, "image generation finished");
        });

        Ok(task_id)
    }

    /// Cancel the running generation.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if no generation with that id is running.
    pub async fn cancel(&self, task_id: &TaskId) -> AppResult<()> {
        match self.running.lock().await.as_ref() {
            Some(r) if &r.task_id == task_id => {
                r.cancel.cancel();
                Ok(())
            }
            _ => Err(AppError::NotFound(format!("image generation {task_id}"))),
        }
    }

    /// Recent generations, newest first.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn history(&self, limit: u32) -> AppResult<Vec<GeneratedImageRow>> {
        self.repo.list_generated(limit).await
    }

    /// The registered realism LoRAs (for the picker).
    ///
    /// # Errors
    /// A persistence error.
    pub async fn list_loras(&self) -> AppResult<Vec<crate::contracts::image::ImageLora>> {
        self.repo.list_loras().await
    }

    /// The saved parameter presets.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn list_presets(&self) -> AppResult<Vec<crate::contracts::image::ImagePreset>> {
        self.repo.list_presets().await
    }

    async fn run<F>(
        &self,
        req: &ImageRequest,
        cancel: &CancellationToken,
        sink: &F,
    ) -> AppResult<Vec<GeneratedImageRow>>
    where
        F: Fn(ImageEvent) + Send + Sync,
    {
        let image_model = self.resolve_image_model().await?;
        let lora = self.resolve_lora(req).await?;

        emit(sink, ImagePhase::Evicting, 0, 0, req.batch_count);
        let evicted = self.evict_others(&image_model).await;
        if !evicted.is_empty() {
            self.settle_wait(cancel).await;
        }

        // Everything from here restores the evicted models on the way out.
        let result = self
            .load_generate_store(req, &image_model, lora, cancel, sink)
            .await;

        emit(sink, ImagePhase::Restoring, 0, 0, req.batch_count);
        let _ = self.lifecycle.unload(&image_model).await;
        for id in evicted {
            if let Err(e) = self.lifecycle.load(&id).await {
                tracing::warn!(model = %id, error = %e, "failed to restore an evicted model");
            }
        }
        result
    }

    async fn load_generate_store<F>(
        &self,
        req: &ImageRequest,
        image_model: &ModelId,
        lora: Option<(String, f32)>,
        cancel: &CancellationToken,
        sink: &F,
    ) -> AppResult<Vec<GeneratedImageRow>>
    where
        F: Fn(ImageEvent) + Send + Sync,
    {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        emit(sink, ImagePhase::Loading, 0, 0, req.batch_count);
        self.lifecycle.load(image_model).await?;

        let instance = self.lifecycle.instance(image_model).await.ok_or_else(|| {
            AppError::BackendUnavailable("image model vanished after load".to_owned())
        })?;
        let img = instance.as_image().ok_or_else(|| {
            AppError::BackendUnavailable("model is not an image model".to_owned())
        })?;

        let args = ImageGenerateArgs {
            prompt: req.prompt.clone(),
            negative: req.negative.clone(),
            width: req.width,
            height: req.height,
            steps: req.steps.unwrap_or(DEFAULT_STEPS),
            guidance: req.guidance.unwrap_or(DEFAULT_GUIDANCE),
            seed: req.seed.unwrap_or_else(fresh_seed),
            batch_count: req.batch_count,
            lora,
        };
        let steps = args.steps;
        let batch = req.batch_count;
        emit(sink, ImagePhase::Generating, 0, steps, batch);

        // Run the generation and drain step-progress frames concurrently — the
        // `sink` closure is not `'static`, so this cannot be a spawned task.
        let (prog_tx, mut prog_rx) = mpsc::unbounded_channel();
        let gen_fut = img.generate(args, prog_tx, cancel.clone());
        tokio::pin!(gen_fut);
        let pngs = loop {
            tokio::select! {
                res = &mut gen_fut => break res?,
                Some(p) = prog_rx.recv() => {
                    sink(ImageEvent::Progress(ImageProgress {
                        phase: ImagePhase::Generating,
                        step: p.step,
                        total_steps: p.total_steps,
                        image_index: p.image_index,
                        batch_count: batch,
                    }));
                }
            }
        };

        // --- store: blob first, then rows (ADR-0009 write order) ---
        let lora_ref: Option<(ImageLoraId, f32)> =
            req.loras.first().map(|s| (s.id.clone(), s.weight));
        let lora_name = match &lora_ref {
            Some((id, _)) => self.repo.resolve_lora(id).await.ok().map(|_| id.clone()),
            None => None,
        };
        let lora_display = self.lora_display_name(lora_name.as_ref()).await;

        let mut rows = Vec::with_capacity(pngs.len());
        for png in pngs {
            let asset = self.blob.put(&png.bytes, "image/png")?;
            self.write_browsable_copy(&png.bytes, png.seed).await;
            let gen_id = self
                .repo
                .record_generation(NewGeneratedImage {
                    asset: asset.clone(),
                    byte_len: png.bytes.len() as u64,
                    prompt: req.prompt.clone(),
                    negative: req.negative.clone(),
                    width: req.width,
                    height: req.height,
                    steps,
                    guidance: req.guidance.unwrap_or(DEFAULT_GUIDANCE),
                    seed: png.seed,
                    lora: lora_ref.clone(),
                    model_id: image_model.clone(),
                })
                .await?;
            rows.push(GeneratedImageRow {
                id: gen_id,
                asset,
                prompt: req.prompt.clone(),
                width: req.width,
                height: req.height,
                seed: png.seed,
                lora: lora_display.clone(),
                created_at: String::new(),
            });
        }
        Ok(rows)
    }

    /// Write a human-named PNG into `output_dir` next to the blob copy. The blob
    /// store is the source of truth; this is a convenience so the user can open
    /// the folder. Failure is logged, never fatal.
    async fn write_browsable_copy(&self, bytes: &[u8], seed: i64) {
        let stamp = OffsetDateTime::now_utc()
            .format(STAMP)
            .unwrap_or_else(|_| "image".to_owned());
        let path = self.output_dir.join(format!("{stamp}-{seed}.png"));
        if let Err(err) = tokio::fs::create_dir_all(&self.output_dir).await {
            tracing::warn!(target: "image", %err, "create image output dir");
            return;
        }
        if let Err(err) = tokio::fs::write(&path, bytes).await {
            tracing::warn!(target: "image", %err, path = %path.display(), "write browsable image copy");
        }
    }

    async fn resolve_image_model(&self) -> AppResult<ModelId> {
        let models = self.registry.list_by_kind(ModelKind::Image).await?;
        models
            .into_iter()
            .find(|m| matches!(m.availability, RegistryAvailability::Ready))
            .map(|m| m.metadata.id)
            .ok_or_else(|| {
                AppError::Conflict(
                    "no image model is registered — run image-model setup first".to_owned(),
                )
            })
    }

    async fn resolve_lora(&self, req: &ImageRequest) -> AppResult<Option<(String, f32)>> {
        match req.loras.first() {
            None => Ok(None),
            Some(sel) => {
                let (file, _default) = self.repo.resolve_lora(&sel.id).await?;
                Ok(Some((file, sel.weight)))
            }
        }
    }

    async fn lora_display_name(&self, id: Option<&ImageLoraId>) -> Option<String> {
        let id = id?;
        self.repo
            .list_loras()
            .await
            .ok()?
            .into_iter()
            .find(|l| &l.id == id)
            .map(|l| l.display_name)
    }

    /// Unload every resident model that is not the image model, returning the
    /// ones we actually unloaded so the caller can restore them.
    async fn evict_others(&self, image_model: &ModelId) -> Vec<ModelId> {
        let mut evicted = Vec::new();
        for status in self.lifecycle.statuses().await {
            if &status.id == image_model {
                continue;
            }
            if matches!(status.state, ModelState::Loaded | ModelState::Busy)
                && self.lifecycle.unload(&status.id).await.is_ok()
            {
                tracing::info!(model = %status.id, "evicted for image generation");
                evicted.push(status.id);
            }
        }
        evicted
    }

    /// Poll the resource snapshot until outstanding GPU reservations clear (or
    /// timeout). A no-op when the probe is unavailable.
    async fn settle_wait(&self, cancel: &CancellationToken) {
        let deadline = tokio::time::Instant::now() + self.settle_timeout;
        loop {
            let snap = self.resources.observe().await;
            if snap.reserved_gpu_mb == 0
                || cancel.is_cancelled()
                || tokio::time::Instant::now() >= deadline
            {
                return;
            }
            tokio::time::sleep(SETTLE_POLL).await;
        }
    }
}

fn emit<F>(sink: &F, phase: ImagePhase, step: u32, total: u32, batch: u32)
where
    F: Fn(ImageEvent) + Send + Sync,
{
    sink(ImageEvent::Progress(ImageProgress {
        phase,
        step,
        total_steps: total,
        image_index: 0,
        batch_count: batch,
    }));
}

fn fresh_seed() -> i64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    i64::from(nanos & 0x7fff_ffff)
}
