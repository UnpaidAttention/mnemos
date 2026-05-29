//! OpenAI embeddings backend.
//!
//! Compatible with Azure OpenAI and any OpenAI-compat server via
//! `OPENAI_BASE_URL`. Defaults to `text-embedding-3-small` (1536-dim) when
//! configured from environment.
//!
//! # Usage
//! ```no_run
//! use mnemos_core::providers::Embedder;
//! use mnemos_core::providers::openai_embedder::{OpenAiConfig, OpenAiEmbedder};
//!
//! async fn example() {
//!     let cfg = OpenAiConfig::default();
//!     // (api_key must be set; default config has it empty)
//!     // let e = OpenAiEmbedder::new(&cfg).unwrap();
//!     // let v = e.embed("hello").await.unwrap();
//! }
//! ```
//!
//! The trait surface matches [`crate::providers::Embedder`]: `dim`, `kind`,
//! `model_id`, and `embed`. `kind()` returns `"openai"`.

use crate::error::{MnemosError, Result};
use crate::providers::Embedder;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for the OpenAI embedding client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    /// Base URL of the OpenAI-compatible server (no trailing `/v1`).
    pub base_url: String,
    /// API key (sent as `Authorization: Bearer <key>`).
    pub api_key: String,
    /// Embedding model name (e.g. `"text-embedding-3-small"`).
    pub model: String,
    /// Expected output dimension. Must match the model.
    pub dim: u32,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com".into(),
            api_key: String::new(),
            model: "text-embedding-3-small".into(),
            dim: 1536,
        }
    }
}

/// Build an [`OpenAiConfig`] from environment variables.
///
/// Reads `OPENAI_API_KEY` (required), `OPENAI_BASE_URL` (default
/// `https://api.openai.com`), and `MNEMOS_EMBEDDER_MODEL` (default
/// `text-embedding-3-small`). The output dimension is inferred from the model
/// name for known OpenAI models; unknown models fall back to 1536.
///
/// Returns an error if `OPENAI_API_KEY` is missing.
pub fn config_from_env() -> Result<OpenAiConfig> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| MnemosError::Internal("OPENAI_API_KEY not set".into()))?;
    let base_url =
        std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com".into());
    let model =
        std::env::var("MNEMOS_EMBEDDER_MODEL").unwrap_or_else(|_| "text-embedding-3-small".into());
    let dim = match model.as_str() {
        "text-embedding-3-small" => 1536,
        "text-embedding-3-large" => 3072,
        "text-embedding-ada-002" => 1536,
        _ => 1536,
    };
    Ok(OpenAiConfig {
        base_url,
        api_key,
        model,
        dim,
    })
}

/// Embedding client that calls `POST {base_url}/v1/embeddings`.
#[derive(Debug)]
pub struct OpenAiEmbedder {
    cfg: OpenAiConfig,
    client: reqwest::Client,
}

impl OpenAiEmbedder {
    /// Build a new client. Returns an error if `cfg.api_key` is empty or the
    /// HTTP client cannot be constructed.
    pub fn new(cfg: &OpenAiConfig) -> Result<Self> {
        if cfg.api_key.is_empty() {
            return Err(MnemosError::Internal("OpenAI API key is empty".into()));
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| MnemosError::Internal(format!("reqwest build: {e}")))?;
        Ok(Self {
            cfg: cfg.clone(),
            client,
        })
    }
}

#[derive(Serialize)]
struct EmbedReq<'a> {
    input: &'a str,
    model: &'a str,
}

#[derive(Deserialize)]
struct EmbedResp {
    data: Vec<EmbedData>,
}

#[derive(Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
}

#[async_trait]
impl Embedder for OpenAiEmbedder {
    fn dim(&self) -> usize {
        self.cfg.dim as usize
    }

    fn model_id(&self) -> &str {
        &self.cfg.model
    }

    fn kind(&self) -> &str {
        "openai"
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/v1/embeddings", self.cfg.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.cfg.api_key)
            .json(&EmbedReq {
                input: text,
                model: &self.cfg.model,
            })
            .send()
            .await
            .map_err(|e| MnemosError::Internal(format!("openai HTTP: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MnemosError::Internal(format!(
                "openai responded {status}: {body}"
            )));
        }
        let parsed: EmbedResp = resp
            .json()
            .await
            .map_err(|e| MnemosError::Internal(format!("openai parse: {e}")))?;
        let v = parsed
            .data
            .into_iter()
            .next()
            .ok_or_else(|| MnemosError::Internal("openai: empty data".into()))?
            .embedding;
        if v.len() != self.cfg.dim as usize {
            return Err(MnemosError::Internal(format!(
                "openai returned dim {} (expected {})",
                v.len(),
                self.cfg.dim
            )));
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_text_embedding_3_small() {
        let cfg = OpenAiConfig::default();
        assert_eq!(cfg.model, "text-embedding-3-small");
        assert_eq!(cfg.dim, 1536);
        assert_eq!(cfg.base_url, "https://api.openai.com");
        assert!(cfg.api_key.is_empty());
    }

    #[test]
    fn new_rejects_empty_api_key() {
        let cfg = OpenAiConfig::default();
        assert!(OpenAiEmbedder::new(&cfg).is_err());
    }
}
