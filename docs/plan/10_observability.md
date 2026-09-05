# Phase 10 — Observability

> **Architecture frozen at Phase 5.** Step detail **finalized at phase entry, 2026-09-06**. Governing: `SECURITY.md` §"no secrets in logs (redaction at the logging boundary)", `PERFORMANCE.md` §"logging cheap on the hot path", **ADR-0015** (no telemetry), Phase 7 error taxonomy, Phase 8 config (`logging.level`). Formalizes the Phase 6 `logging` seed.

## Objective
Local structured logging + diagnostics: levels, timestamps, component/target,
operation name, task id, duration, status, structured errors — with **secret
redaction at the boundary**, no conversation content by default, no cloud
telemetry, and a non-blocking hot-path writer.

## Depends on
Phase 6 (`logging` seed), Phase 7 (`AppError`/`TaskId`), Phase 8 (config — a new
`logging.level` key, schema `v2`).

## Not in this phase
- A metrics / tracing UI.
- Any remote sink (forbidden — ADR-0015).
- **Persistent log file + rotation/retention** and the **diagnostics-bundle IPC
  command** → deferred to **Phase 37 (packaging)**, where a windowed app with no
  terminal makes a file sink necessary and Settings has UI to surface a bundle.
  The in-memory ring buffer lands now so the bundle is a small addition later.
  This resolves the plan's "log retention / rotation policy" open question:
  **decided at Phase 37**.

## Architecture notes
- One facade (`logging` module); every subsystem uses `tracing` macros +
  `logging::operation(...)`. Context (task id, operation) flows via **spans**,
  never manual string interpolation.
- **Redaction at the write boundary:** a `MakeWriter` wrapper runs a redaction
  pass on every serialized line before it is written. Catches secrets regardless
  of how they entered a field or message.
- **Level from config:** `logging::init()` installs a `reload`-able `EnvFilter`
  (env `LOCALAI_LOG` or `info`). After config loads, `setup()` calls
  `logging::set_level(&cfg.logging.level)`. Env still wins if set (dev override).
- **Non-blocking:** `tracing-appender::non_blocking` — bounded buffer, **lossy**
  (drop + count under pressure, never block a hot path). The worker guard is held
  for the process lifetime.
- **Ring buffer:** an in-memory layer keeps the last 256 formatted lines
  (`logging::recent_lines()`) for tests and the future diagnostics bundle.
- Frontend `frontend_log` lines already land in this stream (Phase 6); they get
  the same redaction + ring treatment for free (same writer).

## Performance notes
- Non-blocking lossy writer so a burst of token-stream logs cannot stall
  generation. A real hot-path throughput comparison needs token streaming — that
  measurement is taken at **Phase 16** against its baseline. Phase 10 verifies the
  mechanism (bounded + lossy) is in place.
- The redaction regex set is small and compiled once (`OnceLock`); it runs per
  line, not per field.

## Steps (atomic; each independently verifiable)

**10.1 — Deps**
    Do:     Add `tracing-appender` and `regex` (`regex` is already in the tree —
            no new build cost) to `Cargo.toml`.
    Verify: `cargo build`; `cargo tree -d` shows no new duplicate.

**10.2 — Config schema v2: `logging.level`**
    Do:     `config` — add `LoggingConfig { level: String }` (default `"info"`),
            `AppConfig.logging`, bump `CURRENT_SCHEMA_VERSION` to `2`, add
            `step_1_to_2` (layer onto defaults, stamp version), `ConfigKey::LoggingLevel`.
            Validate: `level` parses as a `tracing` directive.
    Verify: `cargo test config::` — a `v1` file (or versionless) migrates to `v2`
            with `logging.level = "info"`; an invalid level is rejected naming
            `logging.level`; existing config tests still pass.

**10.3 — Reloadable filter + `set_level`**
    Do:     `logging::init()` builds the filter behind `tracing_subscriber::reload`,
            stores the `Handle` in a `OnceLock`. `logging::set_level(&str) ->
            AppResult<()>` swaps it (no-op if `LOCALAI_LOG` is set).
    Verify: `cargo test logging::level_reload` — with a capture layer, an
            `info!` after `set_level("warn")` is dropped; a `warn!` passes.

**10.4 — Non-blocking writer + redaction wrapper**
    Do:     Wrap stdout in `non_blocking` (bounded, lossy); wrap that in
            `RedactWriter`. `redact_line(&str) -> Cow<str>` — regexes:
            `hf_[A-Za-z0-9]{20,}` → `hf_***`;
            `(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{8,}` → `Bearer ***`;
            JSON `"(token|secret|password|api_key|apikey|authorization|hf_token)"\s*:\s*"[^"]*"`
            → `"<key>":"***"`.
    Verify: `cargo test logging::redaction` — a line containing a seeded
            `hf_<40 chars>`, a `Bearer <token>`, and `password=<value>` comes out
            with all three redacted and no original secret substring present.

**10.5 — Ring buffer layer**
    Do:     A `Layer` (or a tee in `RedactWriter`) pushing each written line into
            a `Mutex<VecDeque<String>>` capped at 256. `logging::recent_lines() ->
            Vec<String>`.
    Verify: `cargo test logging::ring_buffer` — after N `info!`s, `recent_lines()`
            returns the last ≤256 in order; lines are already redacted.

**10.6 — `operation` span helper + duration/status**
    Do:     `logging::operation(task_id: Option<&TaskId>, name: &'static str) ->
            Operation`. Opens an `INFO` span with fields `op = name`,
            `task_id = <id or "-">`. `Operation::finish(self, Status)` (or `Drop`)
            emits `info!(elapsed_ms, status, "operation finished")` inside the span.
            `Status` = `Ok | Cancelled | Failed`.
    Verify: `cargo test logging::task_id_propagation` — an async op wrapped in
            `operation(Some(&tid), "demo")` that logs twice (incl. from a spawned
            task via `.in_current_span()` / instrument) → **every** captured line
            carries the same `task_id`; the finish line has `elapsed_ms` + `status`.

**10.7 — Structured error logging**
    Do:     `AppError::log(&self, context: &str)` — `error!(kind = <variant>,
            context, "%self")`; for `Internal` note the cause is already logged at
            source. A `DbError`/subsystem error logs its chain when mapped.
    Verify: `cargo test logging::error_logging` — `AppError::Validation("x").log("y")`
            produces one line with `kind="Validation"`, `context="y"`.

**10.8 — Conversation-content policy**
    Do:     Convention (documented in the module + `SECURITY.md` cross-ref):
            message/prompt/transcript text is logged only at `trace!`, never
            `debug`/`info`; a `logging::content_preview(&str)` helper returns a
            length + hash, not the text, for `info`-level breadcrumbs.
    Verify: `cargo test logging::no_content_at_default` — a `trace!(content=…)` is
            absent from output under the default `info` filter; `content_preview`
            returns no substring of the input.

**10.9 — Wiring + egress check**
    Do:     `lib.rs setup()` calls `logging::set_level` from the effective config
            after load. `git grep` the `logging` module for any network symbol.
    Verify: `npm run tauri dev` — logs still structured, level reflects config,
            no errors. `rg -n 'reqwest|TcpStream|hyper|ureq|http' src-tauri/src/logging*`
            → nothing.

**10.10 — Gate run + docs + commit**
    Do:     `node scripts/check.mjs`; `docs/verification/09_phase10_observability.md`;
            update `SECURITY.md` (redaction mechanism), `src-tauri/README.md`,
            `ARCHITECTURE.md` §2, `PERFORMANCE.md` (non-blocking note),
            `docs/decisions/README.md` (note the retention decision → Phase 37);
            `ROADMAP.md` §1/§4/§5. Commit.
    Verify: check suite green; every gate check recorded.

## Verification gate
1. A single operation's log lines all carry its task id. *(10.6)*
2. A seeded secret / PII string does not appear in any log output. *(10.4)*
3. Conversation content is absent from logs at the default level. *(10.8)*
4. Level filtering from config works. *(10.2, 10.3)*
5. A traffic capture during logging shows no external egress — here: the logging
   path has **no** network sink by construction (stdout + in-memory only); a
   `git grep` confirms no network symbol in the module. Full packet capture is
   Phase 32. *(10.9)*
6. Hot-path logging uses a non-blocking, bounded, lossy writer (mechanism
   verified; the token-throughput comparison is taken at Phase 16). *(10.4)*
7. Full check suite green; `src/bindings` regenerated for the new config types. *(10.10)*

## ADRs / open questions
- **Log retention / rotation** → **decided: Phase 37** (with the file sink and the
  diagnostics bundle). Recorded in `docs/decisions/README.md`.
- No new ADR — this phase implements frozen `SECURITY.md` / `PERFORMANCE.md`
  policy.
