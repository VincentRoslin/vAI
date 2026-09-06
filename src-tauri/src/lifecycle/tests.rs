//! Phase 14 gate coverage — state machine, coalescing, reservation lifecycle,
//! retry, cancellation, liveness. All against `FakeBackend` + `MockProbe`.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tempfile::TempDir;

use super::backend::{FakeBackend, ModelBackend};
use super::{LifecycleManager, RetryPolicy};
use crate::contracts::ids::ModelId;
use crate::contracts::model::{Device, ModelBackend as BackendName, ModelKind, ModelState};
use crate::db::Db;
use crate::ipc::AppError;
use crate::models::{ModelDraft, ModelRegistry};
use crate::resources::probe::MockProbe;
use crate::resources::ResourceManager;

const BACKEND_KEY: &str = "fake";

struct Fixture {
    _tmp: TempDir,
    manager: Arc<LifecycleManager>,
    backend: Arc<FakeBackend>,
    resources: Arc<ResourceManager>,
    model: ModelId,
}

async fn fixture_with(load_delay: Duration, estimated_vram_mb: Option<u32>) -> Fixture {
    fixture_full(load_delay, estimated_vram_mb, None).await
}

/// `gpu` = `Some((total_mb, used_mb))` to override the mock GPU (default is a
/// 16 GB card with 2 GB used).
async fn fixture_full(
    load_delay: Duration,
    estimated_vram_mb: Option<u32>,
    gpu: Option<(u64, u64)>,
) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let models_dir = tmp.path().join("models");
    std::fs::create_dir_all(&models_dir).unwrap();
    let file = models_dir.join("m.gguf");
    std::fs::write(&file, b"GGUF fake").unwrap();

    let db = Db::open(&tmp.path().join("localai.db")).await.unwrap();
    db.migrate().await.unwrap();
    let registry = Arc::new(ModelRegistry::new(Arc::new(db)));
    let model = registry
        .register(
            ModelDraft {
                display_name: "Fake 8B".to_owned(),
                kind: ModelKind::Llm,
                backend: BackendName(BACKEND_KEY.to_owned()),
                quant: None,
                path: file,
                streaming: true,
                context_tokens: Some(8192),
                estimated_vram_mb,
                devices: vec![Device::Cuda],
                config: json!({}),
            },
            &models_dir,
        )
        .await
        .unwrap();

    let probe = Arc::new(MockProbe::new()); // 16 GB GPU, 2 GB used
    if let Some((total, used)) = gpu {
        probe.set_gpu(total, used);
    }
    let resources = Arc::new(ResourceManager::new(probe, 1_500));
    resources.observe().await;

    let manager = Arc::new(LifecycleManager::new(
        Arc::clone(&registry),
        Arc::clone(&resources),
        RetryPolicy {
            max_attempts: 3,
            backoff_base: Duration::from_millis(5),
        },
    ));
    let backend = Arc::new(FakeBackend::new(load_delay));
    manager.register_backend(BACKEND_KEY, Arc::clone(&backend) as Arc<dyn ModelBackend>);

    Fixture {
        _tmp: tmp,
        manager,
        backend,
        resources,
        model,
    }
}

async fn fixture() -> Fixture {
    fixture_with(Duration::from_millis(10), Some(6_000)).await
}

// ---------------------------------------------------------------- state machine

#[tokio::test]
async fn an_unknown_model_reads_unloaded() {
    let f = fixture().await;
    assert_eq!(
        f.manager.state(&ModelId::from_trusted("never-seen")).await,
        ModelState::Unloaded
    );
}

#[tokio::test]
async fn load_reaches_loaded_and_holds_a_reservation() {
    let f = fixture().await;
    f.manager.load(&f.model).await.expect("loaded");

    assert_eq!(f.manager.state(&f.model).await, ModelState::Loaded);
    assert_eq!(f.backend.loads(), 1);
    assert!(f.resources.snapshot().await.reserved_gpu_mb > 0);

    let statuses = f.manager.statuses().await;
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].vram_mb, Some(4_096)); // FakeInstance measured
}

#[tokio::test]
async fn a_second_load_of_a_loaded_model_is_a_noop() {
    let f = fixture().await;
    f.manager.load(&f.model).await.unwrap();
    f.manager.load(&f.model).await.unwrap();
    assert_eq!(f.backend.loads(), 1);
}

// ---------------------------------------------------------------- pre-flight

#[tokio::test]
async fn insufficient_resources_are_rejected_before_any_spawn() {
    // 6 000 MB estimate; free 692 − 1 500 margin → 0 usable.
    let f = fixture_full(Duration::from_millis(10), Some(6_000), Some((8_192, 7_500))).await;

    let err = f.manager.load(&f.model).await.unwrap_err();
    assert!(matches!(err, AppError::ResourceExhausted(_)));
    assert_eq!(f.manager.state(&f.model).await, ModelState::Unloaded);
    assert_eq!(f.backend.loads(), 0, "backend must not be called");
    assert_eq!(f.resources.snapshot().await.reserved_gpu_mb, 0);
}

// ---------------------------------------------------------------- coalescing

#[tokio::test]
async fn concurrent_loads_of_the_same_model_coalesce() {
    let f = fixture_with(Duration::from_millis(60), Some(6_000)).await;
    let mut handles = Vec::new();
    for _ in 0..5 {
        let manager = Arc::clone(&f.manager);
        let id = f.model.clone();
        handles.push(tokio::spawn(async move { manager.load(&id).await }));
    }
    for h in handles {
        h.await.unwrap().expect("all callers see Ok");
    }
    assert_eq!(f.backend.loads(), 1, "exactly one backend load");
    assert_eq!(f.manager.state(&f.model).await, ModelState::Loaded);
}

// ---------------------------------------------------------------- unload

#[tokio::test]
async fn unload_releases_the_reservation() {
    let f = fixture().await;
    f.manager.load(&f.model).await.unwrap();
    assert!(f.resources.snapshot().await.reserved_gpu_mb > 0);

    f.manager.unload(&f.model).await.expect("unloaded");
    assert_eq!(f.manager.state(&f.model).await, ModelState::Unloaded);
    assert_eq!(f.backend.shutdowns(), 1);
    assert_eq!(f.resources.snapshot().await.reserved_gpu_mb, 0);

    // Idempotent.
    f.manager.unload(&f.model).await.expect("still ok");
}

// ---------------------------------------------------------------- busy

#[tokio::test]
async fn busy_blocks_unload_until_the_guard_drops() {
    let f = fixture().await;
    f.manager.load(&f.model).await.unwrap();

    let guard = f.manager.begin_use(&f.model).await.expect("busy");
    assert_eq!(f.manager.state(&f.model).await, ModelState::Busy);
    let err = f.manager.unload(&f.model).await.unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));

    guard.release().await;
    assert_eq!(f.manager.state(&f.model).await, ModelState::Loaded);
    f.manager.unload(&f.model).await.expect("now unloads");
}

#[tokio::test]
async fn begin_use_on_a_non_loaded_model_conflicts() {
    let f = fixture().await;
    let err = f.manager.begin_use(&f.model).await.unwrap_err();
    assert!(matches!(err, AppError::Conflict(_) | AppError::NotFound(_)));
}

// ---------------------------------------------------------------- retry

#[tokio::test]
async fn load_failure_retries_then_parks_in_failed() {
    let f = fixture().await;
    f.backend.fail_next(5); // more than max_attempts
    let err = f.manager.load(&f.model).await.unwrap_err();
    assert!(matches!(err, AppError::BackendUnavailable(_)));
    assert_eq!(f.manager.state(&f.model).await, ModelState::Failed);
    assert_eq!(f.backend.loads(), 3, "bounded at max_attempts");
    assert_eq!(f.resources.snapshot().await.reserved_gpu_mb, 0);

    let status = &f.manager.statuses().await[0];
    assert!(status.error.is_some());
}

#[tokio::test]
async fn load_recovers_within_the_retry_budget() {
    let f = fixture().await;
    f.backend.fail_next(2); // 2 fail, 3rd succeeds
    f.manager.load(&f.model).await.expect("recovered");
    assert_eq!(f.manager.state(&f.model).await, ModelState::Loaded);
    assert_eq!(f.backend.loads(), 3);
    assert!(f.resources.snapshot().await.reserved_gpu_mb > 0);
}

#[tokio::test]
async fn a_load_after_a_parked_failure_tries_again() {
    let f = fixture().await;
    f.backend.fail_next(5);
    f.manager.load(&f.model).await.unwrap_err();
    assert_eq!(f.manager.state(&f.model).await, ModelState::Failed);

    // Backend is healthy now.
    f.manager
        .load(&f.model)
        .await
        .expect("recovered on a fresh call");
    assert_eq!(f.manager.state(&f.model).await, ModelState::Loaded);
}

// ---------------------------------------------------------------- cancellation

#[tokio::test]
async fn cancel_mid_load_releases_and_returns_unloaded() {
    let f = fixture_with(Duration::from_millis(500), Some(6_000)).await;
    let manager = Arc::clone(&f.manager);
    let id = f.model.clone();
    let load = tokio::spawn(async move { manager.load(&id).await });

    // Let the load claim the slot + start the backend.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(f.manager.state(&f.model).await, ModelState::Loading);
    f.manager
        .cancel_load(&f.model)
        .await
        .expect("cancel accepted");

    let result = load.await.unwrap();
    assert!(matches!(result, Err(AppError::Cancelled)));
    assert_eq!(f.manager.state(&f.model).await, ModelState::Unloaded);
    assert_eq!(f.resources.snapshot().await.reserved_gpu_mb, 0);
}

#[tokio::test]
async fn cancel_when_not_loading_is_a_conflict() {
    let f = fixture().await;
    f.manager.load(&f.model).await.unwrap();
    let err = f.manager.cancel_load(&f.model).await.unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));
}

// ---------------------------------------------------------------- liveness

#[tokio::test]
async fn a_dead_backend_is_detected_and_reconciled() {
    let f = fixture().await;
    f.manager.load(&f.model).await.unwrap();
    assert!(f.resources.snapshot().await.reserved_gpu_mb > 0);

    f.backend.set_healthy(false);
    let failed = f.manager.check_liveness().await;
    assert_eq!(failed, 1);
    assert_eq!(f.manager.state(&f.model).await, ModelState::Failed);
    assert_eq!(f.resources.snapshot().await.reserved_gpu_mb, 0);
    assert!(f.backend.health_checks() >= 1);

    // A subsequent load (healthy again) works.
    f.backend.set_healthy(true);
    f.manager.load(&f.model).await.expect("reloads");
    assert_eq!(f.manager.state(&f.model).await, ModelState::Loaded);
}

#[tokio::test]
async fn check_liveness_is_a_noop_with_nothing_loaded() {
    let f = fixture().await;
    assert_eq!(f.manager.check_liveness().await, 0);
}
