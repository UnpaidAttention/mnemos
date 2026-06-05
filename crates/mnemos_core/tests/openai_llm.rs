//! Tests for the OpenAI chat-completions backend.
//!
//! Uses `wiremock` to stand up a fake `/v1/chat/completions` endpoint.

use mnemos_core::providers::openai_llm::{OpenAiLlm, OpenAiLlmConfig};
use mnemos_core::providers::{CompletionRequest, LlmProvider};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn openai_llm_chat_completion() {
    let server = MockServer::start().await;
    let body = serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "model": "gpt-4o-mini",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "Hello back" },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7 }
    });
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    let cfg = OpenAiLlmConfig {
        base_url: server.uri(),
        api_key: "sk-test".into(),
        model: "gpt-4o-mini".into(),
        timeout_secs: 60,
    };
    let l = OpenAiLlm::new(&cfg).unwrap();
    let req = CompletionRequest::new("You are a helpful assistant.", "hello");
    let out = l.complete(&req).await.unwrap();
    assert_eq!(out, "Hello back");
    assert_eq!(l.model_id(), "gpt-4o-mini");
}

#[tokio::test]
async fn openai_llm_rejects_empty_api_key() {
    let cfg = OpenAiLlmConfig {
        base_url: "http://localhost:9999".into(),
        api_key: String::new(),
        model: "gpt-4o-mini".into(),
        timeout_secs: 60,
    };
    assert!(OpenAiLlm::new(&cfg).is_err());
}

#[tokio::test]
async fn openai_llm_surfaces_http_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
        .mount(&server)
        .await;

    let cfg = OpenAiLlmConfig {
        base_url: server.uri(),
        api_key: "sk-test".into(),
        model: "gpt-4o-mini".into(),
        timeout_secs: 60,
    };
    let l = OpenAiLlm::new(&cfg).unwrap();
    let req = CompletionRequest::new("system", "user");
    let err = l.complete(&req).await.unwrap_err();
    assert!(err.to_string().contains("429"));
}
