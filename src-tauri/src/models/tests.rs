//! Phase 11 gate coverage: CRUD, path confinement, missing-file availability,
//! capability query, invalid-metadata rejection, id stability, cache invalidation.

use std::sync::Arc;

use serde_json::json;
use tempfile::TempDir;

use super::{validate_model_path, Model, ModelDraft, ModelFilter, ModelRegistry};
use crate::contracts::ids::ModelId;
use crate::contracts::model::{Device, ModelBackend, ModelKind, Quant, RegistryAvailability};
use crate::db::Db;
use crate::ipc::AppError;

struct Fixture {
    tmp: TempDir,
    models_dir: std::path::PathBuf,
    registry: ModelRegistry,
}

async fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let models_dir = tmp.path().join("models");
    std::fs::create_dir_all(&models_dir).unwrap();

    let db = Db::open(&tmp.path().join("localai.db")).await.unwrap();
    db.migrate().await.unwrap();

    Fixture {
        registry: ModelRegistry::new(Arc::new(db)),
        models_dir,
        tmp,
    }
}

fn touch(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"GGUF fake").unwrap();
    path
}

fn llm_draft(path: std::path::PathBuf) -> ModelDraft {
    ModelDraft {
        display_name: "Some Instruct 8B".to_owned(),
        kind: ModelKind::Llm,
        backend: ModelBackend("gguf-runtime".to_owned()),
        quant: Some(Quant("Q4_K_M".to_owned())),
        path,
        streaming: true,
        context_tokens: Some(8192),
        estimated_vram_mb: Some(7000),
        devices: vec![Device::Cuda, Device::Cpu],
        config: json!({ "n_gpu_layers": -1 }),
    }
}

// ---------------------------------------------------------------- CRUD

#[tokio::test]
async fn register_then_get_round_trips_metadata() {
    let fx = fixture().await;
    let path = touch(&fx.models_dir, "a.gguf");
    let id = fx
        .registry
        .register(llm_draft(path), &fx.models_dir)
        .await
        .unwrap();

    let got = fx.registry.get(&id).await.unwrap();
    assert_eq!(got.metadata.id, id);
    assert_eq!(got.metadata.display_name, "Some Instruct 8B");
    assert_eq!(got.metadata.kind, ModelKind::Llm);
    assert_eq!(got.metadata.capabilities.context_tokens, Some(8192));
    assert_eq!(
        got.metadata.quant.as_ref().map(|q| q.0.as_str()),
        Some("Q4_K_M")
    );
    assert_eq!(got.availability, RegistryAvailability::Ready);
    assert_eq!(got.devices, vec![Device::Cuda, Device::Cpu]);
}

#[tokio::test]
async fn list_and_list_by_kind() {
    let fx = fixture().await;
    fx.registry
        .register(llm_draft(touch(&fx.models_dir, "a.gguf")), &fx.models_dir)
        .await
        .unwrap();

    let mut stt = llm_draft(touch(&fx.models_dir, "whisper.bin"));
    stt.kind = ModelKind::Stt;
    stt.streaming = false;
    stt.context_tokens = None;
    fx.registry.register(stt, &fx.models_dir).await.unwrap();

    assert_eq!(fx.registry.list().await.unwrap().len(), 2);
    assert_eq!(
        fx.registry
            .list_by_kind(ModelKind::Llm)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        fx.registry
            .list_by_kind(ModelKind::Tts)
            .await
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn update_changes_a_field_and_bumps_updated_at() {
    let fx = fixture().await;
    let id = fx
        .registry
        .register(llm_draft(touch(&fx.models_dir, "a.gguf")), &fx.models_dir)
        .await
        .unwrap();
    let before = fx.registry.get(&id).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let mut draft = llm_draft(touch(&fx.models_dir, "a.gguf"));
    draft.display_name = "Renamed 8B".to_owned();
    fx.registry
        .update(&id, draft, &fx.models_dir)
        .await
        .unwrap();

    let after = fx.registry.get(&id).await.unwrap();
    assert_eq!(after.metadata.display_name, "Renamed 8B");
    assert_eq!(after.metadata.id, id, "id unchanged by update");
    // updated_at moved (created_at is unchanged — checked via the domain row).
    assert_ne!(
        row_of(&fx.registry, &id).await.updated_at,
        row_of(&fx.registry, &id).await.created_at
    );
    let _ = before;
}

#[tokio::test]
async fn delete_removes_the_row() {
    let fx = fixture().await;
    let id = fx
        .registry
        .register(llm_draft(touch(&fx.models_dir, "a.gguf")), &fx.models_dir)
        .await
        .unwrap();
    fx.registry.delete(&id).await.unwrap();

    assert!(matches!(
        fx.registry.get(&id).await.unwrap_err(),
        AppError::NotFound(_)
    ));
    assert!(matches!(
        fx.registry.delete(&id).await.unwrap_err(),
        AppError::NotFound(_)
    ));
}

async fn row_of(registry: &ModelRegistry, id: &ModelId) -> Model {
    registry
        .all()
        .await
        .unwrap()
        .iter()
        .find(|m| &m.id == id)
        .cloned()
        .unwrap()
}

// ---------------------------------------------------------------- path confinement

#[tokio::test]
async fn path_validation_confines_to_the_model_dir() {
    let fx = fixture().await;
    let inside = touch(&fx.models_dir, "ok.gguf");
    assert!(validate_model_path(&inside, &fx.models_dir).is_ok());

    // Outside the model dir.
    let outside = fx.tmp.path().join("elsewhere.gguf");
    std::fs::write(&outside, b"x").unwrap();
    assert!(matches!(
        validate_model_path(&outside, &fx.models_dir).unwrap_err(),
        AppError::Validation(_)
    ));

    // `..` traversal.
    let traversal = fx.models_dir.join("..").join("elsewhere.gguf");
    assert!(matches!(
        validate_model_path(&traversal, &fx.models_dir).unwrap_err(),
        AppError::Validation(_)
    ));

    // Non-existent file inside the dir.
    let missing = fx.models_dir.join("nope.gguf");
    assert!(matches!(
        validate_model_path(&missing, &fx.models_dir).unwrap_err(),
        AppError::NotFound(_)
    ));
}

#[tokio::test]
async fn register_refuses_a_path_outside_the_model_dir() {
    let fx = fixture().await;
    let outside = fx.tmp.path().join("evil.gguf");
    std::fs::write(&outside, b"x").unwrap();
    let draft = llm_draft(outside);
    assert!(fx.registry.register(draft, &fx.models_dir).await.is_err());
}

// ---------------------------------------------------------------- availability

#[tokio::test]
async fn a_removed_file_reads_back_as_missing() {
    let fx = fixture().await;
    let path = touch(&fx.models_dir, "gone.gguf");
    let id = fx
        .registry
        .register(llm_draft(path.clone()), &fx.models_dir)
        .await
        .unwrap();

    std::fs::remove_file(&path).unwrap();
    fx.registry.invalidate(); // force a re-read from DB (availability is fresh either way)

    let got = fx.registry.get(&id).await.unwrap();
    assert_eq!(got.availability, RegistryAvailability::Missing);
    // The row is still there.
    assert_eq!(fx.registry.list().await.unwrap().len(), 1);
}

// ---------------------------------------------------------------- capability query

#[tokio::test]
async fn capability_query_filters_correctly() {
    let fx = fixture().await;

    fx.registry
        .register(llm_draft(touch(&fx.models_dir, "big.gguf")), &fx.models_dir)
        .await
        .unwrap(); // Llm, streaming, ctx 8192

    let mut small = llm_draft(touch(&fx.models_dir, "small.gguf"));
    small.context_tokens = Some(2048);
    fx.registry.register(small, &fx.models_dir).await.unwrap();

    let mut tts = llm_draft(touch(&fx.models_dir, "tts.bin"));
    tts.kind = ModelKind::Tts;
    tts.streaming = false;
    tts.context_tokens = None;
    fx.registry.register(tts, &fx.models_dir).await.unwrap();

    let big_llms = fx
        .registry
        .query(&ModelFilter {
            kind: Some(ModelKind::Llm),
            min_context_tokens: Some(8192),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(big_llms.len(), 1);

    let streamers = fx
        .registry
        .query(&ModelFilter {
            streaming: Some(true),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(streamers.len(), 2);
}

// ---------------------------------------------------------------- rejection

#[tokio::test]
async fn rejects_invalid_metadata() {
    let fx = fixture().await;

    let mut d = llm_draft(touch(&fx.models_dir, "a.gguf"));
    d.display_name = "   ".to_owned();
    assert!(field_err(
        fx.registry.register(d, &fx.models_dir).await,
        "display_name"
    ));

    let mut d = llm_draft(touch(&fx.models_dir, "b.gguf"));
    d.context_tokens = Some(0);
    assert!(field_err(
        fx.registry.register(d, &fx.models_dir).await,
        "context_tokens"
    ));

    let mut d = llm_draft(touch(&fx.models_dir, "c.gguf"));
    d.estimated_vram_mb = Some(0);
    assert!(field_err(
        fx.registry.register(d, &fx.models_dir).await,
        "estimated_vram_mb"
    ));

    let mut d = llm_draft(touch(&fx.models_dir, "d.gguf"));
    d.devices = vec![];
    assert!(field_err(
        fx.registry.register(d, &fx.models_dir).await,
        "devices"
    ));

    let mut d = llm_draft(touch(&fx.models_dir, "e.gguf"));
    d.config = json!([1, 2, 3]);
    assert!(field_err(
        fx.registry.register(d, &fx.models_dir).await,
        "config"
    ));
}

fn field_err(result: Result<ModelId, AppError>, field: &str) -> bool {
    matches!(result, Err(AppError::Validation(m)) if m.contains(field))
}

#[tokio::test]
async fn a_row_with_an_unknown_kind_is_an_error_not_a_panic() {
    let fx = fixture().await;
    let id = fx
        .registry
        .register(llm_draft(touch(&fx.models_dir, "a.gguf")), &fx.models_dir)
        .await
        .unwrap();
    let id_str = id.to_string();

    // Corrupt the kind column directly.
    fx.registry
        .db_for_test()
        .write(move |tx| {
            tx.execute(
                "UPDATE model_entry SET kind='Frobnicator' WHERE id=?1",
                [id_str],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    fx.registry.invalidate();

    assert!(matches!(
        fx.registry.get(&id).await.unwrap_err(),
        AppError::Internal
    ));
}

// ---------------------------------------------------------------- stability

#[tokio::test]
async fn id_is_stable_across_a_registry_rebuild() {
    let tmp = tempfile::tempdir().unwrap();
    let models_dir = tmp.path().join("models");
    std::fs::create_dir_all(&models_dir).unwrap();
    let db_path = tmp.path().join("localai.db");
    let path = models_dir.join("a.gguf");
    std::fs::write(&path, b"x").unwrap();

    let id = {
        let db = Db::open(&db_path).await.unwrap();
        db.migrate().await.unwrap();
        let reg = ModelRegistry::new(Arc::new(db));
        reg.register(llm_draft(path.clone()), &models_dir)
            .await
            .unwrap()
    };

    let db = Db::open(&db_path).await.unwrap();
    db.migrate().await.unwrap();
    let reg = ModelRegistry::new(Arc::new(db));
    let got = reg.get(&id).await.unwrap();
    assert_eq!(got.metadata.id, id);
}

#[tokio::test]
async fn warm_get_baseline() {
    let fx = fixture().await;
    let id = fx
        .registry
        .register(llm_draft(touch(&fx.models_dir, "a.gguf")), &fx.models_dir)
        .await
        .unwrap();
    let _ = fx.registry.get(&id).await.unwrap(); // warm the cache

    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let _ = fx.registry.get(&id).await.unwrap();
    }
    let per = start.elapsed() / 1000;
    println!("warm registry get: {} ns/op", per.as_nanos());
    assert!(per < std::time::Duration::from_millis(1));
}

#[tokio::test]
async fn cache_reflects_writes() {
    let fx = fixture().await;
    assert_eq!(fx.registry.list().await.unwrap().len(), 0); // warms the cache
    fx.registry
        .register(llm_draft(touch(&fx.models_dir, "a.gguf")), &fx.models_dir)
        .await
        .unwrap();
    assert_eq!(
        fx.registry.list().await.unwrap().len(),
        1,
        "cache invalidated on write"
    );
}
