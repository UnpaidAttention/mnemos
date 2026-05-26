use mnemos_core::paths::Paths;
use mnemos_core::providers::{mock::MockEmbedder, Embedder};
use mnemos_core::storage::vec_ops::knn_memory;
use mnemos_core::vault::{RememberOpts, Vault};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn backfill_embeds_memories_inserted_without_embedder() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());

    // Phase 1: vault with NO embedder; insert 3 memories.
    let ids = {
        let v = Vault::open(paths.clone()).await.unwrap();
        let mut ids = vec![];
        for i in 0..3 {
            ids.push(
                v.remember(
                    &format!("body {i}"),
                    RememberOpts {
                        title: Some(format!("t{i}")),
                        ..Default::default()
                    },
                )
                .await
                .unwrap(),
            );
        }
        ids
    };

    // Phase 2: reopen with an embedder, run backfill.
    let emb: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let vault = Vault::open_with_embedder(paths, Some(emb.clone()))
        .await
        .unwrap();
    let stats = vault.backfill_embeddings(8).await.unwrap();
    assert_eq!(stats.embedded, 3);
    assert_eq!(stats.skipped, 0);
    assert_eq!(stats.errors, 0);

    // Verify by KNN.
    for (i, id) in ids.iter().enumerate() {
        let q = emb.embed(&format!("body {i}")).await.unwrap();
        let hits = knn_memory(vault.storage(), &q, 1).await.unwrap();
        assert_eq!(hits[0].memory_id, *id);
    }
}

#[tokio::test]
async fn backfill_skips_already_embedded() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let emb: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let vault = Vault::open_with_embedder(paths, Some(emb.clone()))
        .await
        .unwrap();
    vault
        .remember(
            "already embedded",
            RememberOpts {
                title: Some("x".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let stats = vault.backfill_embeddings(8).await.unwrap();
    assert_eq!(stats.embedded, 0);
    assert_eq!(stats.skipped, 1);
}

#[tokio::test]
async fn backfill_no_embedder_fails_gracefully() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap(); // no embedder
    let result = vault.backfill_embeddings(8).await;
    assert!(result.is_err(), "backfill without an embedder should error");
}
