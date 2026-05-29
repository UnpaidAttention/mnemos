use mnemos_core::storage::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn migration_v9_adds_embedder_kind() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(&tmp.path().join("v9.db")).await.unwrap();
    assert!(s.schema_version().await.unwrap() >= 9);
    let conn = s.conn().unwrap();
    conn.execute(
        "UPDATE vault_meta SET embedder_kind = 'ollama' WHERE id = 1",
        (),
    )
    .await
    .unwrap();
    let mut rows = conn
        .query("SELECT embedder_kind FROM vault_meta WHERE id = 1", ())
        .await
        .unwrap();
    let r = rows.next().await.unwrap().unwrap();
    let kind: String = r.get(0).unwrap();
    assert_eq!(kind, "ollama");
}

#[tokio::test]
async fn fresh_vault_defaults_to_bundled() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(&tmp.path().join("v9b.db")).await.unwrap();
    let conn = s.conn().unwrap();
    let mut rows = conn
        .query("SELECT embedder_kind FROM vault_meta WHERE id = 1", ())
        .await
        .unwrap();
    let r = rows.next().await.unwrap().unwrap();
    let kind: String = r.get(0).unwrap();
    assert_eq!(kind, "bundled", "fresh vault should default to bundled");
}
