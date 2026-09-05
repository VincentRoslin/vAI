# Phase 13 — Resource Manager

> **Architecture frozen at Phase 5** (`PROJECT.md`, `ARCHITECTURE.md`, `AI_PIPELINES.md`, ADR-0001..0015). The design below is settled. Concrete implementation specifics (exact modules, crate APIs, filenames) are filled in at phase entry against the frozen ADRs — they do not change the design.

## Objective
Answer "can this operation safely use the GPU right now?". Lifecycle: request →
reserve → commit → observe → release → reconcile. Mockable hardware provider.
Never assumes model file size equals runtime VRAM.

## Depends on
Phase 11 (registry: estimated resource need), Phase 10 (logging), Phase 7.

## Not in this phase
- Actually loading models (Phase 14).
- Scheduling / arbitration policy (Phase 24) beyond a single serialization point.

## Architecture notes
- One authority for GPU/VRAM accounting. All GPU consumers go through it.
- Per the Phase 3.7 ADR: `nvml-wrapper` for observation; whether per-process VRAM
  is available on this driver (may be whole-GPU only + our own ledger).
- Estimation = weights + KV cache + context + fixed overhead + safety margin;
  correction factor learned from observed commits.
- Load/unload/swap operations are serialized through one async mutex/actor.

## Performance notes
- Observation poll cadence: ~1–2 s while any model is loaded, back off when idle.
- The reserve decision must be fast (no blocking NVML call on the request path —
  use the last observed value + the ledger).

## Step outline
1. Hardware provider trait + a real NVML impl + a mock impl.
2. Reservation ledger (in-memory + optional persistence for crash reconcile).
3. Estimation function (per model type) with a calibration hook.
4. `request(estimate) → Reservation | ResourceExhausted` using observed free −
   margin − outstanding reservations.
5. `commit(reservation, measured)` — replace estimate with first real measurement.
6. `release(reservation)`.
7. Observation loop updating measured usage.
8. `reconcile()` — compare ledger vs observed; recover stale reservations; log
   drift.
9. Single serialization point for reserve/commit/release.
10. Tests (mock hardware): insufficient VRAM, duplicate reservation, concurrent
    requests, failed load releases, cancellation releases, stale reservation
    recovery, process-crash reconcile, external app grabs VRAM → reconcile.

## Verification gate
1. Insufficient VRAM → `ResourceExhausted`, cleanly, no partial reservation.
2. Duplicate reservation for the same operation is rejected.
3. Concurrent requests are serialized; no double-commit of the same VRAM.
4. A failed or cancelled load releases its reservation.
5. A stale reservation (simulated crash) is recovered by `reconcile`.
6. After an external process grabs VRAM, `reconcile` trusts the measurement and
   logs the drift.
7. Mock hardware provider lets all of the above run without a GPU.

## ADRs / open questions
- Eviction policy lives with the scheduler (Phase 24); this phase only exposes the
  accounting it needs.
