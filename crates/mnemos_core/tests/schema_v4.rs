use mnemos_core::storage::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn migration_v4_adds_processed_at() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("v4.db")).await.unwrap();
    assert!(storage.schema_version().await.unwrap() >= 4);

    let conn = storage.conn().unwrap();
    // Column exists and is queryable (NULL by default).
    conn.execute(
        "INSERT INTO sessions (id, started_at) VALUES ('sess_x', '2026-01-01T00:00:00+00:00')",
        (),
    )
    .await
    .unwrap();
    let mut rows = conn
        .query("SELECT processed_at FROM sessions WHERE id = 'sess_x'", ())
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    assert!(row.get::<Option<String>>(0).unwrap().is_none());
}
