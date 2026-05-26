use async_trait::async_trait;
use mnemos_core::error::Result;
use mnemos_core::paths::Paths;
use mnemos_core::providers::{mock::MockEmbedder, Embedder, Reranker};
use mnemos_core::retrieval::{
    hybrid::hybrid_recall, hybrid::hybrid_recall_with_rerank, RecallOpts,
};
use mnemos_core::vault::{RememberOpts, Vault};
use std::sync::Arc;
use tempfile::TempDir;

async fn fixture() -> (TempDir, Vault, Arc<dyn Embedder>) {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let emb: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let vault = Vault::open_with_embedder(paths, Some(emb.clone()))
        .await
        .unwrap();
    (tmp, vault, emb)
}

#[tokio::test]
async fn hybrid_returns_results_when_either_retriever_matches() {
    let (_tmp, vault, emb) = fixture().await;
    let id_a = vault
        .remember(
            "Tauri is the desktop UI we picked",
            RememberOpts {
                title: Some("Tauri choice".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let _ = vault
        .remember(
            "React is a popular JS framework",
            RememberOpts {
                title: Some("React".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let hits = hybrid_recall(
        vault.storage(),
        Some(emb.as_ref()),
        "tauri",
        RecallOpts::default(),
    )
    .await
    .unwrap();
    assert!(!hits.is_empty(), "hybrid should return results");
    assert_eq!(hits[0].memory.id, id_a);
}

#[tokio::test]
async fn hybrid_explain_is_populated_when_requested() {
    let (_tmp, vault, emb) = fixture().await;
    let _ = vault
        .remember(
            "hello world",
            RememberOpts {
                title: Some("h".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let opts = RecallOpts {
        explain: true,
        ..Default::default()
    };
    let hits = hybrid_recall(vault.storage(), Some(emb.as_ref()), "hello", opts)
        .await
        .unwrap();
    assert!(!hits.is_empty());
    let e = hits[0]
        .explain
        .as_ref()
        .expect("explain should be set when requested");
    assert!(e.rrf_score > 0.0);
    assert!(e.weight_strength > 0.0);
    assert!(e.weight_tier > 0.0);
    assert!(e.final_score > 0.0);
}

#[tokio::test]
async fn hybrid_works_without_embedder_falling_back_to_bm25() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap(); // no embedder
    let id = vault
        .remember(
            "findable phrase",
            RememberOpts {
                title: Some("findable".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let hits = hybrid_recall(vault.storage(), None, "findable", RecallOpts::default())
        .await
        .unwrap();
    assert!(
        hits.iter().any(|h| h.memory.id == id),
        "hybrid should still find via BM25 even with no embedder"
    );
}

#[tokio::test]
async fn hybrid_respects_k_limit() {
    let (_tmp, vault, emb) = fixture().await;
    for i in 0..10 {
        vault
            .remember(
                &format!("item {i}"),
                RememberOpts {
                    title: Some(format!("Item {i}")),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }
    let opts = RecallOpts {
        k: 3,
        ..Default::default()
    };
    let hits = hybrid_recall(vault.storage(), Some(emb.as_ref()), "item", opts)
        .await
        .unwrap();
    assert!(hits.len() <= 3);
}

/// Reranker that returns one fewer score than asked for.
struct BrokenReranker;

#[async_trait]
impl Reranker for BrokenReranker {
    async fn rerank(&self, _q: &str, candidates: &[String]) -> Result<Vec<f32>> {
        // Return scores of len = candidates.len() - 1 (or 0 if candidates is empty).
        Ok(vec![1.0_f32; candidates.len().saturating_sub(1)])
    }
}

#[tokio::test]
async fn rerank_score_count_mismatch_errors() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let emb: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let vault = Vault::open_with_embedder(paths, Some(emb.clone()))
        .await
        .unwrap();
    for i in 0..3 {
        vault
            .remember(
                &format!("m{i}"),
                RememberOpts {
                    title: Some(format!("t{i}")),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }
    let reranker: Arc<dyn Reranker> = Arc::new(BrokenReranker);
    let opts = RecallOpts {
        explain: true,
        rerank: true,
        k: 3,
        ..Default::default()
    };
    let result = hybrid_recall_with_rerank(
        vault.storage(),
        Some(emb.as_ref()),
        Some(reranker.as_ref()),
        "m",
        opts,
    )
    .await;
    assert!(result.is_err(), "mismatched score count should error");
}
