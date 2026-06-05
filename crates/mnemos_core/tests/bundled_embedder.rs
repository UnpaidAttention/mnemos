//! Integration tests for the BundledEmbedder HTTP client.
//!
//! These tests require a live `llama-server` listening on 127.0.0.1:7424 with
//! the bundled all-MiniLM-L6-v2 model loaded in embedding mode. To run:
//!
//! ```bash
//! ./assets/llama-server-linux-x86_64 \
//!     --model assets/all-MiniLM-L6-v2.Q8_0.gguf \
//!     --port 7424 --embedding --pooling mean &
//! MNEMOS_TEST_LLAMA_SERVER=1 cargo test -p mnemos_core \
//!     --test bundled_embedder -- --include-ignored
//! ```

use mnemos_core::providers::bundled::BundledEmbedder;
use mnemos_core::providers::Embedder;

#[tokio::test]
#[ignore = "requires a running llama-server at 127.0.0.1:7424 (set MNEMOS_TEST_LLAMA_SERVER=1)"]
async fn bundled_embedder_returns_384_dim_vector() {
    if std::env::var("MNEMOS_TEST_LLAMA_SERVER").is_err() {
        return;
    }
    let e = BundledEmbedder::new("http://127.0.0.1:7424").unwrap();
    let v = e.embed("hello world").await.unwrap();
    assert_eq!(v.len(), 384);
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(
        (norm - 1.0).abs() < 0.1,
        "expected unit-norm vector, got {norm}"
    );
}

#[tokio::test]
#[ignore = "requires a running llama-server"]
async fn bundled_embedder_is_deterministic() {
    if std::env::var("MNEMOS_TEST_LLAMA_SERVER").is_err() {
        return;
    }
    let e = BundledEmbedder::new("http://127.0.0.1:7424").unwrap();
    let v1 = e.embed("the quick brown fox").await.unwrap();
    let v2 = e.embed("the quick brown fox").await.unwrap();
    for (a, b) in v1.iter().zip(v2.iter()) {
        assert!((a - b).abs() < 1e-6);
    }
}
