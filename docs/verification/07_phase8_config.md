# 07 — Phase 8: Configuration

**Date:** 2026-09-06
**Branch:** `main` (local; no remote).
**Method:** new `src-tauri/src/config/` module + `config_*` IPC commands + startup
wiring; 22 unit tests over the gate; a real `tauri dev` launch with the config
load observed in the structured log.

Governing: **ADR-0016** (config format), ADR-0009 (app-data root).
Plan: `docs/plan/08_configuration.md`.

---

## What landed

- `config/mod.rs` — `AppConfig { version, models: { dir, budget_gb } }`,
  `defaults(app_data_root)`, `validate()`, forward migration runner
  (`STEPS`, `step_0_to_1`, `deep_merge`), `ConfigManager` (load / effective /
  keys / `set_user` / `set_session` / `clear_session`, atomic `write_atomic`),
  `ConfigKey` + `ConfigSet` + `ConfigKeyInfo` DTOs.
- `ipc/commands.rs` — `config_get`, `config_set`, `config_keys` (thin; call the
  manager).
- `lib.rs` — `setup()` resolves `app_config_dir` / `app_data_dir`, loads the
  manager, logs the effective config + load time, emits `config://recovered` on
  recovery, `app.manage(manager)`.
- Frontend — `configGet` / `configSet` / `configKeys` in `src/lib/ipc.ts`;
  re-exports in `src/lib/contracts.ts`; mocks in `src/test/setup.ts`.
- Docs — ADR-0016, `docs/decisions/README.md`, `ARCHITECTURE.md` §2,
  `src-tauri/README.md`, `DEVELOPMENT.md` §5.

---

## Gate — execution record

| # | Check | Result |
| - | ----- | ------ |
| 1 | Defaults load with no file present | **PASS** — `load_with_no_file_yields_defaults`: `effective() == defaults`, `recovered == false`, no `config.json` written |
| 2 | Invalid value → boot fails with a named error naming the key | **PASS** — `load_with_an_invalid_value_fails_fast`: a file with `models.dir = "relative/path"` → `Err(AppError::Validation("… models.dir …"))`. Startup propagates it (`lib.rs` `?` in `setup()`). |
| 3 | Out-of-range value rejected by schema validation | **PASS** — `validation_rejects_zero_budget` (names `models.budget_gb`), `validation_rejects_relative_model_dir` (names `models.dir`), `validation_rejects_future_schema_version`, `apply_kv_rejects_non_integer_budget` |
| 4 | A setting change persists across an app restart | **PASS** — `set_user_persists_across_a_reload`: `set_user` two keys → fresh `ConfigManager::load` from the same dir returns the new values; no `.tmp` left |
| 5 | Schema migration upgrades an old config file | **PASS** — `migration_upgrades_a_versionless_file` (no `version` → `version: 1`, keeps values), `migration_fills_missing_sections_from_defaults`, `migration_rejects_a_newer_version` (`{"version":99}` → `Err`) |
| 6 | Session override takes effect and does not persist | **PASS** — `session_override_takes_effect_and_does_not_persist`: `set_session` changes `effective()`, the `config.json` bytes are unchanged, a fresh `load` does not see it. Plus `clear_session_restores_the_base`. |
| 7 | Corrupted file → defaults + backup + warn | **PASS** — `load_with_corrupt_file_recovers_and_backs_up`: garbage bytes → `effective() == defaults`, `recovered == true`, exactly one `config.json.corrupt-<unix>` file. `tracing::warn!` emitted. `load_with_wrong_shape_fails_fast` confirms a *type* error is treated as fail-fast, not recovery. |
| 8 | `git grep` — no direct env / ad-hoc settings access outside the config module | **PASS** — `rg 'std::env::var|env::var|dotenv'` in `src-tauri/src` → nothing. The only env read is `LOCALAI_LOG` in `logging.rs` (`EnvFilter::try_from_env`), the documented exception: the log filter is read before config can be loaded. |
| 9 | Full check suite green; `src/bindings` regenerated + committed | **PASS** — `node scripts/check.mjs` all green; 5 new bindings (`AppConfig`, `ModelsConfig`, `ConfigKey`, `ConfigSet`, `ConfigKeyInfo`). |

`cargo test`: **88 Rust tests** (66 contracts/ipc + 22 config). `vitest`: 5.

---

## Startup baseline (for Phase 31)

| Metric | Value | Notes |
| ------ | ----- | ----- |
| Config load (read + parse + migrate + validate) | **~0.08 ms** | dev log `load_ms: 0.0794`, no file present (defaults path); a small real file is the same order |
| Core start → first frontend IPC round-trip | ~0.5 s | `22:35:25.048` → `25.541` (debug build, Vite warm) — unchanged from Phase 6 |

Config load is ~0.08 ms — three orders of magnitude under the 1 ms target;
negligible on the startup path.

## Decisions taken at phase entry

- **JSON** config file (ADR-0016) — `serde_json` already in tree, matches the IPC
  layer, frontend edits via IPC not by hand.
- **Two failure paths, kept distinct:** unparseable JSON → recover to defaults +
  backup + warn; parseable but wrong value/shape → fail fast with a named error.
- **Minimal schema** — `version` + `models.{dir, budget_gb}`. `vram_safety_margin_mb`
  and friends are added by the phase that consumes them (Article IV).
- `tempfile` added as a **dev-dependency** (atomic-write / recovery tests).
- `#[allow(clippy::needless_pass_by_value)]` per config command — Tauri injects
  `State` by value; expected.

**Phase 8 complete.** Pointer → Phase 9 (SQLite Persistence).
