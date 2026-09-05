# Phase 36 — Maintainability / Architecture Audit

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
Confirm the codebase still matches the frozen architecture: no drift, no duplicate
authorities, ownership boundaries intact, ADRs cover every decision that actually
happened.

## Depends on
All implementation phases (through Phase 33). `ARCHITECTURE.md` (Phase 5).

## Not in this phase
- New features.
- Large refactors — findings become tracked work, not this phase's job (unless a
  boundary violation must be fixed before release).

## Architecture notes
- This is the check that Phase 5's freeze held. Where reality diverged and the
  divergence is *right*, add an ADR. Where it's *wrong*, fix or file it.

## Performance notes
- N/A.

## Step outline
1. Trace every subsystem in the code against `ARCHITECTURE.md`; flag any
   undocumented component or any documented component that doesn't exist.
2. Single-authority check: for state, models, GPU, processes, config,
   conversations, characters, memory, tasks — confirm exactly one owner in code.
3. Boundary check: `git grep` / review that the frontend never touches the
   filesystem/DB/processes; workers never touch the DB; no business logic in IPC
   handlers or React.
4. Duplicate-logic check: no two conversation engines, no two model managers, no
   two config readers.
5. ADR coverage: every decision made during implementation has an ADR; anything
   that changed post-freeze has an updated one.
6. Dead code removed.
7. Write `docs/verification/08_maintainability_audit.md`.

## Verification gate
1. Every subsystem in code is in `ARCHITECTURE.md` and vice versa.
2. Every listed concern has exactly one authority in code (documented in the
   audit).
3. Boundary checks pass (frontend / worker isolation verified by review + grep).
4. No duplicated engine / manager / reader.
5. Every implementation-time decision has an ADR; post-freeze changes have updated
   ADRs.
6. No dead code (linters + manual sweep).

## ADRs / open questions
- Each accepted divergence from the freeze → an ADR.
