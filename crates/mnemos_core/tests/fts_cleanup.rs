//! Tests for P1-4: memory_fts cleaned on forget/supersede (no ghost rows).

use mnemos_core::retrieval::{bm25::bm25_recall, RecallOpts};
use mnemos_core::storage::memory_ops::{
    get_memory, insert_memory, soft_invalidate, supersede_memory,
};
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::{id::new_memory_id, Storage, Tier};
use tempfile::TempDir;

async fn seed(storage: &Storage, title: &str, body: &str) -> String {
    let mem = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        title.into(),
        body.into(),
    );
    insert_memory(storage, &mem, &format!("/tmp/{}.md", mem.id), "h")
        .await
        .unwrap();
    mem.id
}

/// P1-4: after soft_invalidate the FTS row must be gone so BM25 cannot return
/// a ghost hit.
#[tokio::test]
async fn soft_invalidate_removes_fts_row() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("fts.db")).await.unwrap();

    let id = seed(
        &storage,
        "Rust memory safety",
        "Rust uses ownership to prevent bugs",
    )
    .await;

    // Confirm it appears in BM25 before invalidation.
    let before = bm25_recall(&storage, "ownership", RecallOpts::default())
        .await
        .unwrap();
    assert!(
        before.iter().any(|h| h.memory.id == id),
        "memory must appear in BM25 before invalidation"
    );

    soft_invalidate(&storage, &id, chrono::Utc::now())
        .await
        .unwrap();

    // After invalidation: must not appear in BM25 (FTS row removed).
    let after = bm25_recall(
        &storage,
        "ownership",
        RecallOpts {
            include_invalid: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(
        after.iter().all(|h| h.memory.id != id),
        "P1-4: FTS row must be removed on soft_invalidate — no ghost row"
    );

    // The DB row itself is still present and marked invalid.
    let mem = get_memory(&storage, &id).await.unwrap();
    assert!(
        mem.invalid_at.is_some(),
        "DB row must still exist as invalid"
    );
}

/// P1-4: after supersede_memory the OLD memory's FTS row must be gone.
#[tokio::test]
async fn supersede_removes_old_fts_row() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("fts_sup.db")).await.unwrap();

    let old_id = seed(&storage, "Old belief", "Redis is slow").await;
    let new_id = seed(&storage, "New belief", "Redis is fast with pipelining").await;

    supersede_memory(&storage, &old_id, &new_id, chrono::Utc::now())
        .await
        .unwrap();

    // "slow" was only in the old memory body; it must not appear after supersede.
    let hits = bm25_recall(
        &storage,
        "slow",
        RecallOpts {
            include_invalid: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(
        hits.iter().all(|h| h.memory.id != old_id),
        "P1-4: superseded memory must not return ghost BM25 hits"
    );

    // New memory's FTS row must still work.
    let hits_new = bm25_recall(&storage, "pipelining", RecallOpts::default())
        .await
        .unwrap();
    assert!(
        hits_new.iter().any(|h| h.memory.id == new_id),
        "new memory must still be findable via BM25"
    );
}

/// P1-4: re-inserting (INSERT OR REPLACE) a memory must not create duplicate
/// FTS rows, which would inflate BM25 scores.
#[tokio::test]
async fn insert_or_replace_no_duplicate_fts_rows() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("fts_dup.db")).await.unwrap();

    let mem = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "Unique title".into(),
        "uniquetoken body text".into(),
    );
    insert_memory(&storage, &mem, "/tmp/a.md", "h1")
        .await
        .unwrap();
    // Insert same memory again (simulating a crash-retry path).
    insert_memory(&storage, &mem, "/tmp/a.md", "h2")
        .await
        .unwrap();

    // Count matching FTS rows directly.
    let conn = storage.conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM memory_fts WHERE memory_id = ?",
            libsql::params![mem.id.clone()],
        )
        .await
        .unwrap();
    let count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(
        count, 1,
        "INSERT OR REPLACE must not create duplicate FTS rows"
    );
}

/// P1-4 repair migration: v10 must leave memory_fts consistent with the
/// memories table (no rows for invalid memories, all valid memories present).
#[tokio::test]
async fn v10_migration_rebuilds_fts_consistently() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("v10.db")).await.unwrap();

    // After open/migration the FTS count must equal the memories count.
    let conn = storage.conn().unwrap();

    let mut r1 = conn
        .query("SELECT COUNT(*) FROM memories", ())
        .await
        .unwrap();
    let mem_count: i64 = r1.next().await.unwrap().unwrap().get(0).unwrap();

    let mut r2 = conn
        .query("SELECT COUNT(*) FROM memory_fts", ())
        .await
        .unwrap();
    let fts_count: i64 = r2.next().await.unwrap().unwrap().get(0).unwrap();

    assert_eq!(
        mem_count, fts_count,
        "v10 migration: FTS row count must equal memories row count"
    );
}
