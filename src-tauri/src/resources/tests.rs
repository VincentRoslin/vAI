//! Phase 13 gate coverage — all mock-hardware (gate item 7).

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::estimate::{estimate_llm_vram, Calibration, EstimateInput};
use super::probe::{HardwareProbe, MockProbe, NvmlProbe, ProbeError};
use super::{ResourceManager, ResourceRequest};
use crate::contracts::ids::TaskId;
use crate::contracts::resource::ResourceKind;
use crate::ipc::AppError;

fn task(id: &str) -> TaskId {
    TaskId::from_trusted(id)
}

/// A manager over a fresh 16 GB / 32 GB mock, with one measurement taken.
async fn ready_manager(margin_mb: u32) -> (Arc<ResourceManager>, Arc<MockProbe>) {
    let probe = Arc::new(MockProbe::new());
    let mgr = Arc::new(ResourceManager::new(probe.clone(), margin_mb));
    mgr.observe().await;
    (mgr, probe)
}

// ---------------------------------------------------------------- probe

#[test]
fn mock_probe_returns_set_values() {
    let probe = MockProbe::new();
    probe.set_gpu(16_384, 4_096);
    let gpu = probe.gpu().expect("gpu");
    assert_eq!(gpu.total_mb, 16_384);
    assert_eq!(gpu.used_mb, 4_096);
    assert_eq!(gpu.free_mb, 12_288);

    probe.set_ram(65_536, 40_000);
    assert_eq!(probe.ram().expect("ram").available_mb, 40_000);
}

#[test]
fn a_failed_probe_maps_to_backend_unavailable() {
    let probe = MockProbe::new();
    probe.fail_gpu();
    let err = probe.gpu().unwrap_err();
    assert!(matches!(err, ProbeError::Unavailable(_)));
    let app: AppError = err.into();
    assert!(matches!(app, AppError::BackendUnavailable(_)));
}

#[test]
fn the_real_probe_constructs_without_a_gpu() {
    // NVML may or may not be present in CI; either way `new()` must not panic
    // and `gpu()` must return a typed result, not hang.
    let probe = NvmlProbe::new();
    let started = Instant::now();
    let gpu = probe.gpu();
    let ram = probe.ram();
    println!(
        "real probe: gpu={gpu:?} ram={ram:?} ({} µs)",
        started.elapsed().as_micros()
    );
}

// ---------------------------------------------------------------- estimate

#[test]
fn llm_estimate_lands_in_a_sane_band_for_8b_q4() {
    // ~8B Q4_K_M ≈ 4.9 GB file; 32 layers, 8 KV heads, head_dim 128, 8k ctx, f16.
    let input = EstimateInput {
        file_size_mb: 4_900,
        n_layers: 32,
        n_kv_heads: 8,
        head_dim: 128,
        context_tokens: 8_192,
        kv_bytes: 2,
    };
    let est = estimate_llm_vram(&input);
    assert!(
        (5_000..=9_000).contains(&est),
        "estimate {est} MB outside the 5–9 GB band"
    );
    // KV cache for these dims is ~1 GB.
    assert!((768..=1_536).contains(&input.kv_cache_mb()));
}

#[test]
fn calibration_moves_the_factor_toward_measured_over_raw() {
    let mut cal = Calibration::new();
    assert_eq!(cal.apply(1_000, "m", "b"), 1_000); // factor 1.0 until taught

    // Measured consistently 20% above the raw estimate.
    for _ in 0..20 {
        cal.record("m", 1_200, 1_000);
    }
    let factor = cal.factor("m", "b");
    assert!(
        (1.15..=1.25).contains(&factor),
        "factor {factor} not near 1.2"
    );
    assert!(cal.apply(1_000, "m", "b") > 1_150);
}

#[test]
fn calibration_apply_is_monotonic_in_the_factor() {
    let mut low = Calibration::new();
    let mut high = Calibration::new();
    for _ in 0..30 {
        low.record("k", 700, 1_000);
        high.record("k", 1_300, 1_000);
    }
    assert!(low.apply(1_000, "k", "b") < high.apply(1_000, "k", "b"));
}

#[test]
fn calibration_clamps_wild_observations() {
    let mut cal = Calibration::new();
    for _ in 0..50 {
        cal.record("k", 100_000, 1_000); // 100× — must be clamped
    }
    assert!(cal.factor("k", "b") <= 2.0);
}

// ---------------------------------------------------------------- lifecycle

#[tokio::test]
async fn request_within_budget_is_held_then_released() {
    let (mgr, _probe) = ready_manager(1_500).await;
    let res = mgr
        .request(ResourceRequest::gpu(6_000))
        .await
        .expect("granted");
    assert_eq!(res.kind, ResourceKind::Gpu);
    assert_eq!(res.amount_mb, 6_000);

    let snap = mgr.snapshot().await;
    assert_eq!(snap.reserved_gpu_mb, 6_000);

    mgr.release(&res.id).await;
    assert_eq!(mgr.snapshot().await.reserved_gpu_mb, 0);
}

#[tokio::test]
async fn insufficient_vram_is_refused_cleanly() {
    // 14 336 free − 1 500 margin = 12 836 usable.
    let (mgr, _probe) = ready_manager(1_500).await;
    let err = mgr.request(ResourceRequest::gpu(13_000)).await.unwrap_err();
    assert!(matches!(err, AppError::ResourceExhausted(_)));
    // No partial reservation left behind.
    assert_eq!(mgr.snapshot().await.reserved_gpu_mb, 0);
}

#[tokio::test]
async fn request_without_a_measurement_is_backend_unavailable() {
    let probe = Arc::new(MockProbe::new());
    let mgr = ResourceManager::new(probe, 1_500); // no observe()
    let err = mgr.request(ResourceRequest::gpu(100)).await.unwrap_err();
    assert!(matches!(err, AppError::BackendUnavailable(_)));
}

#[tokio::test]
async fn commit_swaps_estimate_for_measurement_in_the_ledger() {
    let (mgr, _probe) = ready_manager(1_500).await;
    let res = mgr
        .request(ResourceRequest::gpu(6_000).calibrated("model-x", "llama.cpp"))
        .await
        .expect("granted");
    mgr.commit(&res.id, 5_200).await.expect("committed");
    assert_eq!(mgr.snapshot().await.reserved_gpu_mb, 5_200);
}

#[tokio::test]
async fn commit_of_an_unknown_reservation_is_not_found() {
    let (mgr, _probe) = ready_manager(1_500).await;
    let err = mgr
        .commit(
            &crate::contracts::ids::ReservationId::from_trusted("nope"),
            1,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)));
}

#[tokio::test]
async fn system_ram_is_a_second_tracked_constraint() {
    let (mgr, probe) = ready_manager(1_500).await;
    probe.set_ram(32_768, 4_000);
    mgr.observe().await;
    let err = mgr
        .request(ResourceRequest::system_ram(8_000))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::ResourceExhausted(_)));
    // GPU and RAM are independent — a GPU request still succeeds.
    mgr.request(ResourceRequest::gpu(6_000))
        .await
        .expect("gpu ok");
}

// ---------------------------------------------------------------- duplicate + concurrency

#[tokio::test]
async fn a_duplicate_reservation_for_the_same_task_is_rejected() {
    let (mgr, _probe) = ready_manager(1_500).await;
    let first = ResourceRequest::gpu(2_000).for_task(task("t-1"));
    mgr.request(first).await.expect("first granted");

    let dup = ResourceRequest::gpu(2_000).for_task(task("t-1"));
    let err = mgr.request(dup).await.unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));

    // A different kind for the same task is fine (GPU + RAM for one load).
    mgr.request(ResourceRequest::system_ram(1_000).for_task(task("t-1")))
        .await
        .expect("ram for same task ok");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_requests_are_serialized_and_never_oversubscribe() {
    // 14 336 free − 1 500 margin = 12 836 usable; 5 000 MB each → exactly 2 fit.
    let (mgr, _probe) = ready_manager(1_500).await;

    let mut handles = Vec::new();
    for i in 0..6 {
        let mgr = Arc::clone(&mgr);
        handles.push(tokio::spawn(async move {
            mgr.request(ResourceRequest::gpu(5_000).for_task(task(&format!("t-{i}"))))
                .await
        }));
    }
    let mut granted = 0;
    let mut exhausted = 0;
    for h in handles {
        match h.await.unwrap() {
            Ok(_) => granted += 1,
            Err(AppError::ResourceExhausted(_)) => exhausted += 1,
            Err(other) => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(granted, 2, "exactly two 5 GB reservations fit");
    assert_eq!(exhausted, 4);
    assert!(mgr.snapshot().await.reserved_gpu_mb <= 12_836);
}

// ---------------------------------------------------------------- observe + reconcile

#[tokio::test]
async fn a_stale_reservation_is_recovered_by_reconcile() {
    let probe = Arc::new(MockProbe::new());
    let mgr = ResourceManager::new(probe, 1_500).with_stale_ttl(Duration::from_millis(50));
    mgr.observe().await;

    mgr.request(ResourceRequest::gpu(4_000))
        .await
        .expect("granted");
    assert_eq!(mgr.snapshot().await.reserved_gpu_mb, 4_000);

    tokio::time::sleep(Duration::from_millis(80)).await;
    let recovered = mgr.reconcile().await;
    assert_eq!(recovered, 1);
    assert_eq!(mgr.snapshot().await.reserved_gpu_mb, 0);

    // A committed reservation of the same age is NOT reclaimed.
    let res2 = mgr
        .request(ResourceRequest::gpu(4_000))
        .await
        .expect("granted");
    mgr.commit(&res2.id, 3_800).await.expect("committed");
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(mgr.reconcile().await, 0);
    assert_eq!(mgr.snapshot().await.reserved_gpu_mb, 3_800);
}

#[tokio::test]
async fn a_cancelled_load_releases_its_reservation() {
    let (mgr, _probe) = ready_manager(1_500).await;
    let res = mgr
        .request(ResourceRequest::gpu(6_000))
        .await
        .expect("granted");
    // Caller hits an error before commit and cleans up.
    mgr.release(&res.id).await;
    assert_eq!(mgr.snapshot().await.reserved_gpu_mb, 0);
    // Release is idempotent.
    mgr.release(&res.id).await;
}

#[tokio::test]
async fn an_external_process_grabbing_vram_is_reconciled_by_measurement() {
    let (mgr, probe) = ready_manager(1_500).await;
    let res = mgr
        .request(ResourceRequest::gpu(4_000))
        .await
        .expect("granted");
    mgr.commit(&res.id, 4_000).await.expect("committed");

    // Something outside the app grabs 8 GB.
    probe.add_gpu_used(8_000);
    mgr.observe().await;
    mgr.reconcile().await; // logs the drift; keeps trusting the measurement

    // free is now 14 336 − 8 000 = 6 336; − margin 1 500 − reserved 4 000 = 836.
    let err = mgr.request(ResourceRequest::gpu(2_000)).await.unwrap_err();
    assert!(matches!(err, AppError::ResourceExhausted(_)));
    mgr.request(ResourceRequest::gpu(800))
        .await
        .expect("small fits");
}

#[tokio::test]
async fn snapshot_reports_measurements_and_reservations() {
    let (mgr, _probe) = ready_manager(1_500).await;
    mgr.request(ResourceRequest::gpu(3_000))
        .await
        .expect("granted");
    let snap = mgr.snapshot().await;
    assert_eq!(snap.gpu.expect("gpu").total_mb, 16_384);
    assert_eq!(snap.ram.expect("ram").total_mb, 32_768);
    assert_eq!(snap.reserved_gpu_mb, 3_000);
}

// ---------------------------------------------------------------- request never blocks on the probe

#[tokio::test]
async fn request_uses_the_cached_snapshot_and_does_not_block_on_the_probe() {
    let probe = Arc::new(MockProbe::new());
    let mgr = Arc::new(ResourceManager::new(probe.clone(), 1_500));
    mgr.observe().await; // populate the cache while the probe is fast

    probe.set_delay(Duration::from_secs(2)); // the probe is now slow

    let started = Instant::now();
    mgr.request(ResourceRequest::gpu(1_000))
        .await
        .expect("granted from cache");
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "request took {:?} — it must not touch the probe",
        started.elapsed()
    );
}
