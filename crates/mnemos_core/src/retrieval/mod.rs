pub mod bm25;
pub mod dense;
pub mod graph_recall;
pub mod hybrid;
pub mod rerank;
pub mod reweight;
pub mod rrf;

use crate::tier::Tier;
use crate::types::Memory;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RecallOpts {
    pub k: usize,
    pub tiers: Option<Vec<Tier>>,
    pub workspace: Option<String>,
    pub include_invalid: bool,
    /// RRF fusion constant. Canonical value is 60.
    pub rrf_k: usize,
    /// Reweighting parameters.
    pub reweight: reweight::ReweightConfig,
    /// Populate per-hit Explain field when true.
    pub explain: bool,
    /// Enable cross-encoder reranking (Task 12+ adds the actual reranker).
    pub rerank: bool,
}

impl Default for RecallOpts {
    fn default() -> Self {
        Self {
            k: 10,
            tiers: None,
            workspace: None,
            include_invalid: false,
            rrf_k: 60,
            reweight: reweight::ReweightConfig::default(),
            explain: false,
            rerank: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallHit {
    pub memory: Memory,
    /// Aggregate score after fusion + reweighting + (optional) rerank.
    pub score: f64,
    /// Rank of this memory in the BM25 retriever's results, if matched there.
    pub bm25_rank: Option<usize>,
    /// Rank of this memory in the Dense retriever's results, if matched there.
    pub dense_rank: Option<usize>,
    /// Raw distance from sqlite-vec for the dense retriever, if matched.
    pub dense_distance: Option<f32>,
    /// Full per-stage trace, populated only when explainability is requested.
    pub explain: Option<Explain>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Explain {
    pub bm25_rank: Option<usize>,
    pub dense_rank: Option<usize>,
    pub dense_distance: Option<f32>,
    pub rrf_score: f64,
    pub weight_recency: f64,
    pub weight_importance: f64,
    pub weight_strength: f64,
    pub weight_tier: f64,
    pub rerank_score: Option<f64>,
    pub final_score: f64,
}
