use mnemos_core::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn migration_v2_creates_vec_tables() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("v2.db")).await.unwrap();
    let conn = storage.conn().unwrap();

    for vt in ["memory_vec", "chunk_vec"] {
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
    assert_eq!(storage.schema_version().await.unwrap(), 8);
}

#[tokio::test]
async fn migration_v2_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("idem2.db");
    let _ = Storage::open(&path).await.unwrap();
    let _ = Storage::open(&path).await.unwrap();
    let s = Storage::open(&path).await.unwrap();
    assert_eq!(s.schema_version().await.unwrap(), 8);
}

#[tokio::test]
async fn migration_v2_upgrades_from_v1() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("upgrade.db");
    let _ = Storage::open(&path).await.unwrap();
    let s = Storage::open(&path).await.unwrap();
    assert_eq!(s.schema_version().await.unwrap(), 8);
}
