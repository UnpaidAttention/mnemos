use mnemos_core::paths::Paths;
use mnemos_core::providers::{mock::MockEmbedder, Embedder};
use mnemos_core::vault::Vault;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn opening_vault_with_mismatched_dim_errors() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let e768: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    {
        let _ = Vault::open_with_embedder(paths.clone(), Some(e768.clone()))
            .await
            .unwrap();
    }
    let e384: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(384));
    let err = Vault::open_with_embedder(paths, Some(e384)).await;
    assert!(err.is_err(), "different dim must error");
    // err.unwrap_err() requires T: Debug; extract the error via .err() instead.
    let e = err.err().expect("is_err() was true");
    let msg = format!("{e:?}");
    assert!(msg.contains("dim"), "error should mention dim: {msg}");
}

#[tokio::test]
async fn opening_vault_with_no_embedder_skips_dim_check() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let e: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let _ = Vault::open_with_embedder(paths.clone(), Some(e))
        .await
        .unwrap();
    let _ = Vault::open(paths).await.unwrap(); // no embedder → no check
}

#[tokio::test]
async fn embedder_model_id_round_trips_through_vault_meta() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let e: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    assert_eq!(e.model_id(), "mock");

    let v = Vault::open_with_embedder(paths.clone(), Some(e))
        .await
        .unwrap();
    let meta = v.storage().get_vault_meta().await.unwrap();
    assert_eq!(meta.embedder_dim, Some(768));
    assert_eq!(meta.embedder_model_id, Some("mock".to_string()));
}
