# 12 — Phase 13: Resource Manager

**Date:** 2026-09-06
**Branch:** `main` (local; no remote).
**Method:** new `src-tauri/src/resources/` module; 20 unit tests over a mock
hardware probe + the real `NvmlProbe`; config v3→v4 migration tests; a
`npm run tauri dev` launch observed for the startup measurement line.

Governing: **ADR-0007** (whole-GPU NVML + reservation ledger; per-process VRAM
confirmed unavailable), **ADR-0010** (lock ordering). Plan:
`docs/plan/13_resource-manager.md`. No new ADR.

---

## What landed

- `resources/probe.rs` — `HardwareProbe` trait (`gpu()` / `ram()`), `NvmlProbe`
  (`nvml-wrapper` GPU 0 `memory_info` + `sysinfo` RAM; NVML init failure is
  non-fatal and surfaces per-call as `ProbeError::Unavailable` → `AppError::
  BackendUnavailable`), `MockProbe` (settable totals/used, `fail_gpu`,
  `add_gpu_used`, `set_delay`).
- `resources/estimate.rs` — `EstimateInput` + `estimate_llm_vram` (weights ≈ file
  size + KV cache from GGUF dims + CUDA context 600 MB + compute buffers 300 MB;
  margin added by the caller, not here); `Calibration` (per-key EMA factor,
  α = 0.3, clamped 0.5–2.0; `apply` / `record` / `factor`).
- `resources/mod.rs` — `ResourceManager`: in-memory ledger (`Vec<LedgerEntry>`),
  `ResourceRequest` (kind, raw estimate, optional task id, optional calibration
  keys), `request` / `commit` / `release` / `observe` / `snapshot` /
  `reconcile`, all behind one `tokio::sync::Mutex<Inner>`. `request` reads the
  **cached** measurement — it never calls the probe. `DEFAULT_STALE_TTL` 120 s,
  `DRIFT_SLACK_MB` 512.
- Config schema **v4**: `resources.vram_safety_margin_mb` (default 1500),
  `ResourcesConfig`, `ConfigKey::VramSafetyMarginMb`, session override,
  `validate()` caps it at 65 536; `step_forward` handles v3→v4.
- Contracts (additive): `GpuMemory`, `RamInfo`, `ResourceSnapshot` in
  `contracts::resource`.
- IPC: `resources_snapshot` → `ResourceSnapshot`. `src/lib/ipc.ts`
  `resourcesSnapshot`, `src/lib/contracts.ts` re-exports.
- `lib.rs` — `start_resource_manager()` builds the manager over `NvmlProbe`,
  takes the first measurement, logs it, spawns `observe_loop` (1.5 s while any
  reservation is outstanding, 10 s idle; `reconcile` each busy tick), `manage`s
  the `Arc<ResourceManager>`.
- Deps: `nvml-wrapper` 0.10, `sysinfo` 0.39 (`system` feature only).

---

## Gate — execution record

All gate checks are mock-hardware (gate item 7) unless noted.

| # | Check | Result |
| - | ----- | ------ |
| 1 | Insufficient VRAM → `ResourceExhausted`, cleanly, no partial reservation | **PASS** — `insufficient_vram_is_refused_cleanly`: 14 336 free − 1 500 margin = 12 836 usable; a 13 000 MB request → `AppError::ResourceExhausted`, `snapshot().reserved_gpu_mb == 0` after. `request_without_a_measurement_is_backend_unavailable` covers the no-probe path → `BackendUnavailable`. |
| 2 | Duplicate reservation for the same operation is rejected | **PASS** — `a_duplicate_reservation_for_the_same_task_is_rejected`: 2nd GPU request with the same `task_id` → `AppError::Conflict`; a `SystemRam` request for the same task still succeeds (a load may hold GPU + RAM). |
| 3 | Concurrent requests serialized; no double-commit of the same VRAM | **PASS** — `concurrent_requests_are_serialized_and_never_oversubscribe` (`multi_thread`, 4 workers): 6 tasks each requesting 5 000 MB against a 12 836 MB budget → exactly **2** `Held`, 4 `ResourceExhausted`, `reserved_gpu_mb ≤ 12 836` throughout. |
| 4 | A failed or cancelled load releases its reservation | **PASS** — `a_cancelled_load_releases_its_reservation`: `request` → `release` before `commit` → `reserved_gpu_mb == 0`; `release` is idempotent. `commit_swaps_estimate_for_measurement_in_the_ledger` shows the committed path. |
| 5 | A stale reservation (simulated crash) is recovered by `reconcile` | **PASS** — `a_stale_reservation_is_recovered_by_reconcile` (TTL 50 ms): an uncommitted reservation older than the TTL → `reconcile` returns 1, `reserved_gpu_mb == 0`, `warn` logged. A **committed** reservation of the same age is **not** reclaimed (`reconcile` returns 0). |
| 6 | After an external process grabs VRAM, `reconcile` trusts the measurement + logs the drift | **PASS** — `an_external_process_grabbing_vram_is_reconciled_by_measurement`: `MockProbe.add_gpu_used(8 000)` → `observe` → `reconcile` logs the external-holder drift; a subsequent 2 000 MB request is now refused (free dropped to the measured value), an 800 MB request fits. |
| 7 | The mock hardware provider lets all of the above run without a GPU | **PASS** — every test above uses `MockProbe`. `the_real_probe_constructs_without_a_gpu` additionally asserts `NvmlProbe::new()` never panics and returns a typed result. |
| 8 | `request` does not block on NVML | **PASS** — `request_uses_the_cached_snapshot_and_does_not_block_on_the_probe`: `MockProbe.set_delay(2 s)` after one `observe()`; `request` returns in **< 100 ms** (measured a few µs) from the cached snapshot. |
| 9 | Full check suite green; bindings regenerated + committed | **PASS** — `node scripts/check.mjs` all green: `cargo fmt` / `clippy -D warnings` / **180 rust tests** (+27; 1 `#[ignore]`d live download) / `tsc` / eslint / prettier / **7 vitest** / `vite build`. New bindings committed: `GpuMemory`, `RamInfo`, `ResourceSnapshot`, `ResourcesConfig` (+ `AppConfig` / `ConfigKey` updated). |

### Real hardware confirmation (not a gate item — evidence for 13.4)

`the_real_probe_constructs_without_a_gpu` with `--nocapture` on the reference
machine:

```
real probe: gpu=Ok(GpuMemory { total_mb: 16303, used_mb: 2288, free_mb: 14014 })
            ram=Ok(RamInfo { total_mb: 31938, available_mb: 16511 }) (12944 µs)
```

`npm run tauri dev` startup line:

```
{"message":"resource manager ready",
 "gpu":"Some(GpuMemory { total_mb: 16303, used_mb: 2330, free_mb: 13972 })",
 "ram":"Some(RamInfo { total_mb: 31938, available_mb: 16163 })",
 "vram_safety_margin_mb":1500}
```

Matches the Phase 3 probe finding: whole-GPU `memory_info` works; per-process
attribution does not (that's why the ledger exists).

---

## Baselines (→ `PERFORMANCE.md` §1)

| Metric | Measured |
| ------ | -------- |
| `request` (cached snapshot + ledger, no probe) | a few µs (< 100 ms ceiling asserted in gate 8) |
| First combined NVML + `sysinfo` probe | ~13 ms (lazy `sysinfo` init + NVML device lookup); off the request path, on a 1.5 s poll |
| `reconcile` (empty-to-small ledger) | sub-millisecond (logged `elapsed_ms`) |

---

## Decisions taken this phase

- **One `Mutex<Inner>` is the gate.** The plan described a `Mutex<()>` guarding a
  separately-locked ledger; a single `tokio::sync::Mutex<Inner>` wrapping the
  ledger + cached snapshot + calibration achieves the same serialization with
  less surface. Nothing re-enters it; the probe is called outside it (`observe`
  reads the probe, then locks to store).
- **`request` is per-`kind`.** A load needing GPU **and** system RAM makes two
  requests; the duplicate check is per `(task_id, kind)` so that's allowed. This
  matches the single-`kind` `Reservation` contract and keeps `request` simple;
  the model-lifecycle manager (Phase 14) orchestrates the pair.
- **Calibration is in-memory** (`model_id` key, `backend` fallback). ADR-0007's
  stored `(estimated, measured)` pairs become durable when the ledger does
  (Phase 33) — the learning logic and its tests are in place now.
- **NVML init failure is non-fatal.** The app runs; `request` for a GPU
  reservation returns `BackendUnavailable` with a clear message until a
  measurement lands.

---

## Not done here (by design)

- Loading/unloading models (Phase 14) — this phase only accounts.
- Eviction / queue ordering (Phase 24 scheduler).
- The full driver-TDR recovery *flow* (Phase 33) — the `reconcile`-from-
  measurement primitive it builds on is here.
- Ledger persistence — in-memory; `reconcile` recovers from the measurement on
  the next start.

**Phase 13 complete** (all 9 gate items pass). Pointer → Phase 14.
