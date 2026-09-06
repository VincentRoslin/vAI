# ROADMAP — LocalAI Bootstrap & Implementation State Machine

> **What this file is.** The authoritative record of *where the project is* and
> *what the next gate is*. A fresh Claude session reads this file, then opens the
> current phase's detailed plan in `docs/plan/` and proceeds — no conversation
> history needed.
>
> **Phase structure**: the LocalAI course, **Phase 0–40**, plus an un-numbered
> **Product Definition** step between Phase 2 and Phase 3. Bootstrap = Phases 0–5;
> implementation begins at Phase 6. Detailed per-phase steps live in
> `docs/plan/NN_*.md` (see `docs/plan/README.md`).
>
> **Stability:** the state-machine mechanics (§1–§3, §5) are fixed. Phase
> *objectives, ordering, and gates* below are the plan. The fine step detail in
> `docs/plan/` for Phases 6–40 is finalized at phase entry / Phase 5.9, because
> architecture research (Phase 3) may still move it.
>
> If reality and this file disagree about *progress*, fix the file in the same
> change that fixes reality.

---

## 1. Current State

| Field            | Value                                                    |
| ---------------- | ------------------------------------------------------- |
| **Phase**        | 22 — Image Generation (FLUX.1 Krea)                     |
| **Stage**        | 22.1 (`NOT STARTED`)                                    |
| **Status**       | `NOT STARTED`                                           |
| **Blocked by**   | needs the owner's FLUX.1 Krea implementation (request at phase entry — ADR-0006/0011, plan 22). |
| **Plan doc**     | `docs/plan/22_image-generation.md`                      |
| **Last updated** | 2026-09-06                                              |
| **Updated by**   | phase-21-memory                                         |

**Architecture frozen (Phase 5).** Binding: `docs/spec/` (the 7 spec docs —
`PROJECT`, `ARCHITECTURE`, `AI_PIPELINES`, `SECURITY`, `PERFORMANCE`,
`UI_GUIDELINES`, `DEVELOPMENT`) + `docs/decisions/0001–0017` (all `ACCEPTED`).
**Phase 6 done** — the app scaffold runs (`docs/verification/05_phase6_bootstrap.md`).
**Phase 7 done** — typed contract vocabulary + `ts-rs` bindings
(`docs/verification/06_phase7_contracts.md`, `docs/contracts.md`).
**Phase 8 done** — config system (`config/` module, ADR-0016,
`docs/verification/07_phase8_config.md`).
**Phase 9 done** — SQLite persistence (`db/` module, ADR-0009,
`docs/verification/08_phase9_persistence.md`).
**Phase 10 done** — observability (`logging/` module,
`docs/verification/09_phase10_observability.md`).
**Phase 11 done** — model registry (`models/` module, ADR-0017,
`docs/verification/10_phase11_registry.md`).
**Phase 12 done** — model acquisition (`acquisition/` module, ADR-0008 amended →
`reqwest`, `docs/verification/11_phase12_acquisition.md`). Live download verified
(`Qwen 0.5B` GGUF, 50.6 MB/s, HF SHA-256 checked, registered). **Gate 5 done
2026-09-06** — `acquire_fixed` pulled faster-whisper large-v3 (→ `models\stt\`)
+ Chatterbox Turbo (→ `models\tts\`) live, both registered. All 9 gates pass.
**Phase 13 done** — resource manager (`resources/` module, ADR-0007,
`docs/verification/12_phase13_resources.md`). `HardwareProbe` (NVML whole-GPU +
`sysinfo` RAM + mock), closed-form VRAM estimate + EMA calibration, in-memory
reservation ledger, `request`/`commit`/`observe`/`release`/`reconcile` behind one
async `Mutex`; `request` never blocks on the driver. Config schema **v4**
(`resources.vram_safety_margin_mb`). Real probe confirmed on the reference
machine (16 303 MB VRAM, 31 938 MB RAM).
**Phase 14 done** — model lifecycle manager (`lifecycle/` module, ADR-0007/0010,
`docs/verification/13_phase14_lifecycle.md`). `ModelBackend`/`LoadedInstance`
traits + `FakeBackend`; state machine `Unloaded→Loading→Loaded⇄Busy→Unloading` +
`Failed` recovery behind one async `Mutex`; concurrent loads coalesce; Phase 13
reservation held across the load, released on every exit; bounded retry; ~2 s
liveness monitor. `ModelState::Busy` added additively. No real backend yet —
Phase 15.
**Phase 19 done** — voice-out (`voice/` grows `chunker` / `playback` / `tts`,
ADR-0005/0013/0018, `docs/verification/19_phase19_voice-out.md`). `ClauseChunker`
(streamed LLM deltas → clauses), `Playback` (`cpal` output on a parked thread +
PCM queue + `stop()` → silent in one buffer), `TtsOutput` (the Chatterbox worker
+ playback, ~1 clause lookahead). `VoiceState::{Thinking,Speaking,Interrupting}`;
`start_listening(id, model_id: Option)` runs the listen→transcribe→think→speak
→listen loop with the capture stream live for **barge-in** (VAD onset after a
500 ms grace, or an explicit stop → `engine.cancel` [persists partial with
`stop_reason=Cancelled`] → `tts.cancel` → `playback.stop` → `Listening`, timed).
Config **v7** (`voice.output_device`). `workers/tts.py` (Chatterbox Turbo,
`ChatterboxTurboTTS.from_local`, 24 kHz; stdout→stderr guard so lib status prints
don't corrupt the protocol — same for `stt.py`). venv gains `torch 2.11.0+cu128`
(overrides the `chatterbox-tts` pin of `torch==2.6.0`, pre-Blackwell).
**All 7 gate items pass** — 30 voice unit tests (real Silero VAD + stdlib fakes
+ a scripted `LlmInstance`) + a **live** gate on the RTX 5080: 3 clauses spoken,
Chatterbox warm RTF ≈ 0.45, **barge-in trigger→silence ≈ 4 ms**. Truncation =
`stop_reason=Cancelled` (frozen Phase 17): persisted = generated-so-far, spoken
= a clause prefix of it. No new ADR.
**Phase 21 done** — persistent per-Persona memory (`memory/` module,
`docs/verification/21_phase21_memory.md`). `V0006` — `memory` + a standalone
`memory_fts` FTS5 table (BM25), written together in one tx (no triggers);
`insert` prunes the lowest-priority row at `PER_SCOPE_CAP=500`. `extract` — a
post-turn schema-constrained `llm.generate` → strict-JSON
`{memories:[{content,kind,importance}]}` → `importance≥3` + word-set-Jaccard
dedup (≥0.6) + length gate → store with provenance; malformed output stores
nothing, never fatal. `retrieve` — `fts_query_for` (lowercase, stopword-drop,
**sorted unique** terms) → deterministic. `MemoryService` — `retrieve` is a pure
read on the response path; `spawn_extraction` is fire-and-forget behind a
`Semaphore(1)`; `MemoryScope::Persona`; a `target:"memory"` recall-gap log.
`contracts::ids::MemoryId`; `contracts::memory` (`MemoryKind`, `Memory`). The
context builder gains a `max_memory_tokens` sub-budget (`MEMORY_CONTEXT_FRACTION`
0.15, additive) capping the ranked memory section. `ConversationEngine::build_prompt`
retrieves per-Persona memory (no persona ⇒ none, FR-56); `run_generation` spawns
extraction on a clean completion. `memory_list` / `memory_delete` IPC (FR-52/53);
a Memories list in Settings; "Show prompt" shows `memory N (M tok)`. **All 7 gate
items pass** — 16 `memory` tests + 3 `context::builder` memory tests + 2
`conversation` end-to-end (extraction → next prompt carries the memory; no
persona ⇒ nothing stored). 328 rust tests, 15 vitest, check suite green.
**No new ADR** — ADR-0012 fixes the design; **no config schema change** (tunables
are module constants); **no new crate** (FTS5 present in the bundled `rusqlite`).
**Phase 20 done** — personas & the one context builder (`context/` module,
`docs/verification/20_phase20_personas.md`). `V0005` — the `persona` table +
`conversation.persona_id` (`ON DELETE SET NULL`, fixed once a turn exists —
FR-17). `context::persona` (`Persona` + `PersonaDraft` + `PersonaRepo` CRUD),
`context::sanitize::strip_control` (removes every `<|…|>` control token from
untrusted strings — SECURITY C2), `context::tokens::estimate_tokens` (deterministic
`max(chars/4, words·0.75)`, no tokenizer dep), `context::builder::ContextBuilder`
(`build(BuildInput) -> BuiltPrompt` — deterministic ChatML; shed order memory →
persona-truncation → drop persona → never the base system; `total_tokens ≤
budget` by construction; `Provenance` logged at `target: "context"`; empty
memory/character slots for P21/P26). `conversation::prompt` **deleted** — the
engine holds the `PersonaRepo` + the builder and assembles every prompt through
it; `preview_prompt` + `chat_prompt_preview` expose the exact string (FR-34).
`contracts::ids::PersonaId`; `Conversation.persona_id` (additive). `persona_*` +
`conversation_set_persona` IPC; persona list/form in Settings, a picker +
"Show prompt" in ChatVoice. **All 6 gate items pass** — 24 `context` tests
(determinism, ordering, injection-safety, budget/truncation, provenance) + a
`conversation` test asserting the exact prompt the scripted adapter receives
carries the active persona. 304 rust tests, 14 vitest, check suite green. No new
ADR (`AI_PIPELINES.md` §6 fixes the design).
**Phase 18 done** — voice-in (`worker/` + `voice/` modules, ADR-0005/0013/0015/
**0018**, `docs/verification/17_phase18_voice-in.md`). `worker::WorkerSupervisor`
= the shared stdio JSON-lines actor (Job Object, `WorkerHello` version check,
`WorkerJobId` mux, ADR-0015 env in one place, bounded restart → `Failed`).
`voice::` = `cpal` capture → `rubato` 16 kHz mono → **Silero v5 VAD in-core**
(`ort` load-dynamic; `SpeechStart/SpeechEnd/MaxDurationCut` state machine) →
segment WAV → `workers/stt.py` (faster-whisper `large-v3` fp16 CUDA) →
`ConversationEngine::add_user_turn(Text)`. Low-confidence transcripts dropped
(never a blank turn). Push-to-talk for v1. Config schema **v6** (`workers.*`,
`voice.input_device`). `.venv` (`uv`, ADR-0018, gitignored ~2.2 GB);
`silero_vad.onnx` (v5.1.2, gitignored). `voice_*` + `chat_generate` IPC; a
hold-to-talk mic in `ChatVoice.tsx`. **All 7 gate items pass** — 25 unit tests
(real VAD + stdlib fake worker) + a **live** capture→transcript gate on the RTX
5080 ("Hello local AI, this is a voice input test." → one user turn; RTF ≪ 1.0).
Voice turns persist `Text` in v1 (→ `Audio{asset,transcript}` with the blob
store; additive).
**Phase 17 done** — conversation engine (`conversation/` module,
`docs/verification/16_phase17_engine.md`). `ConversationService` →
`ConversationEngine` (one path); `send` split into `add_user_turn(typed content)`
+ `generate(sink)`; explicit `GenerationState` (`Idle`/`Generating{task,convo}`)
+ `chat_state` IPC; `render_chatml` renders transcribed audio too (voice-ready).
All 6 gate items pass — the Phase 16 live gate re-run on the re-hosted engine,
TTFT ≈ 44 ms (no regression). Content taxonomy resolved by the frozen Phase 7
contract (media = content-addressed blob via `AssetId`).
**Phase 16 done** — first vertical slice / text chat (`conversation/` module,
`docs/verification/15_phase16_slice.md`). V0004 (`conversation` + `message`);
`ConversationService` (persist user turn → stream one generation via `LlmInstance`
→ persist assistant turn; **reject** a concurrent `send`); `chat_send` streaming
Channel + `chat_cancel`; `register_local_gguf`; rebuilt `ChatVoice.tsx`. **All 9
gate items pass** — a live Rust test runs the whole stack on the RTX 5080
(register → real `llama-server` → "pong" → persist → restart recovery → cancel →
reuse); **end-to-end TTFT ≈ 33 ms**. First real product milestone.
**Phase 15 done** — llama.cpp adapter (`llm/` module, ADR-0003/0004 amended/0013,
`docs/verification/14_phase15_llama.md`). `LlamaBackend`/`LlamaServer`; supervised
`llama-server` child on a free loopback port + `--api-key` bearer + Windows Job
Object; `/health` + `/completion` non-stream + SSE stream client with per-call
deadline + cancel; all llama.cpp JSON confined to `llm/protocol`. **All 9 gate
items pass** — stub tests in the adapter build + **15.D run live on the RTX 5080**
with a **pinned prebuilt** (`b10819`, CUDA 13.3) + the real Qwen 0.5B GGUF:
load ~775 ms, **TTFT ≈ 23 ms, ≈ 278 tok/s**, cancel frees the slot,
external kill detected, no orphan. Config schema **v5** (`runtimes.dir`);
binary + models kept **in-repo** (`runtime/`, `models/`, gitignored) per owner
preference.

**Completed:** Phase 0–2 · Product Definition · Phase 3 (research + ADRs + probes)
· Phase 4 (adversarial review, `03_adversarial_review.md`) · **Phase 5**
(architecture freeze — 7 binding docs + `docs/decisions/0001–0015` all `ACCEPTED`;
`04_phase5_crosscheck.md`) · **Phase 6** (app bootstrap — Tauri v2 + React/TS +
Rust; check suite green; app launches; IPC round-trip verified;
`05_phase6_bootstrap.md`).

**Only one `(Phase, Stage)` pair is ever `IN PROGRESS`.** Advancing the pointer is
itself a state transition and MUST follow §3.

**On external prompts:** the owner relays step prompts from ChatGPT/Gemini guides.
Those may use loose or different numbering. Map a prompt to the **phase by name and
intent**, not to a number it asserts; slot the work where it belongs here and say
where you put it. (See `CLAUDE.md` → Operating Manual.)

---

## 2. Status Vocabulary

| Status         | Meaning                                                                          |
| -------------- | ------------------------------------------------------------------------------- |
| `NOT STARTED`  | No work begun.                                                                  |
| `IN PROGRESS`  | Actively being worked. At most one stage repo-wide.                             |
| `BLOCKED`      | Cannot proceed; `Blocked by` names the phase/stage/external dependency.         |
| `VERIFIED`     | Verification gate ran and passed, evidence recorded. Not yet accepted.          |
| `COMPLETE`     | `VERIFIED` + reviewed + committed. Immutable unless explicitly reopened.        |

Phase-level status is the **minimum** of its stage statuses.

---

## 3. Transition Rules

1. **Forward only through the gate.** A stage moves toward `VERIFIED` only when
   every check in its **Verification Gate** was *physically executed* and its
   evidence (command output, file path, link) is recorded. (`CLAUDE.md` Art. IV.)
2. **No skipping.** Stage `N.k` waits for `N.(k-1)` to be `VERIFIED`/`COMPLETE`
   unless explicitly marked parallel-safe in the phase's `docs/plan/` file.
3. **No phase bleed.** Phase `N` cannot start until Phase `N-1` is `COMPLETE` and
   every `Depends on` entry is `COMPLETE`.
4. **Regression reopens.** If a later change breaks a `COMPLETE` phase's gate, set
   it back to `IN PROGRESS`, move the pointer back, record why in §5.
5. **Blocked is explicit.** `BLOCKED` needs a named blocker + a dated §5 entry.
6. **Every transition updates §1 and §5.**
7. **Gate honesty.** If a gate check cannot be executed in the current
   environment, record it as `NOT EXECUTED` with the reason — never as passed.
8. **Phase entry.** On starting a phase, read `docs/plan/NN_*.md` first; if its
   step detail still carries the "finalized at phase entry" banner, finalize it
   (against the frozen architecture) before doing the work.

---

## 4. Phase Ledger (index)

Each phase: **objective** · **depends on** · **status** · **gate** (summary) ·
**plan doc**. Full steps + full gates live in the linked `docs/plan/` file.

---

### EPOCH 0 — Bootstrap (Phases 0–5)

### Phase 0 — Establish the Process — `COMPLETE`
Rules + state-tracking before any work (`CLAUDE.md`, this file).
Gate: constitution + operating manual + state machine present; repo initialized.

### Phase 1 — Project Foundation — `COMPLETE`
Clean repo, doc skeleton, stack direction recorded.
Gate: commits exist, tree clean; doc placeholders present; stack direction in
`CLAUDE.md`. Deferred: formatter/linter/hooks/README → carried to Phase 6.

### Phase 2 — Environment Audit — `COMPLETE` — `docs/verification/01_env_audit.md`
Know the dev machine before choosing an architecture.
Gate: toolchains + build tools + GPU/VRAM recorded; Rust→MSVC link chain built an
exe. Non-blocking gaps (CUDA toolkit, pnpm, Python env, autocrlf) → Phase 3 / O5.

### ▶ Product Definition *(step)* — `COMPLETE` — `docs/product/requirements.md`
Owner draft (`docs/product/vision.md`) → confirmed requirements: 3-tab structure
(Chat/Voice · Image Generator · Discovery), Persona vs Character split, ~95 FR /
~40 NFR / 20 ARQ / 7 non-goals. A1–A11 resolved. Owner-confirmed 2026-09-05 (§5).

### Phase 3 — Architecture Research — `COMPLETE` — `docs/verification/02_phase3_probes.md`
Research *how* to build the confirmed product; every technology and boundary
questioned against performance, Windows, offline, failure modes. Research stays
open. Input: `docs/research/01–06`, `ARQ-*`, O3/O5.
Gate: every `ARQ` answered or deferred with reason; every decision has a written
comparison + recommendation; draft ADRs in `docs/decisions/`; no code.

### Phase 4 — Adversarial Architecture Review — `COMPLETE` — `docs/verification/03_adversarial_review.md`
Try to break the proposed architecture before committing.
Gate: prioritized risk register (`docs/verification/02_adversarial_review.md`)
covering the mandated failure categories; every high risk mitigated or accepted
with rationale; architecture + ADRs updated.

### Phase 5 — Project Documentation / Architecture Freeze — `COMPLETE` — `docs/verification/04_phase5_crosscheck.md`
Officialize product + architecture: `PROJECT.md`, `ARCHITECTURE.md`,
`AI_PIPELINES.md`, `SECURITY.md`, `PERFORMANCE.md`, `UI_GUIDELINES.md`,
`DEVELOPMENT.md`; final ADRs; **re-derive `docs/plan/06–40`** against the freeze.
Gate: all docs present + mutually consistent; ADRs complete; plan docs finalized;
**no application code**.

---

### EPOCH 1 — Core Platform (Phases 6–15)

### Phase 6 — Tauri + React + Rust Bootstrap — `COMPLETE` — `docs/verification/05_phase6_bootstrap.md`
App launches to a placeholder UI; typed IPC works; fmt/lint/typecheck/test/logging
wired; deferred Phase 1 tooling + `.gitattributes` landed. No AI.
Gate: full verification run green; app launches; IPC round-trip; no model code.

### Phase 7 — Application Contracts — `COMPLETE` — `docs/verification/06_phase7_contracts.md`
Typed serializable contracts (tasks, model metadata/state, generation
requests/events, streaming, cancellation, errors, conversations, messages,
resource reservations, worker jobs). No model names in logic.
Gate: compile; serialize/deserialize round-trip; invalid rejected; documented;
`git grep` finds no model-name literals in logic.

### Phase 8 — Configuration — `COMPLETE` — `docs/verification/07_phase8_config.md`
One typed, validated, versioned config (defaults → user → session). No subsystem
invents its own storage. No secrets in source.
Gate: defaults load; invalid rejected with a named error; persists across restart;
migration works; session override works; corrupt/missing handled without crash.

### Phase 9 — SQLite Persistence — `COMPLETE` — `docs/verification/08_phase9_persistence.md`
Rust-owned persistence: migrations, versioning, pooling, transactions,
repositories, structured errors. Minimum schema. Frontend/workers never touch DB.
Gate: create from empty; migrate up/down + idempotent; commit + rollback; survives
restart; migration failure rolls back + backup; lock contention handled.

### Phase 10 — Observability — `COMPLETE` — `docs/verification/09_phase10_observability.md`
Local structured logging + diagnostics (levels, task id, model id, duration,
status, structured errors). No secrets, no conversation content by default, no
cloud telemetry.
Gate: an operation's log lines all carry its task id; redaction test passes;
level filtering works; no network egress from the logging path.

### Phase 11 — Model Registry — `COMPLETE` — `docs/verification/10_phase11_registry.md`
Model metadata as data (stable id, name, type, backend, path, capabilities,
context, quant, estimated resource need, devices, model config). No loading.
Gate: register a test model; find + read; missing file represented not crashed;
capability query; invalid metadata rejected.

### Phase 12 — Model Acquisition & Picker — `COMPLETE` (all 9 gates; gate 5 run 2026-09-06) — `docs/verification/11_phase12_acquisition.md`
In-app HuggingFace picker + one-shot resumable download for **LLM GGUF**;
checksum verify; disk-budget guard; register on completion. STT/TTS fixed models
acquired once via the same path (no picker).
Gate: pick + download + verify + register a small GGUF; resume interrupted
download; checksum mismatch rejected; insufficient disk refused pre-download;
STT+TTS models acquired via same path; picker works read-only-offline.

### Phase 13 — Resource Manager — `COMPLETE` — `docs/verification/12_phase13_resources.md`
"Can this operation safely use the GPU now?" Lifecycle request → reserve → commit
→ observe → release → reconcile. Mockable hardware. Never file-size == VRAM.
Gate (mocked): insufficient VRAM → clean failure; duplicate reservation rejected;
concurrent serialized; failed/cancelled load releases; stale reservation
recovered; crash → reconcile vs observed. **All 9 gate items pass.**

### Phase 14 — Model Lifecycle Manager — `COMPLETE` — `docs/verification/13_phase14_lifecycle.md`
The only component that loads/unloads managed models. Explicit state machine +
failure/recovery states. No duplicate loads; failed/cancelled loads release.
Gate: load; concurrent same-model load not duplicated; unload; load under
insufficient resources rejected pre-spawn; cancel mid-load releases; forced
failure → recovery; unexpected exit detected + reconciled; state-machine tests.
**All 9 gate items pass** (`FakeBackend`; real backend is Phase 15).

### Phase 15 — llama.cpp Adapter — `COMPLETE` — `docs/verification/14_phase15_llama.md`
First LLM backend behind a clean `LlmBackend` interface; llama.cpp detail confined
to the adapter; transport per the Phase 3 ADR.
Gate: startup + readiness; non-streaming + streaming generation; cancellation
reaches the job; timeout handled; crash detected + recovered; clean shutdown no
orphan; no raw config leaks past the adapter. **All 9 pass** — stub tests +
**15.D live** on the RTX 5080 (pinned prebuilt `b10819` + real Qwen 0.5B GGUF;
TTFT ≈ 23 ms, ≈ 278 tok/s). ADR-0004 amended (pinned prebuilt for v1).

---

### EPOCH 2 — First Slice & Modalities (Phases 16–22)

### Phase 16 — First Vertical Slice / Text Chat — `COMPLETE` — `docs/verification/15_phase16_slice.md`
The whole path: React → IPC → Rust → conversation service → LLM → llama.cpp →
streamed tokens → UI. Text chat only. Reliability over features.
Gate: send → streamed reply; cancel mid-gen, model reusable; conversation
persists; restart restores it; kill llama.cpp mid-gen → recovers; clean shutdown
during gen; concurrent-gen handled per policy (**reject**). **All 9 pass** — a
live full-stack Rust test on the RTX 5080; end-to-end TTFT ≈ 33 ms. **First real
milestone.**

### Phase 17 — Conversation Engine — `COMPLETE` — `docs/verification/16_phase17_engine.md`
Formalize the one shared engine (lifecycle, messages, roles, content, timestamps,
streaming state, cancellation, generation metadata, persistence). No second engine.
Gate: engine unit tests (lifecycle + streaming + cancel + persistence); text chat
re-hosted on it, Phase 16 gate still passes. **All 6 pass** — `ConversationEngine`
(one path), `send` split into `add_user_turn` + `generate`, explicit
`GenerationState`; Phase 16 live gate re-run, TTFT ≈ 44 ms.

### Phase 18 — Voice: capture · Silero VAD · faster-whisper STT — `COMPLETE` — `docs/verification/17_phase18_voice-in.md`
Mic capture → VAD endpointing → STT worker → user message on the engine.
Gate: device discovery + selection; capture → VAD → STT produces a transcript;
STT failure / worker crash / missing device handled; low-confidence dropped;
STT real-time factor within budget. **All 7 pass** — `worker/` (the shared
supervisor) + `voice/` (`cpal` + Silero-in-core + faster-whisper worker); a live
capture→transcript gate on the RTX 5080. ADR-0018 (dev `uv` venv). Config v6.

### Phase 18.5 — Deploy & Diagnostics — `COMPLETE` — `docs/verification/18_phase18.5_deploy-diagnostics.md`
Owner-requested tooling (pulled forward from Phase 37): `scripts/deploy-local.mjs`
(release build + launch in place), a persistent **rotating JSON-lines file log
sink** (`<app_data>/logs/`, redacted, swept), a `diag_export` snapshot + a
Settings button, `build.rs` git-SHA, debug-level UI breadcrumbs. All local —
the probe is a file the owner shares, never a beacon (ADR-0015). Phase 37 still
owns the MSI + clean-machine layout + a portable archive. **All 7 gate items
pass** — release build (2m31s) + file sink verified against the real binary
(redacted, no secrets); diag/Settings unit + component tested.

### Phase 19 — Voice: Chatterbox TTS · playback · barge-in — `COMPLETE` — `docs/verification/19_phase19_voice-out.md`
Assistant text → chunked TTS → playback; user speech or cancel stops LLM + TTS +
playback fast; interrupted turn persisted as truncated.
Gate: text → TTS → playback end to end; barge-in stops everything < ~200 ms and
returns to listening; TTS failure / worker crash handled; truncated turn persisted.
**All 7 pass** — `voice/{chunker,playback,tts}` + the listen→think→speak loop +
barge-in; live on the RTX 5080: 3 clauses spoken, warm RTF ≈ 0.45,
**barge-in trigger→silence ≈ 4 ms**. Config v7; `torch 2.11.0+cu128`.

### Phase 20 — Personas & Context Builder — `COMPLETE` — `docs/verification/20_phase20_personas.md`
Structured persona data + the one context builder (system + persona +
character + conversation + memory + runtime). Verifiable that persona reaches the
model.
Gate: persona stored as structured data; context-builder unit tests; a test
proves the assembled prompt contains the persona; switching persona changes
behaviour in a scripted check. **All 6 pass** — `context/` module (`persona` /
`sanitize` / `tokens` / `builder`), `V0005`, `PersonaId`, `chat_prompt_preview`
(FR-34); `conversation::prompt` deleted. 24 `context` tests + a `conversation`
prompt-carries-persona assertion.

### Phase 21 — Memory — `COMPLETE` — `docs/verification/21_phase21_memory.md`
Extraction → importance/validation → storage → relevant retrieval → context.
SQLite + FTS5 keyword retrieval; embeddings deferred (measured trigger, ADR-0012).
**All 7 gate items pass** — `memory/` module (`repo` + `extract` + `retrieve` +
`MemoryService`), `V0006` (`memory` + standalone `memory_fts`), `MemoryId`,
`contracts::memory`; builder `max_memory_tokens` sub-budget; `memory_*` IPC.

### Phase 22 — Image Generation (FLUX.1 Krea) *(current pointer)* — `NOT STARTED` — `docs/plan/22_image-generation.md`
Local image generation as a resource-managed workload. **Adapt the owner's
existing FLUX.1 Krea implementation — request it at phase entry.**
Gate: generate an image via a worker; VRAM reserved/released around the job;
binary in the file vault + metadata/path/hash in SQLite; cancel mid-gen releases;
worker crash handled.

---

### EPOCH 3 — Resource Arbitration & Characters (Phases 23–29)

### Phase 23 — Model Hot-Swapping — `NOT STARTED` — `docs/plan/23_model-hot-swapping.md`
Share limited VRAM: suspend/unload LLM → load image model → generate → release →
restore LLM. Handle runtimes that can't suspend cleanly.
Gate: swap observed via real VRAM measurement; swap refused when eviction can't
fit; swap doesn't interrupt an in-flight generation; crash during swap →
reconcile, no leaked reservation.

### Phase 24 — Scheduler Formalization — `NOT STARTED` — `docs/plan/24_scheduler.md`
One authoritative arbiter for GPU/worker/task work: queue, priorities,
cancellation, fairness, crash reconciliation.
Gate: competing GPU requests ordered by policy; cancellation removes queued or
running jobs cleanly; no two conflicting GPU jobs run at once; scheduler state
reconciles after a crash.

### Phase 25 — Character Data Model & CRUD — `NOT STARTED` — `docs/plan/25_character-data-model.md`
Characters as structured entities (identity, appearance, personality, interests,
relationship, memory links, reference images, generated images). Not a prompt.
Gate: create/read/update a character as structured data; schema migrations;
persists across restart; validation rejects malformed characters.

### Phase 26 — Character Conversations — `NOT STARTED` — `docs/plan/26_character-conversations.md`
Character conversations on the shared engine; character context assembled from
structured fields via the Phase 20 builder.
Gate: a character conversation runs on the shared engine; a test proves character
identity reaches the model; character memory persists across restart; no
second conversation engine.

### Phase 27 — Persistent Character Identity (image) — `NOT STARTED` — `docs/plan/27_character-identity.md`
Same conceptual character across generated media: reference identity → conditioning
→ generation → identity verification → accept/regenerate.
Gate: reference images stored + linked; generation conditioned on reference
identity; an identity-similarity check gates accept vs regenerate; accepted images
linked to the character in the DB.

### Phase 28 — Typed Character Image Actions — `NOT STARTED` — `docs/plan/28_typed-image-actions.md`
The LLM requests images only via a typed, validated action; Rust validates and
decides. No raw generation command from model output (`CLAUDE.md` Art. III).
Gate: well-formed action → validated → executed; malformed/unknown rejected +
logged, nothing executed; action for another character's id refused; no path
turns model text into a shell/file/process operation.

### Phase 29 — Character Discovery (swipe UX) — `NOT STARTED` — `docs/plan/29_character-discovery.md`
Browse AI-generated characters, view profile + images, choose one → conversation
bound to the persistent entity. Relationship persists on return.
Gate: discovery list renders from stored data; selecting opens a conversation
bound to that entity id; leaving + returning restores relationship + memory +
history.

---

### EPOCH 4 — Product & Hardening (Phases 30–38)

### Phase 30 — UI / UX Pass — `NOT STARTED` — `docs/plan/30_ui-ux.md`
Bring every flow (chat, voice, characters, gallery, discovery, settings) to the
`UI_GUIDELINES.md` bar: loading/empty/error/streaming states, keyboard, a11y.
Gate: every primary flow has loading/empty/error states; primary flow keyboard-
only; automated a11y scan no critical violations; light + dark pass contrast.

### Phase 31 — Performance Audit — `NOT STARTED` — `docs/plan/31_performance-audit.md`
Measure → improve → re-measure. Baselines in `PERFORMANCE.md`.
Gate: recorded baselines (startup, UI responsiveness, LLM latency + throughput,
image gen, model switching, DB ops, memory retrieval); each optimization has
before/after numbers; no regression against the Phase 16 gate.

### Phase 32 — Offline Audit — `NOT STARTED` — `docs/plan/32_offline-audit.md`
Prove the runtime is genuinely local-first.
Gate: network disabled → every core feature works; traffic capture shows no
external egress; the only network paths are explicit model/dependency acquisition
+ update check, each isolated and skippable.

### Phase 33 — Fault Injection / Reliability — `NOT STARTED` — `docs/plan/33_fault-injection.md`
Deliberately break things; confirm graceful behaviour.
Gate: scripted faults (kill llama.cpp; kill each worker; remove model file;
corrupt config; corrupt/lock DB; exhaust VRAM; cancel gen; close app mid-gen;
worker timeout; restart after crash) each have a defined, tested outcome + a
regression test.

### Phase 34 — Security Audit — `NOT STARTED` — `docs/plan/34_security-audit.md`
Threat model + resolved findings. Local ≠ safe.
Gate: `SECURITY.md` threat model; checks pass for arbitrary command execution,
model-action validation, path traversal / fs confinement, worker input
validation, safe process-arg construction, secrets absent from logs + bundle,
loopback-only services; no open high/critical findings.

### Phase 35 — Dependency Audit — `NOT STARTED` — `docs/plan/35_dependency-audit.md`
Every dependency justified, license-compatible, vulnerability-scanned.
Gate: inventory with one-line justification each; license check clean;
`cargo audit` / `npm audit` / Python audit clean or triaged; unused removed.

### Phase 36 — Maintainability / Architecture Audit — `NOT STARTED` — `docs/plan/36_maintainability-audit.md`
Codebase still matches the frozen architecture — no drift, no duplicate authority.
Gate: subsystem-vs-`ARCHITECTURE.md` trace, no undocumented component; no
duplicated state/logic across runtimes; Article I boundaries hold; ADRs exist for
anything that changed during implementation.

### Phase 37 — Packaging (Windows installer) — `NOT STARTED` — `docs/plan/37_packaging.md`
Installer bundling the app, native deps, workers; model acquisition + offline
runtime.
Gate: clean-machine install → working app; workers + native runtimes launch from
the installed layout; first-run model acquisition works + skippable; uninstall
clean; installed app passes the Phase 32 offline gate.

### Phase 38 — Final Architecture Audit — `NOT STARTED` — `docs/plan/38_final-architecture-audit.md`
Last whole-system review before release readiness.
Gate: audit gates 31–36 still green; `ARCHITECTURE.md` / `AI_PIPELINES.md` /
`PROJECT.md` reflect the shipped system; a fresh reader can trace a chat request
and an image request end to end from the docs.

---

### EPOCH 5 — Operationalization & Release (Phases 39–40)

### Phase 39 — Permanent Claude Workflow — `NOT STARTED` — `docs/plan/39_permanent-workflow.md`
Convert the bootstrap discipline into the steady-state process for post-release
development.
Gate: `DEVELOPMENT.md` documents the permanent loop; `CLAUDE.md` updated to
post-bootstrap mode; a sample change run through the loop with evidence.

### Phase 40 — Git Strategy · Context Efficiency · Final Release Gates — `NOT STARTED` — `docs/plan/40_closing.md`
Formalize branching/commit/tag rules (+ hook); review docs for concision + the
fresh-session test; run the release checklist.
Gate: git rules written + hook-enforced; remote/push policy decided + recorded;
fresh session states status + next action correctly from the repo alone; every
phase `COMPLETE`; Phase 16/32/33/37 gates re-run green on the packaged build;
`PROJECT.md` feature list verified item by item; known-issues list published.

---

## 5. Transition Log

Newest first. One line per state transition (§3 rule 6).

| Date       | From | To | By | Note |
| ---------- | ---- | -- | -- | ---- |
| 2026-09-06 | (no state change) | — | live-test-fixes | **Post-Phase-21 live-testing fixes** (`main`, on top of `36ae6b7`). (1) **Release deploy loaded the dev URL** — the Phase-6 scaffold's `src-tauri/Cargo.toml` lacked Tauri's `custom-protocol` feature, so `deploy-local.mjs`'s plain `cargo build --release` ran the binary in dev mode → a connection-refused page in the window. Added `[features] custom-protocol` (not `default`, so bare `cargo test`/`clippy` still don't need `../dist`) + `--features custom-protocol` in the deploy script. (2) **Spawned children popped a console window** on Windows — `job::hide_console` / `hide_console_std` (`CREATE_NO_WINDOW`) at every spawn (`llm/server`, `worker`, `diag`). (3) **Register a local GGUF** — a filename field on the Models page (the `model_register_local` IPC existed, no UI). (4) **UI freshen-up** — `--surface`/`--surface-hover` tokens (Models/Chat CSS referenced them unset); global `input`/`select`/`textarea` styling + token select-chevron; `Settings.tsx` off inline styles → `Settings.css`; tighter Models/chat bar. (5) **ChatVoice "New chat"** — the app only ever opened the latest conversation, so the persona picker (locked once a turn exists, FR-17) could never be changed; also the persona list refetches on New chat + on select focus. (6) **Config schema v8** — `voice.end_of_speech_ms` (300..=5000, default 900, up from the VAD's 700) + `ConfigKey::VoiceEndOfSpeechMs`; a "Voice" section in Settings sets it via `config_set` (persist); `lib.rs::start_voice` feeds it to `VadConfig.min_silence_ms` — the VAD end-of-speech hang time, so the user can pause between sentences. Applies on next launch. 330 rust tests, 18 vitest, check suite green. No new ADR. |
| 2026-09-06 | Phase 21 / 21.1 `IN PROGRESS` | Phase 21 `COMPLETE` → Phase 22 / 22.1 `NOT STARTED` | phase-21-memory | **Persistent per-Persona memory landed.** New `src-tauri/src/memory/` — `repo` (`memory` table + a **standalone** `memory_fts` FTS5 table, `V0006`; both written in one tx, no triggers; `insert(NewMemory, cap)` deletes the single lowest-priority row — `importance ASC, created_at ASC` — when `scope` is at `cap`; `search(scope, fts_query, k)` = `memory_fts MATCH ? AND scope = ? ORDER BY bm25()`; `list`/`get`/`delete`/`count`), `extract` (`run_extraction` — a fixed ChatML schema prompt, `llm.generate` temp 0 / 256 tok / seed 0, `json_object_span` + strict `serde_json` parse of `{memories:[{content,kind,importance}]}`, unknown `kind` dropped + `importance` clamped; `validate` — `importance ≥ MIN_IMPORTANCE` (3), 3..=500 chars after `strip_control`, **word-set Jaccard ≥ 0.6** vs the scope's FTS candidates → drop; else a `NewMemory` with provenance), `retrieve` (`fts_query_for` — lowercase, split on non-alphanumerics, drop a small stopword set + <3-char tokens, **sort + dedup** → order-independent, cap 24 terms, `"t" OR …`), `mod` (`MemoryService::new(db, lifecycle)` — `retrieve` is a **pure read** on the response path + a `target:"memory"` recall-gap log; `spawn_extraction(model_id, &scope, exchange)` fire-and-forget behind a `Semaphore(1)` sharing the loaded model via `lifecycle.begin_use`; `CancellationToken` for `shutdown`; `MemoryScope::Persona(PersonaId)` → `"persona:<id>"`; `list`/`delete` for FR-52/53). `V0006__memory.sql`. `contracts::ids::MemoryId`; `contracts::memory` (`MemoryKind` = Fact/Preference/Event/Trait, `Memory` = the view row). **Context builder** (Phase 20 module, additive): `TokenBudget { max_prompt_tokens, max_memory_tokens }` + `MEMORY_CONTEXT_FRACTION` 0.15 (memory ≤ 0.15×ctx, ≤ half the prompt budget); `system_block` includes ranked memories from the front within the sub-budget, `Provenance.memory_*` truthful, lowest-ranked dropped first, memory still shed before the persona is truncated. **Engine**: `ConversationEngine::new` builds `MemoryService` internally (signature unchanged — Phase 20 pattern); `build_prompt` retrieves per-Persona memory (`last_user_text` = the query; no `persona_id` ⇒ no memory, FR-56); `run_generation` → `maybe_extract_memory` after a clean completion (`EndOfText`/`MaxTokens`/`StopSequence`) with a persona; `shutdown` cancels extraction. IPC: `memory_list(persona_id)`, `memory_delete(id)`. Frontend: `ipc.ts` + `contracts.ts` + `setup.ts` + bindings (`MemoryId`/`MemoryKind`/`Memory`); a Memories section (persona picker → rows → delete) in `Settings.tsx`; `ChatVoice.tsx` "Show prompt" shows `memory N (M tok)`. **All 7 gate items PASS** — 16 `memory` tests (repo round-trip + FTS sync + scope isolation + prune-at-cap + restart; `fts_query_for` order-independence + caps; `validate` importance/length/dedup; `run_extraction` parse of good/noisy/garbage/empty JSON) + 3 `context::builder` memory tests (ranked + budget-capped, injection inert, dropped-before-persona) + 2 `conversation` end-to-end (a persona turn extracts a memory that reaches the next prompt via the scripted adapter + `preview_prompt`; a no-persona conversation stores nothing — `SELECT count(*) FROM memory` = 0). 328 rust tests (9 ignored live), 15 vitest, eslint + tsc + build + check suite green. **No new ADR** (ADR-0012 fixes the design; finalisations — standalone FTS5 synced by the repo, tunables as module constants, Jaccard dedup, pure-read retrieval, `Semaphore(1)` — recorded in ADR-0012 + the verification doc). **No config schema change, no new crate** (FTS5 verified in the bundled `rusqlite`). Recall-gap samples: none in the test corpora; the log is live for real use. Evidence `docs/verification/21_phase21_memory.md`. |
| 2026-09-06 | Phase 21 `NOT STARTED` | Phase 21 / 21.1 `IN PROGRESS` | phase-21-memory | Phase entry: `docs/plan/21_memory.md` finalized (7 steps, 7 gate items) against ADR-0012 + `AI_PIPELINES.md` §9. **No split.** New `src-tauri/src/memory/` — `repo` (`memory` table + a **standalone** `memory_fts` FTS5 index, both written in one tx — no SQL triggers; `insert` prunes the lowest-priority row when a scope hits `PER_SCOPE_CAP=500`; `search` = BM25 scope-filtered), `extract` (post-turn schema-constrained `llm.generate` → strict-JSON `{memories:[{content,kind,importance}]}` → importance≥3 + dedup [`near_match` BM25] + length gate → store with provenance), `retrieve` (`fts_query_for` — lowercase, stopword-drop, **sorted unique** terms `OR`-joined → deterministic), `mod` (`MemoryService` — `retrieve` is a pure read on the response path; `spawn_extraction` is fire-and-forget behind a `Semaphore(1)`, 2nd request dropped; `MemoryScope::Persona(PersonaId)`). `V0006__memory.sql`. `contracts::ids::MemoryId`; `contracts::memory` (`MemoryKind`, `Memory` — the FR-52 view row). **Context builder change** (Phase 20 module, additive): `TokenBudget { max_prompt_tokens, max_memory_tokens }` + `MEMORY_CONTEXT_FRACTION=0.15`; the `system_block` memory loop includes ranked memories from the front within the sub-budget, `Provenance.memory_*` truthful; lowest-ranked dropped first. **Engine**: `ConversationEngine::new` builds `MemoryService` internally (`new` signature unchanged — Phase 20 `PersonaRepo` pattern); `build_prompt` retrieves when the conversation has a `persona_id` (no persona ⇒ no memory, FR-56); `run_generation` spawns extraction on a clean completion with a persona; `shutdown` cancels it. IPC: `memory_list(persona_id)`, `memory_delete(id)`. A Memories list (persona picker → rows → delete) in `Settings.tsx`; "Show prompt" surfaces `memory_items` / `memory_tokens`. **No new ADR** (ADR-0012 fixes the design), **no config schema change** (tunables are module constants — precedent: the builder's `RESPONSE_RESERVE` / `BUDGET_MARGIN`), **no new crate** (FTS5 verified present in the bundled `rusqlite`). Recall-gap samples logged `target:"memory"` to inform the deferred embeddings decision. |
| 2026-09-06 | Phase 20 / 20.1 `IN PROGRESS` | Phase 20 `COMPLETE` → Phase 21 / 21.1 `NOT STARTED` | phase-20-personas | **Personas & the one context builder landed.** New `src-tauri/src/context/` — `persona` (`Persona` + `PersonaDraft` + `PersonaRepo` — all `persona` SQL, `V0005`), `sanitize::strip_control` (regex `<\|[^\|>]*\|>` → space + whitespace-normalise; every untrusted string passes through it — SECURITY C2), `tokens::estimate_tokens` (`max(⌈chars/4⌉, ⌈words·0.75⌉)`, deterministic, no tokenizer dep), `builder::ContextBuilder` (`build(BuildInput) -> BuiltPrompt`: one ChatML `system` turn = base system → persona prose → [character P26] → [memory P21] → `Current date:`; then history newest-first while it fits, kept contiguous, `history_turns_dropped` recorded; shed order when the system block alone is over budget = drop memory → truncate persona (name+summary+clamped personality) → drop persona → **never** the base system; `total_tokens ≤ budget_tokens` by construction; `Provenance` logged `target:"context"`). `V0005__personas.sql` — `persona` table + `conversation.persona_id` (`REFERENCES persona(id) ON DELETE SET NULL`). `contracts::ids::PersonaId`; `Conversation.persona_id: Option<PersonaId>` (additive). `ConversationRepo::{persona_id, set_persona}` — `set_persona` → `Conflict` once the conversation has a message (FR-17), `NotFound` for an unknown persona. `ConversationEngine` now holds the `PersonaRepo` + a `ContextBuilder` (constructed from the same `Db` + the `ModelRegistry`, for `context_tokens`); `stream_once` → `build_prompt` (batches persona + history + model reads → `build`); `conversation::prompt` (`render_chatml`) **deleted**, its tests moved to `context::builder::tests`. `preview_prompt` + **`chat_prompt_preview(conversation_id, model_id) -> PromptPreview{prompt, provenance}`** (FR-34). IPC: `persona_list/get/create/update/delete`, `conversation_set_persona`, `conversation_create(persona_id?)`. Frontend: `ipc.ts` + `contracts.ts` + `setup.ts` + bindings (`Persona`, `PersonaDraft`, `PersonaId`, `PersonaInclusion`, `Provenance`, `PromptPreview`, `Conversation` regen); a persona list/create/edit form in `Settings.tsx`; a persona `<select>` (disabled once messages exist) + a "Show prompt" `<details>` in `ChatVoice.tsx`. **All 6 gate items PASS** — 24 `context` tests (determinism byte-for-byte, assembly order, `<\|im_end\|>` in a persona/history string can't open a turn, 50-turn history vs a tiny budget drops the oldest, huge persona → `Truncated` with the base system kept, `total_tokens ≤ budget` across 5 budgets, switching persona changes the prompt) + `conversation::tests` — the prompt the scripted `LlmInstance` receives contains the active persona's summary + personality; `set_persona` fixed-after-a-turn. 304 rust tests (9 ignored live), 14 vitest, eslint + tsc + build + check suite green. **No new ADR** (`AI_PIPELINES.md` §6 fixes the builder; the token-budget split is recorded in the verification doc, not an ADR). Evidence `docs/verification/20_phase20_personas.md`. |
| 2026-09-06 | Phase 20 `NOT STARTED` | Phase 20 / 20.1 `IN PROGRESS` | phase-20 | Phase entry: `docs/plan/20_personas.md` finalized (7 steps, 6 gate items). **No split.** New `src-tauri/src/context/` — `persona` (`Persona` + `PersonaRepo` — the `persona` table, CRUD), `sanitize` (`strip_control` — removes ChatML control tokens from every untrusted string, SECURITY C2), `tokens` (`estimate_tokens` — chars/4 heuristic, no tokenizer dep), `builder` (`ContextBuilder::build(BuildInput) -> BuiltPrompt` — deterministic; assembly order `system → persona → [character P26] → [memory P21] → runtime` inside one ChatML system turn, then token-budgeted history oldest-dropped-first; `Provenance` logged at `target:"context"`). `V0005__personas.sql` (the `persona` table + `conversation.persona_id` FK, `ON DELETE SET NULL`). `contracts::ids::PersonaId`. `ConversationRepo::{persona_id, set_persona}` (`set_persona` rejected once a turn exists — FR-17). Engine: `stream_once` batches persona + history + model reads → `build` (replaces the Phase 16 `conversation::prompt::render_chatml`, which is **deleted**). IPC: `persona_*` CRUD, `conversation_set_persona`, `conversation_create(persona_id?)`, and **`chat_prompt_preview`** (FR-34 — returns the exact assembled prompt + provenance). Persona list/form in `Settings.tsx`; a picker + "Show prompt" in `ChatVoice.tsx`. **No new ADR** — `AI_PIPELINES.md` §6 fixes the design; token-budget split resolved (memory shed first, then persona-truncation, never the base system; history fills the rest newest-first). |
| 2026-09-06 | Phase 19 / 19.A.1 `IN PROGRESS` | Phase 19 `COMPLETE` → Phase 20 / 20.1 `NOT STARTED` | phase-19 | **Voice-out landed — voice is end to end.** `voice/` grows `chunker` (streamed LLM `TokenDelta`s → complete clauses; end-of-buffer boundary waits for the next push/flush; numbers not split), `playback` (`cpal` **output** stream on a parked thread + a PCM queue; `enqueue` resamples 24 kHz → device rate; `stop()` clears the queue → silent within one output buffer ≈ 10–20 ms; `OutputDevice` + `list_output_devices`), `resample::Resampler16` (generic mono in→out), `tts::TtsOutput` (the `WorkerKind::Tts` supervisor + `Playback`; `speak_stream(clause_rx, cancel)` → worker → temp WAV → enqueue, ~1 clause lookahead; `TtsResult`). `VoiceState::{Thinking,Speaking,Interrupting}`. `VoiceInput::start_listening(id, model_id: Option<ModelId>)` — `Some` runs the **loop** (`Listening → Transcribing → add_user_turn → Thinking → generate [token stream feeds the chunker→TTS] → Speaking → drain → Listening`, until `stop_listening`); the capture stream stays live through `Speaking` for full-duplex. **Barge-in** (VAD `SpeechStart` while answering, after a 500 ms grace so the tail of the user's own utterance doesn't self-interrupt — or an explicit stop): `engine.cancel(task_id)` (engine persists the accumulated text with `stop_reason=Cancelled`) → `tts.cancel()` → `playback.stop()` → await → `Listening`, `trigger_to_silence_ms` logged. VAD onset threshold raised by `voice.vad.playback_duck` (0.2) while speaking (echo duck). Config schema **v7** (`voice.output_device` + `ConfigKey::VoiceOutputDevice`; forward step `6→7`). `voice_output_devices` IPC; `voice_start` gains `model_id`; `lib.rs` wires the TTS `WorkerSupervisor` (`LOCALAI_TTS_MODEL_DIR`). `workers/tts.py` — `ChatterboxTurboTTS.from_local(models/tts, device="cuda")`, 24 kHz, default voice from `conds.pt`; one clause → a `PCM_16` WAV via `soundfile`; **`sys.stdout` → `stderr`** (the protocol writes to a saved handle) so perth / s3tokenizer status prints don't corrupt the JSON-lines stream — same guard added to `stt.py`. `workers/tts_fake.py` (stdlib sine) for CI. venv: **`torch 2.11.0+cu128`** (`chatterbox-tts` hard-pins `torch==2.6.0` whose CUDA wheels predate Blackwell sm_120; `workers/overrides.txt` + `uv … --override`) + `chatterbox-tts 0.1.7` + `soundfile`; faster-whisper (CTranslate2, own CUDA-12 libs) unaffected — verified. `ChatVoice.tsx` — the mic is a **click-toggle** (start/stop a session); with a model loaded it's a hands-free conversation, reply generated + spoken server-side. `eslint.config.js` ignores `.venv/` (chatterbox pulls gradio → JS). **All 7 gate items PASS** — 30 voice unit tests (real Silero VAD + stdlib fakes + a scripted `LlmInstance`: the conversation loop, barge-in truncation, the chunker, playback) + `voice::live_tests` (`#[ignore]`, `LOCALAI_RUN_VOICE_LIVE`) on the RTX 5080: `voice_live_tts_speaks` — real Chatterbox, **3 clauses spoken**, first request 12 s (model-load-dominated), Chatterbox **warm RTF ≈ 0.45**, **barge-in trigger→silence ≈ 4 ms**; 18.B re-run still green. Truncation decision (recorded): persisted = generated-so-far, spoken = a clause prefix of it. **No new ADR** (`stop_reason=Cancelled` is the frozen Phase 17 signal). 277 rust tests (11 ignored live), 10 vitest, check suite green. Evidence `docs/verification/19_phase19_voice-out.md`. |
| 2026-09-06 | Phase 19 / 19.1 `NOT STARTED` | Phase 19 / 19.A.1 `IN PROGRESS` (split 19.A / 19.B) | phase-19 | Phase entry: `docs/plan/19_voice-out.md` finalized. **Split** (precedent: 15 / 18). **19.A** — the Rust half, verifiable against a stdlib fake TTS worker: `voice/chunker` (clause splitter driven by LLM token arrival), `voice/playback` (`cpal` **output** stream on a parked thread + PCM queue + `stop()` → silence + 24 kHz→device resample), `voice/tts` (`TtsOutput` — the TTS `WorkerSupervisor` + playback; `speak_stream` pulls clauses → worker → temp WAV → enqueue, ~1 clause lookahead), `VoiceState::{Thinking,Speaking,Interrupting}`, `start_listening(id, model_id: Option)` → the listen→think→speak→listen loop, and the **barge-in 5-step sequence** (`engine.cancel` → `tts.cancel` → `playback.stop` → await → `Listening`, timed) triggered by VAD onset while `Speaking` or an explicit stop. Config **v7** (`voice.output_device`, `voice.vad.playback_duck`). **19.B** — the real `workers/tts.py` (Chatterbox Turbo, `models/tts/`) + the live speak / barge-in gate on the RTX 5080 — **blocked** on adding PyTorch + the Chatterbox stack (~3–4 GB) to the `.venv` (owner go-ahead; the venv today has CTranslate2 for STT, not torch). **No new ADR** — `stop_reason = Cancelled` (frozen Phase 17) is the truncation signal: persisted = generated-so-far, spoken = a clause prefix of it. Echo: headphones / PTT for v1, VAD onset ducked while `Speaking`; AEC deferred. |
| 2026-09-06 | Phase 18.5 / 18.5.1 `IN PROGRESS` | Phase 18.5 `COMPLETE` → Phase 19 / 19.1 `NOT STARTED` | phase-18.5 | **Deploy & diagnostics landed.** `logging::enable_file_sink` — rotating daily JSON-lines under `<app_data>/logs/localai.jsonl.<date>`, fed the same already-redacted line as stdout via `RedactWriter`; startup `sweep_logs` (keep 7 / ~50 MB, never the last). `build.rs` → `LOCALAI_GIT_SHA`. New `src/diag.rs` — `DiagSnapshot` (build / effective config redacted / registry / resource snapshot / lifecycle / conversation **metadata only** / recent redacted log lines / `nvidia-smi` + OS); `collect()` + `export()` → `<app_data>/diagnostics/diag-<ts>.json`; `diag_snapshot` / `diag_export` IPC. `Settings.tsx` — real page now (read-only effective config + Export-diagnostics button). `ChatVoice.tsx` — `log.info('ui', …)` breadcrumbs (model load/unload, send, voice start/release). `scripts/deploy-local.mjs` — `npm run build` + `cargo build --release` + launch in place; prints git SHA + config/logs/diagnostics paths (`--no-launch` builds only). **All 7 gate items PASS** — `deploy-local.mjs --no-launch` built the release binary in **2m31s** (git_sha `26e2607`); the real release binary boots and writes `%APPDATA%\com.localai.app\logs\localai.jsonl.2026-09-06` (JSON, "log file sink enabled" first line, **no secrets**); `sweep_logs` + `redact_value` + `diag` types unit-tested; `Settings.test.tsx` covers the button. 265 rust tests (8 ignored live), 11 vitest, check suite green. All local (ADR-0015) — no new ADR. Phase 37 still owns the MSI + embedded CPython + sibling layout + first-run acquisition + code signing + a portable archive. Evidence `docs/verification/18_phase18.5_deploy-diagnostics.md`. |
| 2026-09-06 | Phase 19 / 19.1 `NOT STARTED` | Phase 18.5 / 18.5.1 `IN PROGRESS` (insert before 19) | phase-18.5 | **Owner-requested insert.** After live-testing chat + voice by hand, the owner asked for a repeatable way to run the *real* (release) build with the real settings, plus **local** probes the agent can read after a session it did not watch. New `docs/plan/18.5_deploy-diagnostics.md`. Pulls forward the "log file / rotation / diagnostics bundle" work that `docs/decisions/README.md` had earmarked for **Phase 37** — Phase 37 still owns the MSI, embedded CPython, sibling layout, first-run acquisition, code signing, and a *portable* archive. Scope: (1) `scripts/deploy-local.mjs` — `npm run build` + `cargo build --release` + launch in place, prints build SHA + log/diag paths; (2) `logging::enable_file_sink` — rotating daily JSON-lines under `<app_data>/logs/`, reuses `RedactWriter`, startup sweep (keep 7 / ~50 MB); (3) `diag_export` — one JSON file (`<app_data>/diagnostics/diag-<ts>.json`) with build / effective config (redacted) / registry / resource snapshot / lifecycle / recent log lines / host facts (`nvidia-smi`, OS) — **no conversation content**; a Settings button triggers it; (4) `build.rs` git-SHA env; (5) debug-level UI breadcrumbs via the existing `frontend_log` bridge. **No new ADR** — implements the frozen logging + Phase-37-bundle policy; ADR-0015 already forbids any network path (the probe is a file the owner chooses to share). |
| 2026-09-06 | Phase 18 / 18.A.1 `IN PROGRESS` | Phase 18 `COMPLETE` → Phase 19 / 19.1 `NOT STARTED` | phase-18 | **Voice-in landed.** New `src/job.rs` (`JobObject` promoted out of `llm/`). New `src-tauri/src/worker/` — the shared stdio JSON-lines supervisor (Job Object + ADR-0015 env in `env.rs` + `WorkerHello` version/kind check + `WorkerJobId` req/resp mux + `Progress` sink + cancel-abandons-wait + bounded-backoff restart → `Failed`; `layout.rs` resolves interpreter/script dev-vs-ship; `with_env` for per-worker vars). New `src-tauri/src/voice/` — `capture` (`cpal`/WASAPI on a parked thread), `resample` (`rubato` → 16 kHz mono + downmix), `vad` (Silero v5 ONNX via `ort` load-dynamic; 512-sample windows + carried LSTM state; `SpeechStart/SpeechEnd/MaxDurationCut` FSM; `force_endpoint` for PTT release), `segment` (endpointed `s16le` WAV under the cache dir), `mod` (`VoiceInput` — one PTT session; capture→VAD→segment→STT worker→`add_user_turn(Text)`; low-confidence drop policy — empty / `no_speech_prob>0.6` / `avg_logprob<-1.0` → no turn; `watch<VoiceState>`). `workers/stt.py` (faster-whisper `large-v3` fp16 CUDA; ADR-0015 env asserted; CUDA-12/cuDNN-9 DLL path primed) + `workers/stt_fake.py` (stdlib, for CI). **ADR-0018** — dev `uv` venv at `<repo>/.venv` from a pinned `workers/requirements.txt` (`scripts/setup-venv.mjs`; gitignored ~2.2 GB). `models/vad/silero_vad.onnx` (snakers4 v5.1.2, sha `2623a295…`, gitignored). Config schema **v6** — `workers.{dir,python}` + `voice.input_device`; `ConfigKey` → 9. IPC: `voice_input_devices` / `voice_start`(Channel<VoiceState>) / `voice_stop` / `voice_state` + `chat_generate` (reply over existing history — post-voice-turn); `ChatVoice.tsx` hold-to-talk mic + `VoiceState` indicator + auto-reply. Deps: `cpal 0.15`, `rubato 0.15`, `hound 3.5`, `ort 2.0-rc` (load-dynamic). **All 7 gate items PASS** — 25 unit tests (real Silero VAD against a SAPI-synthesised WAV fixture + stdlib fake worker, no venv/GPU) + `voice::live_tests` (`#[ignore]`, `LOCALAI_RUN_VOICE_LIVE`) on the RTX 5080: recorded WAV → real capture → real VAD endpoint → real `stt.py` → "Hello local AI, this is a voice input test." → one `Text` user turn; 7.9 s audio / 3.36 s wall incl. ~2.2 s model load; RTF ≪ 1.0. 251 rust tests, 10 vitest, check suite green. Owner cleared both downloads ("Download fresh versions"). Voice turns persist `Text` in v1 (→ `Audio{asset,transcript}` when the blob store lands; additive, no migration). Evidence `docs/verification/17_phase18_voice-in.md`. |
| 2026-09-06 | Phase 18 `NOT STARTED` | Phase 18 / 18.A.1 `IN PROGRESS` (split 18.A / 18.B) | phase-18 | Phase entry: `docs/plan/18_voice-in.md` finalized. Open ADR questions resolved from ADR-0005 — **batch per VAD segment** (no streaming partials in v1), **push-to-talk** (open-mic deferred). **Split** (precedent: Phase 15): **18.A** = the Rust half — new `src-tauri/src/worker/` (the shared stdio JSON-lines supervisor actor per ADR-0013: Job Object, `WorkerHello` version check, `WorkerJobId` req/resp map, ADR-0015 hostile-network env centralised, bounded restart) + new `src-tauri/src/voice/` (`cpal` 16 kHz capture, `AudioRing`, Silero VAD via `ort`, segment assembly, `VoiceInput` orchestration → `ConversationEngine::add_user_turn(Text)`), config **v6** (`voice.*`, `workers.{dir,python}`), voice IPC + `ChatVoice.tsx` push-to-talk — all verifiable now against `workers/stt_fake.py` (stdlib). **18.B** = the real `workers/stt.py` (faster-whisper `large-v3` fp16) + the live capture→transcript gate on the RTX 5080 — **blocked** on a dev `uv` venv (**ADR-0018**, drafted 18.A.7) and owner go-ahead for ~1 GB of `nvidia-cudnn/cublas-cu12` wheels. Also needs `silero_vad.onnx` (~2 MB). Voice turns persist `Text` in v1 (→ `Audio{asset,transcript}` when the blob store lands; additive). |
| 2026-09-06 | Phase 12 gate 5 `NOT EXECUTED` (deferred) | Phase 12 gate 5 `DONE` — Phase 18 unblocked | phase-12-gate5 | Owner go-ahead "Download fresh versions" (keep in-project, short folders). `acquire_fixed` updated: fixed bundles land in a short `dir` (`stt` / `tts`) under the model dir instead of the sanitized repo name. New `#[ignore]`d live test `live_acquire_fixed_models`: pulled `Systran/faster-whisper-large-v3` (5 files, 2.9 GB, 28.1 s) → `models\stt\model.bin` and `ResembleAI/chatterbox-turbo` (11 files, 3.8 GB, 38.3 s) → `models\tts\t3_turbo_v1.safetensors`, both registered (`Stt` / `Tts`, `Ready`, path confined). In-project, gitignored. Same multi-file engine as gate 1. **Phase 12 now all 9 gates pass.** 233 rust tests, check suite green. Evidence `docs/verification/11_phase12_acquisition.md` gate row 5. |
| 2026-09-06 | Phase 16 `NOT STARTED` | Phase 16 / 16.1 `IN PROGRESS` | phase-16 | Phase entry: `docs/plan/16_vertical-slice-text-chat.md` finalized (12 steps, 9 gate items). New `src-tauri/src/conversation/` — thin service (`repo` = all SQL, `prompt` = ChatML render, `mod` = `ConversationService` + in-flight registry). V0004 migration (`conversation` + `message`). Flow: `chat_send` persists the user msg, spawns a task (`begin_use` → render → `LlmInstance::stream` → forward `GenerationEvent`s over a Channel → persist assistant msg on terminal → `end_use`), returns a `TaskId`. **Concurrency = reject** (one model / one slot → 2nd `chat_send` = `Conflict`; queue is Phase 24; no ADR). Cancellation via a `CancellationToken` per in-flight gen. `AcquisitionService::register_local_gguf` + `model_register_local`/`model_load`/`model_unload` IPC. `ChatVoice.tsx` becomes the real chat page. Exit hook cancels the in-flight gen + unloads models. No new ADR; not a second engine (Phase 17 generalizes). |
| 2026-09-06 | Phase 15 `COMPLETE` (split — 15.D deferred) | Phase 15 `COMPLETE` (all gates) | phase-15 | **Plan 15.D run.** Owner call: a **pinned official prebuilt** instead of the ADR-0004 source build — **ADR-0004 amended** (prebuilt = primary path for v1, source build stays documented). Pin: `ggml-org/llama.cpp` `b10819` (commit `6a1a922d2`), `llama-b10819-bin-win-cuda-13.3-x64.zip` (sha `c9069222…`) + `cudart-…-13.3-x64.zip` (sha `1462a050…`); CUDA 13.3 → Blackwell sm_120 OK. Config schema **v5**: `runtimes.dir` (`RuntimesConfig`, `ConfigKey::RuntimesDir`, session override, default `<app_data>/runtimes`). Owner wants no runtime blobs outside the repo → the machine's `config.json` points `runtimes.dir` + `models.dir` at `<repo>/runtime/llama-server` + `<repo>/models` (both `.gitignore`d, ~1 GB); DB + config.json stay in `%APPDATA%`. `--flash-attn` dropped from the argv (`b10819` made it take an arg; `auto` default is right). 4 `#[ignore]`d live tests (`llm::live_tests`, env-var-gated) on the RTX 5080: **gate 1** load+`/health` ≈ 775 ms · **gate 2** non-stream "red, blue, yellow" + 12-delta stream · **gate 3** cancel → `Cancelled`, slot frees (next gen works) · **gate 5** `taskkill` → `health()` Err · **gate 6** no orphan (`tasklist`) · **gate 8** TTFT ≈ 23 ms, ≈ 278 tok/s (Qwen 0.5B Q4_K_M). WDDM hang not observed over 4 cycles. 212 rust tests (+3; 5 ignored), check suite green, `tauri dev` registers the backend. Evidence `docs/verification/14_phase15_llama.md`. |
| 2026-09-06 | Phase 17 / 17.1 `IN PROGRESS` | Phase 17 `COMPLETE` → Phase 18 / 18.1 `NOT STARTED` | phase-17 | Conversation engine formalized. `ConversationService` → **`ConversationEngine`** (rename; `git grep` clean — one path since Phase 16). `send` split into `add_user_turn(id, content: MessageContent)` (typed — voice/STT seam) + `generate(id, model, sink) -> TaskId`; `send` stays a text convenience. Explicit **`GenerationState`** (`Mutex<Option<Running>>` carrying `GenerationHandle { task_id, conversation_id }`; `generation_state()` → `Idle`/`Generating`; `chat_state` IPC + `chatState()` wrapper). `render_chatml` now renders `MessageContent::Audio { transcript: Some }` too (skips `Image` / untranscribed audio) — voice turns render with no engine change. Contracts (additive): `GenerationHandle`, `GenerationState`. **All 6 gate items PASS** — 19 unit/contract tests (scripted `LlmInstance`; state-machine, `add_user_turn`+`generate` split, typed-audio render) + the Phase 16 `#[ignore]`d live test re-run on the RTX 5080 with a `generation_state` assertion (register → real `llama-server` → "pong" → state `Generating`→`Idle` → restart recovery → cancel → reuse). **End-to-end TTFT ≈ 44 ms** (Phase 16 ≈ 33 ms; timer noise, no regression). 226 rust tests, 9 vitest, check suite green. **No new ADR** — content taxonomy resolved by the frozen Phase 7 contract (media = content-addressed blob via `AssetId`; blob store lands with the first blob feature, P18/P22). Evidence `docs/verification/16_phase17_engine.md`. |
| 2026-09-06 | Phase 17 `NOT STARTED` | Phase 17 / 17.1 `IN PROGRESS` | phase-17 | Phase entry: `docs/plan/17_conversation-engine.md` finalized (7 steps, 6 gate items). The Phase 16 `conversation/` module *is* the single path — Phase 17 reshapes it: `ConversationService` → **`ConversationEngine`**; `send` splits into `add_user_turn(id, content: MessageContent)` + `generate(id, model, sink)` (voice will use the two separately; `send` stays a text convenience); an explicit `GenerationState` (`Idle` / `Generating { task_id, conversation_id }`) + `chat_state` IPC; `render_chatml` also renders `Audio { transcript: Some }`. **No new ADR** — the message-content taxonomy is the frozen Phase 7 contract (media = content-addressed blob via `AssetId`; blob store lands with the first blob feature). |
| 2026-09-06 | Phase 16 / 16.1 `IN PROGRESS` | Phase 16 `COMPLETE` → Phase 17 / 17.1 `NOT STARTED` | phase-16 | **First vertical slice landed.** New `src-tauri/src/conversation/` — `repo` (all SQL for `conversation` + `message`, `V0004`), `prompt` (`render_chatml` — minimal, Phase 20 does the real builder), `mod` (`ConversationService`: `send` persists the user turn, spawns a generation that streams `GenerationEvent`s to a sink + accumulates + persists the assistant turn + `end_use`; **concurrency = reject** — a 2nd `send` → `Conflict`, a queue is Phase 24; `cancel` trips a per-generation `CancellationToken`; `shutdown` for the exit hook). `lifecycle::backend` gained `LlmInstance` + `Completion` (moved from `llm/`) with `LoadedInstance::as_llm()` default `None` (replaces the Phase-15 `Any` downcast); `LifecycleManager::instance()` + `unload_all()`. `acquisition::register_local_gguf`. IPC: `conversation_create`/`list`/`messages`, `chat_send`(Channel)/`chat_cancel`, `model_register_local`/`load`/`unload`. `ChatVoice.tsx` rebuilt (model bar, streaming transcript, composer/Stop). `lib.rs` `setup()` → free fn; exit hook cancels the in-flight gen + unloads. **All 9 gate items PASS** — 7 unit tests (scripted `LlmInstance`) + 1 `#[ignore]`d live test running the whole real stack on the RTX 5080 (register local GGUF → real `llama-server` load → send "pong" → stream → persist → fresh-service restart recovery → cancel a long gen → partial persisted → model reused) + a `ChatVoice` component test. **End-to-end TTFT ≈ 33 ms.** 220 rust tests, 9 vitest, check suite green, `tauri dev` clean (V0004 applied). No new ADR. Bug fixed mid-phase: a `tokio::join!(stream, recv)` deadlock (recv now breaks on the terminal frame). Evidence `docs/verification/15_phase16_slice.md`. |
| 2026-09-06 | (no state change) | — | owner-approved | **Docs tidy:** the 7 frozen spec docs (`PROJECT`, `ARCHITECTURE`, `AI_PIPELINES`, `SECURITY`, `PERFORMANCE`, `UI_GUIDELINES`, `DEVELOPMENT`) moved from the repo root to `docs/spec/` (+ `docs/spec/README.md` index). Root `.md` is now just `README` / `CLAUDE` / `ROADMAP`. Organisational only — no content change. Verified: zero markdown-link references repo-wide (all mentions are bare unique names), so nothing broke; `CLAUDE.md` Document Map + nav table, `README.md`, `docs/spec/DEVELOPMENT.md` §2 layout, and `scripts/check.mjs` updated. |
| 2026-09-06 | Phase 15 / 15.1 `IN PROGRESS` (split) | Phase 15 `COMPLETE` (split — 15.D deferred) → Phase 16 / 16.1 `NOT STARTED` | phase-15 | llama.cpp adapter landed (split scope): `src-tauri/src/llm/` — `protocol` (llama.cpp `/completion` JSON + SSE parser + `stop_type`→`StopReason` — the **only** file with llama.cpp shapes), `server` (`pick_free_port`, `bearer_token` 64-hex/launch, `ServerArgs::to_argv`, `ServerProcess` spawn-under-Job-Object + `try_wait` + graceful-kill), `job` (Windows Job Object `KILL_ON_JOB_CLOSE`; non-Windows no-op), `client` (`LlamaClient` `/health` + non-stream + SSE stream, bearer on every call, per-call `Timeout`, `select!` on `CancellationToken` → `Cancelled` + response drop), `mod` (`LlamaBackend: ModelBackend`; `LlamaServer: LoadedInstance + LlmInstance`; `as_llm` downcast helper; `BACKEND_KEY="llama.cpp"`). `lifecycle::backend::LoadedInstance` gains `as_any` (additive internal hook for Phase 16). `lib.rs` registers the backend iff `<app_data>/runtimes/llama-server.exe` exists (logs + skips otherwise). Deps: `windows` (JobObjects — already transitive) + `tokio` `process` feature (+`signal-hook-registry`). **Gate 4/7/9 + stub sides of 2/3/6 PASS** (13 tests vs an in-process `tiny_http` stub); **1/5/8 + real 2/3/6 NOT EXECUTED — deferred `15.D`** (no CUDA `llama-server`; ADR-0004 build needs a ~3 GB Toolkit; owner wants it fresh; run before Phase 16, with the real Qwen 0.5B GGUF, owner-cleared). 209 rust tests (+13), 7 vitest, check suite green. `tauri dev` clean. No new ADR (confirms ADR-0013). Build recipe → `DEVELOPMENT.md` §5. Evidence `docs/verification/14_phase15_llama.md`. |
| 2026-09-06 | Phase 15 `NOT STARTED` | Phase 15 / 15.1 `IN PROGRESS` (split) | phase-15 | Phase entry: `docs/plan/15_llama-cpp-adapter.md` finalized (10 steps + a deferred `15.D`). **Owner decision: split the phase** — the machine has no CUDA Toolkit / `nvcc` (ADR-0004 build needs a ~3 GB install), and the owner wants a *fresh* llama.cpp, not one reused from another project. New `src-tauri/src/llm/` module built + tested against an **in-process `tiny_http` stub `llama-server`** now: `ModelBackend` impl, process supervision (free port + `--api-key` bearer per ADR-0013 — `llama-server` is upstream HTTP, no named pipe — + Windows Job Object kill-on-close), `/health` poll, `/completion` non-stream + SSE stream client, cancellation + timeout, `LlmInstance` capability trait + `as_any` downcast hook on `LoadedInstance` (additive). Gate items **4, 7, 9 + the stub sides of 2/3/6** PASS now; **1, real-2, real-3, 5, 6-real, 8** (real CUDA `llama-server` + real Qwen 0.5B GGUF) **deferred → §6**. Owner cleared downloading project assets for `15.D`. Deps to add: `windows` (JobObjects), `getrandom`. No new ADR (confirms ADR-0013). |
| 2026-09-06 | Phase 14 / 14.1 `IN PROGRESS` | Phase 14 `COMPLETE` → Phase 15 / 15.1 `NOT STARTED` | phase-14 | Model lifecycle manager landed: `src-tauri/src/lifecycle/` — `backend` (`ModelBackend`/`LoadedInstance` traits via `async-trait`; `FakeBackend` test-only), `mod` (`LifecycleManager`: `HashMap<ModelId, Entry>` behind one `tokio::sync::Mutex`; `load`/`unload`/`cancel_load`/`begin_use`→`BusyGuard`/`end_use`/`check_liveness`/`state`/`statuses`/`register_backend`). State machine `Unloaded→Loading→Loaded⇄Busy→Unloading`, `Failed` with an explicit recovery transition. Concurrent same-model `load`s coalesce on a `Notify` (`enable()`d under the lock). Phase 13 reservation acquired inside `run_load` **before** the backend load, `commit`ed with the instance's measured VRAM, `release`d on every failure / cancel / unload / liveness kill. `RetryPolicy { max_attempts: 3, backoff_base: 500 ms }` exponential; `Cancelled` is not a failure. `~2 s` liveness loop → `Failed` + release + reconcile. Backend `load`/`shutdown`/`health` never called under the transition lock. **Collision resolved:** `ModelState::Busy` added additively to the frozen contract (`docs/contracts.md` §3 precedent). New DTO `LifecycleStatus`; `lifecycle_status` IPC. `lib.rs` builds + manages `Arc<LifecycleManager>`, spawns the liveness loop. Deps: `async-trait` + `tokio-util` promoted to direct deps (already transitive — 0 net crates). **All 9 gate items PASS** (15 async tests, `FakeBackend` + `MockProbe`). 196 rust tests (+16), 7 vitest, check suite green. `tauri dev` clean. No real backend (Phase 15), no eviction/hot-swap (Phase 23/24), no generation. No new ADR. Evidence `docs/verification/13_phase14_lifecycle.md`. |
| 2026-09-06 | Phase 14 `NOT STARTED` | Phase 14 / 14.1 `IN PROGRESS` | phase-14 | Phase entry: `docs/plan/14_model-lifecycle.md` finalized (13 steps, 9 gate items) against ADR-0007 + ADR-0010. New `src-tauri/src/lifecycle/` — `ModelBackend` / `LoadedInstance` traits (`async-trait`; `FakeBackend` test-only), `LifecycleManager` state machine (`Unloaded → Loading → Loaded ⇄ Busy → Unloading`, `Failed` recovery) behind one `tokio::sync::Mutex`; concurrent same-model loads coalesce on a `Notify`; reservation from Phase 13 acquired **before** the backend load, released on every exit path; `RetryPolicy { max_attempts: 3, backoff_base: 500 ms }`; a ~2 s liveness monitor → `Failed` + release + reconcile. **Collision flagged + resolved:** the state machine needs a `Busy` state the frozen `contracts::model::ModelState` lacks → **added additively** (`docs/contracts.md` additive-only; nothing matches it exhaustively). Deps: `async-trait`, `tokio-util` as direct deps (both already in the tree — 0 net crates). No real backend (Phase 15), no eviction/hot-swap (Phase 23/24), no generation. No new ADR. |
| 2026-09-06 | Phase 13 / 13.1 `IN PROGRESS` | Phase 13 `COMPLETE` → Phase 14 / 14.1 `NOT STARTED` | phase-13 | Resource manager landed: `src-tauri/src/resources/` — `probe` (`HardwareProbe` trait; `NvmlProbe` = `nvml-wrapper` GPU 0 `memory_info` + `sysinfo` RAM, NVML init failure non-fatal; `MockProbe`), `estimate` (`estimate_llm_vram` closed form + `Calibration` per-model EMA factor, clamped), `mod` (`ResourceManager`: in-memory `LedgerEntry` vec; `request`/`commit`/`release`/`observe`/`snapshot`/`reconcile` behind one `tokio::sync::Mutex<Inner>`; `request` reads the cached measurement, never the probe; stale-TTL 120 s, drift-slack 512 MB). Config schema **v4**: `resources.vram_safety_margin_mb` (default 1500, `ConfigKey::VramSafetyMarginMb`, session override, `<= 65536`). Contracts (additive): `GpuMemory`, `RamInfo`, `ResourceSnapshot`. IPC `resources_snapshot`. `lib.rs` builds it over `NvmlProbe`, takes the first measurement, spawns the observe loop (1.5 s busy / 10 s idle). Deps: `nvml-wrapper 0.10`, `sysinfo 0.39` (`system` only). **All 9 gate items PASS** (mock hardware) + real probe confirmed on the reference machine (16 303 MB VRAM / 31 938 MB RAM) and in a `tauri dev` launch. 180 rust tests (+27), 7 vitest. Baselines: `request` ~µs, first NVML+`sysinfo` probe ~13 ms (off the request path). No new ADR (implements ADR-0007); ledger persistence + full TDR flow deferred to Phase 33. Evidence `docs/verification/12_phase13_resources.md`. |
| 2026-09-06 | Phase 13 `NOT STARTED` | Phase 13 / 13.1 `IN PROGRESS` | phase-13 | Phase entry: `docs/plan/13_resource-manager.md` finalized (11 steps, 9 gate items) against **ADR-0007** (NVML per-process VRAM confirmed unavailable → whole-GPU `{total,used,free}` + our own reservation ledger; closed-form estimate + per-backend EMA calibration; one async `Mutex` serializes `request`/`commit`/`release`/`reconcile`; system-RAM = 2nd constraint) and ADR-0010 (lock ordering). New `src-tauri/src/resources/` (`probe.rs` — `HardwareProbe` trait + `NvmlProbe`/`sysinfo` + `MockProbe`; `estimate.rs`; `mod.rs` — `ResourceManager`; `tests.rs`). `request` reads a cached snapshot (never blocks on NVML); observe loop 1.5 s loaded / 10 s idle. Config schema **v4**: `resources.vram_safety_margin_mb` (default 1500). Deps: `nvml-wrapper 0.10`, `sysinfo` (`system` only). `resources_snapshot` IPC. No new ADR (implements ADR-0007); ledger persistence + full TDR flow deferred to Phase 33. |
| 2026-09-06 | Phase 12 / 12.1 `IN PROGRESS` | Phase 12 `COMPLETE` (code + offline gates) → Phase 13 / 13.1 `NOT STARTED` | phase-12 | Model acquisition landed: `src-tauri/src/acquisition/` — `gguf` (hand parser), `hf` (search / file-list + header range / fetch), `budget` (pre-transfer guard), `download` (`reqwest` engine: `.part` + `Range` resume + SHA-256 verify + atomic rename + register), `AcquisitionService`, `acquire_fixed` (faster-whisper `large-v3`, `ResembleAI/chatterbox-turbo` — owner-confirmed). `V0003__model_downloads.sql`; config **v3** (`models.min_free_gb`). IPC: `models_list`, `model_delete`, `hf_*`, `download_*`, `downloads_list`, `acquire_fixed`. `/models` picker UI + `Models.test`. **Gates 1–4, 6–9 PASS**: 1/6/8 verified **live** — `Qwen/Qwen2.5-0.5B-Instruct-GGUF` downloaded 491 MB @ 50.6 MB/s, SHA-256 checked against HF's LFS hash, registered with parsed metadata; 2/3/4/6/7 via a mock `Range` server + component test. **Gate 5** (fixed STT/TTS, ~5 GB) deferred → before Phase 18 (§6). The live test surfaced + fixed 3 bugs (LFS hash field `oid` not `sha256`; Windows `\\?\` verbatim paths; `register()` model-dir off-by-one). 153 rust tests (+21). Deps: `reqwest` (0 net crates), `sha2`, `fs4`, `walkdir`, `tiny_http` (dev). Evidence `docs/verification/11_phase12_acquisition.md`. |
| 2026-09-06 | (no state change) | — | owner-approved | **ADR-0008 amended:** transfer client `hf-hub` → hand-rolled `reqwest` range requests. `hf-hub` 1.0 = +121 transitive crates (incl. `aws-lc-sys` C build + `hf-xet`); `reqwest` = 0 net crates (already in the tree via Tauri) and we need it anyway for the HF search API + GGUF header range. Xet dropped for v1. `docs/decisions/0008` + README updated. |
| 2026-09-06 | Phase 12 `NOT STARTED` | Phase 12 / 12.1 `IN PROGRESS` | phase-12 | Phase entry: `docs/plan/12_model-acquisition.md` finalized (11 steps). New `acquisition/` module (`hf.rs`, `gguf.rs`, `download.rs`); `V0003__model_downloads.sql`; config **v3** (`models.min_free_gb`). Crates: `reqwest` (`json`/`rustls-tls`/`stream`), `sha2`, `tiny_http` (dev); **hand-written GGUF header parser** (crates immature). Owner: engine tested against a local mock server; gate 1 a small live GGUF (repo confirmed before fetch); fixed models (12.8) `faster-whisper large-v3` CT2 + `Chatterbox Turbo` downloaded live. |
| 2026-09-06 | Phase 11 / 11.1 `IN PROGRESS` | Phase 11 `COMPLETE` → Phase 12 / 12.1 `NOT STARTED` | phase-11 | Model registry landed: `src-tauri/src/models/` — `V0002__model_registry.sql` (`model_entry` STRICT + kind index), `ModelRegistry` CRUD + capability `query`, `ModelDraft`/`ModelFilter`, `validate_model_path` (confine to model dir, reject `..`, require existence at register), **availability computed from `path.is_file()` at read** (never stored), `Arc<Vec<Model>>` cache cleared on write. Additive contracts (`RegisteredModel`, `RegistryAvailability`, `Device`). **ADR-0017**: UUIDv4 entity ids. `lib.rs` — `Db` → `Arc<Db>` managed, `ModelRegistry` managed. Gate: register→get round-trips ✓ · removed file → `Missing`, no crash ✓ · capability query ✓ · invalid metadata rejected naming the field ✓ · path outside dir / `..` refused ✓ · id stable across a registry rebuild ✓ · check suite green. 130 rust tests (+14). Warm `get` ~18 µs. The dev launch migrated the real DB v1→v2 (2nd backup). Evidence `docs/verification/10_phase11_registry.md`. |
| 2026-09-06 | Phase 11 `NOT STARTED` | Phase 11 / 11.1 `IN PROGRESS` | phase-11 | Phase entry: `docs/plan/11_model-registry.md` finalized (12 steps). New `models/` module + `V0002__model_registry.sql` (`model_entry` STRICT table). Availability (`Ready`/`Missing`) **computed from `path.exists()` at read**, never stored. Path confinement to the model dir at register/update. Whole-list in-memory cache, cleared on write. Additive contracts (`RegisteredModel`, `RegistryAvailability`, `Device`). **ADR-0017** (this phase): entity IDs = UUIDv4 (`uuid` already in the tree). Capability-schema open question resolved: small typed set on `ModelCapabilities`, not free-form flags. LoRAs/presets stay Phase 22. |
| 2026-09-06 | Phase 10 / 10.1 `IN PROGRESS` | Phase 10 `COMPLETE` → Phase 11 / 11.1 `NOT STARTED` | phase-10 | Observability landed: `logging/` — non-blocking lossy JSON writer, **boundary secret redaction** (regex: `hf_`, `Bearer`, secret JSON keys / `key=value`), 256-line in-memory ring buffer, hot-reloadable `EnvFilter` (`set_level` from config), `operation()` span helper (`task_id` + `elapsed_ms` + `status`), `content_preview`, `AppError::log`. Config **schema v2**: `logging.level` (+ `ConfigKey::LoggingLevel`, session override, migration generalised to `step_forward`). Deps: `tracing-appender`, `regex` (already in tree). Gate: op lines carry task_id ✓ · seeded secrets redacted ✓ · no content at info ✓ · config level + reload ✓ · no network symbol in `logging/` ✓ · non-blocking lossy writer ✓ · check suite green. 113 rust tests (+14). **Deferred to Phase 37:** persistent file sink + rotation + diagnostics-bundle command. Evidence `docs/verification/09_phase10_observability.md`. |
| 2026-09-06 | Phase 10 `NOT STARTED` | Phase 10 / 10.1 `IN PROGRESS` | phase-10 | Phase entry: `docs/plan/10_observability.md` finalized (10 steps). Scope: reloadable filter + config `logging.level` (schema **v2**), boundary redaction (regex — `hf_`, `Bearer`, secret JSON keys), non-blocking lossy writer, in-memory ring buffer, `operation()` span helper. **Deferred to Phase 37:** persistent log file + rotation/retention + the diagnostics-bundle IPC command (no terminal-less build or Settings UI yet) — this closes the plan's "retention/rotation" open question. Hot-path throughput comparison → Phase 16. No new ADR (implements frozen `SECURITY.md`/`PERFORMANCE.md`). |
| 2026-09-06 | Phase 9 / 9.1 `IN PROGRESS` | Phase 9 `COMPLETE` → Phase 10 / 10.1 `NOT STARTED` | phase-9 | SQLite persistence landed: `src-tauri/src/db/` — `Db` (writer pool 1 + reader pool 4, `deadpool-sqlite`), per-connection pragma hook (WAL, `busy_timeout`, `foreign_keys`, `synchronous=NORMAL`), `refinery` forward-only grouped migrations with verified `VACUUM INTO` backup (keep 5) + unknown-newer refusal, `write`/`read` helpers, `PRAGMA quick_check` on open, `DbError` + `From<DbError> for AppError`, `AppMetaRepo`, `V0001__init.sql` (`core_app_meta`). `lib.rs` opens+migrates at startup, checkpoints WAL on exit. Deps: `rusqlite 0.37` (bundled), `deadpool-sqlite 0.12`, `refinery 0.9`, `tokio 1`. Gate: create-from-empty ✓ · forward-only + idempotent ✓ · commit persists / error+panic roll back ✓ · survives restart ✓ · grouped rollback + verified backup ✓ · 24 concurrent writers, no `SQLITE_BUSY` ✓ · corrupt file → `DbError::Corruption` ✓ · no rusqlite version split ✓ · check suite green. 99 rust tests (+11). Baselines: insert ~0.12 ms, PK read ~0.05 ms, 100-row tx ~0.16 ms. Evidence `docs/verification/08_phase9_persistence.md`. Entry: gate-2 collision (plan said "migrate down") resolved to forward-only per ADR-0009; `synchronous=NORMAL` kept. |
| 2026-09-06 | Phase 9 `NOT STARTED` | Phase 9 / 9.1 `IN PROGRESS` | phase-9 | Phase entry: `docs/plan/09_sqlite-persistence.md` finalized (11 steps). **Collision flagged + resolved:** the plan's original gate 2 ("migrate down") conflicts with ADR-0009's forward-only decision → reframed to forward-only + idempotent (ADR wins). `synchronous=FULL` deferred item decided: **keep `NORMAL`** (WAL+NORMAL is crash-safe; FULL only guards OS/power loss at a write cost). Topology: `deadpool-sqlite` writer pool (1) + reader pool (4), `refinery` grouped migrations with verified `VACUUM INTO` backup. Blob store explicitly **not** this phase. |
| 2026-09-06 | Phase 8 / 8.1 `IN PROGRESS` | Phase 8 `COMPLETE` → Phase 9 / 9.1 `NOT STARTED` | phase-8 | Config system landed: `src-tauri/src/config/` — `AppConfig` (`version` + `models.{dir, budget_gb}`), forward migration runner, `ConfigManager` (layered defaults ← file ← session, atomic write, corrupt-file backup + recovery), `config_get`/`config_set`/`config_keys` IPC, `setup()` wiring. **ADR-0016** (JSON, `<app_config_dir>/config.json`) closes the format question. Gate: defaults w/o file ✓ · invalid value fails fast naming the key ✓ · out-of-range rejected ✓ · change persists across restart ✓ · versionless file migrates to v1 ✓ · session override non-persistent ✓ · corrupt file → defaults + `config.json.corrupt-*` + warn ✓ · no ad-hoc env/settings reads (bar `LOCALAI_LOG`) ✓ · check suite green ✓. 88 rust tests (+22), config load **~0.08 ms**. Evidence `docs/verification/07_phase8_config.md`. |
| 2026-09-06 | Phase 8 `NOT STARTED` | Phase 8 / 8.1 `IN PROGRESS` | phase-8 | Phase entry: `docs/plan/08_configuration.md` finalized (11 steps). Format decision (`ADR-0016`: JSON, `<app_config_dir>/config.json`, layered defaults/file/session, `version` + forward migrations, atomic write, no secrets) closes the plan's open question. Minimal schema — `version` + `models.{dir, budget_gb}` — grown additively by later phases. |
| 2026-09-06 | Phase 7 / 7.1 `IN PROGRESS` | Phase 7 `COMPLETE` → Phase 8 / 8.1 `NOT STARTED` | phase-7 | Application contracts landed: `src-tauri/src/contracts/` (`ids` · `task` · `model` · `generation` · `conversation` · `resource` · `worker`) + `ipc::error` extended to 9 `kind`s + `ErrorEnvelope`. 42 `ts-rs` bindings (was 5); `src/lib/contracts.ts` import surface; `docs/contracts.md` (evolution rules). Gate: `cargo build`+`tsc` clean, zero warnings ✓ · 66 rust tests (round-trip every type + rejection: unknown variant/tag, missing field, out-of-range `validate()`) ✓ · no model-name literals outside `contracts/` ✓ · bindings committed + in sync ✓ · `TokenDelta` round-trip ~3.3 µs ✓ · no behaviour / handler change ✓. Evidence `docs/verification/06_phase7_contracts.md`. Entry note: plan finalized to 12 steps (ADR-0002 + ADR-0013); scope held to the plan's list — character/persona/config/registry contracts stay with their phases. |
| 2026-09-06 | (no state change) | — | phase-7 | **Phase 6 polish** (`main` `HEAD`): replaced the `app://ready` **event** with an `app_ready` **command** — the event fired in `.setup()` before the webview subscribed, and `listen()` at module-eval threw an unhandled `transformCallback` rejection every launch. Readiness now a command the shell calls once on mount; Phase 6 `app_ping` probe guarded against StrictMode/HMR re-fire (log went from ~14 lines/session → 1). Fresh `tauri dev` verified clean. `docs/verification/05_phase6_bootstrap.md` follow-up resolved. Check suite green. |
| 2026-09-05 | (no state change) | — | owner-agreed | Visual **soft lock** added: `docs/design/visual-language.md` (from an owner reference screenshot) — firm on the shell (left nav ~260px, Settings a plain bottom item), chat geometry (user-right/assistant-left + avatars, ~14px bubbles, 680px text cap), and the radius/spacing scale; the right panel + per-tab layouts left flexible per feature phase. Geometry tokens added to `src/styles/theme.css`. `UI_GUIDELINES.md` §1: **a UI/CSS change never alters behaviour** — check suite must still pass. |
| 2026-09-05 | Phase 6 `NOT STARTED` | Phase 6 `VERIFIED` → Phase 7 / 7.1 `NOT STARTED` | phase-6 | Tauri v2 + React/TS + Rust scaffold (branch `phase-6-bootstrap` → `main` `1f6c6a1`). Single Rust crate (`ipc` + `logging`); typed IPC with `ts-rs` bindings; 3-tab hash-routed shell + error boundary + theme tokens; ESLint/Prettier/rustfmt/clippy/Vitest wired; `.gitattributes` + `.githooks`. Gate: `cargo fmt`/`clippy`/`test` (8) ✓ · `tsc`/`eslint`/`prettier`/`vitest`(2)/`vite build` ✓ · `tauri dev` launches, `app_ping` round-trip observed in the structured log ✓ · no AI/model/network code ✓. Evidence `docs/verification/05_phase6_bootstrap.md`. |
| 2026-09-05 | Phase 5 `IN PROGRESS` | Phase 5 `VERIFIED` → Phase 6 / 6.1 `NOT STARTED` | phase-5 | Architecture **frozen**. 5.9: all `docs/plan/06–40` re-aligned (frozen-architecture banner + governing ADRs). 5.10 cross-check `docs/verification/04_phase5_crosscheck.md` — no contradictions. §7 rewritten (O1–O7 resolved). Nav-efficiency: `CLAUDE.md` "looking for X → go to Y" table + a `README.md` in every `docs/` subdir. |
| 2026-09-05 | Phase 5 `NOT STARTED` | Phase 5 `IN PROGRESS` (5.9) | phase-5 | 5.1–5.8: `PROJECT.md` + `ARCHITECTURE.md` + `AI_PIPELINES.md` + `SECURITY.md` + `PERFORMANCE.md` + `UI_GUIDELINES.md` + `DEVELOPMENT.md` written and frozen. **All 15 ADRs `PROPOSED` → `ACCEPTED`.** `CLAUDE.md` Article I transport ratified (ADR-0013); Document Map updated. |
| 2026-09-05 | Phase 4 `NOT STARTED` | Phase 4 `VERIFIED` → Phase 5 / 5.1 `NOT STARTED` | phase-4 | `docs/verification/03_adversarial_review.md`: architecture assembled + attacked across 8 categories, ~40 risks. **4 critical → ADR changes**: loopback-server auth (ADR-0013, prefer named pipes), Python-worker telemetry lockdown (new ADR-0015), NF4 quant cache now *required* (ADR-0006/0008), GPU-mutex lock-ordering rule (ADR-0010). Also driver-TDR recovery + system-RAM tracking (ADR-0007), backup-before-migrate verify + blob write-order (ADR-0009). 7 risks `ACCEPTED` with rationale. No code. |
| 2026-09-05 | Phase 3 `IN PROGRESS` | Phase 3 `VERIFIED` → Phase 4 / 4.1 `NOT STARTED` | phase-3 | Gate met: 16 decisions in 12 area writeups, 14 draft ADRs (`PROPOSED`), 3 probes run (`docs/verification/02_phase3_probes.md` — NVML per-process VRAM **unavailable** on WDDM/610.88 → ledger-based accounting confirmed; GGUF header parse **works** from a range request; WDDM-hang inconclusive → Phase 15 watch item). Owner: no LoRA trainer; portraits/selfies → prompt-based identity OK. No app code (probes were throwaway, deleted). ADRs ratified at Phase 5. |
| 2026-09-05 | Phase 3 `IN PROGRESS` | Phase 3 `IN PROGRESS` (3.15) | phase-3 | All 12 research areas drafted (`docs/research/phase3/01–12`) + **14 draft ADRs** (`docs/decisions/0001–0014`, `PROPOSED`). Owner's Krea impl folded in (diffusers sidecar, NF4, ~11.4 GB peak). Transport resolved (ADR-0013: loopback HTTP for model servers, stdio for workers). **Biggest risk: character visual identity (ADR-0011) — Krea 2 is text-to-image only; needs per-character LoRA (~45–60 min/char background) → owner sign-off.** Exit items: 3 runnable probes (nvml per-process, GGUF header, WDDM) + owner sign-off, then Phase 4. |
| 2026-09-05 | Phase 3 `NOT STARTED` | Phase 3 `IN PROGRESS` | phase-3 | Research started. First 5 areas + README. Key finding: image gen ⟂ LLM on 16 GB. | `docs/product/vision.md` (owner draft) → `docs/product/requirements.md` (PD.2): 3-tab structure (Chat/Voice · Image Generator · Discovery), Persona vs Character split, ~95 FR / ~40 NFR / 20 ARQ / 7 non-goals. Open points A1–A11 resolved; owner confirmed. Product Definition `COMPLETE`. Pointer → Phase 3. |
| 2026-09-05 | 40-step course (0–39) | 41-step course (0–40) + detailed `docs/plan/` | owner-approved | Full implementation plan written. Inserted **Phase 12 Model Acquisition & Picker**; split Voice → 18/19 and Character System → 25/26; moved Scheduler → 24; merged old 37–39 → Phase 40. `ROADMAP.md` §4 slimmed to an index; per-phase detail now in `docs/plan/NN_*.md`. Tech defaults recorded in §7 + `docs/OVERVIEW.md`. Pointer unchanged (Product Definition). |
| 2026-09-05 | 32-phase working-draft ledger | 40-phase course (0–39) + Product Definition step | owner-delegated | §4 replaced with the LocalAI phase structure from the owner's ChatGPT planning chat. Old Phase 1 → Phase 1 (Project Foundation); old Phase 0 (env audit) → Phase 2. |
| 2026-09-05 | Article I amended | Article I reverted | owner-delegated | Subprocess transport is a Phase 3 architecture-research question, not a Phase 0 decision. §7 O3 reopened. |
| 2026-09-05 | (no state change) | — | owner-delegated | `CLAUDE.md` restructured: Document Map + Operating Manual. |
| 2026-09-05 | (no state change) | — | research | Pre-architecture research added: `docs/research/01–06` + README. Input to Phase 3. |
| 2026-09-05 | old Phase 0 `NOT STARTED` | old Phase 0 `COMPLETE` | audit | Dev-environment audit; `docs/verification/01_env_audit.md`. (Now Phase 2.) |
| 2026-09-05 | old Phase 1 stages 1.1–1.6 | old Phase 1 `COMPLETE` | owner | Foundation closed; deferred tooling carried to bootstrap. (Now Phase 1.) |
| 2026-09-05 | old 1.1/1.2 | `VERIFIED` / `COMPLETE` | bootstrap | Repo initialized; state machine + constitution authored. |

---

## 6. Open Blockers

_None blocking._

- **Phase 12 gate 5 — fixed STT/TTS models — `DONE` (2026-09-06).** Run on the
  owner's "Download fresh versions" go-ahead ahead of Phase 18.
  `live_acquire_fixed_models` pulled both bundles live:
  `Systran/faster-whisper-large-v3` (5 files, 2.9 GB, 28 s) → `models\stt\`,
  `ResembleAI/chatterbox-turbo` (11 files, 3.8 GB, 38 s) → `models\tts\` — each
  registered (`Stt` / `Tts`, `Ready`, path confined), in-project + gitignored,
  short subdirs per owner preference. Phase 12 now all 9 gates pass.

- **Watch item — WDDM hang** on a *sustained* `llama-server` generation (Hyper-V
  on host). **Not observed** in 15.D or Phase 16 (short generations). Keep the
  3-step mitigation ladder (`docs/verification/02_phase3_probes.md`) in reach for
  the longer generations Phase 17+ will produce.

- **Manual UI click-through — DONE (owner, 2026-09-06).** `tauri dev`: loaded
  Qwen, text chat replied (~64 ms), push-to-talk transcribed a spoken phrase into
  a user turn + auto-reply. Both chat and voice-in confirmed on the real machine.
  Prereqs for a repeat: `.venv` (`node scripts/setup-venv.mjs`) + `models/{stt,vad}/`;
  the machine's `config.json` points `models`/`runtimes`/`workers.*` at the repo.
- Phase 1's deferred tooling (formatter/linter/hooks/README) → **done in Phase 6.**

---

## 7. Decisions & open items

**The architecture is frozen (Phase 5, 2026-09-05).** The binding record is
`docs/spec/` (the 7 spec docs) + `docs/decisions/` (ADR-0001…0018, all
`ACCEPTED`). Changes need STOP → propose → approve → ADR.

### Course inserts (owner-approved, additive — not architectural)

- **Phase 15.D** — 15 split; a pinned prebuilt `llama-server` for v1 (ADR-0004
  amended). Done.
- **Phase 18** split into **18.A / 18.B**. Done.
- **Phase 19** split into **19.A / 19.B**. Done. venv gained PyTorch
  (`torch 2.11.0+cu128`, overriding the `chatterbox-tts` pin) — ADR-0018 amended.
- **Phase 18.5 — Deploy & Diagnostics** (2026-09-06, owner-requested). A
  repeatable real-build launch + **local** probes (rotating file log, a
  `diag_export` JSON snapshot, UI breadcrumbs) so the owner can live-test on their
  own time and hand the agent the results. Pulls the log-file / bundle work
  forward from **Phase 37**, which still owns the MSI + clean-machine layout + a
  portable archive. No new ADR (ADR-0015 already forbids the network path).

### Resolved (was O1–O7)

| Item | Resolution |
| ---- | ---------- |
| O1 memory retrieval | FTS5 keyword; embeddings deferred behind a measured trigger (ADR-0012) |
| O2 single vs multi-user | **single-user, local** (NFR-61, `PROJECT.md`) |
| O3 subprocess transport | named-pipe / token'd loopback for model servers, stdio for workers (ADR-0013); `CLAUDE.md` Art. I updated |
| O4 Rust structure | single Tauri crate, documented split triggers (ADR-0001) |
| O5 env gaps | build llama.cpp from source + CUDA Toolkit 13.x (ADR-0004); embedded CPython + shared venv + MSI (ADR-0014); npm + a default `model_dir` pinned at Phase 6 |
| O6 character identity bar | prompt-based, **no LoRA training** (ADR-0011); FR-C90 scoped to portraits/selfies — owner-confirmed |
| O7 Krea 2 quant | NF4 (cached) for v1; benchmark torchao NVFP4 at Phase 22 |

### Deferred (decide during implementation — not architectural)

- NVFP4 vs NF4 for Krea 2 → Phase 22.
- `synchronous=FULL` on the DB writer → Phase 9 with measurements.
- Named pipe vs token'd TCP for `llama-server` → Phase 15 (confirm upstream).
- npm vs pnpm → Phase 6.
- At-rest encryption of the DB + blob store → a later phase, if ever in scope.

### Watch items

- **TTS first-audio is model-load-dominated (Phase 19).** Chatterbox
  `from_local` ≈ 5–6 s on the first request; warm synth RTF ≈ 0.45. Preloading
  the TTS worker when a voice session starts (overlapping the ~6 s load with the
  user's first utterance) would cut perceived first-audio to the warm figure.
  Small optimisation — do it in Phase 31 (perf audit) or opportunistically.
- **WDDM hang** on the first sustained `llama-server` generation (Hyper-V enabled
  on host) → Phase 15, 3-step mitigation ladder in
  `docs/verification/02_phase3_probes.md`.

### Closed (historical)

- Environment-inspection gap → Phase 2. Constitution location → stays in
  `CLAUDE.md`. Generic web-service ledger → replaced with the LocalAI course.
  Phase-list detail → `docs/plan/`. `docs/OVERVIEW.md` → superseded by `PROJECT.md`
  + `ARCHITECTURE.md`.
