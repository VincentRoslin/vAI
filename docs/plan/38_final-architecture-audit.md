# Phase 38 — Final Architecture Audit

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
One last whole-system review before release readiness: audit gates still green,
the binding docs reflect the shipped system, and the system is traceable from the
docs alone.

## Depends on
Phases 31–37 complete.

## Not in this phase
- New work — this is a verification pass. Findings become release blockers or
  tracked post-release items.

## Architecture notes
- The bar: a competent engineer who has never seen the code can, from
  `ARCHITECTURE.md` + `AI_PIPELINES.md` + `PROJECT.md`, correctly describe how a
  chat request and an image request flow through the system.

## Performance notes
- Confirm the Phase 31 audit results still hold on the packaged build (spot-check
  the key metrics).

## Step outline
1. Re-run audit gates 31 (performance spot-check), 32 (offline), 33 (fault
   injection), 34 (security), 35 (dependency), 36 (maintainability) on the
   packaged build.
2. Doc-accuracy pass: `ARCHITECTURE.md`, `AI_PIPELINES.md`, `PROJECT.md`,
   `SECURITY.md`, `PERFORMANCE.md` vs the shipped system — fix every discrepancy.
3. Traceability test: a reader (or a fresh Claude session) traces a chat request
   and an image request end to end using only the docs; note every gap.
4. Write `docs/verification/09_final_architecture_audit.md`.

## Verification gate
1. Audit gates 31–36 re-run and pass on the packaged build (evidence recorded).
2. Every binding doc matches the shipped system (discrepancy list is empty).
3. A reader traces chat + image flows end to end from the docs with no gaps.
4. `docs/verification/09_final_architecture_audit.md` records all of the above.

## ADRs / open questions
- Any remaining `UNDECIDED` ADR must be resolved or explicitly deferred to
  post-release with an owner sign-off.
