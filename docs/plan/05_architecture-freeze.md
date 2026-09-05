# Phase 5 — Project Documentation / Architecture Freeze

## Objective
Officialize the validated product and architecture into the binding documents,
finalize the ADRs, and re-derive the implementation plan (`docs/plan/06–40`)
against the frozen design. After this phase the architecture is stable; changing
it requires the STOP → propose → approve → record loop (`CLAUDE.md` Art. changes,
`docs/decisions/`).

## Depends on
Phase 4 complete (`docs/verification/02_adversarial_review.md`; ADRs updated).

## Not in this phase
- Application code (still none).
- New requirements (those go back to Product Definition).

## Architecture notes
- `PROJECT.md` is the *officialized* product definition — assembled from
  `docs/product/requirements.md` + review outcomes, **not re-invented**.
- Every binding doc must be internally consistent and consistent with `CLAUDE.md`.

## Performance notes
- `PERFORMANCE.md` sets the SLOs/budgets the audit phases (31, 38) measure
  against. Derive them from `NFR-*` and the Phase 3 performance findings.

## Steps

### 5.1 — `PROJECT.md`
Do: write the engineering + product constitution's product half: what LocalAI is,
the `FR`/`NFR` set (officialized), the character-system definition, non-goals.
Verify: every `FR`/`NFR` from `requirements.md` appears (or is explicitly cut with
a reason); owner signs off.

### 5.2 — `ARCHITECTURE.md`
Do: subsystems, responsibilities, boundaries (Article I mapped to real modules),
process model, data-flow diagrams (chat request; voice turn; image request;
character conversation), the single-authority map.
Verify: a reader can trace each of the four flows end to end; every subsystem has
exactly one owner.

### 5.3 — `AI_PIPELINES.md`
Do: each pipeline end to end — LLM (context build → generate → stream → persist);
STT (capture → VAD → transcribe); TTS (text → synth → play → barge-in); image
(action → validate → condition → generate → verify → store); memory (extract →
store → retrieve → inject).
Verify: each pipeline names its worker/backend, its transport, its resource
interaction, and its failure handling.

### 5.4 — `SECURITY.md`
Do: threat model (assets, actors, entry points), the containment rules for
untrusted AI output, filesystem confinement, process-arg safety, loopback-only
services, secret handling. Fold in the Phase 4 security risks.
Verify: every Phase 4 security-category risk maps to a mitigation here.

### 5.5 — `PERFORMANCE.md`
Do: SLOs/budgets (startup, chat TTFT + throughput, voice round-trip, image gen,
model switch, DB ops, memory retrieval); the measurement method for each; the
baseline-then-improve rule.
Verify: every budget is a number with a measurement method; derived from `NFR-*`.

### 5.6 — `UI_GUIDELINES.md`
Do: UX rules, layout primitives, theme tokens (light/dark), required states
(loading/empty/error/streaming), keyboard + accessibility bar.
Verify: the a11y bar is a checkable standard (e.g. "no critical axe violations;
full keyboard path").

### 5.7 — `DEVELOPMENT.md`
Do: the dev loop, environment setup (from the env audit + Phase 3 O5 decisions),
branch/commit conventions (pointer to Phase 40 for the full git strategy),
how to run each subsystem locally, troubleshooting.
Verify: a new contributor could set up from this doc alone (tested for real at
Phase 40 / Phase 36).

### 5.8 — Finalize ADRs
Do: promote every `docs/decisions/` ADR from `PROPOSED` to `ACCEPTED` (or
`UNDECIDED` with what's blocking it and when it will be decided). One decision per
ADR.
Verify: no ADR left `PROPOSED`; every Phase 3/4 decision has an ADR.

### 5.9 — Re-derive `docs/plan/06–40`
Do: for every implementation phase, expand the step outline into concrete steps
(specific modules, crates, files, commands) against the frozen architecture.
Remove the "finalized at phase entry" banner as each is completed. Keep steps
small and each independently verifiable.
Verify: every `docs/plan/NN` for 6–40 has concrete steps with real `Verify:`
lines; no banner remains; `ROADMAP.md` §4 gate summaries still match.

### 5.10 — Cross-check pass
Do: read all binding docs + all plan docs together. Check: no contradictions;
ownership unambiguous everywhere; every gate check physically executable;
`ROADMAP.md` ↔ `docs/plan/` ↔ ADRs agree on names/numbers/dependencies.
Verify: a written cross-check note listing what was checked and that it passed.

## Verification gate
1. `PROJECT.md`, `ARCHITECTURE.md`, `AI_PIPELINES.md`, `SECURITY.md`,
   `PERFORMANCE.md`, `UI_GUIDELINES.md`, `DEVELOPMENT.md` all exist and are
   non-placeholder. — file check.
2. Every `FR`/`NFR` from `requirements.md` is in `PROJECT.md` or explicitly cut
   with a recorded reason.
3. Every `docs/decisions/` ADR is `ACCEPTED` or `UNDECIDED` (none `PROPOSED`).
4. `ARCHITECTURE.md` lets a reader trace chat / voice / image / character flows
   end to end; single-authority map has no gaps.
5. `docs/plan/06–40` all carry concrete steps; no "finalized at phase entry"
   banner remains.
6. Cross-check note exists and reports no contradictions.
7. **Repo still contains no application code.** — `git status`.

## ADRs / open questions this phase resolves
- Ratifies the full ADR set. After this phase, O3/O4/O5 are closed (or explicitly
  `UNDECIDED` with a plan).
