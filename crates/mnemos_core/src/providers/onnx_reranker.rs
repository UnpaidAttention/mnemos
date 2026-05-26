//! ONNX-runtime-backed cross-encoder reranker. Feature: `rerank-onnx`.
//!
//! Default model: `bge-reranker-base`. Model weights are downloaded by the
//! user — Plan 2 does not bundle weights.
//!
//! Expected paths:
//!   `~/.local/share/mnemos/models/bge-reranker-base.onnx`
//!   `~/.local/share/mnemos/models/bge-reranker-base.tokenizer.json`

use crate::error::{MnemosError, Result};
use crate::providers::Reranker;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

/// Configuration for [`OnnxReranker`].
#[derive(Debug, Clone)]
pub struct OnnxRerankerConfig {
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    pub max_seq_len: usize,
}

impl Default for OnnxRerankerConfig {
    fn default() -> Self {
        let base = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
            .map(|p| p.data_dir().join("models"))
            .unwrap_or_else(|| PathBuf::from("./models"));
        Self {
            model_path: base.join("bge-reranker-base.onnx"),
            tokenizer_path: base.join("bge-reranker-base.tokenizer.json"),
            max_seq_len: 512,
        }
    }
}

/// Cross-encoder reranker backed by an ONNX model loaded via `ort`.
///
/// Inference is dispatched to a blocking thread so the async runtime is not
/// blocked.
pub struct OnnxReranker {
    session: Arc<ort::Session>,
    tokenizer: Arc<tokenizers::Tokenizer>,
    max_seq_len: usize,
}

impl OnnxReranker {
    /// Load model and tokenizer from the paths in `cfg`.
    pub fn load(cfg: OnnxRerankerConfig) -> Result<Self> {
        let session = ort::Session::builder()
            .map_err(|e| MnemosError::Internal(format!("ort builder: {e}")))?
            .commit_from_file(&cfg.model_path)
            .map_err(|e| {
                MnemosError::Internal(format!("ort load model {}: {e}", cfg.model_path.display()))
            })?;
        let tokenizer = tokenizers::Tokenizer::from_file(&cfg.tokenizer_path)
            .map_err(|e| MnemosError::Internal(format!("tokenizer load: {e}")))?;
        Ok(Self {
            session: Arc::new(session),
            tokenizer: Arc::new(tokenizer),
            max_seq_len: cfg.max_seq_len,
        })
    }
}

#[async_trait]
impl Reranker for OnnxReranker {
    async fn rerank(&self, query: &str, candidates: &[String]) -> Result<Vec<f32>> {
        let session = self.session.clone();
        let tokenizer = self.tokenizer.clone();
        let max_len = self.max_seq_len;
        let query = query.to_string();
        let candidates = candidates.to_vec();

        tokio::task::spawn_blocking(move || {
            use ndarray::Array2;

            let mut scores = Vec::with_capacity(candidates.len());
            for cand in &candidates {
                let encoding = tokenizer
                    .encode((query.as_str(), cand.as_str()), true)
                    .map_err(|e| MnemosError::Internal(format!("tokenize: {e}")))?;

                let ids: Vec<i64> = encoding
                    .get_ids()
                    .iter()
                    .take(max_len)
                    .map(|i| i64::from(*i))
                    .collect();
                let mask: Vec<i64> = encoding
                    .get_attention_mask()
                    .iter()
                    .take(max_len)
                    .map(|i| i64::from(*i))
                    .collect();

                let seq_len = ids.len();
                let ids_arr = Array2::from_shape_vec((1, seq_len), ids)
                    .map_err(|e| MnemosError::Internal(format!("shape ids: {e}")))?;
                let mask_arr = Array2::from_shape_vec((1, seq_len), mask)
                    .map_err(|e| MnemosError::Internal(format!("shape mask: {e}")))?;

                let outputs = session
                    .run(
                        ort::inputs![
                            "input_ids" => ids_arr,
                            "attention_mask" => mask_arr,
                        ]
                        .map_err(|e| MnemosError::Internal(format!("ort inputs: {e}")))?,
                    )
                    .map_err(|e| MnemosError::Internal(format!("ort run: {e}")))?;

                let out = outputs["logits"]
                    .try_extract_tensor::<f32>()
                    .map_err(|e| MnemosError::Internal(format!("ort extract: {e}")))?;

                let score = out.view().iter().copied().next().unwrap_or(0.0);
                scores.push(score);
            }

            Ok::<Vec<f32>, MnemosError>(scores)
        })
        .await
        .map_err(|e| MnemosError::Internal(format!("rerank join: {e}")))?
    }
}
