//! Graph retriever: seed PPR from the query's BM25 neighborhood, then rank
//! memories by PPR mass. Produces a `RankedId` list for RRF fusion.

use crate::error::Result;
use crate::graph::ppr::{personalized_pagerank, ppr_rank_memories};
use crate::graph::MemoryGraph;
use crate::retrieval::bm25::bm25_recall;
use crate::retrieval::rrf::RankedId;
use crate::retrieval::RecallOpts;
use crate::storage::Storage;
use std::collections::BTreeSet;

/// PPR seeds = entity node-indices mentioned by the top `seed_hits` BM25 results
/// for `query`. Returns a deterministic (sorted) list of node indices.
pub async fn select_seeds(
    storage: &Storage,
    graph: &MemoryGraph,
    query: &str,
    seed_hits: usize,
) -> Result<Vec<usize>> {
    let opts = RecallOpts {
        k: seed_hits.max(1),
        ..Default::default()
    };
    let hits = bm25_recall(storage, query, opts).await?;
    let mut seeds: BTreeSet<usize> = BTreeSet::new();
    for h in &hits {
        if let Some(entities) = graph.entities_for_memory(&h.memory.id) {
            for &e in entities {
                seeds.insert(e);
            }
        }
    }
    Ok(seeds.into_iter().collect())
}

/// Rank memories for `query` via Personalized PageRank seeded on the query's
/// BM25 neighborhood. Empty when the graph is empty or no seeds are found.
pub async fn graph_rank(
    storage: &Storage,
    graph: &MemoryGraph,
    query: &str,
    alpha: f64,
    iterations: usize,
    seed_hits: usize,
) -> Result<Vec<RankedId>> {
    if graph.is_empty() {
        return Ok(vec![]);
    }
    let seeds = select_seeds(storage, graph, query, seed_hits).await?;
    if seeds.is_empty() {
        return Ok(vec![]);
    }
    let scores = personalized_pagerank(graph, &seeds, alpha, iterations);
    Ok(ppr_rank_memories(graph, &scores))
}
