use chrono::Utc;
use mnemos_core::storage::memory_ops::{get_memory, insert_memory, supersede_memory};
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::{id::new_memory_id, Storage, Tier};
use tempfile::TempDir;

#[tokio::test]
async fn supersede_invalidates_old_and_links_new() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("bt.db")).await.unwrap();

    let old = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "User uses Vue".into(),
        "User said they prefer Vue.".into(),
    );
    insert_memory(&storage, &old, "/tmp/old.md", "h1")
        .await
        .unwrap();

    let mut new = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "User uses React".into(),
        "User now prefers React.".into(),
    );
    new.valid_at = Utc::now();
    insert_memory(&storage, &new, "/tmp/new.md", "h2")
        .await
        .unwrap();

    supersede_memory(&storage, &old.id, &new.id, new.valid_at)
        .await
        .unwrap();

    let old_loaded = get_memory(&storage, &old.id).await.unwrap();
    assert!(
        old_loaded.invalid_at.is_some(),
        "old memory should be invalidated"
    );
    assert_eq!(old_loaded.superseded_by.as_deref(), Some(new.id.as_str()));

    let conn = storage.conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM memory_links WHERE source_id = ? AND target_id = ? AND kind = 'supersedes'",
            libsql::params![new.id.clone(), old.id.clone()],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let count: i64 = row.get(0).unwrap();
    assert_eq!(count, 1);
}
