//! Regression: a fresh vault must create its `vec0` tables at the configured
//! embedder's dimension, not the legacy hardcoded 768.
//!
//! v0.8.0 made the bundled embedder (384-dim) the default. Before this fix the
//! static v2 migration created `memory_vec`/`chunk_vec` at `FLOAT[768]`, so the
//! very first `remember` on a fresh vault failed with:
//!   "Dimension mismatch ... Expected 768 dimensions but received 384".
//! Every existing test used `MockEmbedder::new(768)`, which masked the bug.

use mnemos_core::paths::Paths;
use mnemos_core::providers::{mock::MockEmbedder, Embedder};
use mnemos_core::storage::vec_ops::knn_memory;
use mnemos_core::vault::{RememberOpts, Vault};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn fresh_vault_with_384_dim_embedder_can_remember_and_recall() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(384));
    let vault = Vault::open_with_embedder(paths, Some(embedder.clone()))
        .await
        .unwrap();

    let id = vault
        .remember(
            "the bundled embedder produces 384-dimensional vectors",
            RememberOpts {
                title: Some("dim".into()),
                ..Default::default()
            },
        )
        .await
        .expect("remember must succeed at the embedder's native dim");

    let query = embedder
        .embed("the bundled embedder produces 384-dimensional vectors")
        .await
        .unwrap();
    let hits = knn_memory(vault.storage(), &query, 1).await.unwrap();
    assert_eq!(
        hits.len(),
        1,
        "the stored 384-dim vector must be recallable"
    );
    assert_eq!(hits[0].memory_id, id);
}

/// Both vec0 tables must be declared at the embedder's dim on a fresh vault.
#[tokio::test]
async fn fresh_vault_vec_tables_match_embedder_dim() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(384));
    let vault = Vault::open_with_embedder(paths, Some(embedder))
        .await
        .unwrap();

    for table in ["memory_vec", "chunk_vec"] {
        let conn = vault.storage().conn().unwrap();
        let mut rows = conn
            .query(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name=?",
                libsql::params![table],
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().expect("table exists");
        let sql: String = row.get(0).unwrap();
        assert!(
            sql.contains("FLOAT[384]"),
            "{table} should be declared FLOAT[384], got: {sql}"
        );
    }
}
