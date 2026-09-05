# Phase 39 — Permanent Claude Workflow

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
Convert the bootstrap discipline into the steady-state development process for
after release: how features and fixes are proposed, built, verified, and merged
once the phase-by-phase course is done.

## Depends on
Phase 38 (system is release-ready).

## Not in this phase
- Changing the constitution's principles (Articles I–IV stay).

## Architecture notes
- The permanent loop keeps the parts that mattered: inspect before coding,
  smallest correct change, physically-executed verification, ADRs for
  architectural changes, `docs/verification/` evidence for anything gated.
- `ROADMAP.md` shifts from a linear course to a backlog + current-work pointer.

## Step outline
1. Write the permanent loop in `DEVELOPMENT.md`: intake → inspect → plan →
   implement → verify → review → commit, with the checks required at each step.
2. Update `CLAUDE.md` Operating Manual to "post-bootstrap mode" (the docs-only-era
   text no longer applies; the bootstrap-phase guidance becomes historical).
3. Define how new work is scoped: a feature gets a mini plan doc
   (`docs/plan/feature-<name>.md`) using the same template; a bug gets the
   bug-fixing loop.
4. Define the regression-gate policy: which of the Phase 33/34 suites run on every
   change.
5. Run one real sample change (a small feature or fix) through the full loop and
   record the evidence.

## Verification gate
1. `DEVELOPMENT.md` documents the permanent loop with per-step checks.
2. `CLAUDE.md` is updated to post-bootstrap mode; no stale bootstrap-only
   instructions remain.
3. The new-work scoping process (feature plan doc / bug loop) is written down.
4. A sample change has been run through the full loop with recorded verification
   evidence.

## ADRs / open questions
- Regression-suite scope per change type — recorded in `DEVELOPMENT.md`.
