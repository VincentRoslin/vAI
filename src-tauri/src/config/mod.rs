//! Configuration — the single settings authority (ADR-0016, `CLAUDE.md`
//! Article I). Rust owns it; the frontend reads and writes only through the
//! typed `config_*` IPC commands.
//!
//! Layers, folded into the *effective* config:
//! `defaults` (code) ← `file` (`<app_config_dir>/config.json`) ← `session`
//! (in-memory, never written).
//!
//! Two distinct failure paths:
//! - **unparseable file** → back up to `config.json.corrupt-<unix>`, start from
//!   defaults, warn (never a silent wipe);
//! - **parseable but an invalid value** → fail fast with [`AppError::Validation`]
//!   naming the offending key.
//!
//! No secret ever lives in the config file (`SECURITY.md`); there is no field
//! that could hold one.

#[cfg(test)]
mod tests;

use std::fs;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use ts_rs::TS;

use crate::ipc::{AppError, AppResult};

/// Schema version this binary understands. A file with a higher version is
/// refused; a lower (or absent) version is migrated forward on load.
pub const CURRENT_SCHEMA_VERSION: u32 = 5;

const FILE_NAME: &str = "config.json";
const TMP_NAME: &str = "config.json.tmp";
const DEFAULT_MODEL_BUDGET_GB: u32 = 100;
const DEFAULT_MIN_FREE_GB: u32 = 20;
const DEFAULT_LOG_LEVEL: &str = "info";
/// VRAM the resource manager holds back on top of every estimate (ADR-0007).
const DEFAULT_VRAM_SAFETY_MARGIN_MB: u32 = 1500;
/// An obvious fat-finger guard — no single GPU on the roadmap has this much VRAM.
const MAX_VRAM_SAFETY_MARGIN_MB: u32 = 65_536;

// ---------------------------------------------------------------- schema

/// The full configuration document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct AppConfig {
    /// Schema version of this document.
    pub version: u32,
    /// Model storage + budget.
    pub models: ModelsConfig,
    /// Logging.
    pub logging: LoggingConfig,
    /// Resource manager tuning (schema v4).
    pub resources: ResourcesConfig,
    /// Where supervised runtime binaries live (schema v5).
    pub runtimes: RuntimesConfig,
}

/// Location of the supervised runtime binaries (`llama-server`, later the image
/// server + Python workers). Schema v5. See ADR-0003 / ADR-0013.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct RuntimesConfig {
    /// Absolute directory holding `llama-server(.exe)` (+ its CUDA DLLs).
    /// Default: `<app_data>/runtimes`. Point it into the project to keep large
    /// binaries with the repo.
    #[ts(type = "string")]
    pub dir: PathBuf,
}

/// Resource-manager tuning (schema v4). See ADR-0007.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ResourcesConfig {
    /// VRAM (MB) held back on top of every load estimate, absorbing estimation
    /// error and driver/WDDM overhead. Default 1500.
    pub vram_safety_margin_mb: u32,
}

/// Logging configuration (schema v2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct LoggingConfig {
    /// A `tracing` filter directive (`info`, `warn`, `warn,localai=debug`, …).
    /// The `LOCALAI_LOG` env var, when set, overrides this.
    pub level: String,
}

/// Where downloaded models live and how much disk they may use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ModelsConfig {
    /// Absolute directory for downloaded model files. Default: `<app_data>/models`.
    #[ts(type = "string")]
    pub dir: PathBuf,
    /// Maximum disk the model directory may occupy, in GB. Must be `>= 1`.
    pub budget_gb: u32,
    /// Disk headroom (GB) to keep free — a download that would leave less than
    /// this is refused before it starts (schema v3). Must be `>= 1`.
    pub min_free_gb: u32,
}

impl AppConfig {
    /// The built-in defaults, anchored under the app data root.
    #[must_use]
    pub fn defaults(app_data_root: &Path) -> Self {
        Self {
            version: CURRENT_SCHEMA_VERSION,
            models: ModelsConfig {
                dir: app_data_root.join("models"),
                budget_gb: DEFAULT_MODEL_BUDGET_GB,
                min_free_gb: DEFAULT_MIN_FREE_GB,
            },
            logging: LoggingConfig {
                level: DEFAULT_LOG_LEVEL.to_owned(),
            },
            resources: ResourcesConfig {
                vram_safety_margin_mb: DEFAULT_VRAM_SAFETY_MARGIN_MB,
            },
            runtimes: RuntimesConfig {
                dir: app_data_root.join("runtimes"),
            },
        }
    }

    /// Reject a semantically invalid document. Errors name the dotted key.
    ///
    /// # Errors
    /// [`AppError::Validation`] for an unsupported schema version, a
    /// `budget_gb` below 1, or a non-absolute / empty `models.dir`.
    pub fn validate(&self) -> AppResult<()> {
        if self.version > CURRENT_SCHEMA_VERSION {
            return Err(AppError::Validation(format!(
                "config schema version {} is newer than supported ({CURRENT_SCHEMA_VERSION})",
                self.version
            )));
        }
        if self.models.budget_gb < 1 {
            return Err(AppError::Validation(
                "models.budget_gb must be >= 1".to_owned(),
            ));
        }
        if self.models.min_free_gb < 1 {
            return Err(AppError::Validation(
                "models.min_free_gb must be >= 1".to_owned(),
            ));
        }
        if self.models.dir.as_os_str().is_empty() {
            return Err(AppError::Validation(
                "models.dir must not be empty".to_owned(),
            ));
        }
        if !self.models.dir.is_absolute() {
            return Err(AppError::Validation(
                "models.dir must be an absolute path".to_owned(),
            ));
        }
        if self.runtimes.dir.as_os_str().is_empty() || !self.runtimes.dir.is_absolute() {
            return Err(AppError::Validation(
                "runtimes.dir must be a non-empty absolute path".to_owned(),
            ));
        }
        if self.resources.vram_safety_margin_mb > MAX_VRAM_SAFETY_MARGIN_MB {
            return Err(AppError::Validation(format!(
                "resources.vram_safety_margin_mb must be <= {MAX_VRAM_SAFETY_MARGIN_MB}"
            )));
        }
        crate::logging::validate_directive(&self.logging.level)?;
        Ok(())
    }
}

// ---------------------------------------------------------------- migration

fn detect_version(raw: &Value) -> u64 {
    raw.get("version").and_then(Value::as_u64).unwrap_or(0)
}

/// Recursively overlay `overlay` onto `base` (objects merge key-wise; any other
/// value replaces).
fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                deep_merge(b.entry(k).or_insert(Value::Null), v);
            }
        }
        (b, o) => *b = o,
    }
}

/// Migrate a version-`from` document one step forward. The schema is
/// **additive-only** (ADR-0016), so every step is the same: fill any keys the
/// newer version introduced from the defaults, then stamp the new version. A
/// non-additive change (rename / remove) would need a bespoke arm here.
fn step_forward(raw: Value, app_data_root: &Path, from: u32) -> Value {
    let mut base = serde_json::to_value(AppConfig::defaults(app_data_root))
        .expect("defaults always serialize");
    deep_merge(&mut base, raw);
    base["version"] = Value::from(from + 1);
    base
}

fn migrate(raw: Value, app_data_root: &Path) -> AppResult<Value> {
    let start = detect_version(&raw);
    let current = u64::from(CURRENT_SCHEMA_VERSION);
    if start > current {
        return Err(AppError::Validation(format!(
            "config schema version {start} is newer than supported ({CURRENT_SCHEMA_VERSION})"
        )));
    }
    let mut value = raw;
    let mut v = start;
    while v < current {
        let from = u32::try_from(v).map_err(|_| AppError::Internal)?;
        value = step_forward(value, app_data_root, from);
        v += 1;
    }
    Ok(value)
}

// ---------------------------------------------------------------- session layer

#[derive(Debug, Default, Clone)]
struct SessionOverrides {
    models_dir: Option<PathBuf>,
    models_budget_gb: Option<u32>,
    models_min_free_gb: Option<u32>,
    logging_level: Option<String>,
    vram_safety_margin_mb: Option<u32>,
    runtimes_dir: Option<PathBuf>,
}

impl SessionOverrides {
    fn is_empty(&self) -> bool {
        self.models_dir.is_none()
            && self.models_budget_gb.is_none()
            && self.models_min_free_gb.is_none()
            && self.logging_level.is_none()
            && self.vram_safety_margin_mb.is_none()
            && self.runtimes_dir.is_none()
    }

    fn apply(&self, cfg: &mut AppConfig) {
        if let Some(dir) = &self.models_dir {
            cfg.models.dir.clone_from(dir);
        }
        if let Some(dir) = &self.runtimes_dir {
            cfg.runtimes.dir.clone_from(dir);
        }
        if let Some(budget) = self.models_budget_gb {
            cfg.models.budget_gb = budget;
        }
        if let Some(min_free) = self.models_min_free_gb {
            cfg.models.min_free_gb = min_free;
        }
        if let Some(level) = &self.logging_level {
            cfg.logging.level.clone_from(level);
        }
        if let Some(margin) = self.vram_safety_margin_mb {
            cfg.resources.vram_safety_margin_mb = margin;
        }
    }
}

// ---------------------------------------------------------------- keys

/// A settable configuration key. The set is small and explicit; it grows
/// additively as phases add settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum ConfigKey {
    /// `models.dir` — absolute path.
    ModelsDir,
    /// `models.budget_gb` — integer `>= 1`.
    ModelsBudgetGb,
    /// `models.min_free_gb` — integer `>= 1`.
    ModelsMinFreeGb,
    /// `logging.level` — a `tracing` filter directive.
    LoggingLevel,
    /// `resources.vram_safety_margin_mb` — integer MB, `0..=65536`.
    VramSafetyMarginMb,
    /// `runtimes.dir` — absolute path.
    RuntimesDir,
}

impl ConfigKey {
    /// Every overridable key.
    pub const ALL: [Self; 6] = [
        Self::ModelsDir,
        Self::ModelsBudgetGb,
        Self::ModelsMinFreeGb,
        Self::LoggingLevel,
        Self::VramSafetyMarginMb,
        Self::RuntimesDir,
    ];

    #[must_use]
    fn dotted(self) -> &'static str {
        match self {
            Self::ModelsDir => "models.dir",
            Self::ModelsBudgetGb => "models.budget_gb",
            Self::ModelsMinFreeGb => "models.min_free_gb",
            Self::LoggingLevel => "logging.level",
            Self::VramSafetyMarginMb => "resources.vram_safety_margin_mb",
            Self::RuntimesDir => "runtimes.dir",
        }
    }

    #[must_use]
    fn value_type(self) -> &'static str {
        match self {
            Self::ModelsDir | Self::RuntimesDir => "path",
            Self::ModelsBudgetGb | Self::ModelsMinFreeGb | Self::VramSafetyMarginMb => "integer",
            Self::LoggingLevel => "log-directive",
        }
    }

    fn current(self, cfg: &AppConfig) -> String {
        match self {
            Self::ModelsDir => cfg.models.dir.display().to_string(),
            Self::ModelsBudgetGb => cfg.models.budget_gb.to_string(),
            Self::ModelsMinFreeGb => cfg.models.min_free_gb.to_string(),
            Self::LoggingLevel => cfg.logging.level.clone(),
            Self::VramSafetyMarginMb => cfg.resources.vram_safety_margin_mb.to_string(),
            Self::RuntimesDir => cfg.runtimes.dir.display().to_string(),
        }
    }
}

/// Apply a raw string value for `key` onto `cfg` (does not validate the whole
/// document — the caller does that after).
fn apply_kv(cfg: &mut AppConfig, key: ConfigKey, raw: &str) -> AppResult<()> {
    match key {
        ConfigKey::ModelsDir => cfg.models.dir = PathBuf::from(raw),
        ConfigKey::RuntimesDir => cfg.runtimes.dir = PathBuf::from(raw),
        ConfigKey::ModelsBudgetGb => {
            cfg.models.budget_gb = raw.trim().parse().map_err(|_| {
                AppError::Validation(format!("models.budget_gb must be an integer, got {raw:?}"))
            })?;
        }
        ConfigKey::ModelsMinFreeGb => {
            cfg.models.min_free_gb = raw.trim().parse().map_err(|_| {
                AppError::Validation(format!(
                    "models.min_free_gb must be an integer, got {raw:?}"
                ))
            })?;
        }
        ConfigKey::LoggingLevel => raw.trim().clone_into(&mut cfg.logging.level),
        ConfigKey::VramSafetyMarginMb => {
            cfg.resources.vram_safety_margin_mb = raw.trim().parse().map_err(|_| {
                AppError::Validation(format!(
                    "resources.vram_safety_margin_mb must be an integer, got {raw:?}"
                ))
            })?;
        }
    }
    Ok(())
}

/// Request body for [`crate::ipc::commands::config_set`].
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ConfigSet {
    /// Which key to change.
    pub key: ConfigKey,
    /// The new value, as a string (parsed per key).
    pub value: String,
    /// `true` writes it to the config file; `false` sets a session-only override.
    pub persist: bool,
}

/// One row of [`ConfigManager::keys`].
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ConfigKeyInfo {
    /// The key.
    pub key: ConfigKey,
    /// Dotted path (`models.dir`).
    pub dotted: String,
    /// A hint at the expected value form (`path`, `integer`).
    pub value_type: String,
    /// Current effective value, stringified.
    pub current: String,
}

// ---------------------------------------------------------------- manager

#[derive(Debug)]
struct State {
    /// defaults + persisted file values.
    base: AppConfig,
    session: SessionOverrides,
    recovered: bool,
}

/// Owns the effective configuration and its persistence. Held in Tauri managed
/// state; every subsystem reads config through this.
#[derive(Debug)]
pub struct ConfigManager {
    config_dir: PathBuf,
    state: RwLock<State>,
}

impl ConfigManager {
    /// Load configuration from `config_dir`, using `app_data_root` to anchor
    /// defaults. A missing file yields defaults (not written). An unparseable
    /// file is backed up and defaults are used. A parseable file that fails
    /// migration / validation returns `Err` (fail fast).
    ///
    /// # Errors
    /// Propagates [`AppError::Validation`] for an invalid config value, or
    /// [`AppError::Internal`] for an unexpected I/O failure.
    pub fn load(config_dir: &Path, app_data_root: &Path) -> AppResult<Self> {
        let path = config_dir.join(FILE_NAME);
        let (base, recovered) = match fs::read_to_string(&path) {
            Err(err) if err.kind() == ErrorKind::NotFound => {
                (AppConfig::defaults(app_data_root), false)
            }
            Err(err) => return Err(AppError::internal("read config file", err)),
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(raw) => {
                    let migrated = migrate(raw, app_data_root)?;
                    let cfg: AppConfig = serde_json::from_value(migrated).map_err(|err| {
                        AppError::Validation(format!("config file has the wrong shape: {err}"))
                    })?;
                    cfg.validate()?;
                    (cfg, false)
                }
                Err(parse_err) => {
                    Self::back_up_corrupt(config_dir, &text)?;
                    tracing::warn!(%parse_err, "config file is unparseable — starting from defaults");
                    (AppConfig::defaults(app_data_root), true)
                }
            },
        };

        Ok(Self {
            config_dir: config_dir.to_path_buf(),
            state: RwLock::new(State {
                base,
                session: SessionOverrides::default(),
                recovered,
            }),
        })
    }

    fn back_up_corrupt(config_dir: &Path, contents: &str) -> AppResult<()> {
        let stamp = OffsetDateTime::now_utc().unix_timestamp();
        let backup = config_dir.join(format!("{FILE_NAME}.corrupt-{stamp}"));
        fs::write(&backup, contents)
            .map_err(|err| AppError::internal("back up corrupt config", err))
    }

    /// Whether the last load recovered from a corrupt file.
    #[must_use]
    pub fn recovered(&self) -> bool {
        self.read().recovered
    }

    /// The effective configuration: base folded with session overrides.
    #[must_use]
    pub fn effective(&self) -> AppConfig {
        let state = self.read();
        let mut cfg = state.base.clone();
        state.session.apply(&mut cfg);
        cfg
    }

    /// Every overridable key with its current effective value.
    #[must_use]
    pub fn keys(&self) -> Vec<ConfigKeyInfo> {
        let effective = self.effective();
        ConfigKey::ALL
            .into_iter()
            .map(|key| ConfigKeyInfo {
                key,
                dotted: key.dotted().to_owned(),
                value_type: key.value_type().to_owned(),
                current: key.current(&effective),
            })
            .collect()
    }

    /// Set a persisted user value: validate, write the file atomically, then
    /// commit in memory.
    ///
    /// # Errors
    /// [`AppError::Validation`] if the value is invalid; [`AppError::Internal`]
    /// on a write failure (the in-memory config is left unchanged).
    pub fn set_user(&self, key: ConfigKey, raw: &str) -> AppResult<()> {
        let mut state = self.write();
        let mut next = state.base.clone();
        apply_kv(&mut next, key, raw)?;
        next.validate()?;
        self.write_atomic(&next)?;
        state.base = next;
        Ok(())
    }

    /// Set a session-only override: validated against the current effective
    /// config, kept in memory, never written.
    ///
    /// # Errors
    /// [`AppError::Validation`] if the resulting effective config is invalid.
    pub fn set_session(&self, key: ConfigKey, raw: &str) -> AppResult<()> {
        let mut state = self.write();
        let mut probe = state.base.clone();
        state.session.apply(&mut probe);
        apply_kv(&mut probe, key, raw)?;
        probe.validate()?;
        match key {
            ConfigKey::ModelsDir => state.session.models_dir = Some(PathBuf::from(raw)),
            ConfigKey::RuntimesDir => state.session.runtimes_dir = Some(PathBuf::from(raw)),
            ConfigKey::ModelsBudgetGb => {
                state.session.models_budget_gb = Some(probe.models.budget_gb);
            }
            ConfigKey::ModelsMinFreeGb => {
                state.session.models_min_free_gb = Some(probe.models.min_free_gb);
            }
            ConfigKey::LoggingLevel => {
                state.session.logging_level = Some(probe.logging.level.clone());
            }
            ConfigKey::VramSafetyMarginMb => {
                state.session.vram_safety_margin_mb = Some(probe.resources.vram_safety_margin_mb);
            }
        }
        Ok(())
    }

    /// Drop all session overrides.
    pub fn clear_session(&self) {
        self.write().session = SessionOverrides::default();
    }

    /// Whether any session override is active.
    #[must_use]
    pub fn has_session_overrides(&self) -> bool {
        !self.read().session.is_empty()
    }

    fn write_atomic(&self, cfg: &AppConfig) -> AppResult<()> {
        fs::create_dir_all(&self.config_dir)
            .map_err(|err| AppError::internal("create config dir", err))?;
        let json = serde_json::to_string_pretty(cfg)
            .map_err(|err| AppError::internal("serialize config", err))?;
        let tmp = self.config_dir.join(TMP_NAME);
        {
            let mut file = fs::File::create(&tmp)
                .map_err(|err| AppError::internal("create temp config", err))?;
            file.write_all(json.as_bytes())
                .map_err(|err| AppError::internal("write temp config", err))?;
            file.sync_all()
                .map_err(|err| AppError::internal("fsync temp config", err))?;
        }
        fs::rename(&tmp, self.config_dir.join(FILE_NAME))
            .map_err(|err| AppError::internal("commit config", err))?;
        Ok(())
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, State> {
        self.state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, State> {
        self.state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
