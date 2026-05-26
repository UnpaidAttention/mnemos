//! Score reweighting: applies recency, importance, strength, and tier
//! multipliers to a base score. Pure; no I/O.

use crate::types::Memory;
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReweightConfig {
    /// Per-day exponential decay rate for recency. Default 0.02 → halves over ~35 days.
    pub recency_decay: f64,
    /// Importance multiplier: factor = (1 + importance_weight * importance).
    pub importance_weight: f64,
    /// Per-tier multiplicative weights, applied directly.
    pub tier_weight_working: f64,
    pub tier_weight_episodic: f64,
    pub tier_weight_semantic: f64,
    pub tier_weight_procedural: f64,
    pub tier_weight_reflection: f64,
}

impl Default for ReweightConfig {
    fn default() -> Self {
        Self {
            recency_decay: 0.02,
            importance_weight: 1.0,
            tier_weight_working: 2.0,
            tier_weight_episodic: 0.8,
            tier_weight_semantic: 1.0,
            tier_weight_procedural: 1.5,
            tier_weight_reflection: 1.2,
        }
    }
}

impl ReweightConfig {
    pub fn tier_weight(&self, tier: crate::Tier) -> f64 {
        use crate::Tier::*;
        match tier {
            Working => self.tier_weight_working,
            Episodic => self.tier_weight_episodic,
            Semantic => self.tier_weight_semantic,
            Procedural => self.tier_weight_procedural,
            Reflection => self.tier_weight_reflection,
        }
    }
}

pub fn apply_reweight(base_score: f64, memory: &Memory, cfg: &ReweightConfig) -> f64 {
    let age_days = (Utc::now() - memory.created_at).num_seconds() as f64 / 86_400.0;
    let recency = (-cfg.recency_decay * age_days.max(0.0)).exp();
    let importance = 1.0 + cfg.importance_weight * memory.importance;
    let strength = memory.strength;
    let tier = cfg.tier_weight(memory.tier);
    base_score * recency * importance * strength * tier
}

/// Per-factor breakdown for explainability.
pub struct ReweightBreakdown {
    pub recency: f64,
    pub importance: f64,
    pub strength: f64,
    pub tier: f64,
    pub final_score: f64,
}

pub fn apply_reweight_with_breakdown(
    base_score: f64,
    memory: &Memory,
    cfg: &ReweightConfig,
) -> ReweightBreakdown {
    let age_days = (Utc::now() - memory.created_at).num_seconds() as f64 / 86_400.0;
    let recency = (-cfg.recency_decay * age_days.max(0.0)).exp();
    let importance = 1.0 + cfg.importance_weight * memory.importance;
    let strength = memory.strength;
    let tier = cfg.tier_weight(memory.tier);
    let final_score = base_score * recency * importance * strength * tier;
    ReweightBreakdown {
        recency,
        importance,
        strength,
        tier,
        final_score,
    }
}
