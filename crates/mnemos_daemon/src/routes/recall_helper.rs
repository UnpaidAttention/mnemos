//! Shared recall path used by both the REST search endpoint and the MCP recall
//! tool, so retriever wiring (embedder, reranker, graph) lives in one place.

use mnemos_core::error::Result;
use mnemos_core::graph::MemoryGraph;
use mnemos_core::retrieval::hybrid::hybrid_recall_full;
use mnemos_core::retrieval::{RecallHit, RecallOpts};

use crate::state::AppState;

/// Run hybrid recall: BM25 + Dense + (optional) graph PPR, with reranking when
/// requested + configured. The graph is built per-call from storage and is
/// skipped automatically when empty.
pub async fn recall(state: &AppState, query: &str, mut opts: RecallOpts) -> Result<Vec<RecallHit>> {
    opts.ppr_alpha = state.config.retrieval.ppr_alpha;
    opts.ppr_iterations = state.config.retrieval.ppr_iterations;

    let embedder = state.vault.embedder().cloned();
    let embedder_ref = embedder.as_ref().map(|a| a.as_ref());

    let graph = if opts.graph {
        let g = MemoryGraph::load(state.vault.storage()).await?;
        if g.is_empty() {
            None
        } else {
            Some(g)
        }
    } else {
        None
    };

    let reranker = state.reranker.clone();
    let reranker_ref = reranker.as_ref().map(|a| a.as_ref());

    hybrid_recall_full(
        state.vault.storage(),
        embedder_ref,
        reranker_ref,
        graph.as_ref(),
        query,
        opts,
    )
    .await
}

/// Global-mode recall over community summaries.
pub async fn global(state: &AppState, query: &str, k: usize) -> Result<Vec<RecallHit>> {
    let embedder = state.vault.embedder().cloned();
    let embedder_ref = embedder.as_ref().map(|a| a.as_ref());
    mnemos_core::retrieval::graph_recall::global_recall(
        state.vault.storage(),
        embedder_ref,
        query,
        k,
    )
    .await
}
