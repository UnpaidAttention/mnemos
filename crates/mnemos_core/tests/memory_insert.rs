use mnemos_core::id::new_memory_id;
use mnemos_core::storage::memory_ops::{get_memory, insert_memory};
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::{Storage, Tier};
use tempfile::TempDir;

#[tokio::test]
async fn insert_then_get_roundtrips() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("t.db")).await.unwrap();

    let mut mem = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "Insert test".into(),
        "body content for insertion".into(),
    );
    mem.tags = vec!["tag-a".into(), "tag-b".into()];
    let file_path = format!("/tmp/{}.md", mem.id);
    let content_hash = "abc123".to_string();

    insert_memory(&storage, &mem, &file_path, &content_hash)
        .await
        .unwrap();

    let loaded = get_memory(&storage, &mem.id).await.unwrap();
    assert_eq!(loaded.id, mem.id);
    assert_eq!(loaded.title, mem.title);
    assert_eq!(loaded.body, mem.body);
    assert_eq!(loaded.tags, vec!["tag-a", "tag-b"]);
    assert_eq!(loaded.tier, Tier::Semantic);
}

#[tokio::test]
async fn insert_writes_to_fts() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("fts.db")).await.unwrap();

    let mem = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "Unique findable title".into(),
        "Distinctive body phrase about platypus".into(),
    );
    insert_memory(&storage, &mem, "/tmp/a.md", "h")
        .await
        .unwrap();

    let conn = storage.conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT memory_id FROM memory_fts WHERE memory_fts MATCH 'platypus'",
            (),
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap();
    assert!(row.is_some(), "FTS did not index the body");
}
