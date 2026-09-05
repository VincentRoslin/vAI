# Phase 8 — Configuration

> **Architecture frozen at Phase 5.** Step detail **finalized at phase entry, 2026-09-06**. Governing: **ADR-0009** (app-data root), **ADR-0016** (config format — JSON, layered, versioned; written this phase to close the plan's open question), `SECURITY.md` §path-confinement, `CLAUDE.md` Article I (Rust owns config + filesystem).

## Objective
One typed, validated, versioned configuration system: defaults → user config file
→ session overrides → effective config. Every subsystem reads config through it.
No subsystem invents its own settings storage. No secrets in source or in the
config file.

## Depends on
Phase 6 (Tauri builder, `AppError`), Phase 7 (`AppError` taxonomy, contract style).

## Not in this phase
- Feature-specific settings UI (each feature phase adds its own).
- Secret storage mechanism — HF token etc. go to the OS credential store at their
  own phase (ADR-0008 / `SECURITY.md`); Phase 8 only guarantees "never in the
  config file, never in logs".
- A large schema. The schema starts at **`version` + `models.{dir, budget_gb}`** —
  the smallest set with a near-term consumer (Phase 12). Later phases extend it
  additively (`vram_safety_margin_mb` → Phase 13, etc.).

## Architecture notes
- Rust owns config (Article I). The frontend reads/writes only via typed IPC.
- File: `<app_config_dir>/config.json` (`%APPDATA%\com.localai.app\config.json`
  on Windows), resolved from the Tauri `PathResolver` in `setup()`. The loader
  itself takes a directory argument so it is testable without Tauri.
- Layering: `defaults` ← `file` (persisted user values) ← `session` (in-memory,
  never written). `effective()` folds them.
- **Corrupted (unparseable) file** → back it up to `config.json.corrupt-<rfc3339>`,
  start from defaults, emit a `tracing::warn!` + a `config://recovered` event.
  Never a silent wipe.
- **Parseable but invalid value** (out of range, bad path) → **fail fast at boot**
  with `AppError::Validation` naming the key. This is a deliberate different path
  from "corrupted".
- Schema version: `CURRENT_SCHEMA_VERSION = 1`. On load, read `version` from the
  raw JSON (absent ⇒ `0`); run ordered migration steps `v → v+1` up to current;
  `version` newer than current ⇒ `AppError::Validation`. Migration operates on
  `serde_json::Value`, then the result is deserialized + validated.
- Atomic write: serialize → write `config.json.tmp` → `fsync` → rename over
  `config.json`.
- Path confinement: `models.dir` must be absolute; the default is under the app
  data root. (Full traversal/confinement enforcement is `db`/blob-store scope;
  here we only reject a clearly-wrong value.)

## Performance notes
- Config load is on the startup path. Target: parse + validate + migrate <1 ms for
  a normal file. Record the number in `docs/verification/07_phase8_config.md` and
  the `PERFORMANCE.md` startup breakdown.

## Steps (atomic; each independently verifiable)

**8.1 — ADR-0016**
    Do:     Write `docs/decisions/0016-configuration.md` (JSON, `<app_config_dir>`,
            layered defaults/file/session, schema `version` + forward migrations,
            atomic write, secrets excluded). Add to `docs/decisions/README.md`.
    Verify: file present; README table row added.

**8.2 — Schema + defaults**
    Do:     `config/mod.rs` — `AppConfig { version: u32, models: ModelsConfig }`,
            `ModelsConfig { dir: PathBuf, budget_gb: u32 }`. `AppConfig::defaults(
            app_data_root: &Path)` (models.dir = `<root>/models`, budget_gb = 100).
            `Serialize + Deserialize`; **not** a `contracts::` type (config is not a
            wire contract — but the *effective* view is returned over IPC, so it
            derives `TS` + exports a binding).
    Verify: `cargo test config::defaults_are_valid` — `defaults(...).validate()` is
            `Ok`; `version == CURRENT_SCHEMA_VERSION`.

**8.3 — Validation**
    Do:     `AppConfig::validate(&self) -> AppResult<()>` — `budget_gb >= 1`;
            `models.dir` absolute + non-empty. Errors are `AppError::Validation`
            with the dotted key in the message.
    Verify: `cargo test config::validation_*` — `budget_gb = 0` rejected naming
            `models.budget_gb`; relative `models.dir` rejected naming `models.dir`.

**8.4 — Migration runner**
    Do:     `migrate(raw: Value) -> AppResult<Value>` — detect version (absent ⇒ 0),
            apply `step_0_to_1` (ensure `version`, fill missing `models.*` with
            defaults), loop to `CURRENT_SCHEMA_VERSION`; reject a newer version.
    Verify: `cargo test config::migration_*` — a `{"models":{...}}` file with no
            `version` upgrades to `version: 1` keeping its values; `{"version": 99}`
            is rejected.

**8.5 — Loader + corrupt recovery**
    Do:     `ConfigManager::load(config_dir: &Path, app_data_root: &Path)` —
            no file ⇒ defaults (not written yet); unparseable ⇒ back up +
            defaults + `warn!` + set a `recovered` flag; parseable ⇒ migrate →
            deserialize → `validate()` (Err propagates = fail fast).
    Verify: `cargo test config::load_*` — missing file ⇒ defaults; garbage bytes ⇒
            defaults + a `config.json.corrupt-*` file exists + `recovered` true;
            invalid value ⇒ `Err(Validation)`.

**8.6 — Persistence (atomic)**
    Do:     `ConfigManager::save(&self)` — serialize base config, temp-write,
            fsync, rename. `set_user(key, value)` mutates base + saves.
    Verify: `cargo test config::persistence_*` — `set_user` then a fresh `load`
            from the same dir returns the new value; a `.tmp` file never remains.

**8.7 — Session overrides**
    Do:     `SessionOverrides` (sparse: `Option` per leaf); `set_session(key,
            value)` mutates it only; `effective()` = base folded with session.
    Verify: `cargo test config::session_*` — `set_session` changes `effective()`;
            a fresh `load` from the same dir does **not** see it; `.json` unchanged.

**8.8 — Config keys + typed IPC**
    Do:     `ConfigKey` enum (`ModelsDir`, `ModelsBudgetGb`) with parse/format +
            an `overridable()` list. Commands in `ipc/commands.rs`:
            `config_get() -> AppConfig` (effective),
            `config_set(ConfigSet { key: ConfigKey, value: String, persist: bool })`,
            `config_keys() -> Vec<ConfigKeyInfo>` (key, value type, current value).
            Register in `lib.rs`; `ConfigManager` in `.manage()`.
            `src/lib/ipc.ts` wrappers + `src/lib/contracts.ts` re-export.
    Verify: `cargo test` for command logic; `npx tsc --noEmit`; bindings regenerated
            + committed.

**8.9 — Wire a consumer + startup wiring**
    Do:     `setup()` builds the `ConfigManager` from the resolved dirs, logs the
            effective `models.dir` at startup (the first real read), emits
            `config://recovered` if recovery happened. Record load time.
    Verify: `npm run tauri dev` — structured log shows the effective config load +
            timing, no errors; `docs/verification/07_phase8_config.md` records it.

**8.10 — Gate 8 grep + docs**
    Do:     `git grep` for `std::env::var` / ad-hoc settings reads outside
            `config/` and the logging bootstrap; document the one allow-listed
            exception (`LOCALAI_LOG`, read before config can exist). Update
            `src-tauri/README.md`, `ARCHITECTURE.md` §2/§3, `DEVELOPMENT.md`
            (config file location + reset), `PERFORMANCE.md` startup breakdown.
    Verify: grep clean but for the documented exception; indexes updated.

**8.11 — Gate run + commit**
    Do:     `node scripts/check.mjs`; write `docs/verification/07_phase8_config.md`;
            update `ROADMAP.md` §1 + §4 + §5; commit.
    Verify: check suite green; verification doc records every gate check.

## Verification gate
1. Defaults load with no file present. *(8.5)*
2. Invalid value → boot fails with a named error naming the key. *(8.3, 8.5)*
3. Out-of-range value rejected by schema validation. *(8.3)*
4. A setting change persists across an app restart (fresh `load`). *(8.6)*
5. Schema migration upgrades an old config file. *(8.4)*
6. Session override takes effect and does not persist. *(8.7)*
7. Corrupted file → app starts on defaults, backs up the bad file, warns. *(8.5)*
8. `git grep` for direct env / ad-hoc settings access outside the config module
   returns nothing (bar the documented `LOCALAI_LOG` bootstrap knob). *(8.10)*
9. Full check suite green; `src/bindings` regenerated + committed. *(8.11)*

## ADRs / open questions
- **ADR-0016** (this phase) closes "config file format" — JSON.
- Later schema growth (`vram_safety_margin_mb`, model defaults, UI prefs) is
  additive and owned by the phase that needs the key.
