//! Image-generation contracts (Phase 22, ADR-0006). The request the Image tab
//! sends, the streaming events it gets back, and the LoRA / preset / history
//! rows it renders.
//!
//! Krea 2 Turbo is the only image model (ADR-0006), so there is no model field.
//! The `prompt` is **untrusted** user text — it is handed to the sidecar as an
//! opaque string (diffusers tokenizes it; it is never shell- or eval-ed).
//! [`ImageEvent`] is adjacently tagged, mirroring
//! [`crate::contracts::generation::GenerationEvent`], so the TypeScript side is
//! a discriminated union that narrows on `.type`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::contracts::ids::{AssetId, GeneratedImageId, ImageLoraId, ImagePresetId};
use crate::ipc::AppError;

/// Smallest accepted image dimension (px).
pub const MIN_DIM: u32 = 512;
/// Largest accepted image dimension (px).
pub const MAX_DIM: u32 = 1664;
/// Both dimensions must be a multiple of this (latent grid constraint).
pub const DIM_MULTIPLE: u32 = 16;
/// `width * height` may not exceed this.
pub const MAX_PIXELS: u32 = MAX_DIM * MAX_DIM;
/// Hard cap on denoising steps.
pub const MAX_STEPS: u32 = 50;
/// Hard cap on images per request.
pub const MAX_BATCH: u32 = 8;
/// Hard cap on prompt length.
pub const MAX_PROMPT_CHARS: usize = 2_000;
/// Hard cap on `guidance_scale`.
pub const MAX_GUIDANCE: f32 = 10.0;

/// One LoRA applied to a generation. v1 applies at most one (`loras[0]`); the
/// list is carried so multi-LoRA stacking is a non-breaking later add
/// (ADR-0006).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct LoraSelection {
    /// Registry id of the LoRA.
    pub id: ImageLoraId,
    /// Adapter weight, `0.0..=1.0`.
    pub weight: f32,
}

/// A request to generate one or more images with Krea 2 Turbo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ImageRequest {
    /// The positive prompt (untrusted user text).
    pub prompt: String,
    /// Optional negative prompt. Inert at `guidance_scale 0` (Turbo's default) —
    /// carried so raising guidance is the only change needed to use it.
    pub negative: Option<String>,
    /// Image width in px — a multiple of 16 within `512..=1664`.
    pub width: u32,
    /// Image height in px — a multiple of 16 within `512..=1664`.
    pub height: u32,
    /// Denoising steps; `None` = the model default (8).
    pub steps: Option<u32>,
    /// `guidance_scale`; `None` = the model default (0.0).
    pub guidance: Option<f32>,
    /// RNG seed; `None` = a fresh seed per request. Batch image `i` uses
    /// `seed + i`.
    pub seed: Option<i64>,
    /// How many images to generate (`1..=8`).
    pub batch_count: u32,
    /// LoRA selection — 0 or 1 entry in v1.
    #[serde(default)]
    pub loras: Vec<LoraSelection>,
}

impl ImageRequest {
    /// # Errors
    /// [`AppError::Validation`] if any field is out of range (see the module
    /// constants), or if more than one LoRA is selected (v1 limit).
    #[allow(clippy::missing_panics_doc)]
    pub fn validate(&self) -> Result<(), AppError> {
        let v = |m: String| Err(AppError::Validation(m));

        if self.prompt.trim().is_empty() {
            return v("prompt must not be empty".to_owned());
        }
        if self.prompt.chars().count() > MAX_PROMPT_CHARS {
            return v(format!("prompt must be <= {MAX_PROMPT_CHARS} characters"));
        }
        for (label, dim) in [("width", self.width), ("height", self.height)] {
            if dim % DIM_MULTIPLE != 0 {
                return v(format!(
                    "{label} must be a multiple of {DIM_MULTIPLE}, got {dim}"
                ));
            }
            if !(MIN_DIM..=MAX_DIM).contains(&dim) {
                return v(format!(
                    "{label} must be within {MIN_DIM}..={MAX_DIM}, got {dim}"
                ));
            }
        }
        if self.width * self.height > MAX_PIXELS {
            return v(format!(
                "width * height must be <= {MAX_PIXELS}, got {}",
                self.width * self.height
            ));
        }
        if let Some(steps) = self.steps {
            if !(1..=MAX_STEPS).contains(&steps) {
                return v(format!("steps must be within 1..={MAX_STEPS}, got {steps}"));
            }
        }
        if let Some(g) = self.guidance {
            if !g.is_finite() || !(0.0..=MAX_GUIDANCE).contains(&g) {
                return v(format!(
                    "guidance must be within 0.0..={MAX_GUIDANCE}, got {g}"
                ));
            }
        }
        if !(1..=MAX_BATCH).contains(&self.batch_count) {
            return v(format!(
                "batch_count must be within 1..={MAX_BATCH}, got {}",
                self.batch_count
            ));
        }
        if self.loras.len() > 1 {
            return v("multi-LoRA stacking is not supported in v1 — select one LoRA".to_owned());
        }
        for lora in &self.loras {
            if !lora.weight.is_finite() || !(0.0..=1.0).contains(&lora.weight) {
                return v(format!(
                    "LoRA weight must be within 0.0..=1.0, got {}",
                    lora.weight
                ));
            }
        }
        Ok(())
    }
}

/// Which stage of the (currently manual — Phase 23 automates it) evict → load →
/// generate → restore sequence a job is in. Drives the UI's "waiting…" copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum ImagePhase {
    /// Unloading the LLM (+ TTS) to free VRAM for the image model.
    Evicting,
    /// Loading Krea 2 Turbo.
    Loading,
    /// Running the diffusion steps.
    Generating,
    /// Unloading Krea 2 and reloading the LLM.
    Restoring,
}

/// Progress of one image generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ImageProgress {
    /// Current stage.
    pub phase: ImagePhase,
    /// Completed denoising steps for the current image (0 outside `Generating`).
    pub step: u32,
    /// Total steps for the current image.
    pub total_steps: u32,
    /// 0-based index of the current image within the batch.
    pub image_index: u32,
    /// Batch size.
    pub batch_count: u32,
}

/// One generated image with its provenance — the `generated_image` row as the
/// frontend sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct GeneratedImageRow {
    /// Stable id.
    pub id: GeneratedImageId,
    /// Content address of the PNG bytes — resolve via `image_bytes`.
    pub asset: AssetId,
    /// The prompt used.
    pub prompt: String,
    /// Image width in px.
    pub width: u32,
    /// Image height in px.
    pub height: u32,
    /// The seed used for this image.
    pub seed: i64,
    /// LoRA display name, or `None` for the base model.
    pub lora: Option<String>,
    /// RFC-3339 creation timestamp.
    pub created_at: String,
}

/// One frame of a streaming image generation. Adjacently tagged:
/// `{ "type": "Progress", "data": { ... } }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum ImageEvent {
    /// A progress update.
    Progress(ImageProgress),
    /// The generation finished — all images are in the blob store.
    Done {
        /// The generated images, in batch order.
        images: Vec<GeneratedImageRow>,
    },
    /// The generation failed. No further frames follow. The LLM has been
    /// restored.
    Error {
        /// The failure.
        error: AppError,
    },
    /// The generation was cancelled. No further frames follow. The LLM has been
    /// restored and no partial image was written.
    Cancelled,
}

/// A realism LoRA available to Krea 2, as the picker sees it (the file path
/// never crosses the wire).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ImageLora {
    /// Registry id — put in a [`LoraSelection`].
    pub id: ImageLoraId,
    /// Human-readable name.
    pub display_name: String,
    /// Base model this LoRA is trained for (`"krea2"`).
    pub base_compat: String,
    /// Suggested adapter weight.
    pub default_weight: f32,
    /// Free-form tags (`"realism"`, `"skin"`, `"nsfw"`).
    pub tags: Vec<String>,
}

/// A saved image-generation parameter set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ImagePreset {
    /// Stable id.
    pub id: ImagePresetId,
    /// Unique name.
    pub name: String,
    /// The parameters.
    pub params: ImagePresetParams,
}

/// The parameter values a preset carries.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ImagePresetParams {
    /// Image width in px.
    pub width: u32,
    /// Image height in px.
    pub height: u32,
    /// Denoising steps.
    pub steps: u32,
    /// `guidance_scale`.
    pub guidance: f32,
}
