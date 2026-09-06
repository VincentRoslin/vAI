# 13 — Phase 14: Model Lifecycle Manager

**Date:** 2026-09-06
**Branch:** `main` (local; no remote).
**Method:** new `src-tauri/src/lifecycle/` module; 15 async unit tests over a
controllable `FakeBackend` + `MockProbe`-backed `ResourceManager`; a
`npm run tauri dev` launch observed for the wiring + liveness loop.

Governing: **ADR-0007** (reservations), **ADR-0010** (the transition holder
performs the whole transition, no re-entry). Plan:
`docs/plan/14_model-lifecycle.md`. **No new ADR.**

---

## What landed

- `lifecycle/backend.rs` — `ModelBackend` (`load(&LoadRequest, CancellationToken)
  -> AppResult<Box<dyn LoadedInstance>>`) and `LoadedInstance` (`health`,
  `measured_vram_mb`, `shutdown`) traits (`async_trait`). `#[cfg(test)]
  FakeBackend` — knobs `load_delay`, `fail_next(n)`, `set_healthy(bool)`, and
  `loads` / `shutdowns` / `health_checks` counters. No real process; llama.cpp
  is Phase 15.
- `lifecycle/mod.rs` — `LifecycleManager`. `Entry { state: ModelState,
  reservation, instance: Arc<dyn LoadedInstance>, cancel: CancellationToken,
  notify, last_error }` in a `HashMap<ModelId, Entry>` behind one
  `tokio::sync::Mutex`. Public: `load` / `unload` / `cancel_load` / `begin_use`
  (→ `BusyGuard`) / `end_use` / `check_liveness` / `state` / `statuses` /
  `register_backend`.
  - **Claim/coalesce:** `load` on `Loading`/`Unloading` waits on the entry's
    `Notify` (registered under the lock via `Notified::enable()`, closing the
    wake race) and re-evaluates; on `Loaded`/`Busy` returns `Ok`; on
    `Unloaded`/`Failed` claims the load (`state = Loading`).
  - **Reservation:** acquired from `ResourceManager` **inside** `run_load`,
    before the backend `load`; `commit`ed with `measured_vram_mb()` on success;
    `release`d on every failure / cancel / later unload / liveness kill.
  - **Retry:** `RetryPolicy { max_attempts: 3, backoff_base: 500 ms }`,
    exponential; `Cancelled` is not a failure (no retry). After the budget the
    entry parks in `Failed`; a fresh `load` clears it and tries again.
  - Backend `load` / `shutdown` / `health` are **never** called while the
    transition lock is held (instances are stored as `Arc` and cloned out).
- Config: none (no schema change).
- Contract (additive): `ModelState::Busy` — the frozen `contracts::model::
  ModelState` reserved this enum for "Phase 14" and only lacked `Busy`. Added
  additively (`docs/contracts.md` §3 precedent note); the TS union grew, no
  version bump. New DTO `LifecycleStatus { id, state, vram_mb, error }`.
- IPC: `lifecycle_status` → `Vec<LifecycleStatus>` (read-only). `src/lib/ipc.ts`
  `lifecycleStatus`, `src/lib/contracts.ts` re-export.
- `lib.rs` — `start_lifecycle_manager(registry, resources)` builds the manager
  (no backends registered — Phase 15 registers llama.cpp), `manage`s the
  `Arc<LifecycleManager>`, spawns `liveness_loop` (~2 s).
- Deps: `async-trait` + `tokio-util` promoted to **direct** deps (both already
  in the tree via `refinery` / `reqwest` — 0 net crates).

---

## Gate — execution record

All checks are `FakeBackend` + `MockProbe` (no real model, no GPU needed).

| # | Check | Result |
| - | ----- | ------ |
| 1 | Load a model → `Loaded`, a reservation is held | **PASS** — `load_reaches_loaded_and_holds_a_reservation`: state `Loaded`, `FakeBackend.loads == 1`, `resources.snapshot().reserved_gpu_mb > 0`, `statuses()[0].vram_mb == Some(4096)` (the instance's measured figure). `a_second_load_of_a_loaded_model_is_a_noop` → `loads == 1`. |
| 2 | Two concurrent load requests → one backend load | **PASS** — `concurrent_loads_of_the_same_model_coalesce`: 5 concurrent `load(id)` (60 ms fake load) → all 5 `Ok`, `FakeBackend.loads == 1`, state `Loaded`. |
| 3 | Unload releases the reservation and returns to `Unloaded` | **PASS** — `unload_releases_the_reservation`: after `unload`, state `Unloaded`, `shutdowns == 1`, `reserved_gpu_mb == 0`; a second `unload` is `Ok` (idempotent). |
| 4 | Load under insufficient resources rejected **before any process spawn** | **PASS** — `insufficient_resources_are_rejected_before_any_spawn`: 8 192 MB GPU / 7 500 used → 692 free − 1 500 margin → 0 usable; a 6 000 MB model → `AppError::ResourceExhausted`, state `Unloaded`, **`FakeBackend.loads == 0`**, `reserved_gpu_mb == 0`. |
| 5 | Cancel mid-load releases resources and returns to `Unloaded` | **PASS** — `cancel_mid_load_releases_and_returns_unloaded`: 500 ms fake load, `cancel_load` after 50 ms → the `load` call returns `AppError::Cancelled`, state `Unloaded`, `reserved_gpu_mb == 0`. `cancel_when_not_loading_is_a_conflict` covers the guard. |
| 6 | Forced load failure → `Failed`, reservation released, bounded retry observed | **PASS** — `load_failure_retries_then_parks_in_failed`: `fail_next(5)`, `max_attempts 3` → state `Failed`, `FakeBackend.loads == 3`, `reserved_gpu_mb == 0`, `statuses()[0].error` set. `load_recovers_within_the_retry_budget`: `fail_next(2)` → `Loaded`, `loads == 3`. `a_load_after_a_parked_failure_tries_again`: a fresh `load` after a park → `Loaded`. |
| 7 | Killing the backend is detected; state reconciles; a later load works | **PASS** — `a_dead_backend_is_detected_and_reconciled`: `set_healthy(false)` → `check_liveness()` returns 1, state `Failed`, `reserved_gpu_mb == 0`; `set_healthy(true)` + `load` → `Loaded`. `check_liveness_is_a_noop_with_nothing_loaded` → 0. |
| 8 | State-machine tests cover every transition + failure edge | **PASS** — the 15 tests exercise: `Unloaded→Loading→Loaded`, `Loaded→Unloading→Unloaded`, `Loaded↔Busy` (`busy_blocks_unload_until_the_guard_drops`), `Loading→Unloaded` (cancel), `Loading→Failed→Loading` (retry + recover), `Failed→Unloaded` (unload of a failed model), plus the `Conflict`/`NotFound` edges (`begin_use_on_a_non_loaded_model_conflicts`, `cancel_when_not_loading_is_a_conflict`, `an_unknown_model_reads_unloaded`). |
| 9 | Full check suite green; bindings regenerated + committed | **PASS** — `node scripts/check.mjs` all green: `cargo fmt` / `clippy -D warnings` / **196 rust tests** (+16; 1 `#[ignore]`d live download) / `tsc` / eslint / prettier / **7 vitest** / `vite build`. New/changed bindings committed: `ModelState` (`+ "Busy"`), `LifecycleStatus`. |

### Wire-in confirmation (14.12)

`npm run tauri dev` on the reference machine — clean startup, no lifecycle
errors, liveness loop silent with nothing loaded:

```
{"message":"database ready","schema_version":3,...}
{"message":"resource manager ready","gpu":"Some(GpuMemory { total_mb: 16303, used_mb: 14319, free_mb: 1983 })",...}
```

(`lifecycle_status` is registered in the invoke handler; it returns `[]` until a
Phase 15 backend loads something.)

---

## Baselines (→ `PERFORMANCE.md` §1)

| Metric | Measured |
| ------ | -------- |
| Lifecycle state transitions (excl. the backend load) | sub-millisecond — a `HashMap` edit under one `Mutex` |
| Full 15-test suite | ~0.24 s (fake backends, 5–60 ms simulated loads) |

The real load cost is entirely the backend's (Phase 15 measures llama.cpp).

---

## Decisions taken this phase

- **Collision — `ModelState::Busy`.** Flagged in the plan: the state machine
  needs `Busy` (loaded **and** serving) so `unload` can refuse a model in use;
  the frozen `ModelState` had `Unloaded/Loading/Loaded/Failed/Unloading`.
  Resolved by adding `Busy` **additively** — its doc comment already scoped the
  enum to "Phase 14", nothing matched it exhaustively, and `docs/contracts.md`
  §3 now records the precedent. No parallel enum, no ADR.
- **Retry is per-`load`-call.** The internal loop retries up to `max_attempts`;
  exhaustion parks the entry in `Failed`; a later explicit `load` starts fresh.
  Simpler than a persistent failure counter and matches the gate.
- **Reservation estimate.** `metadata.estimated_vram_mb` when present, else a
  conservative `6144 MB` floor (a guess must not over-commit into an OOM).
  Phase 13 calibration refines it once `commit` sees a real measurement.
- **`async-trait` + `tokio-util` as direct deps.** Native `async fn` in a
  public trait can't express the `Send` bound `tokio::spawn` needs;
  `async-trait` boxes the future. Both crates are already transitive — 0 net
  crates, same precedent as the Phase 12 `reqwest` decision.

---

## Not done here (by design)

- A real backend — Phase 15 (llama.cpp) registers the first `ModelBackend`.
- Generation / streaming / generation-cancellation — Phase 15/16.
- Eviction / hot-swap to fit a new load — Phase 23/24. This phase **refuses** a
  load that doesn't fit; it never unloads another model.
- Persisting lifecycle state — runtime only, empty on start.

**Phase 14 complete** (all 9 gate items pass). Pointer → Phase 15.
