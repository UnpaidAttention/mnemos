//! Hybrid retrieval orchestrator. Runs BM25 and Dense in parallel, fuses with
//! RRF, applies reweighting, optionally reranks, returns top-k.

use crate::error::Result;
use crate::providers::Embedder;
use crate::retrieval::bm25::bm25_recall;
use crate::retrieval::dense::dense_recall;
use crate::retrieval::reweight::apply_reweight_with_breakdown;
use crate::retrieval::rrf::{rrf_fuse, RankedId};
use crate::retrieval::{Explain, RecallHit, RecallOpts};
use crate::storage::memory_ops::get_memory;
use crate::storage::Storage;
use std::collections::HashMap;

pub async fn hybrid_recall(
    storage: &Storage,
    embedder: Option<&dyn Embedder>,
    query: &str,
    opts: RecallOpts,
) -> Result<Vec<RecallHit>> {
    // Over-fetch from each retriever so RRF has material to fuse.
    let stage_k = (opts.k * 5).max(opts.k);
    let stage_opts = RecallOpts {
        k: stage_k,
        explain: false, // we'll populate explain at the end
        rerank: false,  // rerank only at the orchestration layer
        ..opts.clone()
    };

    // Run BM25 always; Dense only when an embedder is available.
    let bm25 = bm25_recall(storage, query, stage_opts.clone()).await?;
    let dense = if let Some(e) = embedder {
        dense_recall(storage, e, query, stage_opts.clone()).await?
    } else {
        vec![]
    };

    // Build RankedId lists for RRF.
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

    let fused = rrf_fuse(&[&bm25_ranked, &dense_ranked], opts.rrf_k);

    // Index per-retriever rank and distance for explain / RecallHit fields.
    let bm25_rank_by_id: HashMap<&str, usize> = bm25_ranked
        .iter()
        .map(|r| (r.id.as_str(), r.rank))
        .collect();
    let dense_rank_by_id: HashMap<&str, usize> = dense_ranked
        .iter()
        .map(|r| (r.id.as_str(), r.rank))
        .collect();
    let dense_dist_by_id: HashMap<&str, f32> = dense
        .iter()
        .filter_map(|h| h.dense_distance.map(|d| (h.memory.id.as_str(), d)))
        .collect();

    // Hydrate Memory for each fused id, apply reweight, package as RecallHit.
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
            explain,
        });
    }

    // Sort by reweighted score (higher = better).
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(opts.k);

    Ok(hits)
}
