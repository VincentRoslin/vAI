# Phase 14 — Model Lifecycle Manager

> **Status: IN PROGRESS** — step detail finalized at phase entry (2026-09-06)
> against the frozen architecture (ADR-0007, ADR-0010, ADR-0013) +
> `docs/verification/12_phase13_resources.md`.

> **Architecture frozen at Phase 5** (`PROJECT.md`, `ARCHITECTURE.md`, `AI_PIPELINES.md`, ADR-0001..0017). The design below is settled. Concrete implementation specifics (exact modules, crate APIs, filenames) are filled in at phase entry against the frozen ADRs — they do not change the design.

## Objective
The **only** component that loads and unloads managed models. An explicit state
machine with explicit failure/recovery states. No duplicate loads (concurrent
requests coalesce). Every exit path — success, failure, cancellation, crash —
releases the Phase 13 reservation.

## Depends on
Phase 13 (`resources::ResourceManager` — reserve/commit/release/reconcile),
Phase 11 (`models::ModelRegistry` — `get` → `RegisteredModel`), Phase 10
(`operation` spans), Phase 7 (`ModelState`, `AppError`).

## Not in this phase
- A real backend — **Phase 15** (llama.cpp) is the first. This phase ships a
  `FakeBackend` (test-only) behind the `ModelBackend` trait.
- Inference / generation, streaming, cancellation *of a generation* (Phase 15/16).
- Model **hot-swapping** / eviction to fit a new load — **Phase 23/24**. This
  phase refuses a load that doesn't fit; it never evicts another model.
- UI, and any IPC beyond a read-only `lifecycle_status` debug command.
- Persisting lifecycle state — it is **runtime** state (like the Phase 13
  ledger): in-memory, empty on start, nothing to reconcile from disk.

## Collision with the frozen contract — flagged
`contracts::model::ModelState` (frozen Phase 7) has `Unloaded / Loading / Loaded
/ Failed / Unloading` — its own doc comment says it is "Runtime state … in the
lifecycle manager (Phase 14)". The state machine needs a **`Busy`** state
(loaded **and** serving a generation) so `unload` can refuse a model in use.
**Recommendation, taken:** add `Busy` to `ModelState` **additively**
(`docs/contracts.md` — additive-only; no code matches it exhaustively; the TS
union just grows). No new enum, one runtime-state type as the freeze intended.
Recorded in the transition log + verification doc.

## Architecture notes
- **New module `src-tauri/src/lifecycle/`** (`mod.rs` — the manager + state
  machine, `backend.rs` — the `ModelBackend` / `LoadedInstance` traits +
  `FakeBackend` under `#[cfg(test)]`, `tests.rs`).
- **Ownership:** the lifecycle manager owns *runtime* model state
  (`ARCHITECTURE.md` §3 "Model load / unload"). The registry owns *metadata*;
  the resource manager owns *accounting*. No overlap.
- **`ModelBackend` trait** (`async_trait`; both `async-trait` and `tokio-util`
  are already in the tree — 0 net crates):
  ```
  trait ModelBackend: Send + Sync {
      async fn load(&self, req: &LoadRequest, cancel: CancellationToken)
          -> AppResult<Box<dyn LoadedInstance>>;
  }
  trait LoadedInstance: Send + Sync {
      async fn health(&self) -> AppResult<()>;   // Err → the instance is dead
      fn measured_vram_mb(&self) -> Option<u32>;  // for commit(); None → keep the estimate
      async fn shutdown(&self);
  }
  ```
  `LoadRequest { model: RegisteredModel }`. Backends are registered by their
  `ModelBackend(String)` key; an unknown backend → `AppError::Validation`.
- **State machine** (`LifecycleState`): `Unloaded → Loading → Loaded ⇄ Busy`,
  `Loaded → Unloading → Unloaded`, `Loading → {Unloaded on cancel, Failed on
  error}`, `Failed → Loading` (recovery) or `Failed → Unloaded` (give up after
  the retry budget). One `tokio::sync::Mutex<Inner>` serializes every
  transition; the backend `load`/`shutdown`/`health` calls happen **outside** the
  lock (the lock is re-taken to commit the result), same discipline as Phase 13.
- **Coalescing:** each entry carries an `Arc<tokio::sync::Notify>`. A `load` that
  finds the model `Loading` awaits the `Notify`, then reads the settled state and
  returns accordingly — it never starts a second backend `load`.
- **Reservation lifecycle:** `request` (GPU, calibrated by `model_id`/`backend`,
  estimate from `metadata.estimated_vram_mb` or a floor) happens **before** the
  backend `load` — a `ResourceExhausted` is returned with the model still
  `Unloaded` and no process spawned (gate 4). On load success →
  `commit(measured_or_estimate)`. On every failure / cancel / later unload /
  detected crash → `release`.
- **Retry:** `RetryPolicy { max_attempts: 3, backoff_base: 500 ms }` (exponential:
  500 ms, 1 s). Injectable for tests. After `max_attempts` the entry parks in
  `Failed` and `load` returns the last error; a later explicit `load` clears the
  counter and tries again. Tuning constants, **no ADR**.
- **Liveness monitor:** one task spawned in `setup()` polls `health()` for every
  `Loaded`/`Busy` entry every ~2 s. An `Err` → `Failed`, `release` the
  reservation, notify. The Phase 13 observe loop then sees the freed VRAM.
- **`begin_use` / `end_use`** (or a `BusyGuard`): mark `Busy` / back to `Loaded`
  around a generation. `unload` on a `Busy` model → `AppError::Conflict`.

## Performance notes
- A load is seconds to minutes. `load()` is `async` and yields — it never blocks
  a thread — but it does await completion; cancellation comes from a concurrent
  `cancel_load(id)` tripping the `CancellationToken`.
- The transition lock is held only for state edits, never across a backend call.
- Record: `FakeBackend` load/unload timing (trivial) and the mutex-contention
  shape from the concurrent-load test.

## Steps

**14.1 — Contract: `ModelState::Busy` (additive)**
    Do:     Add `Busy` to `contracts::model::ModelState`; doc it; update the
            `contracts::tests` round-trip list; regenerate bindings.
    Verify: `cargo test contracts`; `git diff src/bindings/ModelState.ts` shows
            only the added member.

**14.2 — `backend.rs`: traits + `FakeBackend`**
    Do:     `ModelBackend` / `LoadedInstance` traits (above); `LoadRequest`.
            `#[cfg(test)] FakeBackend` with knobs: `load_delay`,
            `fail_next(n)`, `set_healthy(bool)`, `measured_vram_mb`, and call
            counters (`loads`, `shutdowns`, `health_checks`).
    Verify: `cargo test lifecycle::backend` — fake loads after its delay; honours
            cancellation; `fail_next(2)` fails twice then succeeds.

**14.3 — Manager skeleton + state machine**
    Do:     `LifecycleManager::new(registry, resources, RetryPolicy)`,
            `register_backend(key, Arc<dyn ModelBackend>)`. `Inner { entries:
            HashMap<ModelId, Entry> }`, `Entry { state, reservation:
            Option<ReservationId>, instance: Option<Box<dyn LoadedInstance>>,
            cancel: Option<CancellationToken>, notify: Arc<Notify>, failures,
            last_error }`. `state(id)`, `statuses()`.
    Verify: `cargo test lifecycle::state` — a fresh id reads `Unloaded`; illegal
            transitions rejected by the helper.

**14.4 — `load`: reserve → backend → Loaded**
    Do:     Registry `get` (must be `Ready`); estimate; `resources.request`;
            set `Loading`; call `backend.load` outside the lock; on `Ok`
            `commit` + `Loaded` + stash the instance + notify; return.
    Verify: `cargo test lifecycle::load_reaches_loaded` — state `Loaded`, a
            reservation is `Held` (`resources.snapshot().reserved_gpu_mb > 0`),
            `FakeBackend.loads == 1`.

**14.5 — Pre-flight rejection**
    Do:     A `request` that returns `ResourceExhausted` → propagate, entry stays
            `Unloaded`, backend never called.
    Verify: `cargo test lifecycle::insufficient_resources_rejected_before_spawn`
            — `FakeBackend.loads == 0`, state `Unloaded`.

**14.6 — Coalescing**
    Do:     `load` on a `Loading` entry awaits `notify`, re-reads state, returns
            (`Loaded` → `Ok`, `Failed` → `Err`). `load` on `Loaded`/`Busy`
            returns `Ok` immediately.
    Verify: `cargo test lifecycle::concurrent_loads_coalesce` — 5 concurrent
            `load(id)` → `FakeBackend.loads == 1`, all 5 `Ok`.

**14.7 — `unload`**
    Do:     `Loaded` → `Unloading` → `instance.shutdown()` (outside the lock) →
            `resources.release` → `Unloaded` + notify. `Busy` → `Conflict`.
            `Unloaded` → `Ok` (idempotent).
    Verify: `cargo test lifecycle::unload_releases` — reservation gone
            (`reserved_gpu_mb == 0`), state `Unloaded`, `shutdowns == 1`; a
            `Busy` model → `Conflict`.

**14.8 — Busy tracking**
    Do:     `begin_use(id)` (`Loaded` → `Busy`, else `Conflict`/`NotFound`),
            `end_use(id)` (`Busy` → `Loaded`). A `BusyGuard` returned by
            `begin_use` that `end_use`s on drop (best-effort via a stored
            `Arc<LifecycleManager>` + `tokio::spawn`).
    Verify: `cargo test lifecycle::busy_blocks_unload` — mid-use `unload` →
            `Conflict`; after the guard drops, `unload` succeeds.

**14.9 — Load failure + bounded retry**
    Do:     Backend `Err` → `release`, `failures += 1`, backoff sleep, retry up
            to `max_attempts`; then `Failed` + return the last error. Cancelled
            is **not** a failure — `release` + `Unloaded`, no retry.
    Verify: `cargo test lifecycle::load_failure_retries_then_parks` —
            `fail_next(5)`, `max_attempts 3` → state `Failed`, `loads == 3`,
            reservation released, error is the backend's. `fail_next(2)` →
            `Loaded`, `loads == 3`.

**14.10 — Cancellation**
    Do:     `cancel_load(id)` trips the entry's `CancellationToken`. The in-flight
            `load` observes `Cancelled` → `release` → `Unloaded` → notify;
            `load()` returns `AppError::Cancelled`.
    Verify: `cargo test lifecycle::cancel_mid_load_releases` — state `Unloaded`,
            `reserved_gpu_mb == 0`, the `load` call returns `Cancelled`.

**14.11 — Liveness monitor + crash reconcile**
    Do:     `check_liveness()` — health-check every `Loaded`/`Busy` entry; `Err`
            → `Failed` + `release` + notify + `warn`. Spawn the loop in
            `setup()` (~2 s). `reconcile_with_resources()` helper the monitor
            calls.
    Verify: `cargo test lifecycle::backend_death_is_detected_and_reconciled` —
            `set_healthy(false)` → `check_liveness()` → state `Failed`,
            `reserved_gpu_mb == 0`; a subsequent `load` (healthy again) →
            `Loaded`.

**14.12 — Wire-in + IPC**
    Do:     `lib.rs` — build `LifecycleManager` (register no backends yet; Phase
            15 registers llama.cpp), `manage(Arc<..>)`, spawn the liveness loop.
            `lifecycle_status` command → `Vec<LifecycleStatus { id, state:
            ModelState, vram_mb: Option<u32> }>` (new DTO). `src/lib/ipc.ts` +
            `contracts.ts`.
    Verify: `npm run tauri dev` — starts clean, liveness loop logs nothing with
            no models loaded; `lifecycle_status` returns `[]`.

**14.13 — Gate run + docs + commit**
    Do:     `node scripts/check.mjs`; `docs/verification/13_phase14_lifecycle.md`;
            `src-tauri/README.md`, `ARCHITECTURE.md` §2/§3/§7, `docs/contracts.md`,
            `PERFORMANCE.md`, `ROADMAP.md`. Commit.
    Verify: check suite green; every gate item recorded with evidence.

## Verification gate
1. Load a model → `Loaded`, a reservation is held. *(14.4)*
2. Two concurrent load requests for the same model → one backend load. *(14.6)*
3. Unload releases the reservation and returns to `Unloaded`. *(14.7)*
4. Load under insufficient resources is rejected **before any process spawn**. *(14.5)*
5. Cancel mid-load releases resources and returns to `Unloaded`. *(14.10)*
6. A forced load failure → `Failed`, reservation released, bounded retry observed. *(14.9)*
7. Killing the backend is detected; state reconciles; a later load works. *(14.11)*
8. State-machine tests cover every transition + every failure edge. *(14.3, 14.8)*
9. Full check suite green; bindings regenerated + committed. *(14.13)*

## ADRs / open questions
- **No new ADR.** Implements ADR-0007 (reservations) + ADR-0010 (the transition
  holder does the whole transition, no re-entry).
- Retry budget (3) + backoff (500 ms base, exponential) are tuning constants,
  revisitable at Phase 31/33.
- `ModelState::Busy` added additively — recorded, not an ADR.
