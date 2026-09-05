# Phase 14 — Model Lifecycle Manager

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
The only component that loads and unloads managed models. Explicit state machine
(Discovered → Available → Loading → Loaded → Busy → Unloading → Available) with
explicit failure and recovery states. No duplicate loads. Failed and cancelled
loads release their resources.

## Depends on
Phase 13 (resource manager), Phase 11 (registry), Phase 10, Phase 7.

## Not in this phase
- A specific backend (Phase 15 is the first).
- UI.
- Inference itself.

## Architecture notes
- Backend-agnostic: works against a `ModelBackend` trait; llama.cpp is the first
  impl (Phase 15).
- Reservations are acquired from Phase 13 *before* a load starts and released on
  every exit path (success-then-later-unload, failure, cancellation, crash).
- Concurrent requests for the same model coalesce onto one load.
- Process/instance liveness is monitored; unexpected exit → `Failed` + reconcile.

## Performance notes
- Load is slow (seconds to minutes). Surface progress. Do not hold the request
  thread — return a handle, drive load on a task.

## Step outline
1. Define the state machine + transitions + the `ModelBackend` trait.
2. `load(model_id)` — reserve → spawn/init backend → readiness check → `Loaded`;
   coalesce concurrent same-model requests.
3. Pre-flight rejection: insufficient resources → reject before spawn.
4. `unload(model_id)` — `Unloading` → stop backend → release reservation →
   `Available`.
5. Busy tracking: mark `Busy` during a generation, back to `Loaded` after.
6. Failure handling: load failure → release reservation → `Failed`; bounded
   retry/backoff; park in `Failed` after N.
7. Cancellation: cancel an in-progress load → release → `Available`.
8. Liveness monitor: unexpected backend exit → `Failed` → `reconcile` with the
   resource manager.
9. State-machine unit tests (every transition + every failure edge).

## Verification gate
1. Load a model; it reaches `Loaded` and a reservation is held.
2. Two concurrent load requests for the same model result in one load.
3. Unload releases the reservation and returns to `Available`.
4. Load under insufficient resources is rejected before any process spawn.
5. Cancel mid-load releases resources and returns to `Available`.
6. A forced load failure → `Failed`, reservation released, bounded retry observed.
7. Killing the backend process is detected; state reconciles; a subsequent load
   works.
8. State-machine tests cover all transitions.

## ADRs / open questions
- Retry budget + backoff numbers.
