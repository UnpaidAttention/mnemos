use mnemos_core::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn opens_fresh_db_and_reports_schema_version() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let storage = Storage::open(&db_path).await.unwrap();
    assert!(db_path.exists());
    assert_eq!(storage.schema_version().await.unwrap(), 9);
}

#[tokio::test]
async fn reopening_existing_db_does_not_double_migrate() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    {
        let s = Storage::open(&db_path).await.unwrap();
        assert_eq!(s.schema_version().await.unwrap(), 9);
    }
    {
        let s = Storage::open(&db_path).await.unwrap();
        assert_eq!(s.schema_version().await.unwrap(), 9);
    }
}
