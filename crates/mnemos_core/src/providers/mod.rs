//! Provider abstractions for embeddings and reranking.
//!
//! Implementations live in submodules:
//! - `ollama` — HTTP client for Ollama embedding API (default; populated in Task 5)
//! - `mock`   — deterministic test stub
//! - `onnx_reranker` — bge-reranker-base via ONNX (feature: `rerank-onnx`; Task 12)

pub mod mock;
pub mod ollama;

#[cfg(feature = "rerank-onnx")]
pub mod onnx_reranker;

use crate::error::Result;
use async_trait::async_trait;

/// Generates fixed-dimensional float embeddings for arbitrary text.
#[async_trait]
pub trait Embedder: Send + Sync {
    /// Vector dimension produced by this embedder (e.g. 768 for nomic-embed-text).
    fn dim(&self) -> usize;

    /// Stable identifier for the model (used to detect model swaps).
    /// Override per implementation; default is "unknown".
    fn model_id(&self) -> &str {
        "unknown"
    }

    /// Embed a single string.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed a batch. Default impl just calls `embed` in a loop. Implementations
    /// that support real batching should override.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            out.push(self.embed(t).await?);
        }
        Ok(out)
    }
}

/// Cross-encoder reranker: re-scores `(query, candidate)` pairs.
#[async_trait]
pub trait Reranker: Send + Sync {
    /// Returns a vector of scores, one per candidate, in the same order as input.
    /// Higher score = better match. Range and scale are implementation-defined.
    async fn rerank(&self, query: &str, candidates: &[String]) -> Result<Vec<f32>>;
}
