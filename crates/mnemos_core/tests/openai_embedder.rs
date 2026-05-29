//! Tests for the OpenAI embeddings backend.
//!
//! Uses `wiremock` to stand up a fake OpenAI server; no real API key required.

use mnemos_core::providers::openai_embedder::{OpenAiConfig, OpenAiEmbedder};
use mnemos_core::providers::Embedder;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn openai_embedder_sends_correct_request_shape() {
    let server = MockServer::start().await;
    let body = serde_json::json!({
        "data": [{
            "embedding": vec![0.1f32; 1536],
            "index": 0,
            "object": "embedding"
        }],
        "model": "text-embedding-3-small",
        "object": "list",
        "usage": { "prompt_tokens": 2, "total_tokens": 2 }
    });
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    let cfg = OpenAiConfig {
        base_url: server.uri(),
        api_key: "sk-test".into(),
        model: "text-embedding-3-small".into(),
        dim: 1536,
    };
    let e = OpenAiEmbedder::new(&cfg).unwrap();
    let v = e.embed("hello").await.unwrap();
    assert_eq!(v.len(), 1536);
    assert_eq!(e.dim(), 1536);
    assert_eq!(e.kind(), "openai");
    assert_eq!(e.model_id(), "text-embedding-3-small");
}

#[tokio::test]
async fn openai_embedder_rejects_empty_api_key() {
    let cfg = OpenAiConfig {
        base_url: "http://localhost:9999".into(),
        api_key: String::new(),
        model: "text-embedding-3-small".into(),
        dim: 1536,
    };
    assert!(OpenAiEmbedder::new(&cfg).is_err());
}

#[tokio::test]
async fn openai_embedder_surfaces_http_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .mount(&server)
        .await;

    let cfg = OpenAiConfig {
        base_url: server.uri(),
        api_key: "sk-test".into(),
        model: "text-embedding-3-small".into(),
        dim: 1536,
    };
    let e = OpenAiEmbedder::new(&cfg).unwrap();
    let err = e.embed("hello").await.unwrap_err();
    assert!(err.to_string().contains("401"));
}
