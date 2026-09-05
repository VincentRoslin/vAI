# Phase 3 — Architecture Research

## Objective
Research *how* to build the product defined in `docs/product/requirements.md`.
Every technology and boundary is questioned against performance, maintainability,
Windows-first, offline, and failure modes. The research stays genuinely open — no
reverse-engineering a predetermined design to justify a technology already named.
Output: a written comparison + recommendation per decision, and draft ADRs.

## Depends on
Product Definition confirmed. Inputs: `docs/product/requirements.md` (`ARQ-*`),
`docs/OVERVIEW.md` (technical areas + recorded tech defaults), `docs/research/01–06`,
`docs/verification/01_env_audit.md` (O5 gaps), `ROADMAP.md` §7 (O3–O5).

## Not in this phase
- Any application code, scaffolding, or dependency installation.
- `PROJECT.md` / `ARCHITECTURE.md` content (Phase 5).
- Final ADR ratification (drafts only here; finalized Phase 5.8).

## Architecture notes
- The output feeds Phase 4 (attack) then Phase 5 (freeze). Keep every decision
  reversible until Phase 5.
- Respect `CLAUDE.md` Article I as the fixed frame: Rust authoritative core;
  frontend presentation-only via typed IPC; subprocesses isolated + non-
  authoritative. Research decides the *mechanisms*, not these boundaries.
- Recorded tech defaults (faster-whisper, Silero VAD, Chatterbox, FLUX.1 Krea, HF
  GGUF picker) are the *starting hypothesis* for their decision — confirm fit or
  pick the alternative, and say why.

## Performance notes
- Every recommendation states its performance implication on *this* machine
  (Ryzen 9800X3D, RTX 5080 16 GB, 32 GB RAM, single NVMe, 215 GB free).
- VRAM budget is the tightest constraint — 16 GB shared between desktop, LLM,
  image model, and STT/TTS. Coexistence math is mandatory for 3.4, 3.7, 3.9, 3.10.

## Steps
Each step produces a short written comparison in the phase's working notes:
**options · pros/cons · performance · Windows + offline fit · failure modes ·
recommendation**. Collected into draft ADRs at 3.15.

### 3.1 — Frame the decisions
Do: build one tracking table listing every decision to make — derived from
`ARQ-*`, the `docs/OVERVIEW.md` §5 technical areas, and O3–O5. Each row: id,
question, inputs, owner of the eventual ADR.
Verify: every `ARQ` maps to at least one decision row; the table is committed to
the phase working notes.

### 3.2 — Desktop shell & Rust structure
Do: Tauri v2 capabilities vs the product's needs (windowing, tray, IPC, updater,
bundler); **single Tauri crate with modules vs Cargo workspace** (O4, default:
single crate).
Verify: recommendation recorded with the crate-structure decision and its trigger
for revisiting (e.g. "split when a worker-supervisor crate is reused by tests").

### 3.3 — IPC design
Do: commands vs events vs channels for each interaction class (request/response,
lifecycle broadcast, token stream); typed-contract generation (`tauri-specta` /
`ts-rs` / hand-rolled); the error model (one boundary enum + per-domain
`thiserror`); cancellation representation.
Verify: a table mapping each interaction class → mechanism; error-model sketch;
contract-generation recommendation with build-step impact.

### 3.4 — LLM runtime (**resolves O3**)
Do: llama.cpp integration mode — supervised `llama-server` over loopback HTTP vs
`llama-cpp-2` FFI vs other. Cover: streaming, cancellation reaching the job,
parallel slots, crash isolation, upgrade path. **CUDA 13 / Blackwell sm_120 build
strategy** (build llama.cpp from source vs prebuilt CUDA binaries — O5).
Verify: transport decision recorded with rationale; a build-strategy decision with
the exact toolchain/steps; both captured as draft ADRs.

### 3.5 — Model acquisition (Phase 12 input)
Do: HF API for search + file listing; download mechanism (`hf-hub` crate vs
manual `reqwest`); resumability (range requests, `.part`); checksum source (HF
provides SHA256 in the LFS pointer / API); GGUF header parsing for quant/context;
disk-budget model.
Verify: a recommended download path with the resume + verify + register flow
sketched; GGUF-metadata parsing approach confirmed against a real GGUF file
header.

### 3.6 — Persistence
Do: `rusqlite` + pool (`deadpool-sqlite`) vs `sqlx`; dedicated-writer vs plain
pool; schema-group layout (one file, table-prefix groups); migration tool
(`refinery` / `sqlx` / hand-rolled `user_version`); content-addressed blob store
layout; backup mechanism.
Verify: recommendation with the pragma set, pool topology, and migration-test
matrix; blob-store path scheme written down.

### 3.7 — Resource / VRAM management
Do: `nvml-wrapper` behaviour on Blackwell + WDDM — **verify whether per-process
VRAM is reported** (env audit showed `N/A` for compute processes); estimation
formula (weights + KV cache + context + overhead + margin); reservation ledger;
reconcile cadence.
Verify: a runnable `nvml-wrapper` probe confirms which metrics are available on
this driver; estimation formula written with a calibration plan.

### 3.8 — Scheduler
Do: the arbitration model for GPU/worker/task work — queue, priorities,
cancellation, fairness; where it is introduced (Phase 23) and formalized (Phase
24); relationship to the resource manager (13).
Verify: a state/interaction sketch; the 13↔24 responsibility split written down.

### 3.9 — Voice pipeline
Do: **faster-whisper** worker — model size/quant that fits alongside the LLM,
streaming (partials) vs batch-on-endpoint, CTranslate2 CUDA build for sm_120;
**Silero VAD** — endpointing thresholds, false-trigger handling; **Chatterbox**
worker — synthesis latency, clause chunking for barge-in, VRAM cost; audio I/O
(`cpal`, WASAPI shared vs exclusive); open-mic echo handling (or headphones-only
for v1).
Verify: each of the three workers has a recommended model + a measured or
documented VRAM/latency figure; barge-in latency budget set; audio-crate decision
recorded. Confirm faster-whisper + Chatterbox coexist with a loaded LLM in 16 GB
or document the hot-swap requirement.

### 3.10 — Image generation (**request the owner's implementation**)
Do: **ask the owner for their existing FLUX.1 Krea implementation from the other
project.** Review it. Plan the adaptation to this architecture: worker boundary,
resource-manager integration, storage (filesystem blob + SQLite metadata),
cancellation. FLUX.1 Krea VRAM cost; whether LLM must be evicted during generation
(Phase 23).
Verify: the owner's code is received and reviewed; an adaptation plan is written
listing what is reused, what is rewritten, and why; VRAM/coexistence answer
recorded.

### 3.11 — Character identity
Do: reference-image conditioning options for FLUX.1 Krea (IP-Adapter / reference
/ LoRA / other); the identity-similarity check for accept-vs-regenerate (face or
image embedding, threshold, cost).
Verify: a recommended conditioning approach + a recommended similarity check with
its runtime cost; both as draft ADRs; note the accepted limitation (no guaranteed
identity).

### 3.12 — Memory
Do: SQLite schema for memories (fields, importance, provenance, timestamps);
retrieval starting with **FTS5 keyword search**; the concrete criteria that would
justify adding embeddings later (measured recall gap, corpus size).
Verify: schema sketch + retrieval approach + the "introduce embeddings when X"
trigger written down.

### 3.13 — Worker protocol (if stdio chosen in 3.4/3.9)
Do: JSON-lines framing, `ready` handshake + protocol version, request/cancel
messages, progress/chunk/result/error messages, backpressure policy, binary-asset
handoff via controlled paths, Windows non-blocking stdin (reader-thread pattern).
Verify: a one-page protocol spec draft.

### 3.14 — Packaging
Do: Tauri bundler (MSI / NSIS); how Python workers ship (embedded Python / `uv`
venv / PyInstaller-frozen) and the size/complexity trade; native libs (CUDA
runtime, llama.cpp binary, CTranslate2); first-run model acquisition; fully
offline install path.
Verify: a recommended packaging approach with an estimated installer size and the
offline-install story.

### 3.15 — Consolidate into draft ADRs
Do: for every decision in the 3.1 table, write a draft ADR in `docs/decisions/`
(`NNNN-title.md`: context, options considered, decision, reason, consequences,
`STATUS: PROPOSED`). List residual open items for Phase 4 to attack.
Verify: `docs/decisions/` has one draft ADR per decision row; each `ARQ` is
answered by an ADR or explicitly deferred with a reason.

## Verification gate
1. The 3.1 decision table is complete — every `ARQ` maps to at least one row. —
   reviewer check.
2. Every decision row has a written comparison (options / pros-cons / performance
   / Windows+offline / failure modes / recommendation). — file check.
3. `nvml-wrapper` per-process capability on this driver is **confirmed by a
   runnable probe**, not assumed. — probe output recorded.
4. GGUF-metadata parsing is **confirmed against a real GGUF header**. — output
   recorded.
5. The owner's FLUX.1 Krea implementation has been received and reviewed; an
   adaptation plan exists. — file check.
6. `docs/decisions/` contains one `STATUS: PROPOSED` ADR per decision.
7. VRAM coexistence math for LLM + STT + TTS (+ image via hot-swap) is written
   down with real figures.
8. **No application code, no dependencies installed.** — `git status` shows only
   docs; no `Cargo.toml` / `package.json` for the app.

## ADRs / open questions this phase resolves
- O3 (transport), O4 (crate structure), O5 (CUDA build, Python env, package
  manager, model storage) → draft ADRs.
- Produces the full draft ADR set that Phase 4 attacks and Phase 5 ratifies.
