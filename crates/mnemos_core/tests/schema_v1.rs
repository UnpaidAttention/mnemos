use mnemos_core::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn migration_v1_creates_all_expected_tables() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("v1.db")).await.unwrap();
    let conn = storage.conn().unwrap();

    let expected = [
        "memories",
        "chunks",
        "sessions",
        "entities",
        "entity_mentions",
        "entity_edges",
        "memory_links",
        "memory_chunks",
        "audit_log",
        "schema_migrations",
    ];
    for table in expected {
        let mut rows = conn
            .query(
                "SELECT name FROM sqlite_master WHERE type='table' AND name=?",
                libsql::params![table],
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap();
        assert!(row.is_some(), "missing table: {table}");
    }
}

#[tokio::test]
async fn migration_v1_creates_fts5_tables() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("fts.db")).await.unwrap();
    let conn = storage.conn().unwrap();

    for vt in ["memory_fts", "chunk_fts"] {
        let mut rows = conn
            .query(
                "SELECT name FROM sqlite_master WHERE name=?",
                libsql::params![vt],
            )
            .await
            .unwrap();
        assert!(
            rows.next().await.unwrap().is_some(),
            "missing virtual table: {vt}"
        );
    }
}

#[tokio::test]
async fn migration_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("idem.db");
    let _ = Storage::open(&path).await.unwrap();
    let _ = Storage::open(&path).await.unwrap();
    let s = Storage::open(&path).await.unwrap();
    // Schema version advances with each migration; v5 is now the latest.
    assert_eq!(s.schema_version().await.unwrap(), 5);
}
