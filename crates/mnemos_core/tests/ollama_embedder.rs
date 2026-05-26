use mnemos_core::providers::ollama::{OllamaConfig, OllamaEmbedder};
use mnemos_core::providers::Embedder;

/// Requires Ollama running with `nomic-embed-text` pulled.
/// Run with: `cargo test --test ollama_embedder -- --ignored`
#[tokio::test]
#[ignore]
async fn ollama_embeds_text_at_correct_dim() {
    let e = OllamaEmbedder::new(OllamaConfig::default());
    let v = e
        .embed("the quick brown fox")
        .await
        .expect("ollama embed failed — is it running?");
    assert_eq!(v.len(), 768, "nomic-embed-text returns 768-d vectors");
    assert!(v.iter().any(|x| *x != 0.0), "expected non-zero vector");
}

#[tokio::test]
#[ignore]
async fn ollama_batch_matches_single() {
    let e = OllamaEmbedder::new(OllamaConfig::default());
    let batch = e.embed_batch(&["a".into(), "b".into()]).await.unwrap();
    let a = e.embed("a").await.unwrap();
    assert_eq!(batch[0].len(), 768);
    assert_eq!(batch[0], a);
}

#[tokio::test]
#[ignore]
async fn ollama_batch_processes_more_than_8_inputs_in_order() {
    let e = OllamaEmbedder::new(OllamaConfig::default());
    let texts: Vec<String> = (0..20).map(|i| format!("input #{i}")).collect();
    let vectors = e.embed_batch(&texts).await.unwrap();
    assert_eq!(vectors.len(), 20);
    // Each vector should be 768d (nomic-embed-text)
    for v in &vectors {
        assert_eq!(v.len(), 768);
    }
    // Order preservation: embedding "input #0" once and via batch should match.
    let v0_alone = e.embed("input #0").await.unwrap();
    assert_eq!(vectors[0], v0_alone);
}

#[test]
fn ollama_config_defaults() {
    let c = OllamaConfig::default();
    assert_eq!(c.base_url, "http://localhost:11434");
    assert_eq!(c.model, "nomic-embed-text");
    assert_eq!(c.dim, 768);
}
