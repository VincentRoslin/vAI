# src-tauri/ — Rust application core

The authoritative nucleus (`CLAUDE.md` Article I, `ARCHITECTURE.md` §2). Owns
application state, persistence, model lifecycle, resource management, scheduling,
process supervision, and IPC. Single crate, one module per subsystem
(`docs/decisions/0001-single-rust-crate.md`).

## Current modules (Phase 6)

| Module | What | Doc |
| ------ | ---- | --- |
| `lib.rs` | The Tauri builder + `run()` | — |
| `main.rs` | Thin bin entry | — |
| `logging.rs` | `tracing` JSON logging; `LOCALAI_LOG` env filter; no network sink | observability → Phase 10 |
| `ipc/` | Typed IPC boundary — `commands`, `error::AppError`, events | `docs/decisions/0002-ipc-design.md` |

## Modules added by later phases

`config` (P8) · `db` + blob store (P9) · `models` registry (P11) · `acquisition`
(P12) · `resources` (P13) · `lifecycle` (P14) · `llm` adapter (P15) ·
`conversation` engine (P17) · `voice` (P18–19) · `context` builder (P20) ·
`memory` (P21) · `image` (P22) · `scheduler` (P23–24) · `characters` (P25–29).
Each phase registers its module here and in `ARCHITECTURE.md` §3.

## Notes

- `crate-type` includes `cdylib`/`staticlib` for Tauri's mobile targets — leave as
  the scaffold set it.
- `[lints]` in `Cargo.toml` denies warnings + runs clippy `all` + `pedantic`.
- `cargo test` runs the `export_bindings` test which regenerates `../src/bindings/`.
