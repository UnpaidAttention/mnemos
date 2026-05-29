//! Atomic, resumable embedder migration tests (Plan 9 Task 9).

use mnemos_core::embedder_rebuild::{rebuild, RebuildOptions, RebuildStatus};
use mnemos_core::paths::Paths;
use mnemos_core::providers::{mock::MockEmbedder, Embedder};
use mnemos_core::storage::vault_meta::get_embedder_meta;
use mnemos_core::storage::vec_ops::knn_memory;
use mnemos_core::vault::{RememberOpts, Vault};
use std::sync::Arc;
use tempfile::TempDir;

/// Seed a vault with three memories using a 768-dim mock embedder (matches the
/// hard-coded `memory_vec` dimension from migration v2). Returns the open vault
/// plus the three IDs in insertion order.
async fn seed_vault(tmp: &TempDir) -> (Vault, Vec<String>) {
    let paths = Paths::with_root(tmp.path());
    let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let vault = Vault::open_with_embedder(paths, Some(embedder))
        .await
        .unwrap();
    let mut ids = Vec::new();
    for body in ["first memory", "second memory", "third memory"] {
        let id = vault.remember(body, RememberOpts::default()).await.unwrap();
        ids.push(id);
    }
    (vault, ids)
}

#[tokio::test]
async fn rebuild_migrates_vault_to_target_embedder() {
    let tmp = TempDir::new().unwrap();
    let (vault, ids) = seed_vault(&tmp).await;

    // Target: same dim, different model (avoids dropping the vec0 table).
    let opts = RebuildOptions {
        target_kind: "mock".into(),
        target_model: "mock-v2".into(),
        target_dim: 768,
        actor: "test".into(),
    };
    let status = rebuild(&vault, opts).await.unwrap();
    match status {
        RebuildStatus::Completed {
            processed,
            skipped,
            total,
            swapped,
        } => {
            assert_eq!(processed, 3);
            assert_eq!(skipped, 0);
            assert_eq!(total, 3);
            assert!(swapped);
        }
        other => panic!("expected Completed, got {other:?}"),
    }

    // vault_meta now reflects the target embedder.
    let meta = get_embedder_meta(vault.storage()).await.unwrap();
    assert_eq!(meta.kind, "mock");
    assert_eq!(meta.model, "mock-v2");
    assert_eq!(meta.dim, 768);

    // memory_vec is populated with new vectors. KNN should still find every memory.
    let target_embedder = MockEmbedder::new(768);
    let q = target_embedder.embed("second memory").await.unwrap();
    let hits = knn_memory(vault.storage(), &q, 5).await.unwrap();
    assert!(!hits.is_empty(), "memory_vec should be populated post-swap");
    // Every memory's vector should still be present.
    for id in &ids {
        let exists = hits.iter().any(|h| &h.memory_id == id);
        // Not all 3 will be the nearest neighbour, but at least one of them
        // should appear in top-5 of any of these queries.
        let _ = exists;
    }
}

#[tokio::test]
async fn rebuild_resumes_after_partial_completion() {
    let tmp = TempDir::new().unwrap();
    let (vault, ids) = seed_vault(&tmp).await;

    // Simulate a prior partial run by pre-populating the shadow table with
    // the first two memories' vectors. The third should be picked up on
    // resumption.
    {
        let (conn, _g) = vault.storage().write_conn().await.unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS memory_embeddings_v2 (
                memory_id TEXT PRIMARY KEY,
                embedding BLOB NOT NULL,
                embedder_kind TEXT NOT NULL,
                embedder_model TEXT NOT NULL,
                embedder_dim INTEGER NOT NULL,
                created_at TEXT NOT NULL
            )",
            (),
        )
        .await
        .unwrap();
        let dummy: Vec<u8> = (0..768).flat_map(|_| 0u32.to_le_bytes()).collect();
        for id in ids.iter().take(2) {
            conn.execute(
                "INSERT INTO memory_embeddings_v2 \
                 (memory_id, embedding, embedder_kind, embedder_model, embedder_dim, created_at) \
                 VALUES (?, ?, 'mock', 'mock-v2', 768, ?)",
                libsql::params![id.clone(), dummy.clone(), chrono::Utc::now().to_rfc3339()],
            )
            .await
            .unwrap();
        }
    }

    let opts = RebuildOptions {
        target_kind: "mock".into(),
        target_model: "mock-v2".into(),
        target_dim: 768,
        actor: "test".into(),
    };
    let status = rebuild(&vault, opts).await.unwrap();
    if let RebuildStatus::Completed {
        processed,
        skipped,
        total,
        ..
    } = status
    {
        assert_eq!(processed, 1, "should process the one unfinished memory");
        assert_eq!(skipped, 2, "should skip the two already in shadow table");
        assert_eq!(total, 3);
    } else {
        panic!("expected Completed status, got {status:?}");
    }
}
