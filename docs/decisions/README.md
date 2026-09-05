# Architecture Decision Records

One ratified decision per file: `NNNN-title.md`. Format: Context · Options
considered · Decision · Consequences.

**Status lifecycle:** `PROPOSED` (Phase 3 draft) → `ACCEPTED` (ratified at Phase
5) → superseded by a later ADR if it changes. `UNDECIDED` marks a decision that
Phase 5 could not close, with what's blocking it.

Do not silently change an accepted decision through implementation. If
implementation shows an ADR is wrong: stop → explain → propose → get approval →
new/updated ADR → implement (`CLAUDE.md`).

| ADR | Title | Status | Research |
| --- | ----- | ------ | -------- |
| 0001 | Single Rust crate with module boundaries | PROPOSED | `phase3/01` |
| 0002 | IPC design: Channels, Events, one AppError, ts-rs | PROPOSED | `phase3/01` |
| 0003 | LLM runtime: `llama-server` as a loopback-HTTP child | PROPOSED | `phase3/02` |
| 0004 | Build llama.cpp from source, pinned, sm_120 | PROPOSED | `phase3/02` |
| 0005 | Voice pipeline: faster-whisper fp16 / Silero / Chatterbox / cpal | PROPOSED | `phase3/03` |
| 0006 | Image subsystem: diffusers sidecar, Krea 2 Turbo NF4, LoRA+preset | PROPOSED | `phase3/04` |
| 0007 | Resource manager: nvml + reservation ledger | PROPOSED | `phase3/05` |
| 0008 | Model acquisition: hf-hub + downloads table | PROPOSED | `phase3/06` |
| 0009 | Persistence: rusqlite + dedicated writer + refinery + blob store | PROPOSED | `phase3/07` |
| 0010 | Scheduler: priority queue, interactive LLM never preempted | PROPOSED | `phase3/08` |
| 0011 | Character identity: prompt-based (reference sheet + canonical caption + seed + gate), no LoRA training | PROPOSED | `phase3/09` |
| 0012 | Memory FTS5 + discrete relationship stages | PROPOSED | `phase3/10` |
| 0013 | Subprocess transport: loopback HTTP for servers, stdio for workers | PROPOSED | `phase3/11` |
| 0014 | Packaging: MSI, embedded CPython + shared venv | PROPOSED | `phase3/12` |
| 0015 | Python worker network lockdown (offline + no-telemetry env) | PROPOSED | Phase 4 R-C2 |

Phase 4 (`docs/verification/03_adversarial_review.md`) revised ADRs
0006, 0007, 0008, 0009, 0010, 0013 and added 0015. All still `PROPOSED`;
ratified together at Phase 5.

Open for Phase 5 / owner: NVFP4-vs-NF4 for Krea 2 (benchmark at Phase 22);
`synchronous=FULL` on the DB writer (perf vs durability); named-pipe vs
token'd-TCP for `llama-server` (confirm upstream supports pipe / `--api-key`).
