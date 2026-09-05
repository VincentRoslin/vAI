# 03 — Phase 4: Adversarial Architecture Review

**Date:** 2026-09-05
**Input:** `docs/research/phase3/` + `docs/decisions/0001–0014` (all `PROPOSED`).
**Method:** assemble the proposed architecture (§1), attack it by category
(§2), resolve every realistic risk (§3 register). No application code.

---

## 1. The proposed architecture (assembled from the ADRs)

```
┌───────────────────────────────────────────────────────────────┐
│  React / TS frontend — PRESENTATION ONLY                       │
│  Tabs: Chat/Voice · Image Generator · Discovery · Models · Settings │
│  State: view state + last server snapshot (invalidated by events) │
└───────────────┬───────────────────────────────────────────────┘
        typed Tauri IPC (ADR-0002): Commands · Events · Channels
                │  cancel(taskId) → CancellationToken
┌───────────────▼───────────────────────────────────────────────┐
│  RUST CORE — single crate (ADR-0001), authoritative            │
│                                                               │
│  ipc │ config │ observability(tracing) │ db │ blob store       │
│  conversation engine (ONE — chat/voice/character)             │
│  context builder (persona/character + memory + relationship)   │
│  model registry │ acquisition (hf-hub + downloads table)       │
│  resource manager (nvml whole-GPU + reservation ledger)        │
│  scheduler (priority queue; ONE serialization mutex)           │
│  lifecycle manager (only loader/unloader of managed models)    │
│  worker supervisor (spawn/health/restart; Windows Job Object)  │
│  adapters: LlmBackend │ ImageBackend │ Stt │ Tts │ Embedder    │
└──┬──────────────┬──────────────┬──────────────┬───────────────┘
   │ loopback HTTP│ loopback HTTP│ stdio JSON-lines            │
   ▼              ▼              ▼              ▼               ▼
llama-server   image-server   stt (faster-  tts          face-embedder
(GGUF, CUDA)   (diffusers,    whisper fp16) (Chatterbox) (InsightFace)
               Krea 2 NF4)
   └── SQLite (rusqlite, dedicated writer + read pool, WAL, refinery) ──┘
   └── blobs/  content-addressed  <sha256>  (images, audio, references) ─┘
```

**Load-bearing invariants:**
- Image generation ⟂ LLM on 16 GB → every image evicts the LLM (+ TTS).
- One GPU serialization point; only one heavy GPU job at a time.
- NVML gives whole-GPU numbers only (per-process confirmed unavailable).
- Frontend never touches DB / processes / filesystem / network.
- Model/character output is data; the only structured effect is an allow-listed
  typed action validated in Rust.
- Local-first: only the acquisition/update paths touch the network.

---

## 2. Attack summary by category

| Category | Worst realistic outcome | Covered by design? |
| -------- | ---------------------- | ------------------ |
| Resource exhaustion (§R1–R6) | VRAM/RAM OOM, disk full | mostly — **system RAM** and the **NF4 requant cost** need elevation |
| Process / crash (§R7–R12) | orphaned model server holding 11 GB; **driver TDR** | mostly — TDR (whole-GPU reset) handling needs an explicit path |
| Concurrency (§R13–R17) | deadlock on the GPU mutex; cancel not propagating | needs **explicit lock-ordering rule** + cancel-everywhere discipline |
| Data (§R18–R23) | DB corruption; partial migration; dangling blob | adequate — tighten backup-before-migrate + blob write ordering |
| IPC / untrusted AI output (§R24–R29) | **local process calls the loopback image server**; prompt injection via memory | **loopback HTTP needs auth or named pipes**; injection-safety in the context builder |
| Offline / privacy (§R30–R34) | **HF/transformers telemetry**; a dep phones home | **hard env-var lockdown for every Python worker** |
| Character system (§R35–R40) | half-formed character shown; pool starvation | adequate — transactional "ready"; on-demand fallback |
| Duplicate authority (§R41–R45) | drift to two engines; frontend caches domain data | enforced by review + Phase 36 audit; the image server's own VRAM logic must move to the resource manager |

---

## 3. Risk register (prioritized)

Severity = likelihood × impact. **Disposition**: `MITIGATED` (design change or
existing control) or `ACCEPTED` (rationale).

### Critical — architecture changes required

**R-C1 · Loopback HTTP model servers have no authentication.**
Any local process can `POST http://127.0.0.1:<port>/generate` or read results
from the image/LLM server.
- Why permitted: ADR-0013 binds loopback but adds no auth.
- Disposition: **MITIGATED — ADR-0013 revised.** Prefer a **Windows named pipe**
  (has ACLs, no TCP port, no firewall prompt) for both model servers. If TCP
  loopback is kept for `llama-server` (upstream expects it), the Rust core
  generates a **per-launch bearer token**, passes it via `--api-key`, and every
  request carries it; the server rejects unauthenticated requests.
- Owner: transport / worker supervisor. Test: an unauthenticated `curl` to the
  server port is refused (Phase 15 / Phase 34).

**R-C2 · Python workers leak telemetry / phone home.**
`transformers`, `huggingface_hub`, `diffusers`, and friends send usage telemetry
and check for updates by default. Violates NFR-1/2/10/12.
- Why permitted: no ADR covered dependency egress behaviour.
- Disposition: **MITIGATED — new ADR-0015.** Every Python worker is launched with
  a fixed hostile-network env: `HF_HUB_OFFLINE=1`, `TRANSFORMERS_OFFLINE=1`,
  `HF_HUB_DISABLE_TELEMETRY=1`, `DISABLE_TELEMETRY=1`, `HF_HUB_DISABLE_IMPLICIT_TOKEN=1`,
  `DO_NOT_TRACK=1`, no proxy. Only the acquisition path (Rust `hf-hub`) is allowed
  network. Phase 32 offline audit verifies with a packet capture.
- Owner: worker supervisor launch code. Test: Phase 32 traffic capture shows zero
  egress from workers.

**R-C3 · NF4 re-quantization on every image session (~90 s) + ~24 GB bf16 in RAM.**
On a 32 GB machine, loading bf16 Krea 2 (~24 GB) to quantize it every time risks
RAM exhaustion + swap thrash, and the 90 s wait is a severe UX regression.
- Why permitted: ADR-0006 listed "cache the quantized weights" as an
  *optimization*.
- Disposition: **MITIGATED — ADR-0006 + ADR-0008 revised: the NF4 quant cache is
  REQUIRED, not optional.** Quantize once during model acquisition, persist the
  NF4 weights, load NF4 directly (~6 GB) thereafter. The bf16 model is only ever
  resident during that one-time acquisition step, with a RAM pre-check.
- Owner: image subsystem + acquisition. Test: Phase 22 — a warm image session
  never loads the bf16 model; peak system RAM in normal use stays well under
  32 GB.

**R-C4 · GPU serialization mutex ↔ scheduler deadlock.**
The scheduler holds the one GPU mutex during an evict/load/swap; if any step
calls back into the resource manager (which also wants the mutex) or waits on a
worker that needs a mutex-guarded resource, it deadlocks.
- Why permitted: ADR-0007/0010 name "one serialization point" but not the lock
  discipline.
- Disposition: **MITIGATED — ADR-0010 revised with an explicit rule:** the
  scheduler acquires the mutex, performs the *entire* transition (query resource
  manager state via non-locking reads, unload, wait, load, commit), then
  releases. Resource-manager mutating calls are only made by the mutex holder.
  Workers never call back into the resource manager. A lock-ordering doc lands in
  `ARCHITECTURE.md` (Phase 5).
- Owner: scheduler + resource manager. Test: Phase 13/24 — a fault-injection test
  that submits an image job + a character-image job + a chat generation
  simultaneously completes without hanging.

### High — mitigations within the existing design

**R-H1 · Driver TDR / whole-GPU reset.** A CUDA fault on Blackwell can reset the
whole GPU, not just one process — every model server + worker loses its context
at once.
- Disposition: **MITIGATED — ADR-0007 gains a "GPU reset" path:** if *all* GPU
  consumers' health checks fail within a short window, treat it as a device
  reset → kill every GPU subprocess, `reconcile` from zero, reload the last-known
  desired state, surface a single "the graphics driver reset — recovering" notice.
- Owner: resource manager + supervisor. Test: Phase 33 fault injection
  (simulate by killing all GPU subprocesses at once).

**R-H2 · Orphaned model server after an app hard-crash** (holding 6–12 GB VRAM).
- Disposition: **MITIGATED (existing + added).** Windows Job Object with
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so children die with the parent even on a
  panic; **plus** a startup sweep that kills any `llama-server` / `image-server`
  process not spawned by this instance (match on the bundled binary path).
- Owner: supervisor. Test: Phase 33 — `taskkill /f` the app mid-generation → no
  orphan on relaunch.

**R-H3 · Prompt injection via memory / character text / retrieved content.** The
product feeds user and model text back into prompts everywhere.
- Disposition: **MITIGATED (design).** The context builder (Phase 20): (a) renders
  all untrusted sections (persona, character, memory, history) inside fixed
  delimiters that are stripped/escaped from the content itself; (b) the system
  instruction block is always first and structurally separated; (c) model output
  is untrusted regardless of what the prompt contained — the only privileged path
  is the allow-listed typed action, validated in Rust. `ACCEPTED` residual: a
  model may still be *persuaded* by injected text to say things — that's a
  content outcome, not a security breach, and there is no content policy (NFR-15).
- Owner: context builder + Phase 28/34. Test: Phase 20 — injected delimiter/
  instruction text cannot change the assembled prompt's structure.

**R-H4 · Cancellation doesn't reach the running job.** Token fires but the stream
read is blocked / the worker ignores the cancel.
- Disposition: **MITIGATED (discipline).** Every await point `select!`s on the
  `CancellationToken`; the hard stop is dropping the `reqwest` request (connection
  close → server frees the slot) or closing the worker's stdin + killing it;
  cancellation is verified to free the GPU slot in Phase 15/22, not assumed.
- Owner: adapters. Test: Phase 15/16 gate already requires "cancel mid-generation,
  model stays reusable".

**R-H5 · VRAM/RAM estimate too low → OOM on load.** Especially first-run before
calibration.
- Disposition: **MITIGATED.** Conservative safety margin (config, ~1.5 GB default);
  a large first-run margin that tightens as `(estimated, measured)` pairs
  accumulate; **never auto-retry the same config** after an OOM — surface "free
  X GB or pick a smaller quant". KV-cache term uses the real configured context
  and the header-parsed layer/head counts.
- Owner: resource manager. Test: Phase 13 mock-hardware OOM path.

**R-H6 · Discovery pool starvation during active chat.** Top-up needs the GPU;
the GPU is busy with the user's conversation; the pool empties.
- Disposition: **MITIGATED.** On-demand single-character generation with a
  "finding someone…" state when the pool is empty; pool size + top-up threshold
  in config; top-up only runs on true idle. `ACCEPTED` residual: heavy swiping
  while actively chatting will be slow (one GPU).
- Owner: scheduler + discovery. Test: Phase 29 — empty the pool, confirm the
  on-demand path.

**R-H7 · Migration backup fails (disk full) but migration proceeds.**
- Disposition: **MITIGATED — ADR-0009 tightened:** the migration runner verifies
  the `VACUUM INTO` backup exists and is non-zero **before** applying any
  migration; if the backup fails, refuse to migrate and surface the disk problem.
- Owner: db layer. Test: Phase 9 — simulate a failed backup, confirm no migration
  runs.

**R-H8 · The image server's own `exclusive_vram` logic vs the resource manager.**
The owner's code manages its own VRAM (unloads LLM+TTS). In LocalAI that must be
the resource manager's job — two authorities is R-C4 waiting to happen.
- Disposition: **MITIGATED — ADR-0006 note:** the adapted image server exposes
  *generate* only; **all** load/unload/eviction decisions move to the Rust
  scheduler + resource manager. The `exclusive_vram` behaviour is reimplemented
  as a scheduler contract, not kept in the Python server.
- Owner: image subsystem adaptation (Phase 22). Test: Phase 22 — the image server
  never unloads another model itself.

### Medium — mitigations mostly already in the plan

| ID | Risk | Disposition | Owner | Test |
| -- | ---- | ----------- | ----- | ---- |
| R-M1 | Worker never sends `ready` (stale venv / missing model) | MITIGATED — handshake timeout → kill+restart → `Failed` after N; version check | supervisor | Phase 33 |
| R-M2 | Process hangs (alive, no progress) | MITIGATED — health ping + generation timeout → kill+restart | supervisor | Phase 33 |
| R-M3 | `image-server` crash leaves the LLM unloaded | MITIGATED — scheduler restore is in `finally`, runs on crash too | scheduler | Phase 22/33 |
| R-M4 | KV-cache blow-up from a huge context setting | MITIGATED — context capped in config; KV estimate uses it; optional KV quant | LLM adapter + RM | Phase 15 |
| R-M5 | Model bigger than VRAM even alone | MITIGATED — registry estimate vs measured free → refuse before spawn | lifecycle mgr | Phase 14 |
| R-M6 | Blob store fills the disk (batch-and-pick discards) | MITIGATED — only accepted images stored; blob disk budget; prune policy | blob store | Phase 9/22 |
| R-M7 | SQLite corruption on power loss | MITIGATED — `quick_check` on startup + restore-from-backup; consider `synchronous=FULL` on the writer | db layer | Phase 9/33 |
| R-M8 | Blob orphan / dangling link on crash | MITIGATED — write blob → fsync → commit row; reconcile job | blob store | Phase 9 |
| R-M9 | Downloads table vs `hf-hub` resume-state disagreement | MITIGATED — startup reconcile of the table against `.part` files; `hf-hub` re-verifies | acquisition | Phase 12 |
| R-M10 | Stale UI after a missed event | MITIGATED — events carry a seq; frontend re-fetches on focus/reconnect | frontend | Phase 30 |
| R-M11 | Unnecessary LLM reloads (chat↔image thrash) | MITIGATED — batch image jobs; "image/discovery mode" keeps the LLM unloaded while in those tabs; predictive eviction | scheduler | Phase 23/31 |
| R-M12 | Half-formed character shown in Discovery | MITIGATED — a character is `ready` only when profile + N images commit in one transaction; feed filters on `ready` | character subsystem | Phase 25/29 |
| R-M13 | Relationship-state cache vs DB desync | MITIGATED — DB row is source of truth; cache invalidated on the transactional write | character subsystem | Phase 26 |
| R-M14 | Character references a user-deleted image | MITIGATED — reference images are refcount-protected from deletion; UI warns | character + blob | Phase 25 |
| R-M15 | IPC payload flood / oversized | MITIGATED — command payload size limit; Channel bounded buffer | ipc | Phase 7 |
| R-M16 | Conversation content leaks into a worker's stderr on error | MITIGATED — workers log operation metadata not payloads; boundary redaction | observability | Phase 10 |
| R-M17 | Unbounded context growth in long conversations | MITIGATED — context builder truncation + token budget with provenance | context builder | Phase 20 |
| R-M18 | Config read outside the config system (drift) | MITIGATED — `git grep` gate in Phase 8 + Phase 36 audit | audit | Phase 8/36 |
| R-M19 | Two conversation engines (drift over time) | MITIGATED — Phase 17 deletes the vertical-slice service; Phase 36 audit + `git grep` | audit | Phase 17/36 |
| R-M20 | `tokio::process::Child` not awaited → handle leak | MITIGATED — supervisor always awaits child exit | supervisor | Phase 6 |

### Accepted (no change — rationale)

| ID | Risk | Rationale |
| -- | ---- | --------- |
| R-A1 | Character visual identity drifts under big pose/scene changes | Owner-scoped to best-effort v1 (portraits/selfies); documented in FR-C90; revisit if Krea 2 gets reference conditioning |
| R-A2 | "Memory poisoning" — user/model plants false memories that steer behaviour | Single-user local sandbox; it's the user's world to shape; not a security boundary; no content policy (NFR-15) |
| R-A3 | A model can be *persuaded* by injected text to produce unwanted content | No content policy by design (NFR-15); structural injection is prevented (R-H3), semantic persuasion is out of scope |
| R-A4 | DB locked by the user opening the `.db` in an external tool | `busy_timeout` + retry → typed error; rare; user error |
| R-A5 | Heavy swiping during active chat is slow | One GPU; on-demand fallback keeps it functional; acceptable |
| R-A6 | Unsigned installer → SmartScreen warning | Deferred until a cert exists (ADR-0014); documented |
| R-A7 | WDDM hang on first sustained `llama-server` generation | Inconclusive from inspection; Phase 15 watch item with a 3-step mitigation ladder (`02_phase3_probes.md`) |

---

## 4. ADR changes from this review

| ADR | Change |
| --- | ------ |
| **ADR-0006** | NF4 quant cache is **required** not optional (R-C3); image server exposes *generate* only, no self-managed VRAM (R-H8) |
| **ADR-0007** | + GPU-reset / driver-TDR recovery path (R-H1); system RAM tracked as a second constraint (R-C3) |
| **ADR-0008** | NF4 quantization performed + cached during acquisition (R-C3) |
| **ADR-0009** | verify backup succeeded before migrating (R-H7); blob write→fsync→commit ordering (R-M8) |
| **ADR-0010** | explicit GPU-mutex lock-ordering rule (R-C4); scheduler restore runs on crash paths (R-M3) |
| **ADR-0013** | **prefer named pipes**; if TCP loopback, a per-launch bearer token (R-C1) |
| **ADR-0015 (new)** | Python worker network lockdown — offline + no-telemetry env for every worker (R-C2) |

All ADRs remain `PROPOSED`; the changes above are applied to the ADR files and
ratified together at Phase 5.

---

## 5. Gate

- [x] Risk register covers all 8 mandated categories (§2).
- [x] Every realistic risk has a disposition, an owning subsystem, and a test.
- [x] Every `ACCEPTED` risk has a rationale (§3).
- [x] Architecture changes captured as ADR edits (§4).
- [x] No application code.

**Phase 4 complete.** Next: Phase 5 — write the binding docs, ratify the ADRs,
re-derive `docs/plan/06–40`, freeze.
