use mnemos_core::paths::Paths;
use mnemos_core::providers::{mock::MockEmbedder, Embedder};
use mnemos_core::storage::vec_ops::knn_memory;
use mnemos_core::vault::{RememberOpts, Vault};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn vault_with_embedder_embeds_on_remember() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let vault = Vault::open_with_embedder(paths, Some(embedder.clone()))
        .await
        .unwrap();

    let id = vault
        .remember(
            "test body",
            RememberOpts {
                title: Some("t".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // The vector should be in memory_vec; KNN with the same text should find this row first.
    let query = embedder.embed("test body").await.unwrap();
    let hits = knn_memory(vault.storage(), &query, 1).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory_id, id);
}

#[tokio::test]
async fn vault_without_embedder_skips_vector_insert() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();

    let id = vault
        .remember(
            "body",
            RememberOpts {
                title: Some("t".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // No embedding written — KNN against any query should return nothing for this memory.
    let dummy_query = vec![0.0_f32; 768];
    let hits = knn_memory(vault.storage(), &dummy_query, 10).await.unwrap();
    assert!(hits.iter().all(|h| h.memory_id != id));
}

#[tokio::test]
async fn forget_removes_vector() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let vault = Vault::open_with_embedder(paths, Some(embedder.clone()))
        .await
        .unwrap();

    let id = vault
        .remember(
            "delete me",
            RememberOpts {
                title: Some("trash".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    vault.forget(&id, Some("test")).await.unwrap();

    let query = embedder.embed("delete me").await.unwrap();
    let hits = knn_memory(vault.storage(), &query, 10).await.unwrap();
    assert!(
        hits.iter().all(|h| h.memory_id != id),
        "forgotten memory's vector should be removed (or memory hidden)"
    );
}
