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

## ⚠️ Constitution collision surfaced by this research (topic 02 + 03)

`CLAUDE.md` **Article I** currently says:

> Python workers … **communicate exclusively via JSON-lines … over stdin/stdout**.
> … Workers do not open sockets, files, or database connections.

`llama-server` is an HTTP server on a localhost socket. Supervising it with
`tokio::process` and consuming its streaming HTTP endpoint (topic 02) **does not
fit the "worker" definition** as written.

**Recommended resolution** (needs owner approval — see
`docs/research` memory / `project-constitution-guide-tensions`):

Introduce two distinct categories in the Constitution:

| Category | Examples | Transport | Rationale |
| -------- | -------- | --------- | --------- |
| **Managed model backend** | `llama-server` (and any future HTTP inference server) | Rust-supervised child process; **localhost HTTP**, bound to `127.0.0.1` on an ephemeral port, no external interface | These ship as HTTP servers upstream; reimplementing their protocol over stdio is wasted effort and a maintenance liability |
| **Stateless worker** | STT, TTS, image generation (Python) | Rust-supervised child process; **JSON-lines over stdin/stdout only** | We own these scripts; stdio keeps them trivially sandboxable and killable |

Both remain: (a) spawned and supervised only by the Rust core, (b) never holders
of authoritative state, (c) never direct SQLite clients, (d) bound to the resource
manager for any GPU use. The only thing that changes is that a *managed model
backend* is allowed a localhost HTTP socket.

**Until the owner rules on this**, topic 02 documents the HTTP approach as the
*likely* path but also notes the stdio-only alternative.
