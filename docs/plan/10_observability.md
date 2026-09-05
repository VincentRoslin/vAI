# Phase 10 — Observability

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
Local structured logging + diagnostics: levels, timestamps, component/service,
operation, task id, model id (where applicable), duration, status, structured
errors. No secrets, no conversation content by default, no cloud telemetry.

## Depends on
Phase 6 (basic logging exists); this phase formalizes it.

## Not in this phase
- A metrics/tracing UI.
- Any remote sink.

## Architecture notes
- One logging facade; all subsystems use it. Context (task id, model id) flows via
  spans, not manual string interpolation.
- The frontend forwards its logs to Rust (from Phase 6); this phase gives them the
  same structure.
- Redaction is a policy applied at the logging boundary.

## Performance notes
- Logging must be cheap on the hot path (token streaming): non-blocking writer,
  bounded buffer, drop-with-count under pressure rather than block.

## Step outline
1. Structured JSON logging with the required fields; level filtering from config.
2. Span/context propagation so an operation's lines share a task id.
3. Duration + status captured for each instrumented operation.
4. Structured error logging (the Phase 7 error taxonomy, with the full chain).
5. Redaction policy (no secrets, no full PII, no conversation content by default)
   applied at the boundary.
6. A local diagnostics bundle command (recent logs + system info, redacted) for
   support.
7. Tests: task-id propagation across an async operation; redaction of a seeded
   secret; level filtering; no network egress from the logging path.

## Verification gate
1. A single operation's log lines all carry its task id.
2. A seeded secret / PII string does not appear in any log output (redaction
   test).
3. Conversation content is absent from logs at the default level.
4. Level filtering from config works.
5. A traffic capture during logging shows no external egress.
6. Hot-path logging does not measurably regress token throughput (compare to the
   Phase 6 / future Phase 16 baseline).

## ADRs / open questions
- Log retention / rotation policy.
