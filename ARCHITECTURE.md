# ARCHITECTURE.md — LocalAI

**Status:** Frozen at Phase 5, 2026-09-05. Binding.
Rationale for each choice is in `docs/decisions/` (ADR-0001…0015). Product it
realises: `PROJECT.md`. Rules it obeys: `CLAUDE.md` Articles I–IV.

Changes require the STOP → propose → approve → new/updated ADR loop.

---

## 1. System overview

```
┌─────────────────────────────────────────────────────────────────────┐
│ React / TypeScript frontend — PRESENTATION ONLY                      │
│ Tabs: Chat/Voice · Image Generator · Discovery · Models · Settings   │
│ Holds: view state + last server snapshot (invalidated by events)     │
└───────────────┬─────────────────────────────────────────────────────┘
                │  typed Tauri IPC (ADR-0002)
                │  Commands (req/resp) · Events (broadcast) · Channels (streams)
                │  cancel(taskId) → CancellationToken
┌───────────────▼─────────────────────────────────────────────────────┐
│ RUST CORE — single crate, module boundaries (ADR-0001)               │
│                                                                     │
│  ipc  ·  config  ·  observability (tracing)  ·  db  ·  blob store    │
│  conversation engine  (ONE — chat / voice / character)              │
│  context builder  (system + persona/character + memory + relationship)│
│  model registry  ·  acquisition (hf-hub + downloads table)           │
│  resource manager (NVML whole-GPU + reservation ledger + RAM watch)  │
│  scheduler  (priority queue; ONE GPU serialization mutex)            │
│  lifecycle manager  (the only loader/unloader of managed models)     │
│  worker supervisor  (spawn / health / restart; Windows Job Object)   │
│  adapters:  LlmBackend · ImageBackend · Stt · Tts · Embedder         │
└──┬────────────┬───────────────┬──────────────┬──────────────┬────────┘
   │ pipe/token │ pipe/token    │ stdio JSONL  │ stdio JSONL  │ stdio JSONL
   ▼            ▼               ▼              ▼              ▼
 llama-server  image-server   stt            tts            face-embedder
 (GGUF, CUDA,  (diffusers,    (faster-       (Chatterbox    (InsightFace,
  built from   Krea 2 Turbo   whisper fp16,  Turbo,         accept/reject
  source)      NF4 cached)    VAD-segmented) clause-chunked) identity gate)
   └── SQLite (rusqlite; dedicated writer + read pool; WAL; refinery) ──┘
   └── blobs/  content-addressed  <sha256>   (images, audio, references) ┘
```

## 2. The three runtimes and their boundaries (`CLAUDE.md` Article I)

### Rust core — authoritative
Owns: application state, orchestration, configuration, all SQLite access, the
blob store, model lifecycle, resource/VRAM/RAM management, scheduling, task
management, process supervision, IPC, security-sensitive operations, business
logic. Single crate (`src-tauri/`); modules interact through defined interfaces.

Modules (see `src-tauri/README.md` for the live list): **as of Phase 12** —
`lib.rs` (Tauri builder + config/DB/logging/registry wiring in `setup()`, WAL
checkpoint on exit), `logging` (JSON stdout, non-blocking lossy, boundary secret
redaction, ring buffer, config-driven reloadable level — no network sink), `ipc`
(`commands`, `error::{AppError, ErrorEnvelope}`), `contracts` (the serializable
vocabulary for the IPC **and** worker boundaries — `docs/contracts.md`; no
behaviour), `config` (the settings authority — one JSON file, layered
defaults/file/session, ADR-0016), `db` (SQLite — writer/reader pools, `refinery`
forward-only migrations with verified backup, `DbError`, ADR-0009), `models` (the
model registry — `model_entry` rows, CRUD + capability query, path confinement,
computed availability, UUIDv4 ids per ADR-0017), `acquisition` (HF picker +
`reqwest` download engine — `.part` + `Range` resume + SHA-256 verify + budget
guard + auto-register; **the only runtime network egress**; ADR-0008). Each later
phase adds its module and registers it in `src-tauri/README.md` and §3 here.

### React / TypeScript — presentation only
Owns: rendering, UI, interaction, transient view state. Never accesses SQLite, AI
processes, the filesystem, or the network. Communicates only through typed Tauri
IPC. Holds a cache of the last server snapshot, invalidated by sequenced events;
re-fetches on focus/reconnect. One shared conversation component renders Tab 1 and
Tab 3 conversations against different sources.

### Subprocesses — isolated, non-authoritative
Two transport categories (ADR-0013):

| Category | Members | Transport |
| -------- | ------- | --------- |
| **Model server** | `llama-server`, `image-server` | Windows **named pipe** preferred (ACL-scoped); else `127.0.0.1:<free port>` + a per-launch **bearer token** |
| **Stateless worker** | STT, TTS, face-embedder | **JSON-lines over stdin/stdout**; `stderr` = logs only; audio as framed PCM or a short temp file |

All subprocesses: Rust-spawned & supervised (Job Object kills them with the app),
non-authoritative, no direct SQLite, no independent GPU management (all via the
resource manager / scheduler), backend detail confined to one Rust adapter,
loopback only, launched with the offline/no-telemetry env (ADR-0015).

## 3. Single-authority map

| Concern | Sole authority |
| ------- | -------------- |
| Application / durable state | Rust core |
| SQLite access + migrations | `db` module (dedicated writer) |
| Blob storage | `blob store` module |
| Filesystem | Rust core (path-confined) |
| Configuration | `config` module |
| Model load / unload | `lifecycle manager` |
| GPU/VRAM + system-RAM accounting | `resource manager` |
| GPU job ordering + eviction | `scheduler` |
| Process spawn / health / restart | `worker supervisor` |
| Conversations (all modalities) | `conversation engine` (one) |
| Prompt assembly | `context builder` (one) |
| Memory | `memory` module (FTS5, per-scope) |
| Relationship state | `character` module (DB row is truth) |
| Executing AI-proposed effects | Rust, via allow-listed typed actions only |

No component may duplicate another's authority. The Phase 36 audit enforces this.

## 4. The 16 GB VRAM constraint (drives everything)

Effective GPU budget ≈ 13–14 GB (desktop compositor takes 1–2 GB). Measured
footprints:

| Workload | ~VRAM | Coexists with LLM? |
| -------- | ----- | ------------------ |
| LLM (GGUF ~8B, Q4_K_M, 8k ctx) | 6–8 GB | — |
| STT + VAD + TTS | 5–7 GB | **yes** (voice needs no swap) |
| Image (Krea 2 Turbo NF4) — idle | ~1.6 GB | yes (sidecar resident) |
| Image (Krea 2 Turbo NF4) — generating | **~11.4 GB** | **no** (LLM + TTS evicted) |

**Consequence:** every image generation (Tab 2 and character-sent) triggers a
scheduler eviction of the LLM (+ TTS), the generation, then restore. NVML reports
whole-GPU numbers only (per-process confirmed unavailable on this driver) — the
resource manager relies on a reservation ledger + measured `free`.

## 5. Cross-cutting flows

### Chat request (text)
`React → invoke(sendMessage) → conversation engine → context builder (persona +
memory) → LlmBackend → llama-server (SSE) → token deltas → Tauri Channel → UI`.
Cancellation: `cancel(taskId)` → `CancellationToken` + drop the HTTP request.
Async post-turn: memory extraction (schema-constrained LLM pass → validate → FTS5).

### Voice turn
`mic (cpal) → Silero VAD endpoint → stt worker (JSONL) → transcript → conversation
engine (same path as text) → LLM stream → clause-chunked tts worker → playback
(small buffer)`. Barge-in: VAD speech-onset → engine `Interrupting` state → cancel
LLM + TTS + playback (<~200 ms) → persist truncated assistant turn → `Listening`.

### Image request (Tab 2 or character-sent)
`request → scheduler.enqueue(image_gen) → [wait for no interactive LLM gen] →
acquire GPU mutex → unload LLM (+ TTS) → wait for VRAM to settle → image-server
generate (8 steps, NF4) → [Tab 3: face-embedder identity gate → accept | regen] →
write blob + SQLite row → release mutex → restore LLM (+ TTS) → deliver`.
Restore runs even if the image job crashes.

### Character conversation
`Discovery select → load character (batched read: profile + appearance +
relationship + memory links) → conversation engine → context builder (character
section + character-scoped memory + relationship stage guidance) → LLM stream`.
A `send_image` typed action from the model → Rust validates (schema, allow-list,
`character_id` match, stage gate) → image request (above) → gallery + conversation.

## 6. Persistence

- **SQLite** one file, table-prefixed groups: `core_ conv_ persona_ char_ mem_
  model_ asset_`. `rusqlite` + `deadpool-sqlite`; **one dedicated write connection**
  (writes serialized) + a 2–4 read pool; WAL + `busy_timeout` + `foreign_keys=ON`
  + `synchronous=NORMAL`.
- **Migrations:** `refinery`, forward-only, transactional; `VACUUM INTO` backup
  **verified non-zero before** any migration runs.
- **Blob store:** `app_data/blobs/<sha256[0:2]>/<sha256>`; write blob → fsync →
  commit the `asset_*` row; a reconcile job finds orphans/dangling links.
- `mem_*` uses FTS5 (BM25); retrieval filtered by `persona:<id>` or
  `character:<id>`.

## 7. Resource manager + scheduler

- **Resource manager accounts:** NVML `memory_info` + a reservation ledger +
  system-RAM watch; closed-form VRAM estimate (GGUF header → layers/heads/ctx →
  KV cache + weights + overhead + margin) with a learned per-model correction.
- **Scheduler orders:** priority queue — (1) interactive LLM gen (never
  preempted), (2) STT/TTS, (3) user image gen, (4) character/pool generation
  (idle-only). One GPU serialization mutex; **the mutex holder performs the entire
  transition** (read state, unload, settle-wait, load, commit) with no re-entry.
- **Driver TDR / whole-GPU reset:** if all GPU consumers' health fails together →
  kill all GPU subprocesses → reconcile from zero → reload desired state → one
  "driver reset — recovering" notice.

## 8. Untrusted AI output (`CLAUDE.md` Article III)

Model and character text is data. The only structured effect is a typed action
from a fixed allow-list, validated in Rust before any effect. `send_image` is the
first such action: `{character_id, image_type, scene, mood, clothing, context}` →
schema + allow-list + `character_id`-matches-conversation + relationship-stage
gate. Malformed/unknown/cross-character → rejected + audit-logged, nothing runs.
No code path deserializes raw model text into a command type. The context builder
renders all untrusted sections inside fixed delimiters that are stripped from the
content, so injected text cannot restructure the prompt.

## 9. Offline enforcement

Runtime network egress is forbidden except: (a) the Rust `hf-hub` acquisition path
(explicit user action), (b) an optional, default-off update check. Every Python
worker runs with `HF_HUB_OFFLINE=1 / TRANSFORMERS_OFFLINE=1 / *DISABLE_TELEMETRY /
DO_NOT_TRACK` (ADR-0015) and loads from local paths only. Frontend: no CDN fonts,
no analytics, a restrictive CSP. Phase 32 verifies with a packet capture.

## 10. Risk register

`docs/verification/03_adversarial_review.md` — ~40 risks with dispositions,
owning subsystems, and the phase that tests each. Critical mitigations are folded
into the ADRs above.
