# Phase 13 — Resource Manager

> **Status: COMPLETE** (2026-09-06) — all 9 gate items pass;
> evidence `docs/verification/12_phase13_resources.md`.

> **Architecture frozen at Phase 5.** Step detail **finalized at phase entry, 2026-09-06**. Governing: **ADR-0007** (NVML whole-GPU `{total,used,free}` + our own reservation ledger — per-process VRAM is **confirmed unavailable** on this driver, probe `02_phase3_probes.md`; closed-form estimate + learned correction; one async mutex; system-RAM as a 2nd constraint; TDR path), **ADR-0010** (lock ordering — the mutex holder performs the whole transition), ADR-0016 (`vram_safety_margin_mb` → config schema v4).

## Objective
The single authority for "can this operation safely use the GPU (and system RAM)
right now?". Lifecycle: **request → reserve → commit → observe → release →
reconcile**. Hardware access is behind a trait with a real (NVML + sysinfo) impl
and a mock. Model **file size is never assumed to equal runtime VRAM**.

## Depends on
Phase 7 (`Reservation`/`ResourceKind` contracts), Phase 10 (`operation` spans),
Phase 11 (`ModelMetadata.estimated_vram_mb`), Phase 8 (config).

## Not in this phase
- Loading / unloading models (Phase 14) — this phase only *accounts*.
- Scheduling / eviction / priorities (Phase 24) — beyond the one serialization mutex.
- The driver-TDR recovery *flow* (kill-all + reload) — Phase 33; this phase has
  the `reconcile`-from-measurement primitive it builds on.
- Persisting the ledger to SQLite — in-memory for Phase 13; a process crash loses
  reservations, which `reconcile` recovers from the measurement on next start.

## Architecture notes
- **New module `src-tauri/src/resources/`** (`mod.rs`, `probe.rs` — the trait +
  NVML/sysinfo impl + mock, `estimate.rs` — the VRAM formula + calibration,
  `tests.rs`).
- **`HardwareProbe` trait**: `gpu() -> ProbeResult<GpuMemory>` and `ram() ->
  ProbeResult<RamInfo>` — both cheap. Real impl: `nvml-wrapper` (dlopen's
  `nvml.dll`, no CUDA link-time dep) + `sysinfo` (`system` feature only). Mock:
  atomically-settable totals/used so tests drive "an external app grabbed VRAM".
- **Ledger** (`Mutex<Vec<LedgerEntry>>`): `{ id: ReservationId, kind, estimate_mb,
  committed_mb: Option<u32>, task_id: Option<TaskId>, state, created_at }`.
  `outstanding(kind)` = Σ of `committed_mb.unwrap_or(estimate_mb)` for `Held` /
  `Requested` entries.
- **`request` is fast**: uses the **last observed** snapshot + the ledger, no
  NVML call on the request path. `effective_free = observed_free − margin −
  outstanding`. GPU and RAM checked independently for the request's needs.
- **One `tokio::sync::Mutex<()>`** (`gate`) is taken for the whole of
  `request` / `commit` / `release` / `reconcile` — never re-entered, never held
  across an `.await` on anything but the ledger. (ADR-0010 lock ordering; the
  full load transition holds it at Phase 14.)
- **Estimate** (`estimate.rs`): `estimate_llm_vram(EstimateInput) -> u32` —
  `weights + kv_cache + cuda_context(≈600) + compute_buffers(≈300) + margin`,
  where `weights ≈ file_size_mb` (or `params × bytes_per_weight(quant)`) and
  `kv_cache = n_layers × 2 × n_kv_heads × head_dim × ctx × kv_bytes / 1MiB`.
  A **`Calibration`** map (`model_id → factor`, falling back to a `backend`
  default; **in-memory this phase** — persistence rides with the ledger, Phase 33)
  multiplies the estimate; `commit` updates the factor from
  `measured / raw_estimate` (EMA). ADR-0007's stored `(estimated, measured)`
  pairs become durable when the ledger does.
- **Observation loop**: `setup()` spawns a task that calls `observe()` every
  ~1500 ms **while the ledger is non-empty**, backing off to ~10 s when idle.
- **`reconcile`**: if `observed_used` exceeds `Σ committed + slack` → an external
  holder; log the drift (`operation` span, `warn`), keep trusting the
  measurement. Reservations `Held` for > `STALE_TTL` with no `commit` → released
  + logged (a crashed caller).

## Performance notes
- `request` must not block on NVML — verified by a test that stalls the probe and
  checks `request` still returns from the cached snapshot.
- Observation poll: 1.5 s loaded / 10 s idle. Record one `observe()` timing.

## Steps

**13.1 — Deps + config v4**
    Do:     Add `nvml-wrapper` 0.10, `sysinfo` (`system` only). Config schema
            **v4**: `resources.vram_safety_margin_mb` (default 1500) — a new
            `ResourcesConfig`, `ConfigKey::VramSafetyMarginMb`, session override,
            validate (`>= 0`, `<= gpu total` is not enforced — it may exceed a
            small GPU harmlessly). `step_forward` handles v3→v4.
    Verify: `cargo build`; `cargo test config::` — v3→v4 migration + validation.

**13.2 — Contracts (additive)**
    Do:     `contracts::resource` — `GpuMemory { total_mb, used_mb, free_mb }`,
            `RamInfo { total_mb, available_mb }`, `ResourceSnapshot { gpu, ram,
            reserved_gpu_mb, reserved_ram_mb }`. Round-trip tests.
    Verify: `cargo test contracts`; bindings regenerate.

**13.3 — `HardwareProbe` trait + mock**
    Do:     `probe.rs` — the trait, `MockProbe` (atomics for total/used, GPU +
            RAM), `ProbeError` (→ `AppError::BackendUnavailable`).
    Verify: `cargo test resources::probe` — mock returns set values; a probe that
            errors surfaces `BackendUnavailable`.

**13.4 — NVML + sysinfo real impl**
    Do:     `NvmlProbe` — lazily `Nvml::init()`, `device(0).memory_info()`;
            `sysinfo::System` for RAM (`refresh_memory`). Init failure → a probe
            that always errors (the app still runs; loads will be refused with a
            clear message). Feature-gate nothing — it's the default.
    Verify: `npm run tauri dev` — a startup log line with real
            `gpu total/used/free` + `ram total/available` (probe already
            confirmed working in Phase 3).

**13.5 — Estimation + calibration**
    Do:     `estimate.rs` — `EstimateInput { file_size_mb, n_layers, n_kv_heads,
            head_dim, context_tokens, kv_bytes }`, `estimate_llm_vram`,
            `Calibration` (EMA `factor` keyed `model_id`, `backend` fallback;
            `apply` / `record`; in-memory).
    Verify: `cargo test resources::estimate` — a known 8B-Q4 / 8k-ctx input lands
            in a sane band (5–9 GB); `record(measured)` moves the factor toward
            `measured/raw`; `apply` is monotonic in the factor.

**13.6 — `ResourceManager`: request / commit / release**
    Do:     `mod.rs` — `ResourceManager::new(probe, config)`, `request(ResourceRequest
            { gpu_mb, ram_mb, task_id }) -> AppResult<Reservation>`,
            `commit(&id, measured_gpu_mb) -> AppResult<()>`, `release(&id)`,
            `snapshot() -> ResourceSnapshot`. All under the `gate` mutex.
    Verify: `cargo test resources::lifecycle` — request within budget → `Held`;
            request exceeding `free − margin − outstanding` → `ResourceExhausted`
            (no ledger entry left); `commit` swaps estimate→measured in
            `outstanding`; `release` removes it.

**13.7 — Duplicate + concurrency**
    Do:     `request` rejects a second reservation carrying the same `task_id`
            while the first is un-terminal (`Conflict`). The `gate` mutex
            serializes concurrent `request`s.
    Verify: `cargo test resources::concurrency` — 2 requests for the same
            `task_id` → 2nd is `Conflict`; N concurrent requests for
            distinct tasks against a budget that fits M<N → exactly M `Held`, the
            rest `ResourceExhausted`, Σ never exceeds the budget.

**13.8 — Observe + reconcile**
    Do:     `observe()` (refresh the cached snapshot), `reconcile()` (external-
            holder drift log; stale-reservation recovery by TTL). A `spawn`ed
            observation loop in `setup()` (1.5 s loaded / 10 s idle).
    Verify: `cargo test resources::reconcile` — a failed/cancelled "load"
            (caller drops without commit, TTL elapsed) → `reconcile` releases it,
            logs it; `MockProbe.set_used(+8 GB)` (external app) → `reconcile`
            logs the drift and `request` now sees less free (trusts the
            measurement).

**13.9 — `request` never blocks on the probe**
    Do:     (design; test only.)
    Verify: `cargo test resources::request_uses_cached_snapshot` — a `MockProbe`
            whose `gpu()` sleeps 2 s; `request` returns in < 50 ms from the last
            `observe()`d value.

**13.10 — IPC + wiring**
    Do:     `resources_snapshot` command → `ResourceSnapshot`. `setup()` builds
            the manager (real probe; falls back to the erroring probe on NVML
            init failure), logs the first snapshot, spawns the observe loop,
            `app.manage`s it. `src/lib/ipc.ts` wrapper + `contracts.ts`.
    Verify: `npm run tauri dev` — snapshot logged; no errors; `resources_snapshot`
            returns real numbers.

**13.11 — Gate run + docs + commit**
    Do:     `node scripts/check.mjs`; `docs/verification/12_phase13_resources.md`;
            `src-tauri/README.md`, `ARCHITECTURE.md` §2/§3, `docs/contracts.md`,
            `PERFORMANCE.md`, `ROADMAP.md`. Commit.
    Verify: check suite green; every gate check recorded.

## Verification gate
1. Insufficient VRAM → `ResourceExhausted`, cleanly, no partial reservation. *(13.6)*
2. Duplicate reservation for the same operation is rejected. *(13.7)*
3. Concurrent requests are serialized; no double-commit of the same VRAM. *(13.7)*
4. A failed or cancelled load releases its reservation. *(13.8 — TTL/`release`)*
5. A stale reservation (simulated crash) is recovered by `reconcile`. *(13.8)*
6. After an external process grabs VRAM, `reconcile` trusts the measurement and
   logs the drift. *(13.8)*
7. The mock hardware provider lets all of the above run without a GPU. *(all)*
8. `request` does not block on NVML. *(13.9)*
9. Full check suite green; bindings regenerated. *(13.11)*

## ADRs / open questions
- No new ADR — implements ADR-0007. Eviction stays with the scheduler (Phase 24).
- Ledger persistence + the full TDR recovery flow are deferred (Phase 33 fault
  injection) — the `reconcile`-from-measurement primitive is here.
