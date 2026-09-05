# 05 — Phase 6: Tauri + React + Rust bootstrap

**Date:** 2026-09-05
**Branch:** `phase-6-bootstrap` → merged to `main` (`1f6c6a1`).
**Method:** scaffold + full check suite + a real `tauri dev` launch with the IPC
round-trip observed in the structured log.

---

## Gate — execution record

| # | Check | Result |
| - | ----- | ------ |
| 1 | `cargo fmt --check` | **PASS** |
| 1 | `cargo clippy --all-targets -- -D warnings` | **PASS** (clippy `all` + `pedantic`; `doc_markdown`, `module_name_repetitions`, `missing_errors/panics_doc` allowed) |
| 1 | `cargo test` | **PASS** — 8 tests (2 command logic, 5 ts-rs binding exports, 1 re-export) |
| 2 | `git diff --exit-code src/bindings` (ts-rs drift) | **PASS** — bindings committed and in sync |
| 2 | `npx prettier --check .` | **PASS** |
| 2 | `npx eslint .` | **PASS** (`no-console` error rule; frontend logs only via `lib/log.ts`) |
| 2 | `npx tsc --noEmit` (strict, `noUncheckedIndexedAccess`) | **PASS** |
| 2 | `npx vitest run` | **PASS** — 2 tests (three primary tabs render; 404 route) |
| 3 | `npx vite build` | **PASS** — 38 modules, `index.js` 289 KB / 92 KB gzip, `index.css` 1.3 KB |
| 4 | App launches | **PASS** — `npm run tauri dev`: `localai.exe` + `msedgewebview2.exe` window; log: `{"message":"LocalAI core starting","version":"0.1.0"}` |
| 5 | Round-trip typed IPC | **PASS** — `ChatVoice` calls `app_ping(nonce)` → Rust returns matching nonce → the frontend forwards `app_ping round-trip ok` back through the `frontend_log` command into the same structured stream (`{"target":"frontend","fields":{"message":"app_ping round-trip ok","source":"frontend::chat"}}`). Full path: React → command → Rust → response → React → command → Rust logger. (Logged twice — React 19 StrictMode double-invokes effects in dev.) |
| 6 | Error boundary | **PASS** — `ErrorBoundary` present at the root; renders a recovery panel + forwards to the Rust log on a render error (unit-covered by the 404 alert test path; a forced-error check is deferred to Phase 30). |
| 7 | Pre-commit hook | **PASS** — the Phase 6 commit ran `.githooks/pre-commit` (fmt + prettier + eslint) and `.githooks/commit-msg` (subject ≤ 72, blank line 2). |
| 8 | `.gitattributes` present; no CRLF warnings | **PASS** — LF normalization added; tree renormalized. |
| 9 | Deferred Phase 1 tooling present | **PASS** — formatter (`rustfmt`, Prettier), linters (`clippy`, ESLint), type check (`tsc`), pre-commit hooks, commit-msg convention, `README.md` quickstart all landed. |
| 10 | No AI / model / network code | **PASS** — `git grep -iE 'llama|whisper|chatterbox|krea|cuda|reqwest|hf-hub'` in `src`/`src-tauri/src` → nothing. Only network is npm/cargo dependency fetching at build time. |
| 11 | Startup-time baseline | Recorded below. |

## Baselines (for Phase 31)

| Metric | Value | Notes |
| ------ | ----- | ----- |
| Cold `tauri dev` core start → first frontend IPC round-trip | **~0.57 s** | from the dev log: core `20:46:22.450` → round-trip `20:46:23.019` (debug build, Vite already warm) |
| Production frontend bundle | 289 KB JS (92 KB gzip) + 1.3 KB CSS | `vite build`, 38 modules |
| Rust debug binary | ~15 MB | `target/debug/localai.exe` |
| `node_modules` | 175 MB | 233 packages, 0 vulnerabilities |

A proper cold-launch number (release build, from double-click) is measured at
Phase 31 — this is the dev-loop reference.

## Notes / small follow-ups (not blocking)

- ~~The `app://ready` event listener in `main.tsx` can miss the event if `listen()`
  attaches after `.setup()` emits it (race).~~ **Resolved 2026-09-06:** replaced
  the startup event with an `app_ready` command (`ipc/commands.rs`), called once
  from `AppShell` (`src/components/AppShell.tsx`). This also removed a
  `transformCallback` unhandled rejection (`listen()` ran at module-eval before
  the Tauri bridge was injected) and the duplicated `app_ping` log lines (the
  round-trip probe now runs once per session, not per mount / HMR reload).
- A forced-render-error test for the `ErrorBoundary` is added in the Phase 30
  UI/UX pass.
- `allowScripts` in `package.json` pins `esbuild@0.28.2` — bump the key when
  esbuild upgrades.

**Phase 6 complete.** Pointer → Phase 7 (Application Contracts).
