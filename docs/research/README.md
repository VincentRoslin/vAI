# Technical Research Notes

Pre-architecture research. **Not decisions.** These documents survey implementation
approaches, trade-offs, and risks so the architecture phase (and its ADRs) can be
made from an informed position. Nothing here is ratified; nothing here authorizes
writing code.

| # | Topic | File | Status |
| - | ----- | ---- | ------ |
| 01 | Tauri command bindings, serialization contracts, typed error propagation | [01_tauri_ipc.md](01_tauri_ipc.md) | draft |
| 02 | Rust supervision of `llama-server` (`tokio::process`, streaming, cancel, recovery) | [02_llama_server_supervision.md](02_llama_server_supervision.md) | draft |
| 03 | Python worker lifecycle — non-blocking stdin/stdout JSON-lines | [03_python_workers_jsonlines.md](03_python_workers_jsonlines.md) | draft |
| 04 | SQLite connection pooling + schema separation (metadata vs file paths) | [04_sqlite_pooling_schema.md](04_sqlite_pooling_schema.md) | draft |
| 05 | VRAM coordination to prevent OOM during model hot-swap | [05_vram_coordination.md](05_vram_coordination.md) | draft |
| 06 | Voice modality interruption flows | [06_voice_interruption.md](06_voice_interruption.md) | draft |

Written 2026-09-05. Author's knowledge cutoff is 2026-01; version numbers and API
shapes below should be re-checked against current crate docs before implementation.
Items marked **SPIKE** need a throwaway proof-of-concept, not more reading.

---

## Open architecture-research question: subprocess transport (topics 02 + 03)

`CLAUDE.md` Article I fixes the *principles* for AI subprocesses (Rust-supervised,
non-authoritative, isolated, no direct SQLite, no independent GPU management) but
**leaves transport open for Phase 3** (ROADMAP §7 O3). The candidates this research
should weigh:

| Candidate | Fits | Trade-off |
| --------- | ---- | --------- |
| **stdio JSON-lines** | workers we author (STT, TTS, image) | trivially sandboxed/killable; we own the protocol; no socket |
| **loopback HTTP** (`127.0.0.1`, no external interface) | upstream inference servers (`llama-server`) that ship as HTTP services | avoids reimplementing their protocol over stdio; adds a local socket + port management |
| **in-process FFI** (`llama-cpp-2`) | any backend, if a child process is unsafe | no socket; a backend crash/OOM can take down the app |

Topic 02 documents the loopback-HTTP path in most detail because it is the leading
candidate for `llama-server`, but it is **not decided**. No code may depend on a
specific transport until the Phase 3 ADR.
