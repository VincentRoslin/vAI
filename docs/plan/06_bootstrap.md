# Phase 6 — Tauri + React + Rust Bootstrap

## Objective
A desktop app that launches to a placeholder UI with working typed IPC, and a full
quality toolchain (format, lint, type-check, test, structured logging). No AI
functionality. This is the first application code.

> ⚠ Step detail below is the plan of record but assumes the Phase 3 ADRs. If Phase
> 3 chose a Cargo workspace (O4) or a specific contract-generation tool, finalize
> the affected steps at phase entry.

## Depends on
Phase 5 complete (architecture frozen; ADRs `ACCEPTED`).

## Not in this phase
- Any model, worker, database schema beyond app bootstrap, image/voice code.
- Any cloud/network code.
- Any dependency not needed to launch + verify the shell.

## Architecture notes
- Ownership from day one: business logic in Rust; React is presentation only; the
  only React↔Rust channel is typed IPC.
- Single Tauri crate with modules unless the Phase 3 ADR said workspace.
- The frontend→Rust log bridge means the frontend never writes logs directly.

## Performance notes
- Record a **startup-time baseline** here (cold launch → window interactive) — it
  is the reference for the Phase 31 performance audit.
- Production build size baseline recorded too.

## Steps

### 6.1 — Scaffold the Tauri v2 app
Do: create the Tauri v2 project (Rust core + React/TS frontend) in the repo root
per the Phase 3 structure ADR. Commit `Cargo.lock` and the JS lockfile (npm or
pnpm per O5).
Verify: `cargo build` and the frontend build both succeed from a clean checkout;
lockfiles are tracked.

### 6.2 — Frontend toolchain
Do: Vite + React + TypeScript with `strict: true`; ESLint + Prettier configs.
Verify: `npm run lint` and `npm run typecheck` exit 0 on the skeleton; a
deliberate type error fails `typecheck`.

### 6.3 — Rust toolchain
Do: `rustfmt.toml`, `clippy.toml`, deny warnings in CI/build (`-D warnings`).
Verify: `cargo fmt --check` and `cargo clippy -- -D warnings` exit 0; a deliberate
lint fails clippy.

### 6.4 — `.gitattributes`
Do: add `.gitattributes` normalizing line endings (`* text=auto eol=lf`, with
binary excludes). Renormalize the existing tree.
Verify: `git add --renormalize .` then `git status` — no CRLF warnings on
subsequent commits.

### 6.5 — Pre-commit hooks + commit convention
Do: a hook running fmt + clippy + lint + typecheck on staged files; a commit-msg
check for the agreed convention.
Verify: a deliberately mis-formatted staged change is rejected by the hook; a
bad commit message is rejected.

### 6.6 — Minimal typed IPC
Do: one command (`app_ping` → returns a typed struct) and one event
(`app_ready`); one shared type definition; contract generation wired if the Phase
3 ADR chose `tauri-specta`/`ts-rs`.
Verify: the frontend calls `app_ping`, receives the typed value, and renders it;
TS types for the command are generated (or hand-written) and compile.

### 6.7 — Placeholder UI
Do: app shell (header/sidebar/content regions), client-side routing skeleton with
a 404 route, theme tokens (light/dark, follows OS), a top-level error boundary.
Verify: app launches; routes work; forcing a render error shows the boundary not a
blank screen; theme follows the OS setting.

### 6.8 — Structured logging
Do: `tracing` in Rust with JSON output, levels, timestamps, a request/task-id
field; an IPC command the frontend uses to forward its log lines to Rust. No
telemetry, no network sink.
Verify: an `app_ping` call produces a correlated log line in Rust with a task id;
a frontend-forwarded log appears in the same stream.

### 6.9 — Test infrastructure
Do: one Rust unit test, one Vitest component test, and a single script that runs
the whole check suite (fmt, clippy, lint, typecheck, rust test, vitest, build).
Verify: the script runs all checks and exits non-zero if any fails.

### 6.10 — Task runner
Do: entry points for `dev`, `build`, `lint`, `fmt`, `test`, `typecheck`, `check`
(the 6.9 script).
Verify: each entry point runs the intended thing.

### 6.11 — `README.md` quickstart
Do: clone → install → run in ≤5 commands; list every root `*.md` and what it is.
Verify: following the README from a clean clone produces a running app with no
undocumented step.

### 6.12 — Full verification run
Do: run the 6.9 check script; launch the app; exercise the IPC round-trip; record
the startup-time + build-size baselines in the phase notes.
Verify: all checks green; app launches; IPC round-trips; baselines recorded.

## Verification gate
1. `cargo fmt --check` ✓ · `cargo clippy -- -D warnings` ✓ · `cargo check` +
   `cargo test` ✓.
2. `npm run lint` ✓ · `npm run typecheck` ✓ · frontend tests ✓.
3. Production build succeeds; size baseline recorded.
4. App launches: window + React UI appear, Rust core starts, no console errors.
5. A round-trip typed IPC call succeeds and is visible in the UI.
6. A forced render error is caught by the error boundary.
7. Pre-commit hook rejects a mis-formatted change and a bad commit message.
8. `.gitattributes` present; no CRLF warnings on commit.
9. Deferred Phase 1 tooling (formatter/linter/hooks/README) is all present.
10. `git grep` finds no model / AI / network code.
11. Startup-time baseline recorded for Phase 31.

## ADRs / open questions this phase resolves
- Closes the Phase 1 tooling deviation.
- Confirms the Phase 3 crate-structure and contract-generation ADRs in practice
  (raise a new ADR if reality differs).
