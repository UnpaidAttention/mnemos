use mnemos_core::providers::{mock::MockEmbedder, Embedder};
use mnemos_core::rebuild::rebuild_index;
use mnemos_core::storage::vec_ops::knn_memory;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::{paths::Paths, Tier};
use std::sync::Arc;
use tempfile::TempDir;

/// Calling `rebuild_index` on an already-populated vault must succeed cleanly.
///
/// Before the fix, every INSERT hit `UNIQUE constraint failed: memories.file_path`
/// because the function re-indexed without first truncating the existing rows.
#[tokio::test]
async fn rebuild_handles_populated_db_idempotently() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    {
        let vault = Vault::open(paths.clone()).await.unwrap();
        for i in 0..3 {
            vault
                .remember(
                    &format!("body {i}"),
                    RememberOpts {
                        title: Some(format!("Title {i}")),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
        }
    }
    // DB is still populated; rebuild must not fail with constraint errors.
    let stats = rebuild_index(&paths).await.unwrap();
    assert_eq!(stats.memories_indexed, 3);
    assert_eq!(stats.errors, 0);

    // Rows should still be queryable after the rebuild.
    let vault = Vault::open(paths.clone()).await.unwrap();
    let memories = vault
        .list(mnemos_core::storage::memory_ops::ListFilter::default())
        .await
        .unwrap();
    assert_eq!(memories.len(), 3);
}

#[tokio::test]
async fn rebuild_recreates_index_from_files() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());

    // Create three memories
    let ids = {
        let vault = Vault::open(paths.clone()).await.unwrap();
        let mut ids = vec![];
        for i in 0..3 {
            let id = vault
                .remember(
                    &format!("body {i}"),
                    RememberOpts {
                        title: Some(format!("Title {i}")),
                        tier: Tier::Semantic,
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            ids.push(id);
        }
        ids
    };

    // Wipe the DB; files remain
    tokio::fs::remove_file(&paths.db_path).await.unwrap();

    // Rebuild
    let stats = rebuild_index(&paths).await.unwrap();
    assert_eq!(stats.memories_indexed, 3);
    assert_eq!(stats.errors, 0);

    // Verify
    let vault = Vault::open(paths.clone()).await.unwrap();
    for id in &ids {
        let mem = vault.get(id).await.unwrap();
        assert!(mem.title.starts_with("Title "));
    }
}

#[tokio::test]
async fn rebuild_re_embeds_when_embedder_provided() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let emb: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));

    // Seed with embeddings
    let id = {
        let v = Vault::open_with_embedder(paths.clone(), Some(emb.clone()))
            .await
            .unwrap();
        v.remember(
            "body",
            RememberOpts {
                title: Some("t".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap()
    };

    // Wipe DB; rebuild with same embedder
    tokio::fs::remove_file(&paths.db_path).await.unwrap();
    let stats = mnemos_core::rebuild::rebuild_index_with_embedder(&paths, Some(emb.clone()))
        .await
        .unwrap();
    assert_eq!(stats.memories_indexed, 1);
    assert_eq!(stats.embeddings_indexed, 1);
    assert_eq!(stats.errors, 0);

    // KNN should find the memory after rebuild
    let v_after = Vault::open_with_embedder(paths.clone(), Some(emb.clone()))
        .await
        .unwrap();
    let q = emb.embed("body").await.unwrap();
    let hits = knn_memory(v_after.storage(), &q, 1).await.unwrap();
    assert_eq!(hits[0].memory_id, id);
}

#[tokio::test]
async fn rebuild_without_embedder_skips_embeddings() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let emb: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));

    {
        let v = Vault::open_with_embedder(paths.clone(), Some(emb.clone()))
            .await
            .unwrap();
        let _ = v
            .remember(
                "body",
                RememberOpts {
                    title: Some("t".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }
    tokio::fs::remove_file(&paths.db_path).await.unwrap();

    let stats = mnemos_core::rebuild::rebuild_index_with_embedder(&paths, None)
        .await
        .unwrap();
    assert_eq!(stats.memories_indexed, 1);
    assert_eq!(stats.embeddings_indexed, 0);
}
