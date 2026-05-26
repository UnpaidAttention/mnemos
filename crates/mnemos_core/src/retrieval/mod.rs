pub mod bm25;
pub mod dense;
pub mod reweight;
pub mod rrf;

use crate::tier::Tier;
use crate::types::Memory;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallOpts {
    pub k: usize,
    pub tiers: Option<Vec<Tier>>,
    pub workspace: Option<String>,
    pub include_invalid: bool,
}

impl Default for RecallOpts {
    fn default() -> Self {
        Self {
            k: 10,
            tiers: None,
            workspace: None,
            include_invalid: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Default, Serialize)]
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
