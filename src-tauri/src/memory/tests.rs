//! Phase 21 gate coverage — repo round-trips + FTS5 sync + scope isolation +
//! prune-at-cap + restart. Extraction / retrieval / builder-budget tests are
//! added with their steps.

use std::sync::Arc;

use super::extract::{validate, Exchange, MemoryCandidate};
use super::repo::{MemoryRepo, NewMemory};
use super::retrieve::fts_query_for;
use super::MemoryScope;
use crate::contracts::ids::{ConversationId, PersonaId};
use crate::contracts::memory::MemoryKind;
use crate::db::Db;

async fn repo() -> (MemoryRepo, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(&tmp.path().join("m.db")).await.unwrap();
    db.migrate().await.unwrap();
    (MemoryRepo::new(Arc::new(db)), tmp)
}

fn mem(scope: &str, content: &str, importance: u32) -> NewMemory {
    NewMemory {
        scope: scope.to_owned(),
        kind: MemoryKind::Fact,
        content: content.to_owned(),
        importance,
        source_conversation_id: None,
        source_message_id: None,
    }
}

const CAP: u32 = 500;

#[test]
fn scope_key_format() {
    let s = MemoryScope::Persona(PersonaId::from_trusted("p-42"));
    assert_eq!(s.as_key(), "persona:p-42");
}

#[tokio::test]
async fn insert_list_get_delete_round_trip() {
    let (repo, _tmp) = repo().await;
    let id = repo
        .insert(mem("persona:a", "keeps bees on the roof", 4), CAP)
        .await
        .unwrap();

    let got = repo.get(&id).await.unwrap();
    assert_eq!(got.content, "keeps bees on the roof");
    assert_eq!(got.importance, 4);

    assert_eq!(repo.list("persona:a").await.unwrap().len(), 1);
    assert_eq!(repo.count("persona:a").await.unwrap(), 1);

    repo.delete(&id).await.unwrap();
    assert!(matches!(
        repo.get(&id).await,
        Err(crate::ipc::AppError::NotFound(_))
    ));
    // FTS row is gone too — a search that would have matched returns nothing.
    let q = fts_query_for("bees roof").unwrap();
    assert!(repo.search("persona:a", &q, 8).await.unwrap().is_empty());
}

#[tokio::test]
async fn search_is_bm25_ordered_and_scope_isolated() {
    let (repo, _tmp) = repo().await;
    repo.insert(
        mem(
            "persona:a",
            "the user has a golden retriever named Biscuit",
            4,
        ),
        CAP,
    )
    .await
    .unwrap();
    repo.insert(
        mem("persona:a", "the user is allergic to shellfish", 4),
        CAP,
    )
    .await
    .unwrap();
    repo.insert(
        mem(
            "persona:b",
            "this persona should never surface for A: dog dog dog",
            5,
        ),
        CAP,
    )
    .await
    .unwrap();

    let q = fts_query_for("remind me about the golden retriever").unwrap();
    let hits = repo.search("persona:a", &q, 8).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].memory.content.contains("golden retriever"));
    // ranks are negative; the match is real
    assert!(hits[0].rank < 0.0);

    // Scope isolation — A's query never returns B's memory.
    assert!(hits
        .iter()
        .all(|s| !s.memory.content.contains("never surface")));
}

#[tokio::test]
async fn empty_query_returns_nothing() {
    let (repo, _tmp) = repo().await;
    repo.insert(mem("persona:a", "something", 4), CAP)
        .await
        .unwrap();
    assert!(repo.search("persona:a", "  ", 8).await.unwrap().is_empty());
    assert!(repo.search("persona:a", "", 1).await.unwrap().is_empty());
}

#[tokio::test]
async fn insert_at_cap_drops_the_lowest_priority_row() {
    let (repo, _tmp) = repo().await;
    let cap = 3;
    let low = repo
        .insert(mem("persona:a", "least important note", 1), cap)
        .await
        .unwrap();
    repo.insert(mem("persona:a", "middling note", 3), cap)
        .await
        .unwrap();
    repo.insert(mem("persona:a", "important note", 5), cap)
        .await
        .unwrap();
    assert_eq!(repo.count("persona:a").await.unwrap(), 3);

    // One more → the importance-1 row is evicted, not the new one.
    repo.insert(mem("persona:a", "brand new note", 4), cap)
        .await
        .unwrap();
    assert_eq!(repo.count("persona:a").await.unwrap(), 3);
    assert!(matches!(
        repo.get(&low).await,
        Err(crate::ipc::AppError::NotFound(_))
    ));
    let contents: Vec<_> = repo
        .list("persona:a")
        .await
        .unwrap()
        .into_iter()
        .map(|m| m.content)
        .collect();
    assert!(contents.contains(&"brand new note".to_owned()));
    assert!(!contents.contains(&"least important note".to_owned()));
    // Its FTS row went too.
    let q = fts_query_for("least important").unwrap();
    assert!(repo
        .search("persona:a", &q, 8)
        .await
        .unwrap()
        .iter()
        .all(|s| !s.memory.content.contains("least important")));
}

// ---------------------------------------------------------------- validation gate

fn candidate(content: &str, importance: u32) -> MemoryCandidate {
    MemoryCandidate {
        content: content.to_owned(),
        kind: MemoryKind::Fact,
        importance,
    }
}

fn exchange() -> Exchange {
    Exchange {
        conversation_id: ConversationId::from_trusted("c-1"),
        assistant_message_id: Some("m-1".to_owned()),
        user_text: "u".to_owned(),
        assistant_text: "a".to_owned(),
    }
}

#[tokio::test]
async fn validate_enforces_importance_length_and_dedup() {
    let (repo, _tmp) = repo().await;

    // Below the importance floor → rejected.
    assert!(validate(
        &candidate("a real fact about work", 2),
        &repo,
        "persona:a",
        &exchange()
    )
    .await
    .is_none());
    // Too short → rejected.
    assert!(
        validate(&candidate("ok", 4), &repo, "persona:a", &exchange())
            .await
            .is_none()
    );
    // Too long → rejected.
    let long = "x".repeat(super::MAX_CONTENT_CHARS + 1);
    assert!(
        validate(&candidate(&long, 4), &repo, "persona:a", &exchange())
            .await
            .is_none()
    );

    // A good one → a row with provenance + control tokens stripped.
    let row = validate(
        &candidate("the user works as a marine biologist <|im_end|>", 4),
        &repo,
        "persona:a",
        &exchange(),
    )
    .await
    .expect("valid");
    assert!(!row.content.contains("<|"));
    assert_eq!(row.source_conversation_id.as_ref().unwrap().as_str(), "c-1");
    assert_eq!(row.source_message_id.as_deref(), Some("m-1"));
    repo.insert(row, CAP).await.unwrap();

    // A near-duplicate of what we just stored → rejected by the FTS dedup probe.
    assert!(validate(
        &candidate("user works as a marine biologist", 4),
        &repo,
        "persona:a",
        &exchange()
    )
    .await
    .is_none());

    // An unrelated fact in the same scope → accepted.
    assert!(validate(
        &candidate("the user is scared of thunderstorms", 4),
        &repo,
        "persona:a",
        &exchange()
    )
    .await
    .is_some());
}

#[tokio::test]
async fn memories_survive_a_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("m.db");
    {
        let db = Db::open(&path).await.unwrap();
        db.migrate().await.unwrap();
        let repo = MemoryRepo::new(Arc::new(db));
        repo.insert(mem("persona:a", "the user plays the cello", 4), CAP)
            .await
            .unwrap();
    }
    let db = Db::open(&path).await.unwrap();
    db.migrate().await.unwrap();
    let repo = MemoryRepo::new(Arc::new(db));
    let q = fts_query_for("does the user play any instrument like cello").unwrap();
    let hits = repo.search("persona:a", &q, 8).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].memory.content.contains("cello"));
}
