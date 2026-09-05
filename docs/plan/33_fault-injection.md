# Phase 33 — Fault Injection / Reliability

> **Architecture frozen at Phase 5** (`PROJECT.md`, `ARCHITECTURE.md`, `AI_PIPELINES.md`, ADR-0001..0015). The design below is settled. Concrete implementation specifics (exact modules, crate APIs, filenames) are filled in at phase entry against the frozen ADRs — they do not change the design.

## Objective
Deliberately break things and confirm the system behaves predictably and
recovers. Every fault gets a defined outcome and a regression test.

## Depends on
Features complete (through Phase 29), Phase 31 (performance stable). Uses the
error taxonomy (Phase 7), lifecycle/resource/scheduler recovery (13/14/23/24).

## Not in this phase
- New features.
- Fixing performance (Phase 31).

## Architecture notes
- Faults are injected through test seams (mock hardware, killable child processes,
  a corruptible test DB), not by hacking production code paths.
- "Predictable" = a typed error, a recorded state transition, and a recovery path
  — not a crash, hang, or silent data loss.

## Performance notes
- Recovery time for each fault recorded (e.g. crash detection latency, restart
  time).

## Step outline
For each fault: reproduce → observe → define the expected outcome → make it true
if it isn't → add a regression test.
1. Kill llama.cpp mid-generation.
2. Kill each worker (STT, TTS, image) mid-task.
3. Remove a model file while registered / while loaded.
4. Corrupt the config file.
5. Corrupt the SQLite DB; lock the DB from outside.
6. Exhaust VRAM (mock + real).
7. Cancel a generation at every stage.
8. Close the app during generation / during a download / during a swap.
9. Worker timeout (worker alive but not responding).
10. Restart after an unclean shutdown.
11. Disk full mid-download / mid-image-write.
12. Write `docs/verification/05_fault_injection.md` (fault → expected → observed →
    test).

## Verification gate
1. Every fault in the list has: a defined expected outcome, an observed result
   matching it, and a regression test in the suite.
2. No fault produces a crash, a hang, an orphan process, or silent data loss.
3. The regression suite runs in CI/the check script and passes.
4. Recovery times recorded.

## ADRs / open questions
- Any fault whose "correct" behaviour is a judgement call → confirm with the
  owner, record in the audit doc.
