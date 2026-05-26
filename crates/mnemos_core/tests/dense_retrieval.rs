use mnemos_core::providers::{mock::MockEmbedder, Embedder};
use mnemos_core::retrieval::{dense::dense_recall, RecallOpts};
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::{paths::Paths, Tier};
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
async fn dense_recall_finds_nearest_memory() {
    let (_tmp, vault, emb) = fixture().await;
    let id_target = vault
        .remember(
            "Tauri preference",
            RememberOpts {
                title: Some("Tauri choice".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let _ = vault
        .remember(
            "React notes",
            RememberOpts {
                title: Some("React notes".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let hits = dense_recall(
        vault.storage(),
        emb.as_ref(),
        "Tauri preference",
        RecallOpts::default(),
    )
    .await
    .unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0].memory.id, id_target);
    assert!(hits[0].dense_rank.is_some());
    assert!(hits[0].dense_distance.is_some());
}

#[tokio::test]
async fn dense_recall_respects_tier_filter() {
    let (_tmp, vault, emb) = fixture().await;
    let _ = vault
        .remember(
            "rule",
            RememberOpts {
                title: Some("rule".into()),
                tier: Tier::Procedural,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let id_sem = vault
        .remember(
            "rule fact",
            RememberOpts {
                title: Some("rule fact".into()),
                tier: Tier::Semantic,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let opts = RecallOpts {
        tiers: Some(vec![Tier::Semantic]),
        ..Default::default()
    };
    let hits = dense_recall(vault.storage(), emb.as_ref(), "rule", opts)
        .await
        .unwrap();
    assert!(hits.iter().all(|h| h.memory.tier == Tier::Semantic));
    assert!(hits.iter().any(|h| h.memory.id == id_sem));
}

#[tokio::test]
async fn dense_recall_hides_invalidated_by_default() {
    let (_tmp, vault, emb) = fixture().await;
    let id = vault
        .remember(
            "doomed",
            RememberOpts {
                title: Some("d".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    vault.forget(&id, None).await.unwrap();

    // Forget removed the vector; KNN should return nothing matching this body.
    let hits = dense_recall(
        vault.storage(),
        emb.as_ref(),
        "doomed",
        RecallOpts::default(),
    )
    .await
    .unwrap();
    assert!(hits.iter().all(|h| h.memory.id != id));
}
