pub mod bm25;

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
    pub score: f64,
    pub bm25_rank: Option<usize>,
}
