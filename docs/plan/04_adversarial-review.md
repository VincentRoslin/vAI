# Phase 4 — Adversarial Architecture Review

## Objective
Try to break the architecture proposed in Phase 3 *before* committing to it.
Produce a prioritized risk register; for every realistic risk, either revise the
architecture or accept it with a written rationale.

## Depends on
Phase 3 complete (draft ADRs in `docs/decisions/`, all `STATUS: PROPOSED`).

## Not in this phase
- Application code.
- New feature scope.
- Ratifying ADRs (Phase 5).

## Architecture notes
- Assume the application becomes large and long-lived. A risk that is "unlikely
  now" but "catastrophic and permanent" still counts.
- Every mitigation names the **owning subsystem** and a **test approach** — a
  mitigation with no test is not a mitigation.

## Performance notes
- Include performance-failure modes as risks: unnecessary model reloads, excessive
  IPC traffic, excessive re-render, unbounded context growth, VRAM thrash on
  hot-swap.

## Steps

### 4.1 — Assemble the target
Do: write a single reviewable description of the proposed architecture (subsystems,
boundaries, data flow, process model) from the Phase 3 ADRs.
Verify: the description exists and every draft ADR is reflected in it.

### 4.2 — Attack: resource exhaustion
Do: VRAM exhaustion, RAM exhaustion, disk full mid-download, model larger than
VRAM, simultaneous model loads, duplicate model load, KV-cache blow-up from long
context.
Verify: each scenario has an entry: does the architecture prevent / detect /
degrade / crash? mitigation, owner, test.

### 4.3 — Attack: process & crash handling
Do: llama.cpp crash, worker crash, orphaned processes after app crash, worker
never sends `ready`, process hangs (no crash, no progress), zombie on Windows.
Verify: entries as above; confirm the Job-Object / supervisor story covers orphan
cleanup.

### 4.4 — Attack: concurrency
Do: deadlocks, races on shared state, cancellation that doesn't propagate, stuck
generation jobs, two swaps in flight, scheduler + resource-manager lock ordering.
Verify: entries; confirm a single serialization point for load/unload/swap.

### 4.5 — Attack: data
Do: SQLite lock contention, DB corruption, migration failure mid-run, migration
partially applied, backup missing, blob file orphaned / blob path dangling,
concurrent writes.
Verify: entries; confirm migration is transactional + backed up, and a
reconcile job exists for blobs.

### 4.6 — Attack: IPC & untrusted AI output
Do: malformed IPC payload, oversized payload, IPC flood, malformed model output,
malformed typed action, typed action referencing another character, model output
that looks like an instruction, prompt-injection via retrieved memory or character
text.
Verify: entries; confirm Article III containment holds for every path a model
influences.

### 4.7 — Attack: offline & privacy
Do: accidental network egress (a dependency phones home, a font/CDN fetch, an
update check on a hot path), a feature that silently needs the network, secrets in
logs, secrets in the frontend bundle, PII in logs.
Verify: entries; confirm the only network code paths are the explicit, isolated
acquisition/update ones.

### 4.8 — Attack: character system
Do: character identity drift across images, character state corruption, memory
poisoning, relationship state desync, a character referencing deleted assets,
discovery showing a half-generated character.
Verify: entries; confirm character state has one authority and identity has a
verification step.

### 4.9 — Attack: duplicate authority / duplicated logic
Do: frontend caching data the core owns, a worker holding state, config read
outside the config system, two components deciding model state, business logic in
an IPC handler or in React.
Verify: entries; confirm the single-authority map from Phase 3 has no gaps.

### 4.10 — Resolve
Do: for every risk marked realistic: choose *revise architecture* (update the ADR
+ the assembled description) or *accept* (write the rationale). Re-attack anything
that changed.
Verify: no realistic risk is left "open"; every one is `mitigated` or
`accepted (rationale)`.

### 4.11 — Write the register
Do: `docs/verification/02_adversarial_review.md` — prioritized table: risk,
category, likelihood, impact, disposition, owning subsystem, test approach, ADR
reference.
Verify: the file exists and covers every category in 4.2–4.9.

## Verification gate
1. `docs/verification/02_adversarial_review.md` exists and covers all mandated
   categories (4.2–4.9). — file check.
2. Every risk has a disposition (`mitigated` / `accepted`), an owning subsystem,
   and a test approach. — reviewer check, no blanks.
3. Every `accepted` risk has a written rationale.
4. Every architecture change is reflected in the relevant ADR and the assembled
   description (4.1).
5. No application code. — `git status`.

## ADRs / open questions this phase resolves
- Hardens or overturns the Phase 3 draft ADRs. Anything overturned gets a new
  draft ADR for Phase 5 to ratify.
