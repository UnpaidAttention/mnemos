//! Integration tests for the BundledEmbedder HTTP client.
//!
//! The mock-server tests below (P2-22) run unconditionally — they spin up a
//! wiremock server and exercise the HTTP client path without real assets.
//! They cover both the single-embed and batch-embed code paths.
//!
//! The real-asset tests require a live `llama-server` listening on
//! 127.0.0.1:7424 with the bundled all-MiniLM-L6-v2 model loaded in
//! embedding mode.  To run them:
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
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

// ---------------------------------------------------------------------------
// P2-22: wiremock-based tests — run in CI without assets
// ---------------------------------------------------------------------------

/// Helper: build a well-formed single-embedding response body with the given
/// vector.  Uses index=0 as llama-server does.
fn embed_response(v: Vec<f32>) -> serde_json::Value {
    json!({
        "object": "list",
        "model": "all-MiniLM-L6-v2",
        "data": [{ "object": "embedding", "index": 0, "embedding": v }],
        "usage": { "prompt_tokens": 3, "total_tokens": 3 }
    })
}

/// Helper: build a batch response where each item is a 384-dim zero vector at
/// the given index.
fn batch_embed_response(count: usize) -> serde_json::Value {
    let data: Vec<serde_json::Value> = (0..count)
        .map(|i| {
            let v: Vec<f32> = vec![0.0f32; 384];
            json!({ "object": "embedding", "index": i, "embedding": v })
        })
        .collect();
    json!({
        "object": "list",
        "model": "all-MiniLM-L6-v2",
        "data": data,
        "usage": {}
    })
}

/// Exercises the single-embed path (BundledEmbedder::embed) against a
/// wiremock mock server.  No llama-server binary or GGUF assets required.
#[tokio::test]
async fn mock_bundled_embedder_embed_returns_correct_dim() {
    let server = MockServer::start().await;

    // Return a 384-element unit-ish vector.
    let expected: Vec<f32> = (0..384).map(|i| (i as f32) / 384.0).collect();
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(embed_response(expected.clone())))
        .mount(&server)
        .await;

    let embedder = BundledEmbedder::new(server.uri()).unwrap();
    let result = embedder.embed("hello world").await.unwrap();

    assert_eq!(result.len(), 384, "embed() must return exactly 384 floats");
    // Spot-check a few values to confirm the response was decoded correctly.
    assert!(
        (result[0] - expected[0]).abs() < 1e-6,
        "first element mismatch"
    );
    assert!(
        (result[383] - expected[383]).abs() < 1e-6,
        "last element mismatch"
    );
}

/// Exercises the batch-embed path (BundledEmbedder::embed_batch) against a
/// wiremock mock server.  Verifies that two texts produce two 384-dim vectors
/// and that chunking works for a batch that fits in a single request.
#[tokio::test]
async fn mock_bundled_embedder_batch_embed_returns_correct_shape() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(batch_embed_response(2)))
        .mount(&server)
        .await;

    let embedder = BundledEmbedder::new(server.uri()).unwrap();
    let texts = vec!["foo".to_string(), "bar".to_string()];
    let results = embedder.embed_batch(&texts).await.unwrap();

    assert_eq!(
        results.len(),
        2,
        "embed_batch should return one vector per input"
    );
    for (i, v) in results.iter().enumerate() {
        assert_eq!(v.len(), 384, "vector {i} should have 384 dimensions");
    }
}

/// Verifies that embed() returns an error when the server returns a non-2xx
/// status, exercising the error path in the HTTP client.
#[tokio::test]
async fn mock_bundled_embedder_handles_server_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(503).set_body_string("service unavailable"))
        .mount(&server)
        .await;

    let embedder = BundledEmbedder::new(server.uri()).unwrap();
    let result = embedder.embed("hello").await;

    assert!(
        result.is_err(),
        "embed() must propagate HTTP 503 as an error"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("503"),
        "error message should include the HTTP status code, got: {msg}"
    );
}

/// Verifies that embed_batch() returns an error on non-2xx responses,
/// exercising the batch error path.
#[tokio::test]
async fn mock_bundled_embedder_batch_handles_server_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .mount(&server)
        .await;

    let embedder = BundledEmbedder::new(server.uri()).unwrap();
    let texts = vec!["a".to_string(), "b".to_string()];
    let result = embedder.embed_batch(&texts).await;

    assert!(
        result.is_err(),
        "embed_batch() must propagate HTTP 500 as an error"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("500"),
        "error message should include the HTTP status code, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Real-asset tests (ignored by default)
// ---------------------------------------------------------------------------

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
