//! Shared recall path used by both the REST search endpoint and the MCP recall
//! tool, so the embedder-ref + rerank branching lives in exactly one place.

use mnemos_core::error::Result;
use mnemos_core::retrieval::hybrid::{hybrid_recall, hybrid_recall_with_rerank};
use mnemos_core::retrieval::{RecallHit, RecallOpts};

use crate::state::AppState;

/// Run hybrid recall, applying the cross-encoder reranker when requested and
/// configured. Returns ranked hits.
pub async fn recall(state: &AppState, query: &str, opts: RecallOpts) -> Result<Vec<RecallHit>> {
    let embedder = state.vault.embedder().cloned();
    let embedder_ref = embedder.as_ref().map(|a| a.as_ref());
    if opts.rerank && state.reranker.is_some() {
        let rr = state.reranker.clone().unwrap();
        hybrid_recall_with_rerank(
            state.vault.storage(),
            embedder_ref,
            Some(rr.as_ref()),
            query,
            opts,
        )
        .await
    } else {
        hybrid_recall(state.vault.storage(), embedder_ref, query, opts).await
    }
}
