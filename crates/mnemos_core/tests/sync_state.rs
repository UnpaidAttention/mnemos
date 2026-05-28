use chrono::Utc;
use mnemos_core::storage::Storage;
use mnemos_core::sync::state::{
    list_unresolved_conflicts, record_conflict, record_pull, record_push, resolve_conflict,
};
use tempfile::TempDir;

#[tokio::test]
async fn sync_state_records_and_lists_conflicts() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(&tmp.path().join("sync.db")).await.unwrap();
    let now = Utc::now();

    record_push(&s, now, None).await.unwrap();
    record_pull(&s, now, Some("git remote unreachable"))
        .await
        .unwrap();
    let id = record_conflict(&s, "memories/mem_x.md", "filesystem", Some("Syncthing"))
        .await
        .unwrap();

    let open = list_unresolved_conflicts(&s).await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].path, "memories/mem_x.md");

    resolve_conflict(&s, id, now).await.unwrap();
    assert!(list_unresolved_conflicts(&s).await.unwrap().is_empty());
}
