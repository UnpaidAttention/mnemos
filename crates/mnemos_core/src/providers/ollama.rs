//! HTTP client for Ollama's `/api/embeddings` endpoint.
//!
//! # Usage
//! ```no_run
//! use mnemos_core::providers::Embedder;
//! use mnemos_core::providers::ollama::{OllamaConfig, OllamaEmbedder};
//!
//! async fn example() {
//!     let embedder = OllamaEmbedder::new(OllamaConfig::default());
//!     let vec = embedder.embed("hello world").await.unwrap();
//!     assert_eq!(vec.len(), 768);
//! }
//! ```

use crate::error::{MnemosError, Result};
use crate::providers::Embedder;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for the Ollama embedding client.
#[derive(Debug, Clone)]
pub struct OllamaConfig {
    /// Base URL of the Ollama server.
    pub base_url: String,
    /// Embedding model name (e.g. "nomic-embed-text").
    pub model: String,
    /// Expected output dimension. Must match the model.
    pub dim: usize,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".into(),
            model: "nomic-embed-text".into(),
            dim: 768,
            timeout_secs: 30,
        }
    }
}

/// Embedding client that calls Ollama's `/api/embeddings` endpoint.
#[derive(Debug)]
pub struct OllamaEmbedder {
    config: OllamaConfig,
    client: reqwest::Client,
}

impl OllamaEmbedder {
    /// Create a new embedder from `config`.
    ///
    /// Builds a [`reqwest::Client`] with the configured timeout once at
    /// construction time; subsequent calls reuse the same connection pool.
    pub fn new(config: OllamaConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("failed to build reqwest HTTP client (TLS init failure?)");
        Self { config, client }
    }
}

#[derive(Serialize)]
struct EmbedReq<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Deserialize)]
struct EmbedResp {
    embedding: Vec<f32>,
}

#[async_trait]
impl Embedder for OllamaEmbedder {
    fn dim(&self) -> usize {
        self.config.dim
    }

    fn model_id(&self) -> &str {
        &self.config.model
    }

    fn kind(&self) -> &str {
        "ollama"
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!(
            "{}/api/embeddings",
            self.config.base_url.trim_end_matches('/')
        );
        let resp = self
            .client
            .post(&url)
            .json(&EmbedReq {
                model: &self.config.model,
                prompt: text,
            })
            .send()
            .await
            .map_err(|e| MnemosError::Internal(format!("ollama HTTP: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MnemosError::Internal(format!(
                "ollama responded {status}: {body}"
            )));
        }

        let parsed: EmbedResp = resp
            .json()
            .await
            .map_err(|e| MnemosError::Internal(format!("ollama parse: {e}")))?;

        if parsed.embedding.len() != self.config.dim {
            return Err(MnemosError::Internal(format!(
                "ollama returned {}d, expected {}d (model mismatch?)",
                parsed.embedding.len(),
                self.config.dim
            )));
        }

        Ok(parsed.embedding)
    }

    /// Concurrent fan-out — Ollama serves embeddings in parallel reliably.
    ///
    /// Texts are processed in chunks of up to 8 concurrent requests; results
    /// are returned in the same order as the input slice.  Chunked `join_all`
    /// is used instead of `stream::buffered` to avoid async_trait + HRTB
    /// lifetime issues with borrowed futures.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        use futures::future::join_all;
        const MAX_CONCURRENT: usize = 8;
        let mut out: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
        for batch in texts.chunks(MAX_CONCURRENT) {
            let results = join_all(batch.iter().map(|t| self.embed(t))).await;
            for r in results {
                out.push(r?);
            }
        }
        Ok(out)
    }
}
