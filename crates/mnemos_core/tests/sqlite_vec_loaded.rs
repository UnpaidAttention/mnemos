use mnemos_core::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn vec_extension_is_loaded() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("vec.db")).await.unwrap();
    let conn = storage.conn().unwrap();
    let mut rows = conn.query("SELECT vec_version()", ()).await.unwrap();
    let row = rows
        .next()
        .await
        .unwrap()
        .expect("vec_version() should return a row");
    let v: String = row.get(0).unwrap();
    assert!(
        v.starts_with('v'),
        "expected version string like 'v0.1.x', got {v:?}"
    );
}

#[tokio::test]
async fn vec0_virtual_table_creatable() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("vec.db")).await.unwrap();
    let conn = storage.conn().unwrap();
    conn.execute("CREATE VIRTUAL TABLE t USING vec0(emb FLOAT[8])", ())
        .await
        .expect("vec0 virtual table creation");
}
