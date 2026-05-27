//! Graph retriever: seed PPR from the query's BM25 neighborhood, then rank
//! memories by PPR mass. Produces a `RankedId` list for RRF fusion.

use crate::error::Result;
use crate::graph::ppr::{personalized_pagerank, ppr_rank_memories};
use crate::graph::MemoryGraph;
use crate::providers::Embedder;
use crate::retrieval::bm25::bm25_recall;
use crate::retrieval::dense::dense_recall;
use crate::retrieval::rrf::RankedId;
use crate::retrieval::{RecallHit, RecallOpts};
use crate::storage::Storage;
use crate::types::MemoryType;
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

/// Global-mode (GraphRAG) recall: retrieve over `community_summary` memories
/// only. Uses dense KNN when an embedder is available, else BM25.
pub async fn global_recall(
    storage: &Storage,
    embedder: Option<&dyn Embedder>,
    query: &str,
    k: usize,
) -> Result<Vec<RecallHit>> {
    let opts = RecallOpts {
        k: k.max(1) * 5,
        graph: false,
        ..Default::default()
    };
    let base = if let Some(e) = embedder {
        dense_recall(storage, e, query, opts).await?
    } else {
        bm25_recall(storage, query, opts).await?
    };
    let mut hits: Vec<RecallHit> = base
        .into_iter()
        .filter(|h| h.memory.kind == MemoryType::CommunitySummary)
        .collect();
    hits.truncate(k);
    Ok(hits)
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
