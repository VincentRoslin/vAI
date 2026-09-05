# docs/verification — gate evidence

What was physically run/reviewed and observed, per `CLAUDE.md` Article IV. One
numbered file per verification event.

| # | File | Covers |
| - | ---- | ------ |
| 01 | `01_env_audit.md` | Phase 2 — dev machine (toolchains, GPU/VRAM, build tools, disk); Rust↔MSVC link chain |
| 02 | `02_phase3_probes.md` | Phase 3 — NVML per-process VRAM (unavailable), GGUF header parse (works), WDDM hang (inconclusive → Phase 15 watch) |
| 03 | `03_adversarial_review.md` | Phase 4 — assembled architecture + ~40-risk register + ADR changes |
| 04 | `04_phase5_crosscheck.md` | Phase 5.10 — binding docs ↔ ADRs ↔ plan ↔ ROADMAP consistency |
| 05 | `05_phase6_bootstrap.md` | Phase 6 — Tauri/React/Rust scaffold: check suite + app launch + IPC round-trip; startup baseline |
| 06 | `06_phase7_contracts.md` | Phase 7 — application contracts: `contracts/` module, `ts-rs` regen, round-trip + rejection sweep, `TokenDelta` perf probe |
| 07 | `07_phase8_config.md` | Phase 8 — configuration: `config/` module, layered defaults/file/session, migration, corrupt recovery, `config_*` IPC; startup baseline |
| 08 | `08_phase9_persistence.md` | Phase 9 — SQLite: `db/` module, writer/reader pools, `refinery` forward-only migrations + verified backup, transactions, `DbError`, corruption check; DB baselines |
| 09 | `09_phase10_observability.md` | Phase 10 — logging: non-blocking lossy writer, boundary secret redaction, ring buffer, config-driven reloadable level, `operation()` spans |

Later phases add their gate evidence here (the performance/offline/fault/security/
dependency/maintainability audits, etc.).
