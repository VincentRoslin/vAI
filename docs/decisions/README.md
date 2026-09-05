# Architecture Decision Records

One ratified decision per file: `NNNN-title.md`. Format: Context · Options
considered · Decision · Consequences.

**Status lifecycle:** `PROPOSED` (Phase 3 draft) → `ACCEPTED` (ratified at Phase
5) → superseded by a later ADR if it changes. `UNDECIDED` marks a decision that
Phase 5 could not close, with what's blocking it.

Do not silently change an accepted decision through implementation. If
implementation shows an ADR is wrong: stop → explain → propose → get approval →
new/updated ADR → implement (`CLAUDE.md`).

**ADR-0001…0015 `ACCEPTED` at the Phase 5 freeze (2026-09-05)**; Phase 4 attacks
folded in (0006, 0007, 0008, 0009, 0010, 0013 revised; 0015 added). **ADR-0016
added at Phase 8** to close a question Phase 3 deferred (config format).

| ADR | Title | Status |
| --- | ----- | ------ |
| 0001 | Single Rust crate with module boundaries | ACCEPTED |
| 0002 | IPC: Channels, Events, one AppError, ts-rs, 3-tab shell | ACCEPTED |
| 0003 | LLM runtime: `llama-server` as a supervised child (loopback) | ACCEPTED |
| 0004 | Build llama.cpp from source, pinned, sm_120 | ACCEPTED |
| 0005 | Voice: faster-whisper fp16 / Silero / Chatterbox / cpal | ACCEPTED |
| 0006 | Image subsystem: diffusers sidecar, Krea 2 Turbo NF4 (cache required) | ACCEPTED |
| 0007 | Resource manager: nvml whole-GPU + ledger + RAM watch + TDR path | ACCEPTED |
| 0008 | Model acquisition: reqwest transfer + downloads table + NF4 quant at acquisition | ACCEPTED (transfer client amended `hf-hub`→`reqwest` at Phase 12, 2026-09-06) |
| 0009 | Persistence: rusqlite + dedicated writer + refinery + blob store | ACCEPTED |
| 0010 | Scheduler: priority queue, lock-ordering rule, restore-on-crash | ACCEPTED |
| 0011 | Character identity: prompt-based (sheet + canonical caption + seed + gate) | ACCEPTED |
| 0012 | Memory FTS5 + discrete relationship stages | ACCEPTED |
| 0013 | Transport: named-pipe/token'd loopback for servers, stdio for workers | ACCEPTED |
| 0014 | Packaging: MSI, embedded CPython + shared venv | ACCEPTED |
| 0015 | Python worker network lockdown | ACCEPTED |
| 0016 | Configuration: one JSON file, layered, versioned, Rust-owned | ACCEPTED (Phase 8) |
| 0017 | Entity ID generation: UUIDv4 | ACCEPTED (Phase 11) |

### Decided during implementation (no ADR — implements frozen policy)

- **Config format** → ADR-0016 (Phase 8).
- **Entity ID generation** → ADR-0017 (Phase 11).
- **Log retention / rotation + persistent file sink + diagnostics bundle** →
  **Phase 37** (packaging), when a windowed build with no terminal makes a file
  sink necessary. Phase 10 ships structured logging + redaction + an in-memory
  ring buffer; the file/rotation/bundle is a small addition on top.

### Deferred / to decide during implementation (not blocking the freeze)

- **NVFP4 vs NF4 for Krea 2** — benchmark at Phase 22; NF4 is the v1 baseline.
- **`synchronous=FULL` on the DB writer** — perf vs durability; decide at Phase 9
  with measurements.
- **Named pipe vs token'd TCP for `llama-server`** — confirm upstream supports a
  pipe / `--api-key` at Phase 15; named pipe preferred.
- **npm vs pnpm** — Phase 6.
- **At-rest encryption** for the DB + blob store — a later phase, if shared-machine
  use is ever in scope.
