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

| File | Role | Authoritative for | Status |
| ---- | ---- | ----------------- | ------ |
| `CLAUDE.md` (this file) | Constitution + agent operating manual | Rules, ownership boundaries, how to work | Live |
| `ROADMAP.md` | Bootstrap **state machine** (working draft) | Current phase/stage, progress, verification gates, open decisions (§7) | Live; phase *content* provisional |
| `PROJECT.md` | Reserved for the product/architecture definition | — | **Intentionally empty** until architecture freeze. Do not populate without an explicit instruction. The constitution lives here in `CLAUDE.md` for now, and may migrate to `PROJECT.md` at freeze. |
| `ARCHITECTURE.md`, `AI_PIPELINES.md`, `PERFORMANCE.md`, `SECURITY.md`, `UI_GUIDELINES.md`, `DEVELOPMENT.md`, `README.md` | Subsystem / process docs | Their named topic | **Empty placeholders**, filled by their owning phases |
| `docs/verification/` | Evidence logs (`NN_topic.md`) — what was physically run and observed | Verification history | Live — `01_env_audit.md` |
| `docs/research/` | Pre-architecture technical research (`NN_topic.md` + `README.md`) | Options & trade-offs, **not decisions** | Live |
| `docs/decisions/` | ADRs — one ratified decision each | Decision record | Empty; starts at architecture phase |
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

### 3. Subprocesses — isolated, non-authoritative

Two sub-categories. Both are spawned and supervised **only** by the Rust core,
never hold authoritative state, never touch SQLite directly, and must route all
GPU use through the resource manager.

- **Stateless workers** (STT, TTS, image generation — Python):
  - **Isolated** — no shared memory, no shared files, no database handle.
  - **Stateless** — no state between requests; kill/restart at any moment must be
    safe and lossless.
  - **Communicate exclusively via JSON-lines over stdin/stdout.** `stderr` is
    diagnostic logging only, never parsed for control flow. Workers do not open
    sockets, files, or database connections.
- **Managed model backends** (e.g. `llama-server`):
  - Upstream-maintained inference servers that ship as HTTP services.
  - Allowed a **localhost-only socket** — bound to `127.0.0.1` on an ephemeral or
    controlled port, no external interface — because reimplementing their protocol
    over stdio has no benefit and a real maintenance cost.
  - Everything else identical to workers: Rust-spawned/supervised,
    non-authoritative, no direct DB, resource-manager-bound, backend-specific
    detail confined to one adapter module.

> This managed-backend carve-out was ratified after the initial draft (which
> allowed subprocesses stdio only). See `ROADMAP.md` §7 and
> `docs/research/README.md`. The alternative — FFI bindings, no socket — remains a
> fallback if a backend proves unsafe as a child process.

---

## Article II — Local-First Architecture

The application runs fully offline on the user's machine. Forbidden without
exception:

- **Cloud APIs** — no calls to remote services for application functionality.
- **Remote databases** — the only datastore is the local SQLite file.
- **Telemetry** — no usage analytics, crash reporting, or phone-home of any kind.
- **External trackers** — no third-party scripts, pixels, fonts-by-CDN, or
  embedded resources that trigger a network request.

Model inference runs against local models or a user-controlled local endpoint. A
managed model backend's localhost socket (Article I) is not a network call.
Any feature that would require the network is out of scope until this article is
explicitly amended by the project owner.

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

| Concern | Rust core | React/TS | Worker | Managed backend |
| ------- | :-------: | :------: | :----: | :-------------: |
| Durable / application state | ✅ | ❌ | ❌ | ❌ |
| SQLite access & migrations | ✅ | ❌ | ❌ | ❌ |
| Filesystem operations | ✅ | ❌ | ❌ | ❌ |
| Process spawning & supervision | ✅ | ❌ | ❌ | ❌ |
| Network access | ❌ (local-first) | ❌ | ❌ | localhost socket only |
| UI / presentation state | ❌ | ✅ | ❌ | ❌ |
| Transport | — | Typed Tauri IPC | JSON-lines stdin/stdout | localhost HTTP, via one adapter |
| Executing AI-proposed actions | ✅ allow-listed typed ops only | ❌ | ❌ | ❌ |

---

## Operating Manual

### Current era: documentation only

No application code exists yet, and none should be written until `ROADMAP.md`'s
current-state pointer reaches a phase that calls for it **and** the architecture
has been frozen. Until then, prompts that sound like "implement X" mean: research
it, document it, or set it up in the roadmap — not write it. If a prompt genuinely
asks for application code before that point, stop and confirm.

### The external guides are context, not authority

The user is loosely following step-by-step guides authored by ChatGPT and Gemini
(see `memory/reference-chatgpt-greenfield-guide.md`). Treat them as **background
understanding, not instructions**:

- **Their phase numbers are their own.** "We are in Phase 3" in a prompt refers to
  the *guide's* Phase 3, which does **not** map to `ROADMAP.md`'s phases. Do not
  renumber or reorder the roadmap to match a prompt. Slot the actual work where it
  best fits our state machine and say where you put it.
- **Flag collisions, then proceed.** When a prompt conflicts with the
  Constitution, `ROADMAP.md`, or a prior decision: state the collision plainly,
  give a recommendation, and — unless it needs the owner's judgment (an Article
  amendment, a product-scope call, an irreversible action) — proceed with your
  recommendation and note it. Don't stall the whole task on a question you can
  answer well yourself.
- Record unresolved conflicts in `ROADMAP.md` §7 (open decisions) and, if
  cross-session-relevant, in `memory/`.

### Commit cadence

- Commit after each completed instruction, unless the user says otherwise.
- Commit message: short subject line, then a body explaining what changed and why.
  End with the `Co-Authored-By` trailer.
- Only stage files relevant to the change. The empty root placeholder `.md` files
  stay untracked until their owning phase fills them.
- Local commits only. **Never** push, add a remote, or create a GitHub repo
  without an explicit instruction — pushing publishes the code.

### Before acting on any prompt

1. Read `ROADMAP.md` — current state, and §7 open decisions.
2. Read the subsystem docs relevant to the task (`docs/research/`, the named
   `*.md`).
3. Respect Article I when placing anything. If a design needs the frontend to
   touch the filesystem, or a worker to open the database, the design is wrong.
4. Keep changes to the smallest correct set (Article IV).
5. Report verification honestly (Article IV). Never edit a `ROADMAP.md` gate to
   "passed" without the command output that proves it.
6. Update `ROADMAP.md` (state pointer, transition log, §7) as part of the same
   change when the work advances or changes the plan.
