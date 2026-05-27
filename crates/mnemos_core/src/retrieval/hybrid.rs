//! Hybrid retrieval orchestrator. Runs BM25, Dense, and (optionally) graph PPR,
//! fuses with RRF, applies reweighting, optionally reranks, returns top-k.

use crate::error::Result;
use crate::graph::MemoryGraph;
use crate::providers::{Embedder, Reranker};
use crate::retrieval::bm25::bm25_recall;
use crate::retrieval::dense::dense_recall;
use crate::retrieval::graph_recall::graph_rank;
use crate::retrieval::reweight::apply_reweight_with_breakdown;
use crate::retrieval::rrf::{rrf_fuse, RankedId};
use crate::retrieval::{Explain, RecallHit, RecallOpts};
use crate::storage::memory_ops::get_memory;
use crate::storage::Storage;
use std::collections::HashMap;

/// Seed-hit count for the PPR retriever.
const PPR_SEED_HITS: usize = 5;

/// Full hybrid recall: BM25 + Dense + (optional) graph PPR → RRF → reweight →
/// optional rerank. `graph`/`reranker` may be `None`; PPR is skipped unless a
/// non-empty graph is supplied and `opts.graph` is true.
pub async fn hybrid_recall_full(
    storage: &Storage,
    embedder: Option<&dyn Embedder>,
    reranker: Option<&dyn Reranker>,
    graph: Option<&MemoryGraph>,
    query: &str,
    opts: RecallOpts,
) -> Result<Vec<RecallHit>> {
    let stage_k = opts.k * 5;
    let stage_opts = RecallOpts {
        k: stage_k,
        explain: false,
        rerank: false,
        graph: false,
        ..opts.clone()
    };

    let bm25 = bm25_recall(storage, query, stage_opts.clone()).await?;
    let dense = if let Some(e) = embedder {
        dense_recall(storage, e, query, stage_opts.clone()).await?
    } else {
        vec![]
    };
    let ppr_ranked: Vec<RankedId> = match graph {
        Some(g) if opts.graph && !g.is_empty() => {
            graph_rank(
                storage,
                g,
                query,
                opts.ppr_alpha,
                opts.ppr_iterations,
                PPR_SEED_HITS,
            )
            .await?
        }
        _ => vec![],
    };

    let bm25_ranked: Vec<RankedId> = bm25
        .iter()
        .enumerate()
        .map(|(i, h)| RankedId {
            id: h.memory.id.clone(),
            rank: i + 1,
        })
        .collect();
    let dense_ranked: Vec<RankedId> = dense
        .iter()
        .enumerate()
        .map(|(i, h)| RankedId {
            id: h.memory.id.clone(),
            rank: i + 1,
        })
        .collect();

    let fused = rrf_fuse(&[&bm25_ranked, &dense_ranked, &ppr_ranked], opts.rrf_k);

    let bm25_rank_by_id: HashMap<&str, usize> = bm25_ranked
        .iter()
        .map(|r| (r.id.as_str(), r.rank))
        .collect();
    let dense_rank_by_id: HashMap<&str, usize> = dense_ranked
        .iter()
        .map(|r| (r.id.as_str(), r.rank))
        .collect();
    let ppr_rank_by_id: HashMap<&str, usize> =
        ppr_ranked.iter().map(|r| (r.id.as_str(), r.rank)).collect();
    let dense_dist_by_id: HashMap<&str, f32> = dense
        .iter()
        .filter_map(|h| h.dense_distance.map(|d| (h.memory.id.as_str(), d)))
        .collect();

    let mut hits: Vec<RecallHit> = Vec::with_capacity(fused.len());
    for f in fused.iter() {
        let memory = get_memory(storage, &f.id).await?;
        if !opts.include_invalid && memory.invalid_at.is_some() {
            continue;
        }
        let bw = apply_reweight_with_breakdown(f.score, &memory, &opts.reweight);
        let explain = if opts.explain {
            Some(Explain {
                bm25_rank: bm25_rank_by_id.get(f.id.as_str()).copied(),
                dense_rank: dense_rank_by_id.get(f.id.as_str()).copied(),
                dense_distance: dense_dist_by_id.get(f.id.as_str()).copied(),
                ppr_rank: ppr_rank_by_id.get(f.id.as_str()).copied(),
                rrf_score: f.score,
                weight_recency: bw.recency,
                weight_importance: bw.importance,
                weight_strength: bw.strength,
                weight_tier: bw.tier,
                rerank_score: None,
                final_score: bw.final_score,
            })
        } else {
            None
        };
        hits.push(RecallHit {
            memory,
            score: bw.final_score,
            bm25_rank: bm25_rank_by_id.get(f.id.as_str()).copied(),
            dense_rank: dense_rank_by_id.get(f.id.as_str()).copied(),
            dense_distance: dense_dist_by_id.get(f.id.as_str()).copied(),
            ppr_rank: ppr_rank_by_id.get(f.id.as_str()).copied(),
            explain,
        });
    }

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(opts.k);

    if opts.rerank {
        if let Some(rr) = reranker {
            let candidates: Vec<String> = hits
                .iter()
                .map(|h| format!("{}\n\n{}", h.memory.title, h.memory.body))
                .collect();
            let scores = rr.rerank(query, &candidates).await?;
            if scores.len() != hits.len() {
                return Err(crate::error::MnemosError::Internal(format!(
                    "reranker returned {} scores for {} candidates",
                    scores.len(),
                    hits.len()
                )));
            }
            for (h, s) in hits.iter_mut().zip(scores.iter()) {
                let score_f64 = f64::from(*s);
                h.score = score_f64;
                if let Some(e) = h.explain.as_mut() {
                    e.rerank_score = Some(score_f64);
                    e.final_score = score_f64;
                }
            }
            hits.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }

    Ok(hits)
}

/// BM25 + Dense fusion (no graph, no rerank). Back-compatible wrapper.
pub async fn hybrid_recall(
    storage: &Storage,
    embedder: Option<&dyn Embedder>,
    query: &str,
    opts: RecallOpts,
) -> Result<Vec<RecallHit>> {
    hybrid_recall_full(storage, embedder, None, None, query, opts).await
}

/// Hybrid recall with an optional cross-encoder reranker (no graph).
pub async fn hybrid_recall_with_rerank(
    storage: &Storage,
    embedder: Option<&dyn Embedder>,
    reranker: Option<&dyn Reranker>,
    query: &str,
    opts: RecallOpts,
) -> Result<Vec<RecallHit>> {
    hybrid_recall_full(storage, embedder, reranker, None, query, opts).await
}
