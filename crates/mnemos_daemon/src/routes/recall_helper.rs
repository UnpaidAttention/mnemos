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

// ── Layer 2: entity expansion ────────────────────────────────────────────────

/// Maximum number of entity-expanded memories to append.
const MAX_ENTITY_EXPAND: usize = 6;

/// Expand recall hits by following entity links.
///
/// For each hit's `.entities`, find other valid memories that share at least one
/// entity but are NOT already in the original hit set. Append them with a
/// discounted score so they don't crowd out direct matches.
///
/// Uses a single SQL query with `json_each(entities_json)` to avoid N+1.
pub async fn expand_entities(state: &AppState, mut hits: Vec<RecallHit>) -> Result<Vec<RecallHit>> {
    if hits.is_empty() {
        return Ok(hits);
    }

    // Collect all unique entity names from the original hits.
    let mut entities: Vec<String> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for h in &hits {
        seen_ids.insert(h.memory.id.clone());
        for e in &h.memory.entities {
            let lower = e.to_lowercase();
            if !entities.contains(&lower) {
                entities.push(lower);
            }
        }
    }

    if entities.is_empty() {
        return Ok(hits);
    }

    // Query: find valid memories sharing at least one entity, excluding already-seen IDs.
    // Uses json_each to unnest the entities_json array and match against our list.
    let conn = state.vault.storage().conn()?;
    let id_excludes = seen_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let entity_placeholders = entities.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    let sql = format!(
        "SELECT DISTINCT m.id, m.tier, m.kind, m.title, m.body,
                m.tags_json, m.entities_json, m.links_json, m.provenance_json,
                m.created_at, m.ingested_at, m.valid_at, m.invalid_at, m.superseded_by,
                m.strength, m.importance, m.last_accessed, m.access_count,
                m.workspace, m.source_tool, m.mnemos_version
           FROM memories m, json_each(m.entities_json) je
          WHERE m.invalid_at IS NULL
            AND m.id NOT IN ({id_excludes})
            AND LOWER(je.value) IN ({entity_placeholders})
          ORDER BY m.importance DESC, m.created_at DESC
          LIMIT ?",
    );

    let mut args: Vec<libsql::Value> = Vec::new();
    for id in &seen_ids {
        args.push(id.clone().into());
    }
    for e in &entities {
        args.push(e.clone().into());
    }
    args.push((MAX_ENTITY_EXPAND as i64).into());

    let mut rows = conn.query(&sql, args).await?;
    let mut expanded = Vec::new();
    while let Some(row) = rows.next().await? {
        let mem = mnemos_core::storage::memory_ops::row_to_memory(&row)?;
        // Assign a discounted score so entity-expanded hits rank below direct matches.
        let min_score = hits.last().map(|h| h.score).unwrap_or(0.0);
        expanded.push(RecallHit {
            score: min_score * 0.5, // half the lowest direct-match score
            bm25_rank: None,
            dense_rank: None,
            dense_distance: None,
            ppr_rank: None,
            explain: None,
            memory: mem,
        });
    }

    hits.extend(expanded);
    Ok(hits)
}
