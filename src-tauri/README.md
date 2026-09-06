# src-tauri/ — Rust application core

The authoritative nucleus (`CLAUDE.md` Article I, `ARCHITECTURE.md` §2). Owns
application state, persistence, model lifecycle, resource management, scheduling,
process supervision, and IPC. Single crate, one module per subsystem
(`docs/decisions/0001-single-rust-crate.md`).

## Current modules (Phase 14)

| Module | What | Doc |
| ------ | ---- | --- |
| `lib.rs` | The Tauri builder + `run()`; `setup()` loads config + opens/migrates the DB + builds the registry + acquisition service + resource manager + lifecycle manager into managed state (reconciles interrupted downloads; first hardware measurement + observe loop; liveness loop); exit hook checkpoints the WAL | — |
| `main.rs` | Thin bin entry | — |
| `logging/` | Observability — JSON stdout via a non-blocking lossy writer; boundary **secret redaction**; in-memory ring buffer (`recent_lines`); hot-reloadable filter (`set_level` from config / `LOCALAI_LOG`); `operation()` span helper. No network sink. | `docs/plan/10_observability.md` |
| `ipc/` | Typed IPC boundary — `commands` (`app_*`, `frontend_log`, `config_*`), `error::{AppError, ErrorEnvelope}` | `docs/decisions/0002-ipc-design.md` |
| `contracts/` | The serializable vocabulary for **both** the IPC and worker boundaries — `ids`, `task`, `model`, `generation`, `conversation`, `resource`, `worker`. No behaviour. | `docs/contracts.md` |
| `config/` | The settings authority — one JSON file (`<app_config_dir>/config.json`), layered defaults ← file ← session, schema `version` + forward migrations, atomic write | `docs/decisions/0016-configuration.md` |
| `db/` | SQLite persistence — writer pool (1) + reader pool (4), pragma hook, `refinery` forward-only migrations (`migrations/`) with verified `VACUUM INTO` backup, `write`/`read` helpers, `error::DbError`, `AppMetaRepo` | `docs/decisions/0009-persistence.md` |
| `models/` | Model registry — `model_entry` rows (`V0002`), `ModelRegistry` CRUD + capability `query`, `ModelDraft`/`ModelFilter`, path confinement (`validate_model_path`), availability computed from `path.exists()`, `Arc`-cached list cleared on write. IDs are UUIDv4 (ADR-0017). | `docs/plan/11_model-registry.md` |
| `acquisition/` | Model acquisition — `gguf` (header parser), `hf` (HF API + range fetch), `budget` (pre-transfer guard), `download` (`reqwest` engine: `.part` + `Range` resume + SHA-256 verify + register), `AcquisitionService`, `acquire_fixed`. `model_downloads` (`V0003`). **The only runtime network egress.** | `docs/plan/12_model-acquisition.md`, ADR-0008 |
| `resources/` | Resource manager — `probe` (`HardwareProbe`: `NvmlProbe` whole-GPU + `sysinfo` RAM, `MockProbe`), `estimate` (closed-form LLM VRAM + per-model EMA `Calibration`), `ResourceManager` (in-memory reservation ledger; `request`/`commit`/`release`/`observe`/`reconcile` behind one async `Mutex`; `request` works off a cached snapshot, never the probe). Accounts only — no loading. `resources.vram_safety_margin_mb` (config v4). | `docs/plan/13_resource-manager.md`, ADR-0007 |
| `lifecycle/` | Model lifecycle manager — **the only loader/unloader of managed models**. `backend` (`ModelBackend` / `LoadedInstance` traits; llama.cpp is Phase 15), `LifecycleManager` state machine (`Unloaded → Loading → Loaded ⇄ Busy → Unloading`, `Failed` recovery) behind one async `Mutex`; concurrent same-model loads coalesce; a Phase 13 reservation is acquired before the load and released on every exit path; `RetryPolicy` (3 attempts, 500 ms base); ~2 s liveness monitor → `Failed` + release. `lifecycle_status` IPC. | `docs/plan/14_model-lifecycle.md`, ADR-0007/0010 |

## Modules added by later phases

blob store (with the first blob feature) · `llm` adapter (P15) ·
`conversation` engine (P17) · `voice` (P18–19) · `context` builder (P20) ·
`memory` (P21) · `image` (P22) · `scheduler` (P23–24) · `characters` (P25–29).
Each phase registers its module here and in `ARCHITECTURE.md` §3.

## Notes

- `crate-type` includes `cdylib`/`staticlib` for Tauri's mobile targets — leave as
  the scaffold set it.
- `[lints]` in `Cargo.toml` denies warnings + runs clippy `all` + `pedantic`.
- `cargo test` runs the `export_bindings` test which regenerates `../src/bindings/`.
