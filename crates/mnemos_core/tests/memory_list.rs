use mnemos_core::storage::memory_ops::{insert_memory, list_memories, ListFilter};
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::{id::new_memory_id, Storage, Tier};
use tempfile::TempDir;

fn tagged_memory(tier: Tier, tags: Vec<String>) -> Memory {
    let mut m = Memory::new_now(
        new_memory_id(),
        tier,
        MemoryType::Fact,
        "tagged".into(),
        "tagged body".into(),
    );
    m.tags = tags;
    m
}

/// P2-11: required_tags filter pushes the tag check into SQL rather than
/// loading the whole tier and filtering in Rust.
#[tokio::test]
async fn list_required_tags_filters_in_sql() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("tag.db")).await.unwrap();

    let hardened = tagged_memory(Tier::Reflection, vec!["mnemos:hardened".into()]);
    let unhardened = tagged_memory(
        Tier::Reflection,
        vec!["something-else".into(), "other".into()],
    );
    let both = tagged_memory(
        Tier::Reflection,
        vec!["mnemos:hardened".into(), "extra-tag".into()],
    );

    for m in [&hardened, &unhardened, &both] {
        insert_memory(&storage, m, &format!("/tmp/{}.md", m.id), "h")
            .await
            .unwrap();
    }

    // Filter for the "mnemos:hardened" tag — must return hardened + both,
    // not unhardened.
    let results = list_memories(
        &storage,
        ListFilter {
            tiers: Some(vec![Tier::Reflection]),
            required_tags: vec!["mnemos:hardened".to_owned()],
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let ids: Vec<&str> = results.iter().map(|m| m.id.as_str()).collect();
    assert!(
        ids.contains(&hardened.id.as_str()),
        "hardened memory must be returned; ids={ids:?}"
    );
    assert!(
        ids.contains(&both.id.as_str()),
        "memory with hardened + extra tag must be returned; ids={ids:?}"
    );
    assert!(
        !ids.contains(&unhardened.id.as_str()),
        "unhardened memory must NOT be returned; ids={ids:?}"
    );
    assert_eq!(
        results.len(),
        2,
        "exactly 2 hardened memories; got: {ids:?}"
    );
}

/// P2-11: multiple required_tags must all be present (AND semantics).
#[tokio::test]
async fn list_required_tags_and_semantics() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("tag_and.db")).await.unwrap();

    let both_tags = tagged_memory(Tier::Semantic, vec!["tag-a".into(), "tag-b".into()]);
    let only_a = tagged_memory(Tier::Semantic, vec!["tag-a".into()]);
    let only_b = tagged_memory(Tier::Semantic, vec!["tag-b".into()]);

    for m in [&both_tags, &only_a, &only_b] {
        insert_memory(&storage, m, &format!("/tmp/{}.md", m.id), "h")
            .await
            .unwrap();
    }

    let results = list_memories(
        &storage,
        ListFilter {
            required_tags: vec!["tag-a".to_owned(), "tag-b".to_owned()],
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(
        results.len(),
        1,
        "only the memory with both tags must be returned"
    );
    assert_eq!(results[0].id, both_tags.id);
}

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
