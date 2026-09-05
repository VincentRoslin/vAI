//! Phase 9 gate coverage: open + pragmas, forward-only idempotent migration,
//! transactions (commit / rollback / panic), the repository, concurrent writers,
//! restart, corruption detection, baseline timings.

use std::sync::Arc;
use std::time::Instant;

use tempfile::tempdir;

use super::{Db, DbError};

async fn fresh() -> (tempfile::TempDir, Db) {
    let dir = tempdir().unwrap();
    let db = Db::open(&dir.path().join("localai.db")).await.unwrap();
    db.migrate().await.unwrap();
    (dir, db)
}

#[tokio::test]
async fn opens_and_sets_pragmas() {
    let (_dir, db) = fresh().await;
    let (journal, fk): (String, i64) = db
        .read(|c| {
            Ok((
                c.query_row("PRAGMA journal_mode", [], |r| r.get(0))?,
                c.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?,
            ))
        })
        .await
        .unwrap();
    assert_eq!(journal.to_lowercase(), "wal");
    assert_eq!(fk, 1);

    // A reader connection is query_only.
    let write_via_reader = db
        .read(|c| {
            c.execute("CREATE TABLE nope (x)", [])?;
            Ok(())
        })
        .await;
    assert!(write_via_reader.is_err(), "reader must reject writes");
}

#[tokio::test]
async fn migration_creates_the_schema_and_is_idempotent() {
    let (_dir, db) = fresh().await;

    let has_table: bool = db
        .read(|c| {
            Ok(c.query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='core_app_meta'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false))
        })
        .await
        .unwrap();
    assert!(has_table);

    // Second migrate: no-op, same version. The embedded set currently ends at
    // V0002 (the model registry).
    let v1 = db.migrate().await.unwrap();
    let v2 = db.migrate().await.unwrap();
    assert_eq!(v1, v2);
    assert!(v1 >= 1);
}

#[tokio::test]
async fn migration_takes_a_verified_backup() {
    let dir = tempdir().unwrap();
    let db = Db::open(&dir.path().join("localai.db")).await.unwrap();
    db.migrate().await.unwrap();

    let backups: Vec<_> = std::fs::read_dir(dir.path().join("db-backups"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("pre-migrate-"))
        .collect();
    assert_eq!(backups.len(), 1);
    assert!(backups[0].metadata().unwrap().len() > 0);
}

#[tokio::test]
async fn migration_refuses_a_newer_database() {
    let (_dir, db) = fresh().await;
    // Fake a future migration having been applied.
    db.write(|tx| {
        tx.execute(
            "INSERT INTO refinery_schema_history (version, name, applied_on, checksum)
             VALUES (999, 'future', '2999-01-01T00:00:00+00:00', '0')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let err = db.migrate().await.unwrap_err();
    assert!(matches!(err, DbError::Migration(_)));
}

#[tokio::test]
async fn transaction_commit_persists_and_error_rolls_back() {
    let (_dir, db) = fresh().await;

    db.app_meta().set("a", "1").await.unwrap();
    assert_eq!(db.app_meta().get("a").await.unwrap().as_deref(), Some("1"));

    // A closure that writes then returns Err must leave nothing.
    let result = db
        .write(|tx| {
            tx.execute(
                "INSERT INTO core_app_meta (key, value, updated_at) VALUES ('b', 'x', 'now')",
                [],
            )?;
            Err::<(), _>(DbError::Other("deliberate".into()))
        })
        .await;
    assert!(result.is_err());
    assert!(db.app_meta().get("b").await.unwrap().is_none());
}

#[tokio::test]
async fn transaction_panic_rolls_back_and_pool_recovers() {
    let (_dir, db) = fresh().await;

    let panicked = db
        .write(|tx| {
            tx.execute(
                "INSERT INTO core_app_meta (key, value, updated_at) VALUES ('p', 'x', 'now')",
                [],
            )?;
            panic!("boom inside the transaction");
            #[allow(unreachable_code)]
            Ok(())
        })
        .await;
    assert!(panicked.is_err());

    // The write that panicked left nothing, and the pool still works.
    assert!(db.app_meta().get("p").await.unwrap().is_none());
    db.app_meta().set("after", "ok").await.unwrap();
    assert_eq!(
        db.app_meta().get("after").await.unwrap().as_deref(),
        Some("ok")
    );
}

#[tokio::test]
async fn app_meta_upsert_updates_the_timestamp() {
    let (_dir, db) = fresh().await;
    db.app_meta().set("k", "v1").await.unwrap();
    let first: String = db
        .read(|c| {
            Ok(c.query_row(
                "SELECT updated_at FROM core_app_meta WHERE key='k'",
                [],
                |r| r.get(0),
            )?)
        })
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    db.app_meta().set("k", "v2").await.unwrap();
    let (value, second): (String, String) = db
        .read(|c| {
            Ok(c.query_row(
                "SELECT value, updated_at FROM core_app_meta WHERE key='k'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .await
        .unwrap();

    assert_eq!(value, "v2");
    assert_ne!(first, second);
    assert_eq!(db.app_meta().all().await.unwrap().len(), 1);
}

#[tokio::test]
async fn concurrent_writers_all_succeed() {
    let (_dir, db) = fresh().await;
    let db = Arc::new(db);

    let mut handles = Vec::new();
    for i in 0..24 {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            db.app_meta().set(&format!("key-{i}"), &i.to_string()).await
        }));
    }
    for h in handles {
        h.await.unwrap().unwrap();
    }
    assert_eq!(db.app_meta().all().await.unwrap().len(), 24);
}

#[tokio::test]
async fn data_survives_a_reopen() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("localai.db");
    {
        let db = Db::open(&path).await.unwrap();
        db.migrate().await.unwrap();
        db.app_meta().set("persist", "yes").await.unwrap();
        db.checkpoint_and_optimize().await;
    }
    let db = Db::open(&path).await.unwrap();
    db.migrate().await.unwrap();
    assert_eq!(
        db.app_meta().get("persist").await.unwrap().as_deref(),
        Some("yes")
    );
}

#[tokio::test]
async fn corrupt_file_is_rejected_on_open() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("localai.db");
    std::fs::write(&path, b"this is definitely not a sqlite database").unwrap();

    let err = Db::open(&path).await.unwrap_err();
    assert!(matches!(err, DbError::Corruption(_)));
}

#[tokio::test]
async fn baseline_timings() {
    let (_dir, db) = fresh().await;

    // Warm both pools so the numbers reflect query cost, not connection setup.
    db.app_meta().set("warm", "1").await.unwrap();
    let _ = db.app_meta().get("warm").await.unwrap();

    let t = Instant::now();
    db.app_meta().set("one", "1").await.unwrap();
    let insert = t.elapsed();

    let t = Instant::now();
    let _ = db.app_meta().get("one").await.unwrap();
    let read = t.elapsed();

    let t = Instant::now();
    db.write(|tx| {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO core_app_meta (key, value, updated_at) VALUES (?1, ?2, 'now')",
        )?;
        for i in 0..100 {
            stmt.execute(rusqlite::params![format!("row-{i}"), i.to_string()])?;
        }
        Ok(())
    })
    .await
    .unwrap();
    let batch = t.elapsed();

    println!("insert={insert:?} indexed_read={read:?} tx_100_rows={batch:?}");
    // "warm" + "one" + row-0..row-99
    assert_eq!(db.app_meta().all().await.unwrap().len(), 102);
}
