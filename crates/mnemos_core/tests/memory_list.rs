use mnemos_core::storage::memory_ops::{insert_memory, list_memories, ListFilter};
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::{id::new_memory_id, Storage, Tier};
use tempfile::TempDir;

#[tokio::test]
async fn list_filters_by_tier_and_invalidation() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("l.db")).await.unwrap();

    let a = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "A".into(),
        "body A".into(),
    );
    let b = Memory::new_now(
        new_memory_id(),
        Tier::Working,
        MemoryType::Identity,
        "B".into(),
        "body B".into(),
    );
    let c = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "C".into(),
        "body C".into(),
    );
    insert_memory(&storage, &a, "/tmp/a.md", "h").await.unwrap();
    insert_memory(&storage, &b, "/tmp/b.md", "h").await.unwrap();
    insert_memory(&storage, &c, "/tmp/c.md", "h").await.unwrap();

    let all = list_memories(&storage, ListFilter::default())
        .await
        .unwrap();
    assert_eq!(all.len(), 3);

    let semantic = list_memories(
        &storage,
        ListFilter {
            tiers: Some(vec![Tier::Semantic]),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(semantic.len(), 2);

    mnemos_core::storage::memory_ops::soft_invalidate(&storage, &a.id, chrono::Utc::now())
        .await
        .unwrap();
    let valid_only = list_memories(
        &storage,
        ListFilter {
            tiers: Some(vec![Tier::Semantic]),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        valid_only.len(),
        1,
        "soft-invalidated memory should be hidden by default"
    );

    let incl_invalid = list_memories(
        &storage,
        ListFilter {
            tiers: Some(vec![Tier::Semantic]),
            include_invalid: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(incl_invalid.len(), 2);
}
