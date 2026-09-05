# Phase 24 — Scheduler Formalization

> **Architecture frozen at Phase 5** (`PROJECT.md`, `ARCHITECTURE.md`, `AI_PIPELINES.md`, ADR-0001..0015). The design below is settled. Concrete implementation specifics (exact modules, crate APIs, filenames) are filled in at phase entry against the frozen ADRs — they do not change the design.

## Objective
Turn the ad-hoc arbitration introduced in Phase 23 into one authoritative
scheduler for GPU / worker / task work: queueing, priorities, cancellation,
fairness, and reconciliation after a crash.

## Depends on
Phase 23 (hot-swap arbitration exists), Phase 13 (resources), Phase 14
(lifecycle).

## Not in this phase
- New workloads — only the arbitration of existing ones (LLM gen, image gen,
  STT, TTS).
- Multi-GPU (there is one GPU).

## Architecture notes
- The scheduler is the single place that decides *what runs on the GPU next*. The
  resource manager (13) accounts; the scheduler orders.
- Priorities: interactive (chat/voice generation) above background (image gen,
  memory extraction) — exact policy is an ADR.
- Every queued/running job is cancellable by id.
- Scheduler state is reconstructable after a crash (persisted or re-derivable).

## Performance notes
- Scheduling overhead negligible vs job duration.
- Head-of-line blocking avoided for interactive jobs (a long image generation must
  not stall a chat response beyond the swap cost).

## Step outline
1. Job model (id, kind, priority, resource need, cancellation handle).
2. Queue + admission (calls the resource manager to check feasibility).
3. Priority ordering + fairness rule.
4. Dispatch: pick next → coordinate the swap (Phase 23) → run → release.
5. Cancellation: remove a queued job; signal a running job; clean up.
6. Reconciliation: on startup / after a crash, rebuild the picture from
   the resource manager + lifecycle state.
7. Migrate Phase 23's inline arbitration onto the scheduler; delete the ad-hoc
   path.
8. Tests: ordering by policy, cancellation of queued + running jobs, no two
   conflicting GPU jobs at once, crash reconcile.

## Verification gate
1. Competing GPU requests (e.g. chat generation + image generation) run in
   priority order per the policy.
2. Cancelling a queued job removes it; cancelling a running job stops it and
   releases resources.
3. Two GPU jobs that must not overlap never do.
4. After a simulated crash, the scheduler reconciles to a correct state and
   resumes.
5. Phase 23's inline arbitration code is gone (`git grep`); its gate still passes.
6. An interactive job is not stalled by a background job beyond the swap cost.

## ADRs / open questions
- Priority + fairness policy — ADR.
