use crate::error::{MnemosError, Result};
use crate::providers::{CompletionRequest, LlmProvider, LlmRole};
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;

/// Configuration for [`OllamaLlm`].
#[derive(Debug, Clone)]
pub struct OllamaLlmConfig {
    pub base_url: String,
    pub model: String,
    pub timeout_secs: u64,
}

/// LLM provider backed by Ollama's `POST /api/chat` endpoint.
#[derive(Debug, Clone)]
pub struct OllamaLlm {
    cfg: OllamaLlmConfig,
    client: reqwest::Client,
}

impl OllamaLlm {
    pub fn new(cfg: OllamaLlmConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(cfg.timeout_secs.max(1)))
            .build()
            .map_err(|e| MnemosError::Internal(format!("failed to build reqwest client: {e}")))?;
        Ok(Self { cfg, client })
    }
}

#[derive(Deserialize)]
struct ChatResp {
    message: ChatMsg,
}

#[derive(Deserialize)]
struct ChatMsg {
    content: String,
}

#[async_trait]
impl LlmProvider for OllamaLlm {
    fn model_id(&self) -> &str {
        &self.cfg.model
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<String> {
        let mut messages = vec![serde_json::json!({
            "role": "system",
            "content": req.system,
        })];
        for m in &req.messages {
            let role = match m.role {
                LlmRole::System => "system",
                LlmRole::User => "user",
                LlmRole::Assistant => "assistant",
            };
            messages.push(serde_json::json!({ "role": role, "content": m.content }));
        }
        let mut body = serde_json::json!({
            "model": self.cfg.model,
            "messages": messages,
            "stream": false,
            // Keep the model loaded indefinitely. Mnemos is a background
            // service that must respond to sessions at any time — the
            // default 5-minute timeout would unload the model between
            // sessions, causing cold-start delays and unnecessary CPU
            // churn from repeated model loads.
            "keep_alive": -1,
        });
        if req.json {
            body["format"] = serde_json::json!("json");
        }
        let url = format!("{}/api/chat", self.cfg.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| MnemosError::Internal(format!("ollama chat request failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(MnemosError::Internal(format!(
                "ollama chat returned HTTP {}",
                resp.status()
            )));
        }
        let parsed: ChatResp = resp
            .json()
            .await
            .map_err(|e| MnemosError::Internal(format!("ollama chat decode failed: {e}")))?;
        Ok(parsed.message.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::LlmProvider;

    fn cfg() -> OllamaLlmConfig {
        OllamaLlmConfig {
            base_url: "http://localhost:11434".into(),
            model: "llama3.2".into(),
            timeout_secs: 60,
        }
    }

    #[test]
    fn reports_model_id() {
        assert_eq!(OllamaLlm::new(cfg()).unwrap().model_id(), "llama3.2");
    }

    #[tokio::test]
    #[ignore = "requires a running Ollama with the model pulled"]
    async fn completes_live() {
        // Opt-in: requires a running Ollama. Without the env this no-ops so
        // `cargo test --include-ignored` stays green in CI (which has no
        // Ollama), matching the MNEMOS_TEST_LLAMA_SERVER gate on the bundled
        // embedder integration tests.
        if std::env::var("MNEMOS_TEST_OLLAMA").is_err() {
            eprintln!("skipping completes_live: set MNEMOS_TEST_OLLAMA=1 with a running Ollama");
            return;
        }
        use crate::providers::CompletionRequest;
        let llm = OllamaLlm::new(cfg()).unwrap();
        let req = CompletionRequest::new(
            "You reply with strict JSON only.",
            "Return the JSON object {\"ok\": true}",
        );
        let out = llm.complete(&req).await.unwrap();
        assert!(out.contains("ok"));
    }
}
