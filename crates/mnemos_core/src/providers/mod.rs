//! Provider abstractions for embeddings and reranking.
//!
//! Implementations live in submodules:
//! - `bundled` — HTTP client for the local llama-server child process (default)
//! - `ollama`  — HTTP client for Ollama embedding API
//! - `mock`    — deterministic test stub
//! - `onnx_reranker` — bge-reranker-base via ONNX (feature: `rerank-onnx`; Task 12)

pub mod bundled;
pub mod mock;
pub mod mock_llm;
pub mod ollama;
pub mod ollama_llm;
pub mod openai_embedder;

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

    /// Backend kind tag persisted to `vault_meta.embedder_kind`. One of
    /// `"bundled"`, `"ollama"`, `"openai"`, `"mock"`. Override per
    /// implementation; default is `"unknown"`.
    fn kind(&self) -> &str {
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

/// Role of a single chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmRole {
    System,
    User,
    Assistant,
}

/// One chat message in a completion request.
#[derive(Debug, Clone)]
pub struct LlmMessage {
    pub role: LlmRole,
    pub content: String,
}

/// A chat completion request: a system prompt plus a sequence of messages.
#[derive(Debug, Clone, Default)]
pub struct CompletionRequest {
    pub system: String,
    pub messages: Vec<LlmMessage>,
    /// Hint that the provider should bias toward strict JSON output.
    pub json: bool,
}

impl CompletionRequest {
    /// Convenience constructor: a system prompt and a single user message,
    /// with JSON mode enabled (the pipeline always wants JSON back).
    pub fn new(system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            system: system.into(),
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: user.into(),
            }],
            json: true,
        }
    }

    /// Concatenate all user-message contents with newlines. Deterministic
    /// providers parse markers out of this.
    pub fn joined_user_content(&self) -> String {
        self.messages
            .iter()
            .filter(|m| m.role == LlmRole::User)
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Generates a text completion from a chat-style request.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Stable identifier for the underlying model.
    fn model_id(&self) -> &str {
        "unknown"
    }

    /// Run the completion and return the assistant's text response.
    async fn complete(&self, req: &CompletionRequest) -> Result<String>;
}
