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

Later phases add their gate evidence here (the performance/offline/fault/security/
dependency/maintainability audits, etc.).
