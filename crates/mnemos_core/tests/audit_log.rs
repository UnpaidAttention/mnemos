use mnemos_core::storage::audit::{list_audit, write_audit};
use mnemos_core::Storage;
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn write_audit_entry_and_list_it() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("a.db")).await.unwrap();

    write_audit(
        &storage,
        "mnemos-cli",
        "create",
        Some("mem_X"),
        Some(json!({"title": "test"})),
    )
    .await
    .unwrap();

    let entries = list_audit(&storage, Some("mem_X")).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].action, "create");
    assert_eq!(entries[0].actor, "mnemos-cli");
}

#[tokio::test]
async fn audit_log_rejects_update() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("a.db")).await.unwrap();

    write_audit(&storage, "cli", "create", Some("mem_X"), None)
        .await
        .unwrap();

    let conn = storage.conn().unwrap();
    let result = conn
        .execute(
            "UPDATE audit_log SET action = 'tampered' WHERE memory_id = 'mem_X'",
            (),
        )
        .await;
    assert!(
        result.is_err(),
        "audit_log UPDATE should be blocked by trigger"
    );
}

#[tokio::test]
async fn audit_log_rejects_delete() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("a.db")).await.unwrap();

    write_audit(&storage, "cli", "create", Some("mem_X"), None)
        .await
        .unwrap();

    let conn = storage.conn().unwrap();
    let result = conn
        .execute("DELETE FROM audit_log WHERE memory_id = 'mem_X'", ())
        .await;
    assert!(
        result.is_err(),
        "audit_log DELETE should be blocked by trigger"
    );
}
