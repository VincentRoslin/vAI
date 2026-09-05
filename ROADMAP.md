# ROADMAP — LocalAI Bootstrap & Implementation State Machine

> **What this file is.** The authoritative record of *where the project is* and
> *what the next gate is*. A fresh Claude session should be able to read this file
> and know exactly what to do next without the conversation history.
>
> **Phase structure** follows the LocalAI 40-step course (Phase 0–39) agreed with
> the project owner. Bootstrap = Phases 0–5. Implementation begins at Phase 6.
> The **Product Definition** step sits between Phase 2 and Phase 3.
>
> **Stability:** the state-machine mechanics (§1–§3, §5) are fixed. Individual
> phase *scope and gates* past the current pointer are a best-effort outline and
> will be refined at phase entry and after the Phase 5 architecture freeze. Do not
> treat a future phase's wording as a committed contract; do treat its **existence
> and ordering** as the plan.
>
> If reality and this file disagree about *progress*, fix the file in the same
> change that fixes reality.

---

## 1. Current State

| Field            | Value                                                    |
| ---------------- | ------------------------------------------------------- |
| **Phase**        | Product Definition (between Phase 2 and Phase 3)        |
| **Stage**        | PD.1 — owner provides the LocalAI product vision        |
| **Status**       | `NOT STARTED`                                           |
| **Blocked by**   | Owner input (the product description)                   |
| **Last updated** | 2026-09-05                                              |
| **Updated by**   | restructure                                             |

**Completed:** Phase 0 (process established) · Phase 1 (Project Foundation) ·
Phase 2 (Environment Audit — `docs/verification/01_env_audit.md`).

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
   unless explicitly marked parallel-safe.
3. **No phase bleed.** Phase `N` cannot start until Phase `N-1` is `COMPLETE` and
   every `Depends on` entry is `COMPLETE`.
4. **Regression reopens.** If a later change breaks a `COMPLETE` phase's gate, set
   it back to `IN PROGRESS`, move the pointer back, record why in §5.
5. **Blocked is explicit.** `BLOCKED` needs a named blocker + a dated §5 entry.
6. **Every transition updates §1 and §5.**
7. **Gate honesty.** If a gate check cannot be executed in the current
   environment, record it as `NOT EXECUTED` with the reason — never as passed.

---

## 4. Phase Ledger

Legend: **Objective** · **Depends on** · **Status** · **Gate** (explicit,
physically executable) · stages are defined at phase entry unless shown.

---

### EPOCH 0 — Bootstrap (Phases 0–5)

---

### Phase 0 — Establish the Process
- **Objective**: Rules and state-tracking exist before any work: engineering
  constitution, agent operating manual, this state machine.
- **Depends on**: —
- **Status**: `COMPLETE`
- **Gate** (execution record):
  1. `CLAUDE.md` present with Articles I–IV + Operating Manual. — **PASS** (committed `460494b`, `4e4a6c4`).
  2. `ROADMAP.md` present as a state machine with §1–§3 mechanics. — **PASS**.
  3. Repo initialized, first commit exists, tree clean. — **PASS** (`460494b`).

---

### Phase 1 — Project Foundation
- **Objective**: A clean repo with the documentation skeleton and the language /
  runtime direction recorded.
- **Depends on**: Phase 0
- **Status**: `COMPLETE`
- **Gate** (execution record):
  1. `git log` shows commits; working tree clean. — **PASS**.
  2. Root doc placeholders exist (`ARCHITECTURE.md`, `AI_PIPELINES.md`, …). — **PASS** (present, intentionally empty until their owning phase).
  3. Stack direction recorded: Tauri + React/TS + Rust core + AI subprocesses + local SQLite, local-first. — **PASS** (`CLAUDE.md` Art. I–II, as *direction*, not frozen architecture).
- **Deviation note**: formatter/linter/hooks/README-quickstart (originally scoped
  here) were **not** wired — no source tree yet. Carried to **Phase 6**. Recorded
  by owner directive 2026-09-05.

---

### Phase 2 — Environment Audit
- **Objective**: Know the actual dev machine before choosing an architecture that
  depends on it.
- **Depends on**: Phase 1
- **Status**: `COMPLETE` — evidence `docs/verification/01_env_audit.md`
- **Gate** (execution record):
  1. Rust / Node / Python / Git present, versions recorded. — **PASS** (rustc 1.98.0, node 24.19.0, python 3.11.9, git 2.52.0).
  2. MSVC build tools + Windows SDK + WebView2 present. — **PASS** (VS BuildTools 2022 17.14, MSVC 14.44, SDK 10.0.26100, WebView2 152).
  3. GPU + driver + VRAM known. — **PASS** (RTX 5080, 16 GB, driver 610.88, CUDA runtime 13.3, sm_120).
  4. Rust→MSVC→linker chain builds+runs an exe. — **PASS** (throwaway `cargo build`, exit 0).
  5. Disk headroom recorded. — **PASS** (C: 215 GB free — planning constraint).
- **Non-blocking gaps** (→ ADRs / Phase 3): CUDA Toolkit / `nvcc` absent; no pnpm;
  no Python venv/uv; `git core.autocrlf=true`; RAM below rated speed.

---

### ▶ Product Definition  *(between Phase 2 and Phase 3)*
- **Objective**: The owner describes what LocalAI must *be* — features and desired
  experience — **before** architecture research. Claude captures it as structured
  requirements; it does **not** decide architecture here.
- **Depends on**: Phase 2
- **Status**: `NOT STARTED` — **current pointer**
- **Stages**:
  | Stage | Description | Status |
  | ----- | ----------- | ------ |
  | PD.1 | Owner provides the full product vision (features + UX + character system + offline/perf goals) | `NOT STARTED` |
  | PD.2 | Claude captures it into `docs/product/requirements.md`: functional vs non-functional, ambiguities/contradictions, requirements with architectural consequences, questions architecture research must answer | `NOT STARTED` |
  | PD.3 | Owner confirms the capture is accurate and complete | `NOT STARTED` |
- **Gate**:
  1. `docs/product/requirements.md` exists and separates functional from
     non-functional requirements.
  2. Every feature the owner named appears; none added that they did not; no
     difficult requirement dropped for being hard.
  3. A dated list of "questions architecture research must answer" is present.
  4. Owner has explicitly confirmed accuracy (recorded in §5).
- **Not here**: `PROJECT.md` (stays empty until Phase 5); no technology selection;
  no schema; no subsystem design.

---

### Phase 3 — Architecture Research
- **Objective**: Research *how* to build the confirmed product. Every technology
  and boundary is questioned against performance, maintainability, Windows +
  offline constraints, and failure modes. The research stays genuinely open — no
  reverse-engineering a predetermined design.
- **Depends on**: Product Definition
- **Status**: `NOT STARTED`
- **Preliminary input**: `docs/research/01–06` (Tauri IPC, llama-server
  supervision, Python workers, SQLite, VRAM coordination, voice) — options and
  trade-offs, **not decisions**.
- **Gate**:
  1. Each major choice (Tauri vs alternatives; Rust structure — single crate vs
     workspace; llama.cpp integration mode; subprocess transport; SQLite usage;
     resource/VRAM approach; image-identity approach; packaging) has a written
     comparison: pros, cons, performance, Windows/offline fit, failure modes, and
     a recommendation.
  2. Every Product Definition "research question" is answered or explicitly
     deferred with a reason.
  3. Open decisions are enumerated for Phase 4 to attack.
  4. No implementation code written.

---

### Phase 4 — Adversarial Architecture Review
- **Objective**: Try to break the proposed architecture before committing to it.
- **Depends on**: Phase 3
- **Status**: `NOT STARTED`
- **Gate**:
  1. A prioritized risk report covering at least: VRAM/RAM exhaustion, duplicate/
     conflicting model loads, subprocess crashes, orphaned processes, deadlocks/
     races, cancellation failure, DB locking/corruption, IPC failure, malformed AI
     output, config/migration corruption, offline failure, accidental network
     access, duplicated state/logic, character-identity failure.
  2. Each risk: failure description, why the architecture permits it, realistic?,
     simplest robust mitigation, responsible subsystem, how to test it.
  3. Architecture revised to address every realistic high risk, or the risk is
     explicitly accepted with rationale.

---

### Phase 5 — Project Documentation / Architecture Freeze
- **Objective**: Officialize the validated product + architecture.
- **Depends on**: Phase 4
- **Status**: `NOT STARTED`
- **Gate**:
  1. `PROJECT.md` written = the officialized product definition (from Product
     Definition + review), not newly invented.
  2. `ARCHITECTURE.md`, `AI_PIPELINES.md`, `SECURITY.md`, `PERFORMANCE.md`,
     `UI_GUIDELINES.md`, `DEVELOPMENT.md` written to match the frozen design.
  3. One ADR per significant decision in `docs/decisions/` (decision, context,
     options, choice, reason, consequences). Undecided items marked
     `STATUS: UNDECIDED`.
  4. This ROADMAP's Phases 6–39 re-derived against the frozen architecture.
  5. Cross-check pass: no contradictions between the docs; ownership is
     unambiguous. Repo still contains **no application code**.

---

### EPOCH 1 — Core Platform (Phases 6–14)

---

### Phase 6 — Tauri + React + Rust Bootstrap
- **Objective**: The desktop app launches to a placeholder UI; frontend↔Rust IPC
  works; formatting, linting, type-checking, tests, and structured logging are
  wired. No AI functionality.
- **Depends on**: Phase 5
- **Status**: `NOT STARTED`
- **Gate**: format ✓ · lint ✓ · `cargo check` + Rust tests ✓ · frontend
  type-check + tests ✓ · production build ✓ · app launches, window + React UI
  appear, Rust core starts ✓ · a round-trip IPC call succeeds ✓ · no model code
  present ✓ · **carries forward Phase 1's deferred tooling/hooks/README** ✓ ·
  `.gitattributes` added (CRLF decision) ✓.

---

### Phase 7 — Application Contracts
- **Objective**: Strongly-typed, serializable contracts for tasks, model metadata/
  state, generation requests/events, streaming, cancellation, errors,
  conversations, messages, resource reservations, worker jobs. Model names are not
  embedded in business logic.
- **Depends on**: Phase 6
- **Status**: `NOT STARTED`
- **Gate**: contracts compile ✓ · serialize + deserialize round-trip tests ✓ ·
  invalid payloads rejected ✓ · contracts documented ✓ · `git grep` finds no
  model-name literals in logic ✓.

---

### Phase 8 — Configuration
- **Objective**: One typed, validated, versioned configuration system with
  defaults → user config → session overrides. No subsystem invents its own
  settings storage. No secrets in source.
- **Depends on**: Phase 6
- **Status**: `NOT STARTED`
- **Gate**: defaults load ✓ · invalid config rejected with a named error ✓ ·
  persists across restart ✓ · schema migration works ✓ · session override works ✓
  · corrupted/missing config handled without crash ✓.

---

### Phase 9 — SQLite Persistence
- **Objective**: Rust-owned persistence layer: migrations, schema versioning,
  connection management, transactions, repositories, structured errors. Minimum
  schema only. Frontend and subprocesses never touch the DB.
- **Depends on**: Phase 7
- **Status**: `NOT STARTED`
- **Gate**: DB creates from empty ✓ · migrate up/down ✓ · migrate idempotent ✓ ·
  transaction commit + rollback ✓ · survives restart ✓ · migration failure rolls
  back cleanly + backup written ✓ · lock behaviour under contention handled ✓.

---

### Phase 10 — Observability
- **Objective**: Local structured logging + diagnostics: levels, timestamps,
  component, operation, task id, model id, duration, status, structured errors. No
  secrets, no conversation content by default, no cloud telemetry.
- **Depends on**: Phase 6
- **Status**: `NOT STARTED`
- **Gate**: an operation's log lines all carry its task id ✓ · redaction test
  passes (no secrets/PII) ✓ · log level filtering works ✓ · no network egress
  from the logging path ✓.

---

### Phase 11 — Model Registry
- **Objective**: Model metadata as data: stable id, display name, type, backend,
  path, capabilities, context size, quantization, estimated resource need,
  supported devices, model-specific config. No model loading.
- **Depends on**: Phase 9
- **Status**: `NOT STARTED`
- **Gate**: register a test model ✓ · find + read metadata ✓ · missing file
  represented, not crashed ✓ · capability query works ✓ · invalid metadata
  rejected ✓.

---

### Phase 12 — Resource Manager
- **Objective**: Answer "can this operation safely use the GPU right now?".
  Lifecycle: request → reserve → commit → observe → release → reconcile. Mockable
  hardware provider. Never assumes file size == runtime VRAM.
- **Depends on**: Phase 11
- **Status**: `NOT STARTED`
- **Gate** (mocked hardware): insufficient VRAM → clean failure ✓ · duplicate
  reservation rejected ✓ · concurrent requests serialized ✓ · failed load releases
  reservation ✓ · cancellation releases ✓ · stale reservation recovered ✓ · crash
  → reconcile against observed usage ✓.

---

### Phase 13 — Model Lifecycle Manager
- **Objective**: The only component that loads/unloads managed models. Explicit
  state machine (Discovered → Available → Loading → Loaded → Busy → Unloading →
  …), explicit failure/recovery states. No duplicate loads. Failed/cancelled loads
  release resources.
- **Depends on**: Phase 12
- **Status**: `NOT STARTED`
- **Gate**: load ✓ · concurrent load of same model does not duplicate ✓ · unload ✓
  · load under insufficient resources rejected before spawn ✓ · cancel mid-load
  releases ✓ · forced load failure → recovery ✓ · unexpected process exit detected
  + state reconciled ✓ · state-machine unit tests ✓.

---

### Phase 14 — llama.cpp Adapter
- **Objective**: First LLM backend, behind a clean `LlmBackend` interface.
  llama.cpp-specific detail confined to the adapter. Lifecycle owned by Phase 13,
  resources by Phase 12. Transport per the Phase 3 ADR.
- **Depends on**: Phase 13
- **Status**: `NOT STARTED`
- **Gate**: startup + readiness check ✓ · non-streaming generation ✓ · streaming
  generation ✓ · cancellation reaches the actual job ✓ · timeout handled ✓ ·
  process-crash detected + recovered ✓ · clean shutdown, no orphan process ✓ · no
  raw llama.cpp config leaks past the adapter ✓.

---

### EPOCH 2 — First Slice & Modalities (Phases 15–21)

---

### Phase 15 — First Vertical Slice / Text Chat
- **Objective**: The whole path works: React → IPC → Rust → conversation service →
  LLM service → llama.cpp → streamed tokens → UI. Basic text chat **only**.
  Reliability over features.
- **Depends on**: Phase 14
- **Status**: `NOT STARTED`
- **Gate**: send a message → streamed reply ✓ · cancel mid-generation, model stays
  reusable ✓ · conversation persists ✓ · restart restores the conversation ✓ ·
  kill llama.cpp mid-generation → detected, recovers, next generation works ✓ ·
  clean app shutdown during generation ✓ · concurrent-generation attempt handled
  per policy ✓. **This gate is the first real milestone.**

---

### Phase 16 — Conversation Engine
- **Objective**: Formalize the single conversation engine (lifecycle, messages,
  roles, content, timestamps, streaming state, cancellation, generation metadata,
  persistence) that voice, personas, memory, and characters will all share. No
  second engine, ever.
- **Depends on**: Phase 15
- **Status**: `NOT STARTED`
- **Gate**: engine unit tests cover lifecycle + streaming + cancel + persistence
  ✓ · text chat re-hosted on the engine with no behaviour regression (Phase 15
  gate still passes) ✓.

---

### Phase 17 — Voice
- **Objective**: Voice as a **modality of the conversation engine**: mic capture →
  STT → user message → engine → LLM → TTS → playback, with interruption. Not a
  second chat system. Rust owns orchestration + audio devices; workers do AI only.
- **Depends on**: Phase 16, Phase 12
- **Status**: `NOT STARTED`
- **Gate**: device discovery + selection ✓ · record → STT → reply → TTS end to end
  ✓ · barge-in / cancel stops LLM + TTS and returns to listening ✓ · STT failure,
  TTS failure, worker crash, missing device each handled ✓ · interrupted assistant
  turn persisted as truncated ✓.

---

### Phase 18 — Personas
- **Objective**: Structured persona data + a predictable context builder (system
  instructions + persona + character + conversation + memory + runtime context).
  Verifiable that persona content actually reaches the model.
- **Depends on**: Phase 16
- **Status**: `NOT STARTED`
- **Gate**: persona stored as structured data ✓ · context builder unit tests ✓ ·
  a test proves the assembled prompt sent to the LLM contains the persona ✓ ·
  switching persona changes model behaviour in a scripted check ✓.

---

### Phase 19 — Memory
- **Objective**: Persistent memory: extraction → importance/validation → storage →
  relevant retrieval → context construction. SQLite-based first. No vector DB
  unless measurements justify it.
- **Depends on**: Phase 18, Phase 9
- **Status**: `NOT STARTED`
- **Gate**: memory extracted from a conversation + stored ✓ · retrieval returns
  relevant memories for a new turn ✓ · context stays within a configured budget ✓
  · deterministic retrieval for a fixed store+query ✓ · memory survives restart ✓.

---

### Phase 20 — Image Generation
- **Objective**: Local image generation as a resource-managed workload (worker +
  model lifecycle + VRAM coordination + filesystem for binaries + DB metadata).
  Not a standalone button bolted on.
- **Depends on**: Phase 12, Phase 13, Phase 9
- **Status**: `NOT STARTED`
- **Gate**: generate an image via a worker ✓ · VRAM reserved/released around the
  job ✓ · binary written to the file vault, metadata + path + hash in SQLite ✓ ·
  cancellation mid-generation releases resources ✓ · worker crash handled ✓.

---

### Phase 21 — Model Hot-Swapping
- **Objective**: Different GPU workloads share limited VRAM: suspend/unload LLM →
  load image model → generate → release → restore LLM, coordinated by a scheduler.
  Handle runtimes that cannot suspend cleanly (terminate + restart).
- **Depends on**: Phase 20, Phase 12
- **Status**: `NOT STARTED`
- **Gate**: LLM loaded → image request → swap → generate → LLM restored, all
  observed via real VRAM measurement ✓ · swap refused when even eviction can't fit
  ✓ · swap does not interrupt an in-flight generation (queues/finishes first) ✓ ·
  crash during swap → reconcile, no leaked reservation ✓.

---

### EPOCH 3 — Character Platform (Phases 22–26)

---

### Phase 22 — Character System
- **Objective**: Characters as persistent entities with structured state
  (identity, appearance, personality, interests, relationship, memory, reference
  images, generated images) — not a giant prompt. Reuses the conversation engine +
  memory + persona context builder.
- **Depends on**: Phase 18, Phase 19
- **Status**: `NOT STARTED`
- **Gate**: create/read/update a character as structured data ✓ · a character
  conversation runs on the shared engine ✓ · character context assembled from
  structured fields (test proves it reaches the model) ✓ · character state + memory
  persist across restart ✓.

---

### Phase 23 — Persistent Character Identity
- **Objective**: A character stays the *same conceptual entity* across
  conversations and generated media: reference identity → conditioning →
  generation → identity verification → accept/regenerate. Accepts that perfect
  identity preservation is not guaranteed.
- **Depends on**: Phase 22, Phase 20
- **Status**: `NOT STARTED`
- **Gate**: reference images stored + linked to a character ✓ · generation
  conditioned on reference identity ✓ · an identity-similarity check gates
  accept vs regenerate ✓ · accepted images linked to the character in the DB ✓.

---

### Phase 24 — Typed Character Image Actions
- **Objective**: The LLM requests images only through a **typed, validated action**
  (`character_id`, `image_type`, `scene`, `mood`, `clothing`, `context`, …). Rust
  validates and decides. No raw generation command from model output.
  (`CLAUDE.md` Article III.)
- **Depends on**: Phase 23, Phase 7
- **Status**: `NOT STARTED`
- **Gate**: model emits a well-formed action → validated → executed via the image
  subsystem ✓ · malformed/unknown action rejected + logged, nothing executed ✓ ·
  action referencing another character's id is refused ✓ · no code path turns
  model text into a shell/file/process operation ✓.

---

### Phase 25 — Tinder-Style Character Discovery
- **Objective**: Browse AI-generated characters, view profile + images, choose one
  → start a conversation against the **persistent** character entity (not a
  temporary prompt). Relationship persists on return.
- **Depends on**: Phase 22
- **Status**: `NOT STARTED`
- **Gate**: discovery list renders characters from stored data ✓ · selecting a
  character opens a conversation bound to that entity's id ✓ · leaving and
  returning restores relationship + memory + history ✓.

---

### Phase 26 — Scheduler Formalization
- **Objective**: Formalize the scheduler that arbitrates GPU/worker/task work
  (introduced ad hoc in Phase 21) into one authoritative component: queueing,
  priorities, cancellation, fairness, reconciliation.
- **Depends on**: Phase 21
- **Status**: `NOT STARTED`
- **Gate**: competing GPU requests (LLM + image) are ordered by policy ✓ ·
  cancellation removes a queued or running job cleanly ✓ · no two GPU jobs run
  concurrently when they must not ✓ · scheduler state reconciles after a crash ✓.

---

### EPOCH 4 — Product & Hardening (Phases 27–35)

---

### Phase 27 — UI / UX
- **Objective**: Bring the UI to the bar in `UI_GUIDELINES.md` across all flows:
  chat, voice, characters, gallery, discovery, settings — states for loading,
  empty, error, streaming; keyboard + accessibility.
- **Depends on**: Phases 15–25
- **Status**: `NOT STARTED`
- **Gate**: every primary flow has loading/empty/error states ✓ · primary flow
  completed keyboard-only ✓ · automated a11y scan: no critical violations ✓ ·
  light + dark both pass contrast ✓.

---

### Phase 28 — Performance Audit
- **Objective**: Measure, then improve, then re-measure — no intuition-only
  optimization. Baselines in `PERFORMANCE.md`.
- **Depends on**: Phase 27
- **Status**: `NOT STARTED`
- **Gate**: recorded baselines for startup, UI responsiveness, LLM latency + token
  throughput, image generation, model switching, DB ops, memory retrieval ✓ · each
  applied optimization has before/after numbers ✓ · no regression against the
  Phase 15 reliability gate ✓.

---

### Phase 29 — Offline Audit
- **Objective**: Prove the runtime is genuinely local-first.
- **Depends on**: Phase 27
- **Status**: `NOT STARTED`
- **Gate**: with the network disabled, every core feature works (chat, voice,
  memory, characters, image gen) ✓ · a network-traffic capture during normal use
  shows no external egress ✓ · the only network code paths are explicit
  model/dependency acquisition + update check, each isolated and skippable ✓.

---

### Phase 30 — Fault Injection / Reliability
- **Objective**: Deliberately break things and confirm graceful behaviour.
- **Depends on**: Phase 28, Phase 29
- **Status**: `NOT STARTED`
- **Gate**: scripted faults — kill llama.cpp; kill each worker; remove a model
  file; corrupt config; corrupt/lock the DB; exhaust VRAM; cancel generation;
  close app mid-generation; worker timeout; restart after crash — each has a
  defined, tested outcome and a regression test ✓.

---

### Phase 31 — Security Audit
- **Objective**: Threat model + resolved findings. Local ≠ safe.
- **Depends on**: Phase 24, Phase 20
- **Status**: `NOT STARTED`
- **Gate**: `SECURITY.md` threat model (assets, actors, entry points, mitigations)
  ✓ · checks pass for: arbitrary command execution, model-action validation, path
  traversal / filesystem confinement, worker input validation, safe process-arg
  construction, secrets absent from logs + frontend bundle, loopback-only local
  services ✓ · no open high/critical findings ✓.

---

### Phase 32 — Dependency Audit
- **Objective**: Every dependency (Rust crates, npm packages, Python packages,
  native/AI runtimes) is justified, licensed-compatible, and vulnerability-scanned.
- **Depends on**: Phase 31
- **Status**: `NOT STARTED`
- **Gate**: dependency inventory with a one-line justification each ✓ · license
  check clean ✓ · `cargo audit` / `npm audit` / Python audit clean or triaged ✓ ·
  unused dependencies removed ✓.

---

### Phase 33 — Maintainability / Architecture Audit
- **Objective**: Confirm the codebase still matches the frozen architecture — no
  drift, no duplicate authorities, ownership boundaries intact.
- **Depends on**: Phase 32
- **Status**: `NOT STARTED`
- **Gate**: a trace of each subsystem vs `ARCHITECTURE.md` — no undocumented
  component ✓ · no duplicated state/logic across runtimes ✓ · Article I boundaries
  hold (frontend/worker never touch DB/filesystem/processes) ✓ · ADRs exist for
  any decision that changed during implementation ✓.

---

### Phase 34 — Packaging
- **Objective**: A Windows installer that bundles the app, native deps, workers,
  and handles model acquisition + offline runtime.
- **Depends on**: Phase 33
- **Status**: `NOT STARTED`
- **Gate**: clean-machine install produces a working app ✓ · workers + native
  runtimes launch from the installed layout ✓ · first-run model acquisition works
  and is skippable ✓ · uninstall is clean ✓ · installed app passes the Phase 29
  offline gate ✓.

---

### Phase 35 — Final Architecture Audit
- **Objective**: One last whole-system review before release readiness.
- **Depends on**: Phase 34
- **Status**: `NOT STARTED`
- **Gate**: all prior audit gates (28–33) still green ✓ · `ARCHITECTURE.md`,
  `AI_PIPELINES.md`, `PROJECT.md` reflect the shipped system ✓ · a fresh reader can
  trace a chat request and an image request end to end from the docs ✓.

---

### EPOCH 5 — Operationalization & Release (Phases 36–39)

---

### Phase 36 — Permanent Claude Workflow
- **Objective**: Convert the bootstrap discipline into the steady-state working
  process for ongoing development after release.
- **Depends on**: Phase 35
- **Status**: `NOT STARTED`
- **Gate**: `DEVELOPMENT.md` documents the permanent loop (task intake → inspect →
  plan → implement → verify → review → commit) ✓ · `CLAUDE.md` updated to
  post-bootstrap mode ✓ · a sample change run through the loop with evidence ✓.

---

### Phase 37 — Git Strategy
- **Objective**: Formalize branching, commit conventions, checkpoint discipline,
  and release tagging.
- **Depends on**: Phase 36
- **Status**: `NOT STARTED`
- **Gate**: `DEVELOPMENT.md` git section written ✓ · commit-message + branch rules
  enforced by a hook ✓ · a dry-run release tag + changelog produced ✓ · remote /
  push policy explicitly decided and recorded ✓.

---

### Phase 38 — Context Efficiency
- **Objective**: Make the repo efficient for a fresh Claude session — concise
  authoritative docs, a clear entry path, no stale or contradictory guidance.
- **Depends on**: Phase 37
- **Status**: `NOT STARTED`
- **Gate**: a fresh session, given only the repo, correctly states the project
  status and next action ✓ · doc set reviewed for redundancy/staleness ✓ ·
  `ROADMAP.md` + `CLAUDE.md` within a sane size budget ✓.

---

### Phase 39 — Final Release Gates
- **Objective**: The release checklist. Ship only when every prior epoch's gates
  are green and the product does what `PROJECT.md` says.
- **Depends on**: Phases 0–38
- **Status**: `NOT STARTED`
- **Gate**: every phase in this ledger is `COMPLETE` ✓ · Phase 15, 29, 30, 31
  gates re-run green on the packaged build ✓ · `PROJECT.md` feature list verified
  against the running app, item by item ✓ · known-issues list published ✓.

---

## 5. Transition Log

Newest first. One line per state transition (§3 rule 6).

| Date       | From | To | By | Note |
| ---------- | ---- | -- | -- | ---- |
| 2026-09-05 | 32-phase working-draft ledger | 40-phase course (0–39) + Product Definition step | owner-delegated | §4 replaced with the LocalAI phase structure from the owner's ChatGPT planning chat. Mechanics (§1–3, §5) unchanged. Old Phase 1 → new Phase 1 (Project Foundation); old Phase 0 (env audit) → new Phase 2. Pointer set to **Product Definition / PD.1 / NOT STARTED**. §7 rewritten. |
| 2026-09-05 | Article I amended | Article I reverted | owner-delegated | Managed-backend / stdio split removed from the Constitution — subprocess transport is a Phase 3 architecture-research question, not a Phase 0 decision. `CLAUDE.md` Art. I now states the fixed principles + lists transport candidates as TBD. §7 O3 reopened. |
| 2026-09-05 | (no state change) | — | owner-delegated | `CLAUDE.md` restructured: Document Map + Operating Manual (docs-only era, external-guide handling, commit cadence, collision protocol). |
| 2026-09-05 | (no state change) | — | research | Pre-architecture research added: `docs/research/01–06` + README. Input to Phase 3, not decisions. |
| 2026-09-05 | old Phase 0 `NOT STARTED` | old Phase 0 `COMPLETE` | audit | Dev-environment audit; evidence `docs/verification/01_env_audit.md`. Gate checks physically executed. (Now new Phase 2.) |
| 2026-09-05 | old Phase 1 stages 1.1–1.6 | old Phase 1 `COMPLETE` | owner | Foundation closed by directive; deferred tooling carried to build bootstrap. (Now new Phase 1.) |
| 2026-09-05 | old 1.2 | old 1.2 `COMPLETE` | owner | `ROADMAP.md` + `CLAUDE.md` authored. |
| 2026-09-05 | old 1.1 | old 1.1 `VERIFIED` | bootstrap | Repo initialized on `main`. |
| 2026-09-05 | — | old 1.2 `IN PROGRESS` | bootstrap | State machine established. |

---

## 6. Open Blockers

- **Product Definition / PD.1** — waiting on the owner's product description. This
  is the current pointer; nothing else proceeds until PD is `COMPLETE`.

Phase 6 carries forward Phase 1's deferred formatter/linter/hook/README tooling —
tracked, not blocking.

---

## 7. Open Questions & Resolved Facts

### Resolved facts (binding)

- **R1. LocalAI is a local-first AI *desktop* application for Windows.** Direction:
  Tauri + React/TypeScript + Rust core + isolated AI subprocesses (llama.cpp;
  Python for STT/TTS/image/embeddings) + local SQLite. No cloud, no remote DB, no
  telemetry at runtime (`CLAUDE.md` Art. I–II). *Direction, not frozen
  architecture — Phase 3 confirms the details.*
- **R2. The character system is a core capability**, not a bolt-on: persistent
  entities with structured identity/memory/relationship and identity-consistent
  generated images. Architecture must be designed with it in mind (Phases 22–25).
- **R3. Untrusted AI output is contained** — only allow-listed typed Rust ops;
  applies especially to character image actions (`CLAUDE.md` Art. III, Phase 24).
- **R4. Verification honesty + smallest-correct-implementation** are binding
  (`CLAUDE.md` Art. IV). Everything is questioned for performance/optimization
  during Phases 3–4 and the audit phases (28–35).

### Open questions (for Product Definition → Phase 3)

- **O1. Memory / retrieval mechanism.** Start SQLite-only; a vector DB only if
  measurements justify it (Phase 19). Confirm scope in Product Definition.
- **O2. Single-user vs multi-user.** Working assumption: **single-user, local
  profile**. Confirm in Product Definition (affects characters, settings, at-rest
  encryption).
- **O3. Subprocess transport** (reopened). stdio JSON-lines for authored workers
  vs loopback HTTP for `llama-server` vs in-process FFI — decide in Phase 3, record
  as an ADR. Until then, no code depends on a specific transport.
- **O4. Rust structure** — single Tauri crate with modules vs Cargo workspace.
  Default to single crate; split only with a concrete reason (Phase 3 / Phase 6).
- **O5. Environment gaps from Phase 2** — CUDA build strategy (source vs prebuilt
  CUDA binaries for `llama.cpp`), Python env strategy (venv/uv/embedded), package
  manager (npm vs pnpm), model storage location + disk budget. Resolve in Phase 3;
  some become ADRs.

### Closed

- **Environment-inspection gap** → became **Phase 2**, complete 2026-09-05
  (`docs/verification/01_env_audit.md`).
- **Constitution location** — stays in `CLAUDE.md`; `PROJECT.md` reserved for
  Phase 5.
- **Generic web-service phase ledger** → replaced with the LocalAI 40-phase
  course (this file, 2026-09-05).
