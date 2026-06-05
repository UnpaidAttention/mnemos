use mnemos_core::retrieval::{bm25::bm25_recall, RecallOpts};
use mnemos_core::storage::memory_ops::insert_memory;
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
    let path = format!("/tmp/{}.md", mem.id);
    insert_memory(storage, &mem, &path, "h").await.unwrap();
    mem.id
}

#[tokio::test]
async fn bm25_finds_distinct_terms() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("b.db")).await.unwrap();
    let id_a = seed(
        &storage,
        "Tauri preference",
        "User uses Tauri for desktop apps",
    )
    .await;
    let _id_b = seed(&storage, "React notes", "React is a JS framework").await;
    let _id_c = seed(&storage, "SQL trivia", "Postgres has window functions").await;

    let hits = bm25_recall(&storage, "tauri", RecallOpts::default())
        .await
        .unwrap();
    assert!(!hits.is_empty(), "expected at least one hit for 'tauri'");
    assert_eq!(hits[0].memory.id, id_a);
}

#[tokio::test]
async fn bm25_respects_tier_filter() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("b.db")).await.unwrap();

    // Insert a procedural memory and a semantic memory; both mention 'tdd'
    let proc_mem = mnemos_core::types::Memory::new_now(
        mnemos_core::id::new_memory_id(),
        mnemos_core::Tier::Procedural,
        mnemos_core::types::MemoryType::Rule,
        "Procedural rule".into(),
        "always use TDD".into(),
    );
    insert_memory(&storage, &proc_mem, "/tmp/p.md", "h")
        .await
        .unwrap();
    let id_sem = seed(&storage, "Semantic fact", "user prefers TDD methodology").await;

    let opts = RecallOpts {
        tiers: Some(vec![Tier::Semantic]),
        ..Default::default()
    };
    let hits = bm25_recall(&storage, "tdd", opts).await.unwrap();
    assert!(hits.iter().all(|h| h.memory.tier == Tier::Semantic));
    assert!(hits.iter().any(|h| h.memory.id == id_sem));
}

#[tokio::test]
async fn bm25_hides_invalidated_by_default() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("b.db")).await.unwrap();
    let id = seed(&storage, "Old belief", "User likes Vue").await;

    // P1-4: soft_invalidate now removes the FTS row in the same transaction,
    // so BM25 cannot find the memory anymore (no ghost rows).
    mnemos_core::storage::memory_ops::soft_invalidate(&storage, &id, chrono::Utc::now())
        .await
        .unwrap();

    // Default (include_invalid=false): must not appear.
    let hits = bm25_recall(&storage, "vue", RecallOpts::default())
        .await
        .unwrap();
    assert!(
        hits.iter().all(|h| h.memory.id != id),
        "invalidated memory should be hidden by default"
    );

    // include_invalid=true: also absent from BM25 because the FTS row was
    // removed on invalidation (P1-4). The memory is still in `memories` and
    // accessible via `list_memories(include_invalid=true)`; it is just not
    // searchable by BM25 (correct — ghost FTS rows degrade ranking).
    let hits_all = bm25_recall(
        &storage,
        "vue",
        RecallOpts {
            include_invalid: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(
        hits_all.iter().all(|h| h.memory.id != id),
        "P1-4: FTS row removed on invalidation — BM25 must not return ghost rows"
    );

    // The raw memories table still has the row (accessible by id).
    let mem = mnemos_core::storage::memory_ops::get_memory(&storage, &id)
        .await
        .unwrap();
    assert!(
        mem.invalid_at.is_some(),
        "row still exists and is marked invalid"
    );
}
