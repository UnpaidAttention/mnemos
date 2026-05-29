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

#[tokio::test]
async fn embedder_kind_round_trips_through_vault_meta() {
    use mnemos_core::storage::vault_meta::get_embedder_meta;
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let e: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(384));
    let v = Vault::open_with_embedder(paths.clone(), Some(e))
        .await
        .unwrap();
    let meta = get_embedder_meta(v.storage()).await.unwrap();
    assert_eq!(meta.kind, "mock");
    assert_eq!(meta.model, "mock");
    assert_eq!(meta.dim, 384);
}

/// Vault-authoritative kind check: once a vault has been seeded with a kind,
/// opening it with a different kind (even at the same dim+model) must fail.
#[tokio::test]
async fn opening_vault_with_mismatched_kind_errors() {
    use async_trait::async_trait;
    use mnemos_core::error::Result;
    use mnemos_core::providers::Embedder;

    // Fake embedder that reports kind="bundled" so we can simulate a kind swap.
    struct FakeBundled(usize);
    #[async_trait]
    impl Embedder for FakeBundled {
        fn dim(&self) -> usize {
            self.0
        }
        fn model_id(&self) -> &str {
            "mock"
        }
        fn kind(&self) -> &str {
            "bundled"
        }
        async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.0; self.0])
        }
    }

    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    // Seed with kind=mock.
    let mock: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(384));
    let _ = Vault::open_with_embedder(paths.clone(), Some(mock))
        .await
        .unwrap();
    // Reopen with kind=bundled (same dim, same model) → must fail.
    let bundled: Arc<dyn Embedder> = Arc::new(FakeBundled(384));
    let err = Vault::open_with_embedder(paths, Some(bundled)).await;
    assert!(err.is_err(), "different kind must error");
    let e = err.err().unwrap();
    let msg = format!("{e:?}");
    assert!(msg.contains("kind"), "error should mention kind: {msg}");
}
