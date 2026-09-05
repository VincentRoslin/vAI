# Phase 34 — Security Audit

> **Architecture frozen at Phase 5** (`PROJECT.md`, `ARCHITECTURE.md`, `AI_PIPELINES.md`, ADR-0001..0015). The design below is settled. Concrete implementation specifics (exact modules, crate APIs, filenames) are filled in at phase entry against the frozen ADRs — they do not change the design.

## Objective
A documented threat model and a pass of concrete security checks. Local does not
mean safe.

## Depends on
Features complete (through Phase 29). Phase 28 (typed actions), Phase 22/27
(image/filesystem), `SECURITY.md` (Phase 5).

## Not in this phase
- Feature changes beyond fixing findings.

## Architecture notes
- The audit validates the Article I/III boundaries hold in the real code, not
  just the docs.

## Performance notes
- N/A.

## Step outline
1. Finalize the `SECURITY.md` threat model (assets, actors, entry points,
   mitigations) against the shipped system.
2. Untrusted-AI-output review: every path a model influences → confirm typed-
   action mediation, no raw command/file/process/DB access (Phase 28 pattern
   everywhere).
3. Filesystem: path traversal tests on every path input (model dir, blob store,
   downloads, config); confinement verified.
4. Process arguments: every child-process spawn uses argument vectors, never a
   shell string; test with hostile inputs (spaces, quotes, unicode, `;`).
5. Worker inputs: every worker validates its JSON input; malformed input → error,
   not crash or injection.
6. Secrets: none in source (history scan), none in logs (redaction test), none in
   the frontend bundle (inspect the build).
7. Local services: bound to `127.0.0.1` only; verify with a bind check.
8. Dependency vulnerability scan (overlaps Phase 35 — run it here too).
9. Write `docs/verification/06_security_audit.md`; fix all high/critical; retest.

## Verification gate
1. `SECURITY.md` threat model matches the shipped system.
2. No path a model influences can execute a command, mutate the filesystem,
   launch a process, or run SQL (reviewed + adversarial tests).
3. Path-traversal attempts on every path input are rejected.
4. Every child-process spawn is argument-vector based; hostile-input tests pass.
5. Every worker rejects malformed input safely.
6. History scan: no secrets. Log redaction test: passes. Frontend bundle: no
   secrets.
7. Local services bind loopback only.
8. No open high/critical findings; each fixed finding has a retest note.

## ADRs / open questions
- Any accepted residual risk → documented with rationale in the audit doc.
