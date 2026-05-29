//! OpenAI chat-completions backend.
//!
//! Used by the learning pipeline (reflections, community summaries,
//! entity extraction). Compatible with Azure OpenAI and any OpenAI-compat
//! server via `OPENAI_BASE_URL`. Defaults to `gpt-4o-mini`.

use crate::error::{MnemosError, Result};
use crate::providers::{CompletionRequest, LlmProvider, LlmRole};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for [`OpenAiLlm`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiLlmConfig {
    /// Base URL of the OpenAI-compatible server (no trailing `/v1`).
    pub base_url: String,
    /// API key (sent as `Authorization: Bearer <key>`).
    pub api_key: String,
    /// Chat model name (e.g. `"gpt-4o-mini"`).
    pub model: String,
}

impl Default for OpenAiLlmConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com".into(),
            api_key: String::new(),
            model: "gpt-4o-mini".into(),
        }
    }
}

/// Build an [`OpenAiLlmConfig`] from environment variables.
///
/// Reads `OPENAI_API_KEY` (required), `OPENAI_BASE_URL` (default
/// `https://api.openai.com`), and `MNEMOS_LLM_MODEL` (default `gpt-4o-mini`).
pub fn config_from_env() -> Result<OpenAiLlmConfig> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| MnemosError::Internal("OPENAI_API_KEY not set".into()))?;
    let base_url =
        std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com".into());
    let model = std::env::var("MNEMOS_LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
    Ok(OpenAiLlmConfig {
        base_url,
        api_key,
        model,
    })
}

/// LLM provider backed by `POST {base_url}/v1/chat/completions`.
#[derive(Debug, Clone)]
pub struct OpenAiLlm {
    cfg: OpenAiLlmConfig,
    client: reqwest::Client,
}

impl OpenAiLlm {
    /// Build a new client. Returns an error if `cfg.api_key` is empty or the
    /// HTTP client cannot be constructed.
    pub fn new(cfg: &OpenAiLlmConfig) -> Result<Self> {
        if cfg.api_key.is_empty() {
            return Err(MnemosError::Internal("OpenAI API key is empty".into()));
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| MnemosError::Internal(format!("reqwest build: {e}")))?;
        Ok(Self {
            cfg: cfg.clone(),
            client,
        })
    }
}

#[derive(Deserialize)]
struct ChatResp {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMsg,
}

#[derive(Deserialize)]
struct ChatMsg {
    content: String,
}

#[async_trait]
impl LlmProvider for OpenAiLlm {
    fn model_id(&self) -> &str {
        &self.cfg.model
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<String> {
        let mut messages: Vec<serde_json::Value> = Vec::with_capacity(req.messages.len() + 1);
        if !req.system.is_empty() {
            messages.push(serde_json::json!({
                "role": "system",
                "content": req.system,
            }));
        }
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
        });
        if req.json {
            // OpenAI's JSON mode: nudges the model toward valid JSON output.
            body["response_format"] = serde_json::json!({ "type": "json_object" });
        }
        let url = format!(
            "{}/v1/chat/completions",
            self.cfg.base_url.trim_end_matches('/')
        );
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.cfg.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| MnemosError::Internal(format!("openai chat HTTP: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MnemosError::Internal(format!(
                "openai chat returned HTTP {status}: {body}"
            )));
        }
        let parsed: ChatResp = resp
            .json()
            .await
            .map_err(|e| MnemosError::Internal(format!("openai chat parse: {e}")))?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| MnemosError::Internal("openai chat: empty choices".into()))?
            .message
            .content;
        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_gpt_4o_mini() {
        let cfg = OpenAiLlmConfig::default();
        assert_eq!(cfg.model, "gpt-4o-mini");
        assert_eq!(cfg.base_url, "https://api.openai.com");
        assert!(cfg.api_key.is_empty());
    }

    #[test]
    fn new_rejects_empty_api_key() {
        let cfg = OpenAiLlmConfig::default();
        assert!(OpenAiLlm::new(&cfg).is_err());
    }

    #[test]
    fn reports_model_id() {
        let cfg = OpenAiLlmConfig {
            base_url: "http://localhost".into(),
            api_key: "sk-x".into(),
            model: "gpt-4o-mini".into(),
        };
        assert_eq!(OpenAiLlm::new(&cfg).unwrap().model_id(), "gpt-4o-mini");
    }
}
