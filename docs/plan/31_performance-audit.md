# Phase 31 — Performance Audit

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
Measure the whole system against the `PERFORMANCE.md` budgets, fix the
highest-value bottlenecks, and re-measure. No intuition-only optimization.

## Depends on
Features functionally complete (through Phase 29) and `PERFORMANCE.md` (Phase 5).
Uses the baselines recorded during Phases 6, 12, 15, 16, 22.

## Not in this phase
- New features.
- Speculative optimization with no measured problem.

## Architecture notes
- Optimizations must not violate ownership boundaries or add hidden state.
- Prefer removing work (fewer reloads, fewer IPC round trips, less context) over
  micro-optimizing.

## Performance notes (the whole phase)
Metrics: cold startup, UI responsiveness (input latency, frame time under
streaming), LLM TTFT + tokens/sec, voice round-trip + barge-in latency, image
generation time, model switch latency, DB operation timings, memory retrieval
time, download throughput.

## Step outline
1. Assemble the full baseline table (from recorded baselines + fresh measurement
   where missing).
2. Compare each metric to its `PERFORMANCE.md` budget; list the misses.
3. Profile each missed metric to a concrete bottleneck.
4. For each: implement the highest-value fix; record before/after.
5. Re-measure the full table; confirm no regression elsewhere.
6. Re-run the Phase 16 reliability gate.
7. Write `docs/verification/03_performance_audit.md` (baseline → budget →
   after, per metric).

## Verification gate
1. `docs/verification/03_performance_audit.md` has baseline / budget / result for
   every metric.
2. Every metric meets its budget, or the miss is explicitly accepted with a
   rationale.
3. Every applied optimization has recorded before/after numbers.
4. The Phase 16 reliability gate still passes.
5. No metric regressed while fixing another.

## ADRs / open questions
- Any optimization that changes architecture → an ADR + a note to Phase 36.
