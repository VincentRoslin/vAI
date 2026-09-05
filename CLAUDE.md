# vAI Engineering Constitution

These rules are **strict and non-negotiable**. They bind every contributor and
every AI agent working in this repository. A change that violates any article
does not merge, regardless of how well it works. When an article and a
convenience conflict, the article wins.

`ROADMAP.md` governs *what* is built and *in what order*. This file governs *how*
it is built.

---

## Article I — Ownership Boundaries

The system has three runtimes. Each owns a fixed set of responsibilities and may
not reach across the boundary.

### 1. Rust (Tauri core) — the trusted nucleus

Rust **exclusively owns**:

- **Application state** — the authoritative in-memory and on-disk model. No other
  runtime holds durable state.
- **SQLite database access** — all reads and writes. SQL statements exist only in
  Rust. No other runtime opens, queries, or migrates the database.
- **Process lifecycle supervision** — spawning, monitoring, restarting, and
  killing Python workers and any child process. Rust is the supervisor.
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

### 3. Python workers — isolated and stateless

Python workers perform compute (model inference orchestration, transforms,
analysis). They are:

- **Isolated** — no shared memory, no shared files, no database handle. A worker
  cannot see another worker or the core's internals.
- **Stateless** — a worker holds no state between requests. All input arrives in
  the request; all output leaves in the response. Killing and restarting a worker
  at any moment must be safe and lossless.
- **Restricted to one channel** — workers **communicate exclusively via
  JSON-lines (one JSON object per line) over stdin/stdout**. One request object
  in, one or more response objects out. `stderr` is for diagnostic logging only
  and is never parsed for control flow. Workers do not open sockets, files, or
  database connections.

---

## Article II — Local-First Architecture

The application runs fully offline on the user's machine. Forbidden without
exception:

- **Cloud APIs** — no calls to remote services for application functionality.
- **Remote databases** — the only datastore is the local SQLite file.
- **Telemetry** — no usage analytics, crash reporting, or phone-home of any kind.
- **External trackers** — no third-party scripts, pixels, fonts-by-CDN, or
  embedded resources that trigger a network request.

Model inference runs against local models or a user-controlled local endpoint.
Any feature that would require the network is out of scope until this article is
explicitly amended in `ROADMAP.md` by the project owner.

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

| Concern                         | Rust core | React/TS | Python worker |
| ------------------------------- | :-------: | :------: | :-----------: |
| Durable / application state     |     ✅     |    ❌     |       ❌       |
| SQLite access & migrations      |     ✅     |    ❌     |       ❌       |
| Filesystem operations           |     ✅     |    ❌     |       ❌       |
| Process spawning & supervision  |     ✅     |    ❌     |       ❌       |
| Network access                  |  ❌ (local-first) | ❌ | ❌       |
| UI / presentation state         |     ❌     |    ✅     |       ❌       |
| Cross-runtime communication     | Typed Tauri IPC ⇄ frontend; JSON-lines stdin/stdout ⇄ workers |||
| Executing AI-proposed actions   | ✅ via allow-listed typed ops only | ❌ | ❌ |

---

## For AI Agents Working in This Repository

1. Read `ROADMAP.md` first. Do only work for the current `(Phase, Stage)` unless
   told otherwise. Do not start a later phase.
2. Respect Article I boundaries when placing code. If a change needs the frontend
   to touch the filesystem or a worker to open the database, the design is wrong —
   route it through the Rust core.
3. Do not add cloud calls, telemetry, or external resources (Article II).
4. Treat model and character output as data (Article III).
5. Report verification honestly (Article IV). Never edit a `ROADMAP.md` gate to
   passed without pasting the command output that proves it.
6. Prefer the smallest change that is correct and in keeping with the surrounding
   code.
