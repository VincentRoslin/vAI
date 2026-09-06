//! All SQL for `asset` (image writes only, for now), `image_lora`,
//! `image_preset`, and `generated_image` (`V0007`). Rust owns persistence
//! (`CLAUDE.md` Article I, ADR-0009); nothing else touches these tables.
//!
//! The LoRA registry is **seeded from the confined loras dir** on startup —
//! only for files that are actually present (a row for an absent file would
//! break the picker). Presets are seeded from a fixed list.

// LoRA weights are stored as SQLite REAL (f64) and used as f32 by the sidecar
// contract; the 0.0..=1.0 range makes the narrowing loss-free in practice.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use rusqlite::Row;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::contracts::ids::{AssetId, GeneratedImageId, ImageLoraId, ImagePresetId, ModelId};
use crate::contracts::image::{ImageLora, ImagePreset, ImagePresetParams};
use crate::db::Db;
use crate::ipc::{AppError, AppResult};

/// A generated image ready to record: the blob is already on disk (the caller
/// ran `BlobStore::put`), this writes the `asset` + `generated_image` rows in
/// one transaction.
#[derive(Debug, Clone)]
pub struct NewGeneratedImage {
    /// Content address of the PNG (from `BlobStore::put`).
    pub asset: AssetId,
    /// Blob length in bytes.
    pub byte_len: u64,
    /// The prompt used.
    pub prompt: String,
    /// The negative prompt, if any.
    pub negative: Option<String>,
    /// Width in px.
    pub width: u32,
    /// Height in px.
    pub height: u32,
    /// Denoising steps.
    pub steps: u32,
    /// `guidance_scale`.
    pub guidance: f32,
    /// The seed that produced this image.
    pub seed: i64,
    /// `(lora id, weight)` applied, or `None` for the base model.
    pub lora: Option<(ImageLoraId, f32)>,
    /// The registry id of the image model.
    pub model_id: ModelId,
}

/// All `image_*` + image-side `asset` SQL.
#[derive(Clone)]
pub struct ImageRepo {
    db: Arc<Db>,
}

impl ImageRepo {
    #[must_use]
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// Seed the preset list and the LoRA registry (both idempotent). LoRA rows
    /// are inserted only for `*.safetensors` files present in `loras_dir`.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn seed(&self, loras_dir: &Path) -> AppResult<()> {
        let presets = default_presets();
        let loras = scan_loras(loras_dir);
        let now = now_rfc3339();
        self.db
            .write(move |tx| {
                for (name, p) in presets {
                    tx.prepare_cached(
                        "INSERT INTO image_preset (id, name, params, created_at) \
                         VALUES (?1, ?2, ?3, ?4) ON CONFLICT(name) DO NOTHING",
                    )?
                    .execute(rusqlite::params![
                        Uuid::new_v4().hyphenated().to_string(),
                        name,
                        serde_json::to_string(&p).unwrap_or_else(|_| "{}".to_owned()),
                        now,
                    ])?;
                }
                for lora in loras {
                    tx.prepare_cached(
                        "INSERT INTO image_lora \
                         (id, file, display_name, base_compat, format, default_weight, tags, created_at) \
                         VALUES (?1, ?2, ?3, 'krea2', ?4, ?5, ?6, ?7) \
                         ON CONFLICT(file) DO NOTHING",
                    )?
                    .execute(rusqlite::params![
                        Uuid::new_v4().hyphenated().to_string(),
                        lora.file,
                        lora.display_name,
                        lora.format,
                        lora.default_weight,
                        serde_json::to_string(&lora.tags).unwrap_or_else(|_| "[]".to_owned()),
                        now,
                    ])?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Every registered LoRA, by display name.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn list_loras(&self) -> AppResult<Vec<ImageLora>> {
        let rows: Vec<RawLora> = self
            .db
            .read(|conn| {
                Ok(conn
                    .prepare_cached(
                        "SELECT id, display_name, base_compat, default_weight, tags \
                         FROM image_lora ORDER BY display_name",
                    )?
                    .query_map([], RawLora::from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await?;
        Ok(rows.into_iter().map(RawLora::into_contract).collect())
    }

    /// Resolve a LoRA id to `(basename, default_weight)` for the sidecar.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if the id is unknown.
    pub async fn resolve_lora(&self, id: &ImageLoraId) -> AppResult<(String, f32)> {
        let key = id.to_string();
        let row: Option<(String, f64)> = self
            .db
            .read(move |conn| {
                Ok(conn
                    .prepare_cached("SELECT file, default_weight FROM image_lora WHERE id = ?1")?
                    .query_row([key], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)))
                    .ok())
            })
            .await?;
        row.map(|(f, w)| (f, w as f32))
            .ok_or_else(|| AppError::NotFound(format!("image LoRA {id}")))
    }

    /// Every preset.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn list_presets(&self) -> AppResult<Vec<ImagePreset>> {
        let rows: Vec<(String, String, String)> = self
            .db
            .read(|conn| {
                Ok(conn
                    .prepare_cached("SELECT id, name, params FROM image_preset ORDER BY name")?
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, name, params)| {
                serde_json::from_str::<ImagePresetParams>(&params)
                    .ok()
                    .map(|p| ImagePreset {
                        id: ImagePresetId::from_trusted(id),
                        name,
                        params: p,
                    })
            })
            .collect())
    }

    /// Record one generated image: the `asset` row (idempotent — the blob is
    /// already on disk) and the `generated_image` row, in one transaction.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn record_generation(&self, rec: NewGeneratedImage) -> AppResult<GeneratedImageId> {
        let id = GeneratedImageId::from_trusted(Uuid::new_v4().hyphenated().to_string());
        let key = id.to_string();
        let now = now_rfc3339();
        let idc = key.clone();
        self.db
            .write(move |tx| {
                tx.prepare_cached(
                    "INSERT INTO asset (id, media_type, byte_len, created_at) \
                     VALUES (?1, 'image/png', ?2, ?3) ON CONFLICT(id) DO NOTHING",
                )?
                .execute(rusqlite::params![
                    rec.asset.as_str(),
                    rec.byte_len,
                    now
                ])?;

                let (lora_id, lora_weight) = match &rec.lora {
                    Some((id, w)) => (Some(id.to_string()), Some(f64::from(*w))),
                    None => (None, None),
                };
                tx.prepare_cached(
                    "INSERT INTO generated_image \
                     (id, asset_id, prompt, negative, width, height, steps, guidance, seed, \
                      lora_id, lora_weight, model_id, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                )?
                .execute(rusqlite::params![
                    idc,
                    rec.asset.as_str(),
                    rec.prompt,
                    rec.negative,
                    rec.width,
                    rec.height,
                    rec.steps,
                    f64::from(rec.guidance),
                    rec.seed,
                    lora_id,
                    lora_weight,
                    rec.model_id.as_str(),
                    now,
                ])?;
                Ok(())
            })
            .await?;
        Ok(id)
    }

    /// Recent generations, newest first, with the LoRA display name resolved.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn list_generated(
        &self,
        limit: u32,
    ) -> AppResult<Vec<crate::contracts::image::GeneratedImageRow>> {
        let rows: Vec<RawGenerated> = self
            .db
            .read(move |conn| {
                Ok(conn
                    .prepare_cached(
                        "SELECT g.id, g.asset_id, g.prompt, g.width, g.height, g.seed, \
                                l.display_name, g.created_at \
                         FROM generated_image g \
                         LEFT JOIN image_lora l ON l.id = g.lora_id \
                         ORDER BY g.created_at DESC, g.id DESC LIMIT ?1",
                    )?
                    .query_map([limit], RawGenerated::from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await?;
        Ok(rows.into_iter().map(RawGenerated::into_contract).collect())
    }

    /// Every `asset` id the database knows about — for `blob::reconcile`.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn known_asset_ids(&self) -> AppResult<HashSet<String>> {
        Ok(self
            .db
            .read(|conn| {
                Ok(conn
                    .prepare_cached("SELECT id FROM asset")?
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<HashSet<_>>>()?)
            })
            .await?)
    }
}

// ---------------------------------------------------------------- row mapping

struct RawLora {
    id: String,
    display_name: String,
    base_compat: String,
    default_weight: f64,
    tags: String,
}

impl RawLora {
    fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            display_name: r.get(1)?,
            base_compat: r.get(2)?,
            default_weight: r.get(3)?,
            tags: r.get(4)?,
        })
    }

    fn into_contract(self) -> ImageLora {
        ImageLora {
            id: ImageLoraId::from_trusted(self.id),
            display_name: self.display_name,
            base_compat: self.base_compat,
            default_weight: self.default_weight as f32,
            tags: serde_json::from_str(&self.tags).unwrap_or_default(),
        }
    }
}

struct RawGenerated {
    id: String,
    asset_id: String,
    prompt: String,
    width: u32,
    height: u32,
    seed: i64,
    lora: Option<String>,
    created_at: String,
}

impl RawGenerated {
    fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            asset_id: r.get(1)?,
            prompt: r.get(2)?,
            width: r.get(3)?,
            height: r.get(4)?,
            seed: r.get(5)?,
            lora: r.get(6)?,
            created_at: r.get(7)?,
        })
    }

    fn into_contract(self) -> crate::contracts::image::GeneratedImageRow {
        crate::contracts::image::GeneratedImageRow {
            id: GeneratedImageId::from_trusted(self.id),
            asset: AssetId::from_trusted(self.asset_id),
            prompt: self.prompt,
            width: self.width,
            height: self.height,
            seed: self.seed,
            lora: self.lora,
            created_at: self.created_at,
        }
    }
}

// ---------------------------------------------------------------- seeding

struct SeedLora {
    file: String,
    display_name: String,
    format: String,
    default_weight: f64,
    tags: Vec<String>,
}

fn default_presets() -> Vec<(&'static str, ImagePresetParams)> {
    vec![
        (
            "Square 1024",
            ImagePresetParams {
                width: 1024,
                height: 1024,
                steps: 8,
                guidance: 0.0,
            },
        ),
        (
            "Portrait 928x1232",
            ImagePresetParams {
                width: 928,
                height: 1232,
                steps: 8,
                guidance: 0.0,
            },
        ),
        (
            "Landscape 1232x928",
            ImagePresetParams {
                width: 1232,
                height: 928,
                steps: 8,
                guidance: 0.0,
            },
        ),
    ]
}

/// Curated metadata for the LoRAs the owner ships; anything else in the dir is
/// registered with a derived name and no tags (ADR-0006: files can be dropped
/// in by hand).
fn lora_metadata(file: &str) -> (String, Vec<String>, f64) {
    match file {
        "krea2-realism.safetensors" => ("Realism".to_owned(), vec!["realism".to_owned()], 0.9),
        "krea2-skin.safetensors" => (
            "Skin".to_owned(),
            vec!["skin".to_owned(), "realism".to_owned()],
            0.9,
        ),
        "krea2-lustify-nsfw.safetensors" => (
            "Lustify (NSFW)".to_owned(),
            vec!["nsfw".to_owned(), "realism".to_owned()],
            0.85,
        ),
        other => (derive_title(other), Vec::new(), 0.9),
    }
}

fn derive_title(file: &str) -> String {
    let stem = file.trim_end_matches(".safetensors");
    let mut out = String::new();
    for (i, word) in stem
        .split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .enumerate()
    {
        if i > 0 {
            out.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() {
        stem.to_owned()
    } else {
        out
    }
}

fn scan_loras(dir: &Path) -> Vec<SeedLora> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.to_ascii_lowercase().ends_with(".safetensors") {
            continue;
        }
        let (display_name, tags, default_weight) = lora_metadata(name);
        out.push(SeedLora {
            file: name.to_owned(),
            display_name,
            format: detect_lora_format(&entry.path()),
            default_weight,
            tags,
        });
    }
    out
}

/// Best-effort LoRA format sniff from the safetensors header (a leading
/// `u64` length + that many bytes of JSON whose keys are tensor names).
fn detect_lora_format(path: &Path) -> String {
    let Ok(bytes) = read_header(path) else {
        return "unknown".to_owned();
    };
    let has = |needle: &str| bytes.contains(needle);
    if has("lora_down") || has("lora_up") {
        "kohya".to_owned()
    } else if has("lora_A") || has("lora_B") {
        if has("transformer.") {
            "diffusers".to_owned()
        } else {
            "peft".to_owned()
        }
    } else {
        "unknown".to_owned()
    }
}

fn read_header(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut len_buf = [0u8; 8];
    f.read_exact(&mut len_buf)?;
    let len = u64::from_le_bytes(len_buf).min(4 * 1024 * 1024) as usize;
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}
