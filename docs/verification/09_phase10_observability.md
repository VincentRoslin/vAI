# 09 — Phase 10: Observability

**Date:** 2026-09-06
**Branch:** `main` (local; no remote).
**Method:** `logging` module rework + config schema v2 (`logging.level`); 14 unit
tests (pure redaction + a thread-local capture subscriber for span/level); a real
`tauri dev` launch.

Governing: `SECURITY.md` §C7, `PERFORMANCE.md`, ADR-0015. Plan:
`docs/plan/10_observability.md`.

---

## What landed

- `logging/mod.rs` — JSON stdout via `tracing_appender::non_blocking` (bounded
  8192 lines, **lossy**); `RedactWriter` runs `redact_line` on every line then
  tees it into a 256-line ring buffer (`recent_lines`); hot-reloadable
  `EnvFilter` behind `reload::Handle` with `set_level` / `validate_directive`
  (`LOCALAI_LOG` still wins); `operation(task_id, name) -> Operation` span helper
  emitting `elapsed_ms` + `status` on finish/drop; `content_preview` (length +
  hash, never the text); `Status` enum.
- `ipc/error.rs` — `AppError::kind_str()` + `AppError::log(context)`.
- `config` — schema **v2**: `LoggingConfig { level }` (default `"info"`),
  `ConfigKey::LoggingLevel`, session override, `validate()` checks the directive,
  migration generalised to `step_forward` (additive-only per ADR-0016).
- `lib.rs` — `setup()` calls `logging::set_level` from the effective config.
- Deps: `tracing-appender 0.2`, `regex 1` (already in the tree).

---

## Gate — execution record

| # | Check | Result |
| - | ----- | ------ |
| 1 | A single operation's log lines all carry its task id | **PASS** — `operation_events_all_carry_the_task_id`: `operation(Some(&tid), "demo_op")`, two `info!`s + the finish line — every captured line contains `task-7c1d` and `demo_op`; the finish line has `elapsed_ms` + `"status":"ok"`. `dropped_operation_is_logged_as_failed` → `"status":"failed"`. |
| 2 | A seeded secret / PII string does not appear in any log output | **PASS** — `redaction_removes_every_secret_shape`: a line with an `hf_` token (also as `Bearer hf_…xyz`), `password=hunter2`, `"token":"hf_…"`, `"api_key":"sk-secret-123"` → output contains none of the raw values, and `hf_***` / `Bearer ***` / `password=***` / `"token":"***"` / `"api_key":"***"` instead. `redact_line` is idempotent; clean lines pass through unchanged. |
| 3 | Conversation content is absent from logs at the default level | **PASS** — `content_below_info_is_not_emitted_at_default_level`: a `trace!(content=…)` does not appear under the `info` filter. `content_preview_never_contains_the_text`: the preview contains no substring of the input. |
| 4 | Level filtering from config works | **PASS** — config: `migration_v1_to_v2_adds_logging_defaults`, `validation_rejects_a_bad_logging_level` (names `logging.level`), `apply_kv_sets_logging_level`. logging: `set_level_reloads_the_global_filter` — after `set_level("warn")` the reload handle's current filter is `warn`; `set_level("verbose")` → `Err`. `tauri dev` log shows `logging_level: "info"` from the effective config. |
| 5 | A traffic capture during logging shows no external egress | **PASS (by construction)** — the logging path is stdout + an in-memory `VecDeque` only. `rg 'reqwest\|TcpStream\|hyper::\|ureq\|std::net' src-tauri/src/logging/` → nothing. Full packet capture is Phase 32. |
| 6 | Hot-path logging is non-blocking + bounded + lossy | **PASS (mechanism)** — `NonBlockingBuilder::default().lossy(true).buffered_lines_limit(8192)`. A token-throughput before/after comparison needs streaming and is taken at **Phase 16**. |
| 7 | Full check suite green; bindings regenerated | **PASS** — `node scripts/check.mjs` all green. New/changed bindings: `AppConfig.ts`, `ConfigKey.ts`, `LoggingConfig.ts`. |

`cargo test`: **113 Rust tests** (99 prior + 14 logging/config). `vitest`: 5.

---

## Decisions taken at phase entry

- **Persistent log file + rotation/retention + the diagnostics-bundle IPC
  command → deferred to Phase 37.** No terminal-less build and no Settings UI
  exists yet; the in-memory ring buffer ships now so the bundle is a small
  addition later. This resolves the plan's "log retention / rotation policy" open
  question (recorded in `docs/decisions/README.md`).
- **`validate_directive` is stricter than `EnvFilter`**: `EnvFilter` parses a
  bare unknown word as a target at the default level, so a directive with no
  `,`/`=` must be one of `trace|debug|info|warn|error|off`.
- **Redaction is a line-level regex pass** at the write boundary (`RedactWriter`),
  not a field visitor — it catches a secret regardless of how it entered a field
  or message. Bearer rule runs before the `hf_` rule (which would otherwise
  shorten an `hf_`-prefixed bearer token below the match length).
- **Config migration generalised** to `step_forward` (merge onto defaults + stamp)
  — correct for the additive-only schema (ADR-0016); a non-additive change would
  need a bespoke arm.
- No new ADR — implements frozen `SECURITY.md` / `PERFORMANCE.md` policy.

**Phase 10 complete.** Pointer → Phase 11 (Model Registry).
