# ADR-0016 — Configuration: one JSON file, layered, versioned, Rust-owned

- **Status:** ACCEPTED (decided at Phase 8 entry, 2026-09-06) · **Date:** 2026-09-06
- **Closes:** the "config file format" open question left by Phase 3
  (`docs/plan/08_configuration.md`).
- **Rules it obeys:** `CLAUDE.md` Article I (Rust owns config + filesystem),
  Article II (no secrets phone-home), Article IV (smallest correct), ADR-0009
  (app-data root), `SECURITY.md` (secrets → OS credential store, never config).

## Context
Every subsystem needs settings (model dir + disk budget now; VRAM margin,
retrieval budgets, UI prefs later). Article I: one config authority in Rust, the
frontend touches it only through typed IPC. Phase 3 deferred the file format.

## Options considered
- **Format:** TOML (hand-editable, comments) vs JSON (already in the dependency
  tree via `serde_json`; matches the IPC/contract layer) vs a Tauri store plugin
  (frontend-oriented, extra dependency, blurs Article I).
- **Layering:** single flat file vs defaults + file + session overrides.
- **Versioning:** none / `user_version`-style integer + forward migrations.
- **Session overrides:** persisted vs in-memory-only.

## Decision
- **JSON**, one file: `<app_config_dir>/config.json`
  (`%APPDATA%\com.localai.app\config.json` on Windows), the directory resolved
  from the Tauri `PathResolver`. Rationale: `serde_json` is already a dependency
  (no new crate), the whole IPC surface is already JSON, and the frontend edits
  config through typed IPC — not by hand-editing the file — so TOML's
  hand-edit/comment advantage buys nothing the design uses.
- **Three layers, folded to an effective config:** `defaults` (code) ← `file`
  (persisted user values) ← `session` (in-memory, **never written**).
- **Schema `version`** integer, `CURRENT_SCHEMA_VERSION` in code. On load: read
  `version` (absent ⇒ `0`), run ordered `v → v+1` migration steps on the raw
  `serde_json::Value` up to current, then deserialize + validate. A file whose
  `version` is **newer** than the binary supports is refused (`AppError::Validation`).
- **Two failure paths, deliberately different:**
  - *Unparseable* file ⇒ back up to `config.json.corrupt-<rfc3339>`, start from
    defaults, `tracing::warn!` + a `config://recovered` event. Never a silent wipe.
  - *Parseable but an invalid value* (out of range, non-absolute path) ⇒ **fail
    fast at boot** with `AppError::Validation` naming the dotted key.
- **Atomic write:** serialize → `config.json.tmp` → fsync → rename over the target.
- **No secrets in the file.** A HF token and anything like it goes to the OS
  credential store at its own phase (ADR-0008, `SECURITY.md`). The config module
  has no field that holds a secret.
- **Starting schema (Phase 8):** `version` + `models.{ dir, budget_gb }` — the
  smallest set with a near-term consumer (Phase 12). Growth is additive and owned
  by the phase that introduces each key.
- The loader takes a directory argument (not an `AppHandle`), so it is unit-tested
  without Tauri.

## Consequences
- No new runtime dependency. `tempfile` is added as a **dev-dependency** for the
  atomic-write / corrupt-recovery tests.
- `git grep` gate (Phase 8): no `std::env::var` / ad-hoc settings reads outside
  `config/`. The one allow-listed exception is `LOCALAI_LOG` in `logging.rs` —
  the log filter is read before config can be loaded (logging initializes first).
- The effective config is returned over IPC, so `AppConfig` derives `TS` and
  exports a binding even though it is not a `contracts::` wire type.
- Migrations are cheap to add: one function per version step, each covered by a
  test that upgrades a fixture.
