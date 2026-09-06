//! Phase 8 gate coverage: defaults, validation, migration, load + corrupt
//! recovery, atomic persistence, session overrides.

use std::fs;
use std::path::PathBuf;

use serde_json::json;
use tempfile::tempdir;

use super::{apply_kv, migrate, AppConfig, ConfigKey, ConfigManager, CURRENT_SCHEMA_VERSION};
use crate::ipc::AppError;

fn root() -> PathBuf {
    // An absolute path that exists on every platform the tests run on.
    if cfg!(windows) {
        PathBuf::from(r"C:\localai-test")
    } else {
        PathBuf::from("/localai-test")
    }
}

// ---------------------------------------------------------------- defaults

#[test]
fn defaults_are_valid_and_current() {
    let cfg = AppConfig::defaults(&root());
    cfg.validate().expect("defaults valid");
    assert_eq!(cfg.version, CURRENT_SCHEMA_VERSION);
    assert_eq!(cfg.models.dir, root().join("models"));
    assert!(cfg.models.budget_gb >= 1);
}

// ---------------------------------------------------------------- validation

#[test]
fn validation_rejects_zero_budget() {
    let mut cfg = AppConfig::defaults(&root());
    cfg.models.budget_gb = 0;
    let err = cfg.validate().unwrap_err();
    assert!(matches!(&err, AppError::Validation(m) if m.contains("models.budget_gb")));
}

#[test]
fn validation_rejects_relative_model_dir() {
    let mut cfg = AppConfig::defaults(&root());
    cfg.models.dir = PathBuf::from("models");
    let err = cfg.validate().unwrap_err();
    assert!(matches!(&err, AppError::Validation(m) if m.contains("models.dir")));
}

#[test]
fn validation_rejects_future_schema_version() {
    let mut cfg = AppConfig::defaults(&root());
    cfg.version = CURRENT_SCHEMA_VERSION + 1;
    assert!(cfg.validate().is_err());
}

#[test]
fn apply_kv_rejects_non_integer_budget() {
    let mut cfg = AppConfig::defaults(&root());
    let err = apply_kv(&mut cfg, ConfigKey::ModelsBudgetGb, "lots").unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

#[test]
fn validation_rejects_a_bad_logging_level() {
    let mut cfg = AppConfig::defaults(&root());
    cfg.logging.level = "extremely-verbose".to_owned();
    let err = cfg.validate().unwrap_err();
    assert!(matches!(&err, AppError::Validation(m) if m.contains("logging.level")));
}

#[test]
fn apply_kv_sets_logging_level() {
    let mut cfg = AppConfig::defaults(&root());
    apply_kv(&mut cfg, ConfigKey::LoggingLevel, " warn ").unwrap();
    assert_eq!(cfg.logging.level, "warn");
    cfg.validate().expect("valid");
}

// ---------------------------------------------------------------- migration

#[test]
fn migration_upgrades_a_versionless_file() {
    let raw = json!({ "models": { "budget_gb": 42 } });
    let migrated = migrate(raw, &root()).expect("migrates");
    assert_eq!(migrated["version"], json!(CURRENT_SCHEMA_VERSION));
    assert_eq!(migrated["models"]["budget_gb"], json!(42));
    // A field the old file omitted is filled from defaults.
    assert!(migrated["models"]["dir"].is_string());
}

#[test]
fn migration_forward_fills_new_sections_from_defaults() {
    // A complete v1 document — no `logging`, no `models.min_free_gb`.
    let v1 = json!({
        "version": 1,
        "models": { "dir": if cfg!(windows) { r"C:\m" } else { "/m" }, "budget_gb": 7 },
    });
    let migrated = migrate(v1, &root()).expect("migrates");
    assert_eq!(migrated["version"], json!(CURRENT_SCHEMA_VERSION));
    assert_eq!(migrated["models"]["budget_gb"], json!(7)); // kept
    assert_eq!(migrated["logging"]["level"], json!("info")); // v2 default
    assert_eq!(migrated["models"]["min_free_gb"], json!(20)); // v3 default

    let cfg: AppConfig = serde_json::from_value(migrated).expect("deserializes");
    cfg.validate().expect("valid after migration");
}

#[test]
fn migration_v2_forward_adds_min_free_gb_and_resources() {
    let v2 = json!({
        "version": 2,
        "models": { "dir": if cfg!(windows) { r"C:\m" } else { "/m" }, "budget_gb": 50 },
        "logging": { "level": "warn" },
    });
    let migrated = migrate(v2, &root()).expect("migrates");
    assert_eq!(migrated["version"], json!(CURRENT_SCHEMA_VERSION));
    assert_eq!(migrated["models"]["min_free_gb"], json!(20)); // v3 default
    assert_eq!(migrated["resources"]["vram_safety_margin_mb"], json!(1500)); // v4 default
    assert_eq!(migrated["logging"]["level"], json!("warn")); // kept
}

#[test]
fn migration_v3_forward_adds_resources_and_runtimes() {
    let v3 = json!({
        "version": 3,
        "models": {
            "dir": if cfg!(windows) { r"C:\m" } else { "/m" },
            "budget_gb": 50,
            "min_free_gb": 15,
        },
        "logging": { "level": "warn" },
    });
    let migrated = migrate(v3, &root()).expect("migrates");
    assert_eq!(migrated["version"], json!(CURRENT_SCHEMA_VERSION));
    assert_eq!(migrated["resources"]["vram_safety_margin_mb"], json!(1500)); // v4
    assert!(migrated["runtimes"]["dir"].is_string()); // v5
    assert_eq!(migrated["models"]["min_free_gb"], json!(15)); // kept

    let cfg: AppConfig = serde_json::from_value(migrated).expect("deserializes");
    cfg.validate().expect("valid after migration");
}

#[test]
fn migration_v4_forward_adds_runtimes_workers_and_voice() {
    let v4 = json!({
        "version": 4,
        "models": { "dir": if cfg!(windows) { r"C:\m" } else { "/m" }, "budget_gb": 50, "min_free_gb": 20 },
        "logging": { "level": "info" },
        "resources": { "vram_safety_margin_mb": 800 },
    });
    let migrated = migrate(v4, &root()).expect("migrates");
    assert_eq!(migrated["version"], json!(CURRENT_SCHEMA_VERSION));
    assert_eq!(
        migrated["runtimes"]["dir"],
        json!(root().join("runtimes").display().to_string()) // v5
    );
    assert!(migrated["workers"]["dir"].is_string()); // v6
    assert!(migrated["workers"]["python"].is_string()); // v6
    assert_eq!(migrated["voice"]["input_device"], json!(null)); // v6
    assert_eq!(migrated["resources"]["vram_safety_margin_mb"], json!(800)); // kept

    let cfg: AppConfig = serde_json::from_value(migrated).expect("deserializes");
    cfg.validate().expect("valid after migration");
}

#[test]
fn migration_v5_to_v6_keeps_runtimes_dir() {
    let v5 = json!({
        "version": 5,
        "models": { "dir": if cfg!(windows) { r"C:\m" } else { "/m" }, "budget_gb": 50, "min_free_gb": 20 },
        "logging": { "level": "info" },
        "resources": { "vram_safety_margin_mb": 800 },
        "runtimes": { "dir": if cfg!(windows) { r"C:\rt" } else { "/rt" } },
    });
    let migrated = migrate(v5, &root()).expect("migrates");
    assert_eq!(migrated["version"], json!(6));
    assert_eq!(
        migrated["runtimes"]["dir"],
        json!(if cfg!(windows) { r"C:\rt" } else { "/rt" })
    );
    assert!(migrated["workers"]["python"].is_string());
}

#[test]
fn apply_kv_sets_runtimes_dir() {
    let mut cfg = AppConfig::defaults(&root());
    let p = if cfg!(windows) { r"C:\rt" } else { "/rt" };
    apply_kv(&mut cfg, ConfigKey::RuntimesDir, p).unwrap();
    assert_eq!(cfg.runtimes.dir, PathBuf::from(p));
    cfg.validate().expect("valid");

    apply_kv(&mut cfg, ConfigKey::RuntimesDir, "relative/rt").unwrap();
    assert!(cfg.validate().is_err());
}

#[test]
fn validation_rejects_an_absurd_vram_margin() {
    let mut cfg = AppConfig::defaults(&root());
    cfg.resources.vram_safety_margin_mb = 200_000;
    let err = cfg.validate().unwrap_err();
    assert!(matches!(&err, AppError::Validation(m) if m.contains("vram_safety_margin_mb")));
}

#[test]
fn apply_kv_sets_vram_margin() {
    let mut cfg = AppConfig::defaults(&root());
    apply_kv(&mut cfg, ConfigKey::VramSafetyMarginMb, " 2048 ").unwrap();
    assert_eq!(cfg.resources.vram_safety_margin_mb, 2048);
    cfg.validate().expect("valid");
}

#[test]
fn migration_fills_missing_sections_from_defaults() {
    let migrated = migrate(json!({}), &root()).expect("migrates");
    let cfg: AppConfig = serde_json::from_value(migrated).expect("deserializes");
    assert_eq!(cfg, AppConfig::defaults(&root()));
}

#[test]
fn migration_rejects_a_newer_version() {
    let err = migrate(json!({ "version": 99 }), &root()).unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}

// ---------------------------------------------------------------- load

#[test]
fn load_with_no_file_yields_defaults() {
    let dir = tempdir().unwrap();
    let mgr = ConfigManager::load(dir.path(), dir.path()).expect("loads");
    assert_eq!(mgr.effective(), AppConfig::defaults(dir.path()));
    assert!(!mgr.recovered());
    // Defaults are not written until something changes.
    assert!(!dir.path().join("config.json").exists());
}

#[test]
fn load_with_corrupt_file_recovers_and_backs_up() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("config.json"), b"{ this is not json").unwrap();

    let mgr = ConfigManager::load(dir.path(), dir.path()).expect("recovers");
    assert!(mgr.recovered());
    assert_eq!(mgr.effective(), AppConfig::defaults(dir.path()));

    let backups: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("config.json.corrupt-")
        })
        .collect();
    assert_eq!(backups.len(), 1, "exactly one corrupt backup");
}

#[test]
fn load_with_an_invalid_value_fails_fast() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("config.json"),
        json!({ "version": 1, "models": { "dir": "relative/path", "budget_gb": 10 } }).to_string(),
    )
    .unwrap();

    let err = ConfigManager::load(dir.path(), dir.path()).unwrap_err();
    assert!(matches!(&err, AppError::Validation(m) if m.contains("models.dir")));
}

#[test]
fn load_with_wrong_shape_fails_fast() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("config.json"),
        r#"{ "version": 1, "models": { "dir": 12, "budget_gb": "x" } }"#,
    )
    .unwrap();
    assert!(ConfigManager::load(dir.path(), dir.path()).is_err());
}

// ---------------------------------------------------------------- persistence

#[test]
fn set_user_persists_across_a_reload() {
    let dir = tempdir().unwrap();
    let new_dir = absolute("C:/models-2", "/models-2");

    {
        let mgr = ConfigManager::load(dir.path(), dir.path()).unwrap();
        mgr.set_user(ConfigKey::ModelsBudgetGb, "250").unwrap();
        mgr.set_user(ConfigKey::ModelsDir, new_dir.to_str().unwrap())
            .unwrap();
    }

    let reloaded = ConfigManager::load(dir.path(), dir.path()).unwrap();
    assert_eq!(reloaded.effective().models.budget_gb, 250);
    assert_eq!(reloaded.effective().models.dir, new_dir);
    assert!(!dir.path().join("config.json.tmp").exists(), "no temp left");
}

#[test]
fn set_user_rejects_an_invalid_value_and_keeps_the_old_one() {
    let dir = tempdir().unwrap();
    let mgr = ConfigManager::load(dir.path(), dir.path()).unwrap();
    let before = mgr.effective();

    assert!(mgr.set_user(ConfigKey::ModelsBudgetGb, "0").is_err());
    assert_eq!(mgr.effective(), before);
    assert!(!dir.path().join("config.json").exists());
}

// ---------------------------------------------------------------- session

#[test]
fn session_override_takes_effect_and_does_not_persist() {
    let dir = tempdir().unwrap();

    let mgr = ConfigManager::load(dir.path(), dir.path()).unwrap();
    mgr.set_user(ConfigKey::ModelsBudgetGb, "100").unwrap();
    let file_after_user = fs::read_to_string(dir.path().join("config.json")).unwrap();

    mgr.set_session(ConfigKey::ModelsBudgetGb, "9").unwrap();
    assert_eq!(mgr.effective().models.budget_gb, 9);
    assert!(mgr.has_session_overrides());

    // File untouched by the session override.
    assert_eq!(
        fs::read_to_string(dir.path().join("config.json")).unwrap(),
        file_after_user
    );

    // A fresh load does not see it.
    let reloaded = ConfigManager::load(dir.path(), dir.path()).unwrap();
    assert_eq!(reloaded.effective().models.budget_gb, 100);
}

#[test]
fn clear_session_restores_the_base() {
    let dir = tempdir().unwrap();
    let mgr = ConfigManager::load(dir.path(), dir.path()).unwrap();
    mgr.set_session(ConfigKey::ModelsBudgetGb, "7").unwrap();
    mgr.clear_session();
    assert!(!mgr.has_session_overrides());
    assert_eq!(
        mgr.effective().models.budget_gb,
        AppConfig::defaults(dir.path()).models.budget_gb
    );
}

#[test]
fn keys_lists_every_overridable_key_with_current_values() {
    let dir = tempdir().unwrap();
    let mgr = ConfigManager::load(dir.path(), dir.path()).unwrap();
    let keys = mgr.keys();
    assert_eq!(keys.len(), ConfigKey::ALL.len());
    let budget = keys
        .iter()
        .find(|k| k.dotted == "models.budget_gb")
        .expect("budget key listed");
    assert_eq!(budget.current, "100");
}

// ---------------------------------------------------------------- helpers

fn absolute(windows: &str, unix: &str) -> PathBuf {
    PathBuf::from(if cfg!(windows) { windows } else { unix })
}
