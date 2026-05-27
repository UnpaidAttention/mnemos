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
    pub fn new(cfg: OllamaLlmConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(cfg.timeout_secs.max(1)))
            .build()
            .expect("failed to build reqwest client");
        Self { cfg, client }
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
        assert_eq!(OllamaLlm::new(cfg()).model_id(), "llama3.2");
    }

    #[tokio::test]
    #[ignore = "requires a running Ollama with the model pulled"]
    async fn completes_live() {
        use crate::providers::CompletionRequest;
        let llm = OllamaLlm::new(cfg());
        let req = CompletionRequest::new(
            "You reply with strict JSON only.",
            "Return the JSON object {\"ok\": true}",
        );
        let out = llm.complete(&req).await.unwrap();
        assert!(out.contains("ok"));
    }
}
