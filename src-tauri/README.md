# src-tauri/ — Rust application core

The authoritative nucleus (`CLAUDE.md` Article I, `ARCHITECTURE.md` §2). Owns
application state, persistence, model lifecycle, resource management, scheduling,
process supervision, and IPC. Single crate, one module per subsystem
(`docs/decisions/0001-single-rust-crate.md`).

## Current modules (Phase 8)

| Module | What | Doc |
| ------ | ---- | --- |
| `lib.rs` | The Tauri builder + `run()`; `setup()` loads config into managed state | — |
| `main.rs` | Thin bin entry | — |
| `logging.rs` | `tracing` JSON logging; `LOCALAI_LOG` env filter (the one pre-config env read); no network sink | observability → Phase 10 |
| `ipc/` | Typed IPC boundary — `commands` (`app_*`, `frontend_log`, `config_*`), `error::{AppError, ErrorEnvelope}` | `docs/decisions/0002-ipc-design.md` |
| `contracts/` | The serializable vocabulary for **both** the IPC and worker boundaries — `ids`, `task`, `model`, `generation`, `conversation`, `resource`, `worker`. No behaviour. | `docs/contracts.md` |
| `config/` | The settings authority — one JSON file (`<app_config_dir>/config.json`), layered defaults ← file ← session, schema `version` + forward migrations, atomic write | `docs/decisions/0016-configuration.md` |

## Modules added by later phases

`db` + blob store (P9) · `models` registry (P11) · `acquisition`
(P12) · `resources` (P13) · `lifecycle` (P14) · `llm` adapter (P15) ·
`conversation` engine (P17) · `voice` (P18–19) · `context` builder (P20) ·
`memory` (P21) · `image` (P22) · `scheduler` (P23–24) · `characters` (P25–29).
Each phase registers its module here and in `ARCHITECTURE.md` §3.

## Notes

- `crate-type` includes `cdylib`/`staticlib` for Tauri's mobile targets — leave as
  the scaffold set it.
- `[lints]` in `Cargo.toml` denies warnings + runs clippy `all` + `pedantic`.
- `cargo test` runs the `export_bindings` test which regenerates `../src/bindings/`.
