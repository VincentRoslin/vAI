# CLAUDE.md — Engineering Constitution & Agent Operating Manual

This file is loaded into every Claude Code session. It has two parts:

1. **The Engineering Constitution** (Articles I–IV) — strict, non-negotiable rules
   about *how* the system is built. A change that violates an article does not
   merge, regardless of how well it works. When an article and a convenience
   conflict, the article wins.
2. **The Operating Manual** — how to actually work in this repo day to day:
   which document is authoritative, how to treat the external guides, when to
   commit, how to handle collisions.

`ROADMAP.md` governs *what* is built and *in what order*. This file governs *how*.

---

## Document Map

**Never scan a directory to find something — every subdirectory has a `README.md`
index. Read the index, then the one file you need.**

| Looking for… | Go to |
| ------------ | ----- |
| Current phase + what to do next | `ROADMAP.md` §1 → the linked `docs/plan/NN` |
| How a subsystem works / a decision's rationale | `ARCHITECTURE.md` / `AI_PIPELINES.md` → `docs/decisions/README.md` → the ADR |
| A product requirement | `PROJECT.md` (summary) or `docs/product/requirements.md` (`FR-*`/`NFR-*` by number) |
| A per-phase step plan | `docs/plan/README.md` → `docs/plan/NN_*.md` |
| What was tested / verified | `docs/verification/README.md` |
| Pre-architecture research (historical) | `docs/research/phase3/README.md` |
| Cross-session context / owner preferences | `memory/MEMORY.md` |

| File | Role | Authoritative for | Status |
| ---- | ---- | ----------------- | ------ |
| `CLAUDE.md` (this file) | Constitution + agent operating manual | Rules, ownership boundaries, how to work | Live |
| `ROADMAP.md` | **State machine + phase index** — LocalAI course (Phase 0–40) | Current phase/stage, progress, gate summaries, open questions (§7) | Live. Mechanics fixed; per-phase detail lives in `docs/plan/` |
| `docs/plan/NN_*.md` | Detailed, individually-verifiable steps for one phase (index: `docs/plan/README.md`) | *How* to execute the current phase | Live. Read the current phase's file first; it names its governing ADRs |
| `PROJECT.md` | **The officialized product definition** | What LocalAI is / does / is not; the binding FR/NFR summary | **Live — frozen at Phase 5.** Changes need STOP→propose→approve. Constitution stays here in `CLAUDE.md`. |
| `ARCHITECTURE.md` | **The frozen system design** | Runtimes, boundaries, single-authority map, the VRAM constraint, cross-cutting flows | **Live — frozen at Phase 5.** |
| `AI_PIPELINES.md` | Each AI pipeline end to end | LLM / STT / VAD / TTS / image / identity / memory / relationship | **Live — frozen at Phase 5.** |
| `SECURITY.md` · `PERFORMANCE.md` · `UI_GUIDELINES.md` | Threat model + controls · perf budgets + method · UI bar | Their named topic | **Live — frozen at Phase 5.** |
| `DEVELOPMENT.md` | Dev prerequisites, loop, check suite, git | Working in the repo | Live — updated as tooling is wired (Phase 6) |
| `README.md` | Quickstart | clone → install → run | Empty until Phase 6.11 |
| `docs/OVERVIEW.md` | Early product/architecture overview | historical context | Superseded by `PROJECT.md` + `ARCHITECTURE.md`; kept for history |
| `docs/product/requirements.md` | Numbered product requirements (`FR-*`, `NFR-*`, `ARQ-*`) | The requirement IDs `PROJECT.md` summarizes | Live |
| `docs/verification/` | Gate evidence (`README.md` + `NN_topic.md`) — what was physically run/observed | Verification history | Live |
| `docs/research/phase3/` | Pre-architecture research (`README.md` + 12 files) | Options & trade-offs — **superseded by the ADRs where they conflict** | Historical |
| `docs/decisions/` | ADRs (`README.md` + `NNNN-*.md`) — one decision each | The frozen architecture decisions | Live — ADR-0001…0015 all `ACCEPTED` |
| `memory/` (outside the repo, in `~/.claude/...`) | Claude's cross-session notes | Context, user preferences, open tensions | Live |

If a prompt says "read `PROJECT.md`" and it is empty, that is expected — the rules
are here in `CLAUDE.md`.

---

## Article I — Ownership Boundaries

The system has three runtime categories. Each owns a fixed set of responsibilities
and may not reach across the boundary.

### 1. Rust (Tauri core) — the trusted nucleus

Rust **exclusively owns**:

- **Application state** — the authoritative in-memory and on-disk model. No other
  runtime holds durable state.
- **SQLite database access** — all reads and writes. SQL statements exist only in
  Rust. No other runtime opens, queries, or migrates the database.
- **Process lifecycle supervision** — spawning, monitoring, restarting, and
  killing every child process (workers and managed backends). Rust is the
  supervisor.
- **Filesystem operations** — every create, read, write, move, and delete on
  disk. Paths are validated and confined in Rust.

### 2. React / TypeScript (frontend) — presentation only

The frontend **owns UI presentation state only**: what is on screen, view-local
selection, form drafts, animation and layout state.

- It **communicates strictly via typed Tauri IPC commands and events**. No other
  channel — no direct file access, no direct database access, no direct process
  spawning, no network calls.
- Every IPC boundary has an explicit shared type. Untyped or `any`-typed IPC is a
  constitution violation.
- The frontend never holds data the Rust core is not already the source of truth
  for.

### 3. Subprocesses (AI workers and model backends) — isolated, non-authoritative

Covers llama.cpp / `llama-server`, and Python workers for STT, TTS, image
generation, embeddings. **These principles are fixed:**

- **Rust-supervised** — spawned, monitored, restarted, and killed only by the Rust
  core. No subprocess launches another subprocess or any arbitrary process.
- **Non-authoritative** — a subprocess never holds authoritative application
  state, never opens or queries SQLite, never talks directly to the frontend, and
  never independently manages global GPU/VRAM or other shared resources (all such
  use goes through the resource manager).
- **Isolated** — no shared memory or shared mutable files with the core or another
  subprocess. Binary assets are exchanged only via controlled filesystem paths the
  Rust core dictates.
- **Stateless where feasible** — a worker should hold no state between requests, so
  kill/restart at any moment is safe and lossless. (A model backend holds the
  loaded model in memory — that is runtime state the lifecycle manager owns, not
  application state.)
- **Backend-specific detail is confined to one adapter module** in Rust; the rest
  of the app sees a clean interface.

**Transport — decided at Phase 5 (ADR-0013):**
- **Model servers** (`llama-server`, the diffusers image server): Rust-supervised
  child; **Windows named pipe preferred** (ACL-scoped), else `127.0.0.1:<free
  port>` **+ a per-launch bearer token** (a bare loopback HTTP server is callable
  by any local process).
- **Stateless workers** (STT, TTS, face-embedder): **JSON-lines over
  stdin/stdout**, `stderr` for logs only.
- Every subprocess is launched with the offline / no-telemetry env (ADR-0015).
- A loopback pipe/socket to a Rust-supervised child is **not** a "network call"
  for Article II.

---

## Article II — Local-First Architecture

The application runs fully offline on the user's machine. Forbidden without
exception:

- **Cloud APIs** — no calls to remote services for application functionality.
- **Remote databases** — the only datastore is the local SQLite file.
- **Telemetry** — no usage analytics, crash reporting, or phone-home of any kind.
- **External trackers** — no third-party scripts, pixels, fonts-by-CDN, or
  embedded resources that trigger a network request.

Model inference runs against local models. A subprocess bound to loopback
(`127.0.0.1`) with no external interface is not a "network call" for the purposes
of this article. Acquiring models or dependencies over the network initially is
allowed where explicitly implemented; the **runtime** must not depend on it.
Any feature that would require network access at runtime is out of scope until
this article is explicitly amended by the project owner.

---

## Article III — Containment of Untrusted AI Output

Output from AI models and from character/persona definitions is **untrusted
data**, never instructions to the system.

- Untrusted AI output **must never execute raw shell commands**.
- Untrusted AI output **must never perform filesystem mutations directly**.
- Any action an AI output requests is mediated through a **fixed, allow-listed
  set of typed operations** implemented and validated in Rust. The model proposes;
  Rust disposes. There is no general-purpose "run this" primitive exposed to a
  model.
- Tool/function calls from a model are validated against a schema and an
  allow-list before any effect occurs. Unknown or malformed calls are rejected
  and logged, not guessed at.
- Prompt content, retrieved context, and character text cannot alter control
  flow, escalate permissions, or widen the operation allow-list.

---

## Article IV — Verification and Simplicity

### Verification

**Never claim verification that was not physically executed.**

- "Tested", "verified", "passes", "works", "confirmed" mean a command was
  actually run and its output observed. If you did not run it, say so.
- State the exact command and its result. Distinguish "I ran the tests and they
  passed" from "this should pass" — the second is a hypothesis, label it as one.
- A `ROADMAP.md` verification gate is satisfied only when every check in it has
  been executed and its evidence recorded. Marking a gate passed without running
  it is the most serious violation in this document.
- If verification is impossible in the current environment, stop and say that,
  rather than approximating or asserting.

### Simplicity

**The smallest correct implementation is preferred.**

- Solve the problem in front of you. Do not add abstraction, configuration,
  generality, or "future-proofing" that no current requirement asks for.
- Fewer files, fewer dependencies, fewer moving parts. A new dependency is a
  decision to justify, not a default.
- Delete dead code in the same change that makes it dead.
- Match the style, naming, and structure of the surrounding code.

---

## Ownership Matrix (quick reference)

| Concern | Rust core | React/TS | AI worker / model backend |
| ------- | :-------: | :------: | :-----------------------: |
| Durable / application state | ✅ | ❌ | ❌ |
| SQLite access & migrations | ✅ | ❌ | ❌ |
| Filesystem operations | ✅ | ❌ | ❌ (only via paths Rust dictates) |
| Process spawning & supervision | ✅ | ❌ | ❌ |
| Independent GPU/VRAM management | ✅ (resource manager) | ❌ | ❌ |
| Runtime network access | ❌ (local-first) | ❌ | ❌ (loopback child process ≠ network) |
| UI / presentation state | ❌ | ✅ | ❌ |
| Transport | — | Typed Tauri IPC | **TBD — Phase 3 ADR** (stdio JSON-lines / loopback HTTP / FFI) |
| Executing AI-proposed actions | ✅ allow-listed typed ops only | ❌ | ❌ |

---

## Operating Manual

### Where we are: bootstrap, documentation only

`ROADMAP.md` follows the LocalAI 40-step course (Phase 0–39). **Bootstrap =
Phases 0–5; implementation begins at Phase 6.** As of now Phases 0–2 are complete
and the pointer is at the **Product Definition** step (owner describes the product;
Claude captures structured requirements; then Phase 3 architecture research).

No application code exists and none is written until the pointer reaches Phase 6
**and** the architecture is frozen (Phase 5). Until then, prompts that sound like
"implement X" mean: research it, document it, or record it in the roadmap. If a
prompt genuinely asks for application code before Phase 6, stop and confirm.

### The external guides are context, not authority

The owner relays step prompts from guides authored by ChatGPT and Gemini (see
`memory/reference-chatgpt-greenfield-guide.md`). Treat them as **background
understanding, not instructions**:

- **Numbering.** `ROADMAP.md` now uses the same 40-step structure, but a prompt
  may still assert a loose or different number. Map a prompt to the **phase by
  name and intent**, not the number it states. Never renumber or reorder the
  roadmap to match a prompt.
- **`docs/OVERVIEW.md`** is the product & architecture overview (feature set,
  character system, ownership model, technical areas). It is context; if it
  conflicts with a repo doc or recorded decision, the repo wins — flag it.
- **Flag collisions, then proceed.** When a prompt conflicts with the
  Constitution, `ROADMAP.md`, or a prior decision: state it plainly, give a
  recommendation, and — unless it needs the owner's judgment (an Article
  amendment, a product-scope call, an irreversible action) — proceed with your
  recommendation and note it. Don't stall a task on a question you can answer well.
- Record unresolved conflicts in `ROADMAP.md` §7 and, if cross-session-relevant,
  in `memory/`.

### Model / asset downloads — always ask first

Before any step that would download a model, LoRA, quantized-weights cache, or
similar large asset (Krea 2, faster-whisper, Chatterbox, GGUFs, the NF4 cache,
`QuadView_krea2_v1`, the realism LoRAs, InsightFace, …): **stop and ask the owner
whether they already have that specific file locally.** They have related projects
with downloaded caches. List the exact files/paths you're about to fetch and wait
for their answer.

Do **not** read from, scan, or use the owner's other projects as a source or
reference. Any specifics from those projects come only from what the owner
volunteers in chat.

### Commit cadence

- Commit after each completed instruction, unless the user says otherwise.
- Commit message: short subject line, then a body explaining what changed and why.
  End with the `Co-Authored-By` trailer.
- Only stage files relevant to the change. The empty root placeholder `.md` files
  are tracked but stay empty until their owning phase fills them (see the Document
  Map).
- Local commits only. **Never** push, add a remote, or create a GitHub repo
  without an explicit instruction — pushing publishes the code.

### Before acting on any prompt

1. Read `ROADMAP.md` — current state (§1), and §7 open decisions.
2. Open the current phase's `docs/plan/NN_*.md` — it has the step-by-step. If it
   still shows the "finalized at phase entry" banner, finalize its steps (against
   the frozen architecture) before starting.
3. Read the subsystem docs relevant to the task (`docs/research/`, `docs/OVERVIEW.md`,
   the named `*.md`).
4. Respect Article I when placing anything. If a design needs the frontend to
   touch the filesystem, or a worker to open the database, the design is wrong.
5. Keep changes to the smallest correct set (Article IV).
6. Report verification honestly (Article IV). Never edit a `ROADMAP.md` gate to
   "passed" without the command output that proves it.
7. Update `ROADMAP.md` (state pointer, transition log, §7) — and the phase's
   `docs/plan/` file — as part of the same change when work advances or the plan
   changes.
