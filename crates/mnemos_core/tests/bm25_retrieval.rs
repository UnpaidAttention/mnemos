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
    mnemos_core::storage::memory_ops::soft_invalidate(&storage, &id, chrono::Utc::now())
        .await
        .unwrap();

    let hits = bm25_recall(&storage, "vue", RecallOpts::default())
        .await
        .unwrap();
    assert!(
        hits.iter().all(|h| h.memory.id != id),
        "invalidated memory should be hidden"
    );

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
    assert!(hits_all.iter().any(|h| h.memory.id == id));
}
