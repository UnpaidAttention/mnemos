//! Bundled embedder backend — HTTP client for the local llama-server child
//! process the daemon spawns at 127.0.0.1:7424.
//!
//! llama-server's embedding endpoint accepts:
//!   POST /v1/embeddings    { "input": "<text>", "model": "any" }
//! and returns:
//!   { "data": [{ "embedding": [f32; D], "index": 0 }], "model": "...", ... }
//! which is OpenAI-compatible. We use this OpenAI-compat shape.

use crate::error::{MnemosError, Result};
use crate::providers::Embedder;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// HTTP client that talks to a locally-spawned `llama-server` running the
/// bundled all-MiniLM-L6-v2 GGUF model. The daemon owns the child process;
/// this struct only knows the base URL.
#[derive(Clone, Debug)]
pub struct BundledEmbedder {
    base_url: String,
    client: reqwest::Client,
}

impl BundledEmbedder {
    /// Construct a client pointed at `base_url` (e.g. `"http://127.0.0.1:7424"`).
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client build"),
        }
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
impl Embedder for BundledEmbedder {
    fn dim(&self) -> usize {
        384
    }

    fn model_id(&self) -> &str {
        "all-MiniLM-L6-v2"
    }

    fn kind(&self) -> &str {
        "bundled"
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/v1/embeddings", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .json(&EmbedReq {
                input: text,
                model: "all-MiniLM-L6-v2",
            })
            .send()
            .await
            .map_err(|e| MnemosError::Internal(format!("bundled embedder HTTP: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MnemosError::Internal(format!(
                "bundled embedder responded {status}: {body}"
            )));
        }
        let parsed: EmbedResp = resp
            .json()
            .await
            .map_err(|e| MnemosError::Internal(format!("bundled embedder parse: {e}")))?;
        let v = parsed
            .data
            .into_iter()
            .next()
            .ok_or_else(|| MnemosError::Internal("bundled embedder: empty data".into()))?
            .embedding;
        if v.len() != 384 {
            return Err(MnemosError::Internal(format!(
                "bundled embedder returned dim {} (expected 384)",
                v.len()
            )));
        }
        Ok(v)
    }
}
