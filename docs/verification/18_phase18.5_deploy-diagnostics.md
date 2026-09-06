# Phase 18.5 — Deploy & Diagnostics

**Status: COMPLETE** (2026-09-06). Owner-requested insert before Phase 19 — a
repeatable way to run the real (release) build with the real settings for
hands-on live-testing, with **local** probes the agent can read after a session
it did not watch. Plan: `docs/plan/18.5_deploy-diagnostics.md`. Governing:
ADR-0015 (no telemetry — the probe is a file the owner shares, never a beacon),
`SECURITY.md` (no secrets / no conversation content). **No new ADR** — pulls the
log-file / diagnostics-bundle work forward from Phase 37, which still owns the
MSI, embedded CPython, sibling layout, first-run acquisition, code signing, and a
*portable* archive.

## What shipped

- **`logging::enable_file_sink(dir)`** — a rotating **daily JSON-lines** sink
  (`<app_data>/logs/localai.jsonl.<YYYY-MM-DD>`) fed the *same* already-redacted
  line as stdout through the existing `RedactWriter`. Non-blocking + lossy, its
  own `WorkerGuard`. Wired in `lib.rs::setup()` right after config loads.
  `log_dir()` accessor. `sweep_logs()` runs at startup: keep the newest 7 files,
  then drop oldest while the directory exceeds ~50 MB (never the last file).
- **`build.rs`** — best-effort `git rev-parse --short HEAD` →
  `cargo:rustc-env=LOCALAI_GIT_SHA` (+ `rerun-if-changed` on `.git/HEAD` / refs).
  Empty string in a non-git build.
- **`src/diag.rs`** — `DiagSnapshot { taken_at, build, config, models,
  resources, lifecycle, conversations, recent_logs, host }`. `collect()` builds
  it from live state; `export()` writes
  `<app_data>/diagnostics/diag-<ts>.json` and returns the path. `config` and
  `models` are round-tripped through `logging::redact_line` (defence in depth);
  `recent_logs` are already redacted; `conversations` is **metadata only** (id,
  kind, message count, `updated_at`); `host` is `ver` / `uname` + `nvidia-smi
  --query-gpu` (both optional, read-only). `diag_snapshot` / `diag_export` IPC.
- **`Settings.tsx`** — was a placeholder; now shows the effective config
  read-only + an **Export diagnostics** button that calls `diag_export` and
  displays the written path.
- **`ChatVoice.tsx`** — `log.info('ui', …)` breadcrumbs on model load/unload,
  send, and voice push-to-talk start/release (+ `log.warn` on caught errors) —
  `target:"frontend"` lines so a hands-on session is reconstructable from the
  log alone.
- **`scripts/deploy-local.mjs`** — `npm run build` → `cargo build --release` →
  launch `src-tauri/target/release/localai.exe` in place (detached). Prints
  `git_sha`, `exe`, and the `config` / `logs` / `diagnostics` paths under
  `%APPDATA%\com.localai.app\`. `--no-launch` builds + prints only.

## Gate — execution record

| # | Check | Result |
| - | ----- | ------ |
| 1 | `deploy-local.mjs` builds the release binary against the real `%APPDATA%` config and prints the build SHA + log/diagnostics paths | **PASS** — `node scripts/deploy-local.mjs --no-launch`: `npm run build` + `cargo build --release` produced `src-tauri/target/release/localai.exe`; the script printed `git_sha`, `exe`, and `config` / `logs` / `diagnostics` under `%APPDATA%\com.localai.app\`. _(evidence: build output below)_ |
| 2 | A running session writes a **redacted** rotating JSON-lines log to `<app_data>/logs/`; a planted `Bearer …` / `"token":"…"` never reaches disk | **PASS** — `logging::tests::redaction_removes_every_secret_shape` proves the `RedactWriter` path scrubs `Bearer`/`hf_`/`"token":` before a byte is emitted, and the file sink is fed the *same* redacted string (one code path, `RedactWriter::drop`). `tauri dev` after this phase: `%APPDATA%\com.localai.app\logs\localai.jsonl.2026-09-06` is created and grows with the JSON stream. |
| 3 | Log rotation + sweep bounds growth | **PASS** — `logging::tests::sweep_logs_keeps_the_newest_files` (12 dated files → 7, newest kept, unrelated files untouched) + `sweep_logs_caps_total_size` (90 MB across 3 files → pruned under the 50 MB cap, never deletes the last). Daily roll is `tracing_appender::rolling::daily`. |
| 4 | `diag_export` produces one JSON file with build / config / registry / resources / lifecycle / recent logs / host facts — redacted, **no conversation content** | **PASS** — `diag::tests`: `build_info_is_populated`, `redact_value_scrubs_a_planted_secret` (`"token":"hf_…"` → `"***"`), the auto-generated `export_bindings_*` round-trips for `DiagSnapshot`/`BuildInfo`/`ConversationMeta`/`HostInfo`. `collect()` puts only `ConversationMeta { id, kind, messages, updated_at }` in — never `Message` content. |
| 5 | The Settings **Export diagnostics** button writes the file and shows its path | **PASS** — `src/pages/Settings.test.tsx`: renders the effective config, the button calls `diag_export`, the returned path renders. |
| 6 | UI actions leave `target:"frontend"` breadcrumbs in the log | **PASS** — `ChatVoice.tsx` emits `log.info('ui', …)` for model load/unload, send, voice start/release; the frontend logger forwards to `frontend_log` → the core log stream (`target:"frontend"`), which now also hits the file sink. |
| 7 | Full check suite green; bindings regenerated | **PASS** — `node scripts/check.mjs` all green. New bindings: `DiagSnapshot`, `BuildInfo`, `ConversationMeta`, `HostInfo`. 265 rust tests (8 ignored live), 11 vitest. |

## Build output (gate 1)

```
$ npm run build
$ cargo build --release --manifest-path <repo>/src-tauri/Cargo.toml
=== LocalAI local deploy ===
  git_sha      <sha>
  exe          <repo>\src-tauri\target\release\localai.exe
  config       %APPDATA%\com.localai.app\config.json
  logs         %APPDATA%\com.localai.app\logs
  diagnostics  %APPDATA%\com.localai.app\diagnostics
```

## How the owner uses this

1. `node scripts/deploy-local.mjs` → the real window opens; drive it.
2. **Probes for the agent:** either the agent tails the newest file in
   `%APPDATA%\com.localai.app\logs\`, **or** the owner clicks **Settings →
   Export diagnostics** and hands over the printed `diag-<ts>.json` path.
3. All local — nothing leaves the machine unless the owner sends the file.

## Notes / follow-ups

- **Phase 37 still owns:** the MSI, embedded CPython + sibling-layout resolution,
  first-run model acquisition, code signing, and a *portable* diagnostics archive
  for testing on another machine.
- **Open (→ ROADMAP §7):** a human-readable (non-JSON) log tail for the owner's
  own eyeballing — deferred; the JSON is greppable and the diagnostics file is
  the owner-facing artifact.

**Phase 18.5 complete** — all 7 gate items pass. Pointer → **Phase 19 (Voice:
Chatterbox TTS · playback · barge-in)**.
