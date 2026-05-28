use mnemos_core::storage::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn migration_v7_adds_sync_tables() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(&tmp.path().join("v7.db")).await.unwrap();
    assert!(s.schema_version().await.unwrap() >= 7);
    let conn = s.conn().unwrap();
    conn.execute("INSERT INTO sync_conflicts (ts, path, detected_by, resolved_at) VALUES ('2026-05-28T00:00:00+00:00','foo.md','filesystem',NULL)", ()).await.unwrap();
    conn.execute(
        "UPDATE sync_state SET last_pushed_at = '2026-05-28T00:00:00+00:00' WHERE id = 1",
        (),
    )
    .await
    .unwrap();
    let mut rows = conn
        .query("SELECT last_pushed_at FROM sync_state WHERE id = 1", ())
        .await
        .unwrap();
    let r = rows.next().await.unwrap().unwrap();
    let v: Option<String> = r.get(0).unwrap();
    assert_eq!(v.as_deref(), Some("2026-05-28T00:00:00+00:00"));
}
