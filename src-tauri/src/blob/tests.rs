//! `blob` unit tests — no DB, no network, just a temp dir.

use std::collections::HashSet;

use super::{reconcile, BlobStore};
use crate::contracts::ids::AssetId;
use crate::ipc::AppError;

fn store() -> (BlobStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    (BlobStore::new(dir.path().join("blobs")), dir)
}

#[test]
fn put_is_content_addressed_and_idempotent() {
    let (store, _dir) = store();
    let a = store.put(b"hello world", "text/plain").unwrap();
    let b = store.put(b"hello world", "text/plain").unwrap();
    assert_eq!(a, b, "same bytes -> same id");
    // Known SHA-256 of "hello world".
    assert_eq!(
        a.as_str(),
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
    let different = store.put(b"other", "text/plain").unwrap();
    assert_ne!(a, different);
}

#[test]
fn put_lays_the_blob_out_two_level_and_get_finds_it() {
    let (store, _dir) = store();
    let id = store.put(b"payload", "application/octet-stream").unwrap();
    let path = store.get(&id).unwrap();
    assert!(path.is_file());
    assert_eq!(
        path.parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        &id.as_str()[0..2]
    );
    assert_eq!(store.read(&id).unwrap(), b"payload");
    assert!(store.contains(&id));
    // No stray .tmp left behind.
    let shard = path.parent().unwrap();
    let leftovers: Vec<_> = std::fs::read_dir(shard)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "a .tmp file survived put()");
}

#[test]
fn get_rejects_a_non_sha_id_and_reports_missing_blobs() {
    let (store, _dir) = store();
    let bad = AssetId::from_trusted("../etc/passwd");
    assert!(matches!(store.get(&bad), Err(AppError::Validation(_))));
    assert!(!store.contains(&bad));

    let absent = AssetId::from_trusted("a".repeat(64));
    assert!(matches!(store.get(&absent), Err(AppError::NotFound(_))));
}

#[test]
fn list_ids_returns_only_well_formed_blobs() {
    let (store, _dir) = store();
    let id = store.put(b"x", "text/plain").unwrap();
    // Drop a junk file into a shard — it must be ignored.
    let shard = store.root().join(&id.as_str()[0..2]);
    std::fs::write(shard.join("not-a-blob.txt"), b"junk").unwrap();
    let ids = store.list_ids().unwrap();
    assert_eq!(ids, vec![id]);
}

#[test]
fn reconcile_flags_orphan_blobs_and_dangling_rows() {
    let (store, _dir) = store();
    let kept = store.put(b"kept", "text/plain").unwrap();
    let orphan = store.put(b"orphan", "text/plain").unwrap();

    let mut known = HashSet::new();
    known.insert(kept.as_str().to_owned());
    let dangling = "f".repeat(64);
    known.insert(dangling.clone());

    let report = reconcile(&store, &known).unwrap();
    assert_eq!(report.orphan_blobs, vec![orphan]);
    assert_eq!(report.dangling_rows, vec![AssetId::from_trusted(dangling)]);
}
