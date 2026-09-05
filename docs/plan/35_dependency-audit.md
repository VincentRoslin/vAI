# Phase 35 — Dependency Audit

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
Every dependency — Rust crates, npm packages, Python packages, native/AI runtimes
— is justified, license-compatible with a locally-distributed app, and
vulnerability-scanned. Unused dependencies removed.

## Depends on
Feature-complete build (through Phase 29).

## Not in this phase
- Replacing dependencies unless a finding requires it.

## Architecture notes
- Fewer dependencies is the goal (`CLAUDE.md` Art. IV). A dependency that does
  one small thing we could inline is a removal candidate.

## Performance notes
- Note any dependency contributing disproportionately to build time or bundle /
  installer size.

## Step outline
1. Generate the full dependency inventory (Cargo tree, npm ls, Python freeze,
   native runtimes list).
2. For each direct dependency: one-line justification. Flag any without one.
3. License check: every dependency's license is compatible with local
   distribution; list copyleft ones and their obligations.
4. Vulnerability scan: `cargo audit`, `npm audit`, Python audit — clean or each
   finding triaged (fix / accept with rationale / upgrade).
5. Unused-dependency sweep (cargo-udeps / depcheck / manual) — remove.
6. Size/build-time outliers noted.
7. Write `docs/verification/07_dependency_audit.md`.

## Verification gate
1. `docs/verification/07_dependency_audit.md` lists every direct dependency with a
   justification.
2. License check is clean; copyleft obligations documented.
3. `cargo audit` / `npm audit` / Python audit are clean or every finding is
   triaged with a recorded decision.
4. Unused dependencies removed; the build still passes all prior gates.

## ADRs / open questions
- Any dependency kept despite a vulnerability → ADR with the rationale + the
  monitoring plan.
