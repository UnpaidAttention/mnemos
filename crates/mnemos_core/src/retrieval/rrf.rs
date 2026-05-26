//! Reciprocal Rank Fusion. Pure functions; no I/O.
//!
//! score(id) = Σ over retrievers i:  1 / (k + rank_i(id))
//!
//! k is conventionally 60 in the IR literature.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct RankedId {
    pub id: String,
    /// 1-indexed rank within a single retriever's results.
    pub rank: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FusedId {
    pub id: String,
    pub score: f64,
}

pub fn rrf_fuse(lists: &[&[RankedId]], k: usize) -> Vec<FusedId> {
    let mut acc: HashMap<String, f64> = HashMap::new();
    for list in lists {
        for entry in list.iter() {
            *acc.entry(entry.id.clone()).or_insert(0.0) += 1.0 / (k as f64 + entry.rank as f64);
        }
    }
    let mut out: Vec<FusedId> = acc
        .into_iter()
        .map(|(id, score)| FusedId { id, score })
        .collect();
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}
