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
| **Phase**        | 3 — Architecture Research                               |
| **Stage**        | 3.15 — all 12 areas + 14 draft ADRs done; exit items remain |
| **Status**       | `IN PROGRESS`                                           |
| **Blocked by**   | Owner sign-off on ADR-0011 (per-character LoRA scope); 3 runnable probes (nvml per-process, GGUF header, WDDM) |
| **Plan doc**     | `docs/plan/03_architecture-research.md`                 |
| **Last updated** | 2026-09-05                                              |
| **Updated by**   | phase-3-research                                        |

**Completed:** Phase 0 (process) · Phase 1 (Project Foundation) · Phase 2
(Environment Audit) · **Product Definition** (`docs/product/requirements.md`,
owner-confirmed 2026-09-05).

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

### Phase 3 — Architecture Research *(current pointer)* — `NOT STARTED` — `docs/plan/03_architecture-research.md`
Research *how* to build the confirmed product; every technology and boundary
questioned against performance, Windows, offline, failure modes. Research stays
open. Input: `docs/research/01–06`, `ARQ-*`, O3/O5.
Gate: every `ARQ` answered or deferred with reason; every decision has a written
comparison + recommendation; draft ADRs in `docs/decisions/`; no code.

### Phase 4 — Adversarial Architecture Review — `NOT STARTED` — `docs/plan/04_adversarial-review.md`
Try to break the proposed architecture before committing.
Gate: prioritized risk register (`docs/verification/02_adversarial_review.md`)
covering the mandated failure categories; every high risk mitigated or accepted
with rationale; architecture + ADRs updated.

### Phase 5 — Project Documentation / Architecture Freeze — `NOT STARTED` — `docs/plan/05_architecture-freeze.md`
Officialize product + architecture: `PROJECT.md`, `ARCHITECTURE.md`,
`AI_PIPELINES.md`, `SECURITY.md`, `PERFORMANCE.md`, `UI_GUIDELINES.md`,
`DEVELOPMENT.md`; final ADRs; **re-derive `docs/plan/06–40`** against the freeze.
Gate: all docs present + mutually consistent; ADRs complete; plan docs finalized;
**no application code**.

---

### EPOCH 1 — Core Platform (Phases 6–15)

### Phase 6 — Tauri + React + Rust Bootstrap — `NOT STARTED` — `docs/plan/06_bootstrap.md`
App launches to a placeholder UI; typed IPC works; fmt/lint/typecheck/test/logging
wired; deferred Phase 1 tooling + `.gitattributes` landed. No AI.
Gate: full verification run green; app launches; IPC round-trip; no model code.

### Phase 7 — Application Contracts — `NOT STARTED` — `docs/plan/07_application-contracts.md`
Typed serializable contracts (tasks, model metadata/state, generation
requests/events, streaming, cancellation, errors, conversations, messages,
resource reservations, worker jobs). No model names in logic.
Gate: compile; serialize/deserialize round-trip; invalid rejected; documented;
`git grep` finds no model-name literals in logic.

### Phase 8 — Configuration — `NOT STARTED` — `docs/plan/08_configuration.md`
One typed, validated, versioned config (defaults → user → session). No subsystem
invents its own storage. No secrets in source.
Gate: defaults load; invalid rejected with a named error; persists across restart;
migration works; session override works; corrupt/missing handled without crash.

### Phase 9 — SQLite Persistence — `NOT STARTED` — `docs/plan/09_sqlite-persistence.md`
Rust-owned persistence: migrations, versioning, pooling, transactions,
repositories, structured errors. Minimum schema. Frontend/workers never touch DB.
Gate: create from empty; migrate up/down + idempotent; commit + rollback; survives
restart; migration failure rolls back + backup; lock contention handled.

### Phase 10 — Observability — `NOT STARTED` — `docs/plan/10_observability.md`
Local structured logging + diagnostics (levels, task id, model id, duration,
status, structured errors). No secrets, no conversation content by default, no
cloud telemetry.
Gate: an operation's log lines all carry its task id; redaction test passes;
level filtering works; no network egress from the logging path.

### Phase 11 — Model Registry — `NOT STARTED` — `docs/plan/11_model-registry.md`
Model metadata as data (stable id, name, type, backend, path, capabilities,
context, quant, estimated resource need, devices, model config). No loading.
Gate: register a test model; find + read; missing file represented not crashed;
capability query; invalid metadata rejected.

### Phase 12 — Model Acquisition & Picker — `NOT STARTED` — `docs/plan/12_model-acquisition.md`
In-app HuggingFace picker + one-shot resumable download for **LLM GGUF**;
checksum verify; disk-budget guard; register on completion. STT/TTS fixed models
acquired once via the same path (no picker).
Gate: pick + download + verify + register a small GGUF; resume interrupted
download; checksum mismatch rejected; insufficient disk refused pre-download;
STT+TTS models acquired via same path; picker works read-only-offline.

### Phase 13 — Resource Manager — `NOT STARTED` — `docs/plan/13_resource-manager.md`
"Can this operation safely use the GPU now?" Lifecycle request → reserve → commit
→ observe → release → reconcile. Mockable hardware. Never file-size == VRAM.
Gate (mocked): insufficient VRAM → clean failure; duplicate reservation rejected;
concurrent serialized; failed/cancelled load releases; stale reservation
recovered; crash → reconcile vs observed.

### Phase 14 — Model Lifecycle Manager — `NOT STARTED` — `docs/plan/14_model-lifecycle.md`
The only component that loads/unloads managed models. Explicit state machine +
failure/recovery states. No duplicate loads; failed/cancelled loads release.
Gate: load; concurrent same-model load not duplicated; unload; load under
insufficient resources rejected pre-spawn; cancel mid-load releases; forced
failure → recovery; unexpected exit detected + reconciled; state-machine tests.

### Phase 15 — llama.cpp Adapter — `NOT STARTED` — `docs/plan/15_llama-cpp-adapter.md`
First LLM backend behind a clean `LlmBackend` interface; llama.cpp detail confined
to the adapter; transport per the Phase 3 ADR.
Gate: startup + readiness; non-streaming + streaming generation; cancellation
reaches the job; timeout handled; crash detected + recovered; clean shutdown no
orphan; no raw config leaks past the adapter.

---

### EPOCH 2 — First Slice & Modalities (Phases 16–22)

### Phase 16 — First Vertical Slice / Text Chat — `NOT STARTED` — `docs/plan/16_vertical-slice-text-chat.md`
The whole path: React → IPC → Rust → conversation service → LLM → llama.cpp →
streamed tokens → UI. Text chat only. Reliability over features.
Gate: send → streamed reply; cancel mid-gen, model reusable; conversation
persists; restart restores it; kill llama.cpp mid-gen → recovers; clean shutdown
during gen; concurrent-gen handled per policy. **First real milestone.**

### Phase 17 — Conversation Engine — `NOT STARTED` — `docs/plan/17_conversation-engine.md`
Formalize the one shared engine (lifecycle, messages, roles, content, timestamps,
streaming state, cancellation, generation metadata, persistence). No second engine.
Gate: engine unit tests (lifecycle + streaming + cancel + persistence); text chat
re-hosted on it, Phase 16 gate still passes.

### Phase 18 — Voice: capture · Silero VAD · faster-whisper STT — `NOT STARTED` — `docs/plan/18_voice-in.md`
Mic capture → VAD endpointing → STT worker → user message on the engine.
Gate: device discovery + selection; capture → VAD → STT produces a transcript;
partial + final transcripts; STT failure / worker crash / missing device handled;
STT real-time factor within budget.

### Phase 19 — Voice: Chatterbox TTS · playback · barge-in — `NOT STARTED` — `docs/plan/19_voice-out.md`
Assistant text → chunked TTS → playback; user speech or cancel stops LLM + TTS +
playback fast; interrupted turn persisted as truncated.
Gate: text → TTS → playback end to end; barge-in stops everything < ~200 ms and
returns to listening; TTS failure / worker crash handled; truncated turn persisted.

### Phase 20 — Personas & Context Builder — `NOT STARTED` — `docs/plan/20_personas.md`
Structured persona data + a predictable context builder (system + persona +
character + conversation + memory + runtime). Verifiable that persona reaches the
model.
Gate: persona stored as structured data; context-builder unit tests; a test
proves the assembled prompt contains the persona; switching persona changes
behaviour in a scripted check.

### Phase 21 — Memory — `NOT STARTED` — `docs/plan/21_memory.md`
Extraction → importance/validation → storage → relevant retrieval → context.
SQLite + FTS5 keyword retrieval first; embeddings only if measured need.
Gate: memory extracted + stored; retrieval returns relevant memories; context
within budget; deterministic retrieval for a fixed store+query; survives restart.

### Phase 22 — Image Generation (FLUX.1 Krea) — `NOT STARTED` — `docs/plan/22_image-generation.md`
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

- **Phase 3.10 (image-generation research)** needs the owner's existing FLUX.1
  Krea implementation from their other project. Not blocking 3.1–3.9; request it
  before starting 3.10.

Phase 6 carries forward Phase 1's deferred formatter/linter/hook/README tooling —
tracked, not blocking.

---

## 7. Open Questions & Resolved Facts

### Resolved facts (binding)

- **R1. LocalAI is a local-first AI desktop application for Windows.** Direction:
  Tauri + React/TypeScript + Rust core + isolated AI subprocesses + local SQLite.
  No cloud, no remote DB, no telemetry at runtime (`CLAUDE.md` Art. I–II).
  Direction, not frozen architecture — Phase 3 confirms details.
- **R2. The character system is a core capability**, designed for from the start
  (Phases 25–29): persistent entities with structured identity/memory/relationship
  and identity-consistent generated images.
- **R3. Untrusted AI output is contained** — only allow-listed typed Rust ops;
  especially character image actions (`CLAUDE.md` Art. III, Phase 28).
- **R4. Verification honesty + smallest-correct-implementation** are binding
  (`CLAUDE.md` Art. IV). Everything is questioned for performance in Phases 3–4
  and the audit phases (31–38).

### Recorded tech defaults (strong preference — **validated in Phase 3, not frozen**)

- **LLM**: GGUF models via an in-app **HuggingFace picker + one-shot resumable
  download** (Phase 12).
- **STT**: **faster-whisper**, one pinned model. **VAD**: **Silero**.
- **TTS**: **Resemble Chatterbox**, one pinned model.
- **Image**: **FLUX.1 Krea [dev]** — owner has a working implementation elsewhere
  to adapt; **request it at Phase 3.10 / Phase 22**.
- Phase 3 confirms each fits 16 GB VRAM / Blackwell sm_120 / offline / licensing,
  and records an ADR. If one does not fit, Phase 3 picks the alternative.

### Open questions (for Product Definition → Phase 3)

- **O1. Memory retrieval** — start SQLite + FTS5 keyword search; embeddings only
  if measured need (Phase 21). Confirm scope in Product Definition.
- **O2. Single-user vs multi-user** — working assumption **single-user, local
  profile**. Confirm in Product Definition.
- **O3. Subprocess transport** — **draft ADR-0013**: loopback HTTP for model
  servers (`llama-server`, image), stdio JSON-lines for small workers (STT/TTS/
  embedder/trainer). `CLAUDE.md` Art. I updated at Phase 5.
- **O4. Rust structure** — **draft ADR-0001**: single Tauri crate, documented
  split triggers.
- **O5. Environment gaps** — **draft ADR-0004** (build llama.cpp from source,
  CUDA Toolkit 13.x), **ADR-0014** (embedded CPython + shared venv; MSI). npm vs
  pnpm + model storage default still to pin at Phase 5/6.

### New open items from Phase 3 (for Phase 4 / owner)

- **O6. Character visual identity bar (FR-C90).** Owner ruled out LoRA training
  2026-09-05. ADR-0011 = prompt-based (`QuadView_krea2_v1` reference sheet +
  LLM-captioned canonical appearance block + fixed seed + realism LoRA +
  face-embedding similarity gate). **FR-C90/C91 scoped to best-effort for v1** —
  identity drifts under big pose/scene changes; no in-scope fix until Krea 2 gets
  reference conditioning. Owner to confirm the expected *range* of character
  images (portraits/selfies = fine; full-body varied scenes = visible drift).
- **O7. Krea 2 quant** — NF4 (bitsandbytes, proven) for v1; benchmark torchao
  NVFP4 at Phase 22.

### Closed

- **Environment-inspection gap** → Phase 2, complete 2026-09-05.
- **Constitution location** — stays in `CLAUDE.md`; `PROJECT.md` reserved for
  Phase 5.
- **Generic web-service phase ledger** → replaced with the LocalAI course.
- **Phase-list detail level** → lean index here + `docs/plan/NN_*.md` per phase
  (2026-09-05).
