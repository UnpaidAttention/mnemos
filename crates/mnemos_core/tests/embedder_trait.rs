use mnemos_core::providers::{mock::MockEmbedder, Embedder};

#[tokio::test]
async fn mock_embedder_returns_deterministic_vector() {
    let e = MockEmbedder::new(768);
    let v1 = e.embed("hello").await.unwrap();
    let v2 = e.embed("hello").await.unwrap();
    let v3 = e.embed("world").await.unwrap();
    assert_eq!(v1.len(), 768);
    assert_eq!(v1, v2, "same input → same vector");
    assert_ne!(v1, v3, "different input → different vector");
}

#[tokio::test]
async fn mock_embedder_reports_dim() {
    let e = MockEmbedder::new(384);
    assert_eq!(e.dim(), 384);
}

#[tokio::test]
async fn mock_embedder_batch_matches_single_calls() {
    let e = MockEmbedder::new(8);
    let batch = e
        .embed_batch(&["a".into(), "b".into(), "c".into()])
        .await
        .unwrap();
    let a = e.embed("a").await.unwrap();
    let b = e.embed("b").await.unwrap();
    assert_eq!(batch.len(), 3);
    assert_eq!(batch[0], a);
    assert_eq!(batch[1], b);
}
