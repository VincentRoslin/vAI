//! Model registry — model metadata as queryable rows (`CLAUDE.md` Article I,
//! ADR-0009 `model_*`, Phase 11). Stable [`ModelId`] (ADR-0017), path confined
//! to the configured model dir, availability computed from the filesystem at
//! read time. **No loading, no downloading.**

#[cfg(test)]
mod tests;

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::contracts::ids::ModelId;
use crate::contracts::model::{
    Device, ModelBackend, ModelCapabilities, ModelKind, ModelMetadata, Quant, RegisteredModel,
    RegistryAvailability,
};
use crate::db::Db;
use crate::ipc::{AppError, AppResult};

// ---------------------------------------------------------------- kind <-> text

fn kind_to_db(kind: ModelKind) -> &'static str {
    match kind {
        ModelKind::Llm => "Llm",
        ModelKind::Stt => "Stt",
        ModelKind::Tts => "Tts",
        ModelKind::Image => "Image",
        ModelKind::Embedder => "Embedder",
    }
}

fn kind_from_db(raw: &str) -> AppResult<ModelKind> {
    match raw {
        "Llm" => Ok(ModelKind::Llm),
        "Stt" => Ok(ModelKind::Stt),
        "Tts" => Ok(ModelKind::Tts),
        "Image" => Ok(ModelKind::Image),
        "Embedder" => Ok(ModelKind::Embedder),
        other => Err(AppError::internal(
            "model kind from db",
            format!("unknown kind {other:?}"),
        )),
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

// ---------------------------------------------------------------- domain types

/// A registry row.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// Stable id (UUIDv4).
    pub id: ModelId,
    /// Human-facing name.
    pub display_name: String,
    /// Pipeline role.
    pub kind: ModelKind,
    /// Executing runtime.
    pub backend: ModelBackend,
    /// Quantization, if any.
    pub quant: Option<Quant>,
    /// Absolute path to the model file.
    pub path: PathBuf,
    /// Whether the model streams output.
    pub streaming: bool,
    /// Context length in tokens, when it applies.
    pub context_tokens: Option<u32>,
    /// Rough loaded VRAM footprint in MB, when known.
    pub estimated_vram_mb: Option<u32>,
    /// Devices this model can run on.
    pub devices: Vec<Device>,
    /// Per-backend configuration blob (opaque here).
    pub config: Value,
    /// RFC-3339 creation timestamp.
    pub created_at: String,
    /// RFC-3339 timestamp of the last update.
    pub updated_at: String,
}

impl Model {
    /// Whether the model's payload is present on disk *right now*. A single-file
    /// backend (a GGUF) is a file; a diffusers-layout image model (Phase 22) is
    /// a directory — either counts as present.
    #[must_use]
    pub fn availability(&self) -> RegistryAvailability {
        if self.path.exists() {
            RegistryAvailability::Ready
        } else {
            RegistryAvailability::Missing
        }
    }

    /// The wire view: metadata + on-disk facts.
    #[must_use]
    pub fn to_registered(&self) -> RegisteredModel {
        RegisteredModel {
            metadata: ModelMetadata {
                id: self.id.clone(),
                display_name: self.display_name.clone(),
                kind: self.kind,
                backend: self.backend.clone(),
                quant: self.quant.clone(),
                capabilities: ModelCapabilities {
                    streaming: self.streaming,
                    context_tokens: self.context_tokens,
                },
                estimated_vram_mb: self.estimated_vram_mb,
            },
            path: self.path.display().to_string(),
            availability: self.availability(),
            devices: self.devices.clone(),
        }
    }
}

/// Everything needed to register a model except the id and timestamps.
#[derive(Debug, Clone)]
pub struct ModelDraft {
    /// Human-facing name.
    pub display_name: String,
    /// Pipeline role.
    pub kind: ModelKind,
    /// Executing runtime.
    pub backend: ModelBackend,
    /// Quantization, if any.
    pub quant: Option<Quant>,
    /// Path to the model file (must exist, inside the model dir, at registration).
    pub path: PathBuf,
    /// Whether the model streams output.
    pub streaming: bool,
    /// Context length in tokens, when it applies (`>= 1` if present).
    pub context_tokens: Option<u32>,
    /// Rough loaded VRAM footprint in MB (`>= 1` if present).
    pub estimated_vram_mb: Option<u32>,
    /// Devices this model can run on (non-empty).
    pub devices: Vec<Device>,
    /// Per-backend configuration blob (must be a JSON object).
    pub config: Value,
}

impl ModelDraft {
    /// # Errors
    /// [`AppError::Validation`] naming the offending field.
    pub fn validate(&self) -> AppResult<()> {
        if self.display_name.trim().is_empty() {
            return Err(AppError::Validation(
                "display_name must not be empty".to_owned(),
            ));
        }
        if self.backend.0.trim().is_empty() {
            return Err(AppError::Validation("backend must not be empty".to_owned()));
        }
        if self.context_tokens == Some(0) {
            return Err(AppError::Validation(
                "context_tokens must be >= 1".to_owned(),
            ));
        }
        if self.estimated_vram_mb == Some(0) {
            return Err(AppError::Validation(
                "estimated_vram_mb must be >= 1".to_owned(),
            ));
        }
        if self.devices.is_empty() {
            return Err(AppError::Validation("devices must not be empty".to_owned()));
        }
        if !self.config.is_object() {
            return Err(AppError::Validation(
                "config must be a JSON object".to_owned(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- path confinement

/// Resolve `candidate` and require it to be an existing file inside `models_dir`
/// with no `..` traversal. Returns the canonical path, with the Windows `\\?\`
/// verbatim prefix stripped (some downstream tools mishandle it).
///
/// # Errors
/// [`AppError::Validation`] for a `..` component or a path outside the dir;
/// [`AppError::NotFound`] if the file does not exist.
pub fn validate_model_path(candidate: &Path, models_dir: &Path) -> AppResult<PathBuf> {
    if candidate
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(AppError::Validation(
            "model path must not contain '..'".to_owned(),
        ));
    }
    let dir = models_dir.canonicalize().map_err(|err| {
        AppError::Validation(format!(
            "model dir {} is not accessible: {err}",
            models_dir.display()
        ))
    })?;
    let file = candidate
        .canonicalize()
        .map_err(|_| AppError::NotFound(format!("model file {}", candidate.display())))?;
    if !file.starts_with(&dir) {
        return Err(AppError::Validation(format!(
            "model path {} is outside the model dir",
            candidate.display()
        )));
    }
    Ok(strip_verbatim(file))
}

/// Strip a Windows `\\?\` (or `\\?\UNC\`) extended-length prefix, if present.
/// No-op on other platforms.
#[must_use]
pub fn strip_verbatim(path: PathBuf) -> PathBuf {
    match path.to_str() {
        Some(s) => {
            if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
                PathBuf::from(format!(r"\\{rest}"))
            } else if let Some(rest) = s.strip_prefix(r"\\?\") {
                PathBuf::from(rest)
            } else {
                path
            }
        }
        None => path,
    }
}

// ---------------------------------------------------------------- filter

/// A capability query over the registry.
#[derive(Debug, Clone, Default)]
pub struct ModelFilter {
    /// Restrict to one pipeline role.
    pub kind: Option<ModelKind>,
    /// Restrict to models that do / don't stream.
    pub streaming: Option<bool>,
    /// Restrict to models with at least this much context.
    pub min_context_tokens: Option<u32>,
}

impl ModelFilter {
    fn matches(&self, model: &Model) -> bool {
        if let Some(kind) = self.kind {
            if model.kind != kind {
                return false;
            }
        }
        if let Some(streaming) = self.streaming {
            if model.streaming != streaming {
                return false;
            }
        }
        if let Some(min) = self.min_context_tokens {
            if model.context_tokens.is_none_or(|c| c < min) {
                return false;
            }
        }
        true
    }
}

// ---------------------------------------------------------------- registry

/// The model registry. Held in Tauri managed state (Phase 12 consumes it).
#[derive(Debug)]
pub struct ModelRegistry {
    db: Arc<Db>,
    cache: Mutex<Option<Arc<Vec<Model>>>>,
}

const COLUMNS: &str = "id, display_name, kind, backend, quant, path, streaming, \
    context_tokens, estimated_vram_mb, devices, config, created_at, updated_at";

impl ModelRegistry {
    /// Build a registry over `db`. The cache starts cold.
    #[must_use]
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            cache: Mutex::new(None),
        }
    }

    fn invalidate(&self) {
        *self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }

    #[cfg(test)]
    fn db_for_test(&self) -> &Db {
        &self.db
    }

    /// Every row, newest last. Served from the in-memory cache (a shared `Arc`)
    /// when warm.
    async fn all(&self) -> AppResult<Arc<Vec<Model>>> {
        if let Some(cached) = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
        {
            return Ok(cached);
        }

        let raws: Vec<RawRow> = self
            .db
            .read(|conn| {
                let rows = conn
                    .prepare_cached(&format!(
                        "SELECT {COLUMNS} FROM model_entry ORDER BY created_at, id"
                    ))?
                    .query_map([], RawRow::from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?;

        let models = Arc::new(
            raws.into_iter()
                .map(Model::try_from)
                .collect::<AppResult<Vec<_>>>()?,
        );

        *self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::clone(&models));
        Ok(models)
    }

    /// Register a new model. Validates the draft and confines the path to
    /// `models_dir`, mints a UUIDv4 id, and inserts the row.
    ///
    /// # Errors
    /// [`AppError::Validation`] / [`AppError::NotFound`] for a bad draft or path;
    /// a persistence error otherwise.
    pub async fn register(&self, draft: ModelDraft, models_dir: &Path) -> AppResult<ModelId> {
        draft.validate()?;
        let path = validate_model_path(&draft.path, models_dir)?;

        let id = ModelId::from_trusted(Uuid::new_v4().hyphenated().to_string());
        let now = now_rfc3339();
        let row = InsertRow {
            id: id.to_string(),
            display_name: draft.display_name,
            kind: kind_to_db(draft.kind).to_owned(),
            backend: draft.backend.0,
            quant: draft.quant.map(|q| q.0),
            path: path.to_string_lossy().into_owned(),
            streaming: i64::from(draft.streaming),
            context_tokens: draft.context_tokens.map(i64::from),
            estimated_vram_mb: draft.estimated_vram_mb.map(i64::from),
            devices: serde_json::to_string(&draft.devices).unwrap_or_else(|_| "[]".to_owned()),
            config: draft.config.to_string(),
            created_at: now.clone(),
            updated_at: now,
        };

        self.db
            .write(move |tx| {
                tx.prepare_cached(&format!(
                    "INSERT INTO model_entry ({COLUMNS}) VALUES \
                     (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"
                ))?
                .execute(rusqlite::params![
                    row.id,
                    row.display_name,
                    row.kind,
                    row.backend,
                    row.quant,
                    row.path,
                    row.streaming,
                    row.context_tokens,
                    row.estimated_vram_mb,
                    row.devices,
                    row.config,
                    row.created_at,
                    row.updated_at,
                ])?;
                Ok(())
            })
            .await?;

        self.invalidate();
        Ok(id)
    }

    /// Replace the mutable fields of `id` from `draft` (id and `created_at` are
    /// kept). Bumps `updated_at`.
    ///
    /// # Errors
    /// As [`ModelRegistry::register`]; [`AppError::NotFound`] if `id` is unknown.
    pub async fn update(
        &self,
        id: &ModelId,
        draft: ModelDraft,
        models_dir: &Path,
    ) -> AppResult<()> {
        draft.validate()?;
        let path = validate_model_path(&draft.path, models_dir)?;
        let id_str = id.to_string();
        let now = now_rfc3339();
        let devices = serde_json::to_string(&draft.devices).unwrap_or_else(|_| "[]".to_owned());
        let config = draft.config.to_string();
        let kind = kind_to_db(draft.kind).to_owned();
        let backend = draft.backend.0;
        let quant = draft.quant.map(|q| q.0);
        let path = path.to_string_lossy().into_owned();

        let changed = self
            .db
            .write(move |tx| {
                let n = tx
                    .prepare_cached(
                        "UPDATE model_entry SET display_name=?2, kind=?3, backend=?4, quant=?5, \
                         path=?6, streaming=?7, context_tokens=?8, estimated_vram_mb=?9, \
                         devices=?10, config=?11, updated_at=?12 WHERE id=?1",
                    )?
                    .execute(rusqlite::params![
                        id_str,
                        draft.display_name,
                        kind,
                        backend,
                        quant,
                        path,
                        i64::from(draft.streaming),
                        draft.context_tokens.map(i64::from),
                        draft.estimated_vram_mb.map(i64::from),
                        devices,
                        config,
                        now,
                    ])?;
                Ok(n)
            })
            .await?;

        self.invalidate();
        if changed == 0 {
            return Err(AppError::NotFound(format!("model {id}")));
        }
        Ok(())
    }

    /// Delete a registry row.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if `id` is unknown; a persistence error otherwise.
    pub async fn delete(&self, id: &ModelId) -> AppResult<()> {
        let id_str = id.to_string();
        let removed = self
            .db
            .write(move |tx| {
                let n = tx
                    .prepare_cached("DELETE FROM model_entry WHERE id=?1")?
                    .execute([id_str])?;
                Ok(n)
            })
            .await?;
        self.invalidate();
        if removed == 0 {
            return Err(AppError::NotFound(format!("model {id}")));
        }
        Ok(())
    }

    /// One model by id.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if `id` is unknown.
    pub async fn get(&self, id: &ModelId) -> AppResult<RegisteredModel> {
        self.all()
            .await?
            .iter()
            .find(|m| &m.id == id)
            .map(Model::to_registered)
            .ok_or_else(|| AppError::NotFound(format!("model {id}")))
    }

    /// Every registered model.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn list(&self) -> AppResult<Vec<RegisteredModel>> {
        Ok(self.all().await?.iter().map(Model::to_registered).collect())
    }

    /// Every registered model of one kind.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn list_by_kind(&self, kind: ModelKind) -> AppResult<Vec<RegisteredModel>> {
        Ok(self
            .all()
            .await?
            .iter()
            .filter(|m| m.kind == kind)
            .map(Model::to_registered)
            .collect())
    }

    /// Models matching a capability filter.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn query(&self, filter: &ModelFilter) -> AppResult<Vec<RegisteredModel>> {
        Ok(self
            .all()
            .await?
            .iter()
            .filter(|m| filter.matches(m))
            .map(Model::to_registered)
            .collect())
    }
}

// ---------------------------------------------------------------- row plumbing

struct InsertRow {
    id: String,
    display_name: String,
    kind: String,
    backend: String,
    quant: Option<String>,
    path: String,
    streaming: i64,
    context_tokens: Option<i64>,
    estimated_vram_mb: Option<i64>,
    devices: String,
    config: String,
    created_at: String,
    updated_at: String,
}

struct RawRow {
    id: String,
    display_name: String,
    kind: String,
    backend: String,
    quant: Option<String>,
    path: String,
    streaming: i64,
    context_tokens: Option<i64>,
    estimated_vram_mb: Option<i64>,
    devices: String,
    config: String,
    created_at: String,
    updated_at: String,
}

impl RawRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            display_name: row.get(1)?,
            kind: row.get(2)?,
            backend: row.get(3)?,
            quant: row.get(4)?,
            path: row.get(5)?,
            streaming: row.get(6)?,
            context_tokens: row.get(7)?,
            estimated_vram_mb: row.get(8)?,
            devices: row.get(9)?,
            config: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
        })
    }
}

impl TryFrom<RawRow> for Model {
    type Error = AppError;

    fn try_from(raw: RawRow) -> Result<Self, Self::Error> {
        let devices: Vec<Device> = serde_json::from_str(&raw.devices)
            .map_err(|e| AppError::internal("model row devices json", e))?;
        let config: Value = serde_json::from_str(&raw.config)
            .map_err(|e| AppError::internal("model row config json", e))?;
        Ok(Self {
            id: ModelId::from_trusted(raw.id),
            display_name: raw.display_name,
            kind: kind_from_db(&raw.kind)?,
            backend: ModelBackend(raw.backend),
            quant: raw.quant.map(Quant),
            path: PathBuf::from(raw.path),
            streaming: raw.streaming != 0,
            context_tokens: raw.context_tokens.and_then(|v| u32::try_from(v).ok()),
            estimated_vram_mb: raw.estimated_vram_mb.and_then(|v| u32::try_from(v).ok()),
            devices,
            config,
            created_at: raw.created_at,
            updated_at: raw.updated_at,
        })
    }
}
