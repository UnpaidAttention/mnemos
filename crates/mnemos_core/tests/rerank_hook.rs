use async_trait::async_trait;
use mnemos_core::error::Result;
use mnemos_core::paths::Paths;
use mnemos_core::providers::Reranker;
use mnemos_core::providers::{mock::MockEmbedder, Embedder};
use mnemos_core::retrieval::{hybrid::hybrid_recall_with_rerank, RecallOpts};
use mnemos_core::vault::{RememberOpts, Vault};
use std::sync::Arc;
use tempfile::TempDir;

/// Test reranker: scores 1.0 for candidates containing the query token, 0.0 otherwise.
struct KeywordReranker;

#[async_trait]
impl Reranker for KeywordReranker {
    async fn rerank(&self, query: &str, candidates: &[String]) -> Result<Vec<f32>> {
        let q = query.to_lowercase();
        Ok(candidates
            .iter()
            .map(|c| {
                if c.to_lowercase().contains(&q) {
                    1.0
                } else {
                    0.0
                }
            })
            .collect())
    }
}

#[tokio::test]
async fn reranker_reorders_top_k() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let emb: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let vault = Vault::open_with_embedder(paths, Some(emb.clone()))
        .await
        .unwrap();
    let _ = vault
        .remember(
            "apples and oranges",
            RememberOpts {
                title: Some("a".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let id_match = vault
        .remember(
            "the special-marker is here",
            RememberOpts {
                title: Some("b".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let _ = vault
        .remember(
            "bananas and grapes",
            RememberOpts {
                title: Some("c".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let opts = RecallOpts {
        explain: true,
        rerank: true,
        ..Default::default()
    };
    let reranker: Arc<dyn Reranker> = Arc::new(KeywordReranker);
    let hits = hybrid_recall_with_rerank(
        vault.storage(),
        Some(emb.as_ref()),
        Some(reranker.as_ref()),
        "special-marker",
        opts,
    )
    .await
    .unwrap();

    assert!(!hits.is_empty());
    assert_eq!(hits[0].memory.id, id_match);
    let explain = hits[0].explain.as_ref().unwrap();
    assert!(explain.rerank_score.is_some());
    assert_eq!(explain.rerank_score, Some(1.0));
}

#[tokio::test]
async fn reranker_off_when_opts_rerank_false() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let emb: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let vault = Vault::open_with_embedder(paths, Some(emb.clone()))
        .await
        .unwrap();
    vault
        .remember(
            "body",
            RememberOpts {
                title: Some("t".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let reranker: Arc<dyn Reranker> = Arc::new(KeywordReranker);
    let opts = RecallOpts {
        explain: true,
        rerank: false,
        ..Default::default()
    };
    let hits = hybrid_recall_with_rerank(
        vault.storage(),
        Some(emb.as_ref()),
        Some(reranker.as_ref()),
        "body",
        opts,
    )
    .await
    .unwrap();
    let explain = hits[0].explain.as_ref().unwrap();
    assert!(
        explain.rerank_score.is_none(),
        "rerank_score should remain None when rerank flag is off"
    );
}
