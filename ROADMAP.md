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
| **Phase**        | 12 — Model Acquisition & Picker                         |
| **Stage**        | 12.1 — deps + skeleton + `V0003`                        |
| **Status**       | `IN PROGRESS`                                           |
| **Blocked by**   | —                                                      |
| **Plan doc**     | `docs/plan/12_model-acquisition.md` (finalized 2026-09-06) |
| **Last updated** | 2026-09-06                                              |
| **Updated by**   | phase-12-acquisition                                    |

**Architecture frozen (Phase 5).** Binding: `PROJECT.md`, `ARCHITECTURE.md`,
`AI_PIPELINES.md`, `SECURITY.md`, `PERFORMANCE.md`, `UI_GUIDELINES.md`,
`DEVELOPMENT.md`, `docs/decisions/0001–0015` (all `ACCEPTED`).
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

### Phase 12 — Model Acquisition & Picker *(current pointer)* — `NOT STARTED` — `docs/plan/12_model-acquisition.md`
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

_None._

- **Phase 15 watch item**: WDDM hang risk on the first sustained `llama-server`
  generation (Hyper-V enabled on host — inconclusive from inspection). Mitigations
  in `docs/verification/02_phase3_probes.md`.
- Phase 1's deferred tooling (formatter/linter/hooks/README) → **done in Phase 6.**

---

## 7. Decisions & open items

**The architecture is frozen (Phase 5, 2026-09-05).** The binding record is
`PROJECT.md` + `ARCHITECTURE.md` + `AI_PIPELINES.md` + `SECURITY.md` +
`PERFORMANCE.md` + `UI_GUIDELINES.md` + `DEVELOPMENT.md` + `docs/decisions/`
(ADR-0001…0015, all `ACCEPTED`). Changes need STOP → propose → approve → ADR.

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

- **WDDM hang** on the first sustained `llama-server` generation (Hyper-V enabled
  on host) → Phase 15, 3-step mitigation ladder in
  `docs/verification/02_phase3_probes.md`.

### Closed (historical)

- Environment-inspection gap → Phase 2. Constitution location → stays in
  `CLAUDE.md`. Generic web-service ledger → replaced with the LocalAI course.
  Phase-list detail → `docs/plan/`. `docs/OVERVIEW.md` → superseded by `PROJECT.md`
  + `ARCHITECTURE.md`.
