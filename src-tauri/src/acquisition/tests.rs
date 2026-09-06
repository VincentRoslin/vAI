//! Phase 12 engine coverage — offline gates (2, 3, 4, 6) against a local
//! `tiny_http` server with `Range` support, plus one `#[ignore]`d **live** test
//! (`live_download_qwen_0_5b`, gates 1/6/8) that hits real HuggingFace.

use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::download::{DownloadEngine, DownloadSpec, RegisterPlan};
use super::{budget, AcquisitionService};
use crate::contracts::acquisition::DownloadState;
use crate::contracts::model::ModelKind;
use crate::db::Db;
use crate::models::ModelRegistry;

// ---------------------------------------------------------------- mock server

/// A one-file HTTP server with `Range` support. Returns `(base_url, shutdown)`.
struct MockServer {
    base: String,
    handle: Option<std::thread::JoinHandle<()>>,
    server: Arc<tiny_http::Server>,
}

impl MockServer {
    fn serve(body: Vec<u8>) -> Self {
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let base = format!("http://{}", server.server_addr().to_ip().unwrap());
        let srv = Arc::clone(&server);
        let handle = std::thread::spawn(move || {
            for req in srv.incoming_requests() {
                let range = req
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Range"))
                    .and_then(|h| parse_range(h.value.as_str(), body.len()));
                let (slice, status, extra) = match range {
                    Some((start, end)) => (
                        &body[start..=end],
                        206,
                        vec![tiny_http::Header::from_bytes(
                            &b"Content-Range"[..],
                            format!("bytes {start}-{end}/{}", body.len()).as_bytes(),
                        )
                        .unwrap()],
                    ),
                    None => (&body[..], 200, vec![]),
                };
                let mut resp = tiny_http::Response::new(
                    tiny_http::StatusCode(status),
                    extra,
                    Cursor::new(slice.to_vec()),
                    Some(slice.len()),
                    None,
                );
                resp.add_header(
                    tiny_http::Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..]).unwrap(),
                );
                let _ = req.respond(resp);
            }
        });
        Self {
            base,
            handle: Some(handle),
            server,
        }
    }

    fn url(&self) -> String {
        format!("{}/file.bin", self.base)
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn parse_range(value: &str, len: usize) -> Option<(usize, usize)> {
    let rest = value.strip_prefix("bytes=")?;
    let (start, end) = rest.split_once('-')?;
    let start: usize = start.parse().ok()?;
    let end = if end.is_empty() {
        len - 1
    } else {
        end.parse::<usize>().ok()?.min(len - 1)
    };
    Some((start, end))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

// ---------------------------------------------------------------- fixtures

async fn engine() -> (tempfile::TempDir, Arc<DownloadEngine>) {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(&tmp.path().join("localai.db")).await.unwrap();
    db.migrate().await.unwrap();
    let db = Arc::new(db);
    let registry = Arc::new(ModelRegistry::new(Arc::clone(&db)));
    (tmp, Arc::new(DownloadEngine::new(db, registry)))
}

fn spec(url: String, dest: std::path::PathBuf, sha: Option<String>) -> DownloadSpec {
    let models_dir = dest
        .parent()
        .and_then(std::path::Path::parent)
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    DownloadSpec {
        url,
        repo: "owner/repo".to_owned(),
        revision: "main".to_owned(),
        filename: "file.bin".to_owned(),
        kind: ModelKind::Llm,
        dest_path: dest,
        models_dir,
        expected_size: None,
        sha256_expected: sha,
        register: RegisterPlan::None,
    }
}

async fn wait_terminal(
    engine: &DownloadEngine,
    id: &crate::contracts::ids::DownloadId,
) -> DownloadState {
    for _ in 0..200 {
        let state = engine
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|d| &d.id == id)
            .map(|d| d.state);
        match state {
            Some(DownloadState::Complete | DownloadState::Failed) => return state.unwrap(),
            None => return DownloadState::Failed, // row removed (cancel)
            _ => tokio::time::sleep(Duration::from_millis(25)).await,
        }
    }
    panic!("download did not finish");
}

// ---------------------------------------------------------------- tests

#[tokio::test]
async fn full_download_verifies_and_lands() {
    let body: Vec<u8> = (0u32..50_000).map(|i| (i % 251) as u8).collect();
    let sha = sha256_hex(&body);
    let server = MockServer::serve(body.clone());
    let (tmp, engine) = engine().await;
    let dest = tmp.path().join("models/owner__repo/file.bin");

    let id = engine
        .start(spec(server.url(), dest.clone(), Some(sha)), None)
        .await
        .unwrap();
    assert_eq!(wait_terminal(&engine, &id).await, DownloadState::Complete);
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert!(!dest.with_extension("bin.part").exists());
}

#[tokio::test]
async fn checksum_mismatch_fails_and_cleans_up() {
    let body: Vec<u8> = vec![7u8; 20_000];
    let server = MockServer::serve(body);
    let (tmp, engine) = engine().await;
    let dest = tmp.path().join("models/owner__repo/file.bin");

    let id = engine
        .start(
            spec(server.url(), dest.clone(), Some("deadbeef".repeat(8))),
            None,
        )
        .await
        .unwrap();
    assert_eq!(wait_terminal(&engine, &id).await, DownloadState::Failed);
    assert!(!dest.exists());
    assert!(!part_of(&dest).exists(), "the .part must be deleted");
}

#[tokio::test]
async fn resume_completes_from_a_partial_part_file() {
    let body: Vec<u8> = (0u32..80_000).map(|i| (i % 97) as u8).collect();
    let sha = sha256_hex(&body);
    let server = MockServer::serve(body.clone());
    let (tmp, engine) = engine().await;
    let dest = tmp.path().join("models/owner__repo/file.bin");
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();

    // Simulate an interrupted transfer: 30k bytes already in `.part`.
    std::fs::write(part_of(&dest), &body[..30_000]).unwrap();
    // The engine sees the existing `.part`, Range-requests `bytes=30000-`, and
    // appends the rest.
    let id = engine
        .start(spec(server.url(), dest.clone(), Some(sha)), None)
        .await
        .unwrap();
    assert_eq!(wait_terminal(&engine, &id).await, DownloadState::Complete);
    assert_eq!(std::fs::read(&dest).unwrap(), body);
}

#[tokio::test]
async fn reconcile_on_start_pauses_interrupted_rows() {
    let (tmp, engine) = engine().await;
    let server = MockServer::serve(vec![0u8; 5]);
    let dest = tmp.path().join("models/owner__repo/x.bin");
    // Start + immediately reconcile (races the worker; either way the row exists).
    let _ = engine
        .start(spec(server.url(), dest, None), None)
        .await
        .unwrap();
    // Never errors; afterwards no row is left in an "active-but-orphaned" state.
    let _ = engine.reconcile_on_start().await.unwrap();
    let rows = engine.list().await.unwrap();
    assert!(rows.iter().all(|d| !matches!(
        d.state,
        DownloadState::Downloading | DownloadState::Queued | DownloadState::Verifying
    )));
}

#[tokio::test]
async fn a_completed_gguf_download_is_registered() {
    // A minimal-but-valid GGUF header, padded so it's a plausible "file".
    let mut body = gguf_fixture("llama", 15, 4096, 26);
    body.resize(40_000, 0);
    let sha = sha256_hex(&body);
    let server = MockServer::serve(body);

    let tmp = tempfile::tempdir().unwrap();
    let models_dir = tmp.path().join("models");
    std::fs::create_dir_all(models_dir.join("owner__repo")).unwrap();
    let db = Arc::new(Db::open(&tmp.path().join("d.db")).await.unwrap());
    db.migrate().await.unwrap();
    let registry = Arc::new(ModelRegistry::new(Arc::clone(&db)));
    let engine = Arc::new(DownloadEngine::new(Arc::clone(&db), Arc::clone(&registry)));

    let dest = models_dir.join("owner__repo/model.gguf");
    let mut s = spec(server.url(), dest.clone(), Some(sha));
    s.register = RegisterPlan::GgufLlm {
        backend: "llama.cpp".to_owned(),
    };
    let id = engine.start(s, None).await.unwrap();
    assert_eq!(wait_terminal(&engine, &id).await, DownloadState::Complete);

    let models = registry.list().await.unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].metadata.kind, ModelKind::Llm);
    assert_eq!(models[0].metadata.capabilities.context_tokens, Some(4096));
    assert_eq!(
        models[0].metadata.quant.as_ref().map(|q| q.0.as_str()),
        Some("Q4_K_M")
    );
    assert!(models[0].path.contains("owner__repo"));
}

/// A minimal valid GGUF header (magic + version + 4 KV entries).
fn gguf_fixture(arch: &str, file_type: u32, ctx: u64, blocks: u64) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"GGUF");
    b.extend_from_slice(&3u32.to_le_bytes());
    b.extend_from_slice(&0u64.to_le_bytes());
    b.extend_from_slice(&4u64.to_le_bytes());
    let push_str = |k: &str, v: &str, o: &mut Vec<u8>| {
        o.extend_from_slice(&(k.len() as u64).to_le_bytes());
        o.extend_from_slice(k.as_bytes());
        o.extend_from_slice(&8u32.to_le_bytes());
        o.extend_from_slice(&(v.len() as u64).to_le_bytes());
        o.extend_from_slice(v.as_bytes());
    };
    let push_u32 = |k: &str, v: u32, o: &mut Vec<u8>| {
        o.extend_from_slice(&(k.len() as u64).to_le_bytes());
        o.extend_from_slice(k.as_bytes());
        o.extend_from_slice(&4u32.to_le_bytes());
        o.extend_from_slice(&v.to_le_bytes());
    };
    let push_u64 = |k: &str, v: u64, o: &mut Vec<u8>| {
        o.extend_from_slice(&(k.len() as u64).to_le_bytes());
        o.extend_from_slice(k.as_bytes());
        o.extend_from_slice(&10u32.to_le_bytes());
        o.extend_from_slice(&v.to_le_bytes());
    };
    push_str("general.architecture", arch, &mut b);
    push_u32("general.file_type", file_type, &mut b);
    push_u64(&format!("{arch}.context_length"), ctx, &mut b);
    push_u64(&format!("{arch}.block_count"), blocks, &mut b);
    b
}

#[tokio::test]
async fn service_refuses_an_over_budget_download() {
    let dir = tempfile::tempdir().unwrap();
    // 2 GB file vs a 1 GB budget → refused, no transfer.
    let err = budget::check_budget(2 * 1024 * 1024 * 1024, dir.path(), 1, 1).unwrap_err();
    assert!(matches!(err, crate::ipc::AppError::ResourceExhausted(_)));
}

#[tokio::test]
async fn offline_search_is_a_typed_error_not_a_hang() {
    // An unroutable base URL: connect fails fast.
    let db_tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(&db_tmp.path().join("d.db")).await.unwrap());
    db.migrate().await.unwrap();
    let reg = Arc::new(ModelRegistry::new(Arc::clone(&db)));
    let cfg = crate::config::ConfigManager::load(db_tmp.path(), db_tmp.path()).unwrap();
    let svc = AcquisitionService::new(db, reg, Arc::new(cfg)).with_hf_base("http://127.0.0.1:9");

    let started = std::time::Instant::now();
    let err = svc.search("qwen", 5).await.unwrap_err();
    assert!(matches!(err, crate::ipc::AppError::BackendUnavailable(_)));
    assert!(started.elapsed() < Duration::from_secs(20), "search hung");
}

fn part_of(dest: &std::path::Path) -> std::path::PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".part");
    std::path::PathBuf::from(s)
}

// ---------------------------------------------------------------- live (gated)

/// Gate item 1: a real HuggingFace GGUF, end to end. Downloads into the real app
/// model dir (`%APPDATA%\com.localai.app\models`) so it is available for the
/// Phase 16 vertical slice. Owner-confirmed file (2026-09-06).
///
/// Run explicitly: `cargo test -p localai --lib -- --ignored live_download_qwen`.
#[tokio::test]
#[ignore = "network + ~490 MB; run explicitly for the Phase 12 gate"]
async fn live_download_qwen_0_5b() {
    const REPO: &str = "Qwen/Qwen2.5-0.5B-Instruct-GGUF";
    const FILE: &str = "qwen2.5-0.5b-instruct-q4_k_m.gguf";

    let app_data =
        std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("com.localai.app");
    let db = Arc::new(Db::open(&app_data.join("localai.db")).await.unwrap());
    db.migrate().await.unwrap();
    let registry = Arc::new(ModelRegistry::new(Arc::clone(&db)));
    let cfg = crate::config::ConfigManager::load(&app_data, &app_data).unwrap();
    let models_dir = cfg.effective().models.dir;
    std::fs::create_dir_all(&models_dir).unwrap();
    let svc = AcquisitionService::new(Arc::clone(&db), Arc::clone(&registry), Arc::new(cfg));

    // Idempotent: clear any Qwen row + file from a prior run of this gate.
    for m in registry.list().await.unwrap() {
        if m.path.contains("Qwen2.5-0.5B") {
            let _ = crate::acquisition::delete_model(&registry, &db, &m.metadata.id).await;
        }
    }

    // Pull the file's size + SHA-256 from the HF file listing (proves that path too).
    let files = svc.list_files(REPO).await.expect("list files");
    let target = files
        .iter()
        .find(|f| f.filename == FILE)
        .expect("target file listed");
    println!(
        "listing: {} quant={:?} ctx={:?} size={:?} sha256={:?}",
        target.filename, target.quant, target.context_length, target.size, target.sha256
    );
    assert_eq!(target.quant.as_deref(), Some("Q4_K_M"));

    let started = std::time::Instant::now();
    let id = svc
        .download_gguf(REPO, FILE, target.size, target.sha256.clone(), None)
        .await
        .expect("start download");
    let state = wait_terminal_slow(svc.engine(), &id).await;
    let elapsed = started.elapsed();
    assert_eq!(state, DownloadState::Complete, "download failed");

    #[allow(clippy::cast_precision_loss)]
    let mb = target.size.map_or(0.0, |s| s as f64 / 1e6);
    println!(
        "downloaded {mb:.0} MB in {:.1}s ({:.1} MB/s)",
        elapsed.as_secs_f64(),
        mb / elapsed.as_secs_f64()
    );

    let models = registry.list().await.unwrap();
    let entry = models
        .iter()
        .find(|m| m.path.contains("Qwen2.5-0.5B"))
        .expect("registered");
    assert_eq!(entry.metadata.kind, ModelKind::Llm);
    assert_eq!(
        entry.metadata.quant.as_ref().map(|q| q.0.as_str()),
        Some("Q4_K_M")
    );
    assert_eq!(
        entry.availability,
        crate::contracts::model::RegistryAvailability::Ready
    );
    let dir = crate::models::strip_verbatim(models_dir.canonicalize().unwrap());
    let stored =
        crate::models::strip_verbatim(std::path::Path::new(&entry.path).canonicalize().unwrap());
    assert!(
        stored.starts_with(&dir),
        "path {stored:?} not confined to {dir:?}"
    );
    println!(
        "registered: {} at {}",
        entry.metadata.display_name, entry.path
    );
}

async fn wait_terminal_slow(
    engine: &DownloadEngine,
    id: &crate::contracts::ids::DownloadId,
) -> DownloadState {
    for _ in 0..2400 {
        // up to 20 min
        let d = engine
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|d| &d.id == id);
        match d.as_ref().map(|d| d.state) {
            Some(DownloadState::Complete | DownloadState::Failed) => {
                if let Some(d) = &d {
                    if let Some(e) = &d.error {
                        eprintln!("download error: {e}");
                    }
                }
                return d.unwrap().state;
            }
            _ => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }
    panic!("live download timed out");
}
