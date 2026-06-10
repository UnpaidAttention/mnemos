//! `GET /v1/pipelines` — pipeline status (counters, recent runs, configured model).
//! `POST /v1/maintenance/decay` — trigger an on-demand decay pass.
//! `POST /v1/maintenance/communities` — trigger community detection + summarization.
//! `POST /v1/maintenance/backfill` — retroactively run entity extraction, graph
//!   building, and reflections on all existing semantic memories.
//! `POST /v1/maintenance/wipe-graph` — truncate all entity graph tables.
//! `POST /v1/maintenance/cleanup-graph` — remove garbage entities/edges.
//! `POST /v1/maintenance/bulk-ingest` — ingest markdown files from a directory.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use mnemos_core::pipeline::co_mention::create_co_mention_edges;
use mnemos_core::pipeline::community::detect_and_summarize;
use mnemos_core::pipeline::decay::DecayConfig;
use mnemos_core::pipeline::entities::link_entities;
use mnemos_core::pipeline::graph::update_graph;
use mnemos_core::pipeline::reflect::reflect;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::storage::reflection_ops::{bump_salience, reset_salience};
use mnemos_core::Tier;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/pipelines", get(status))
        .route("/v1/maintenance/decay", post(run_decay))
        .route("/v1/maintenance/communities", post(run_communities))
        .route("/v1/maintenance/backfill", post(run_backfill))
        .route("/v1/maintenance/wipe-graph", post(run_wipe_graph))
        .route("/v1/maintenance/cleanup-graph", post(run_cleanup_graph))
        .route("/v1/maintenance/bulk-ingest", post(run_bulk_ingest))
}

async fn status(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let (counters, recent) = state.pipeline_status.snapshot().await;
    let model = state.llm.as_ref().map(|l| l.model_id().to_string());
    Ok(Json(json!({
        "enabled": state.llm.is_some(),
        "llm_model": model,
        "counters": counters,
        "recent": recent,
    })))
}

async fn run_decay(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let stats = state.vault.run_decay(&DecayConfig::default()).await?;
    Ok(Json(json!({
        "scanned": stats.scanned,
        "decayed": stats.decayed,
        "invalidated": stats.to_invalidate.len(),
        "invalidated_ids": stats.to_invalidate,
    })))
}

async fn run_communities(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let llm = state.llm.clone().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::CONFLICT,
            "no LLM configured; community detection unavailable",
        )
    })?;
    let summaries = detect_and_summarize(
        &state.vault,
        llm.as_ref(),
        state.config.community.min_community_size,
    )
    .await?;
    state
        .events
        .publish(crate::events::Event::CommunityDetected {
            communities: summaries.len(),
        });
    Ok(Json(json!({ "summaries": summaries })))
}

/// Retroactively run entity extraction, graph building, and reflections on all
/// existing semantic memories. This populates the Graph, Reflections, and
/// Knowledge tabs from memories that were created before the LLM was enabled.
///
/// The endpoint is idempotent: re-running it upserts entities/edges rather than
/// duplicating them. Each memory is processed independently; failures on one
/// memory are logged and skipped.
async fn run_backfill(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let llm = state.llm.clone().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::CONFLICT,
            "no LLM configured; backfill unavailable",
        )
    })?;

    // Load all valid semantic memories.
    let memories = state
        .vault
        .list(ListFilter {
            tiers: Some(vec![Tier::Semantic]),
            include_invalid: false,
            ..Default::default()
        })
        .await
        .map_err(|e| ApiError::internal(format!("list memories: {e}")))?;

    let total = memories.len();
    tracing::info!(total, "backfill: starting entity extraction + graph update");

    let mut entities_linked = 0usize;
    let mut edges_created = 0usize;
    let mut errors = 0usize;

    for (i, mem) in memories.iter().enumerate() {
        tracing::info!(
            progress = format!("{}/{}", i + 1, total),
            memory_id = %mem.id,
            "backfill: processing memory"
        );

        // 1. Entity extraction + linking
        match link_entities(state.vault.storage(), &mem.id, &mem.body, llm.as_ref()).await {
            Ok(ids) => entities_linked += ids.len(),
            Err(e) => {
                tracing::warn!(
                    memory_id = %mem.id,
                    error = %e,
                    "backfill: entity linking failed; skipping"
                );
                errors += 1;
                continue;
            }
        }

        // 2. Co-mention edge inference (no LLM needed)
        match create_co_mention_edges(state.vault.storage(), &mem.id, mem.valid_at).await {
            Ok(n) => edges_created += n,
            Err(e) => {
                tracing::warn!(
                    memory_id = %mem.id,
                    error = %e,
                    "backfill: co-mention edges failed; continuing"
                );
            }
        }

        // 3. Relationship extraction + graph edges (LLM-generated triples)
        match update_graph(
            state.vault.storage(),
            &mem.id,
            &mem.body,
            mem.valid_at,
            llm.as_ref(),
        )
        .await
        {
            Ok(ids) => edges_created += ids.len(),
            Err(e) => {
                tracing::warn!(
                    memory_id = %mem.id,
                    error = %e,
                    "backfill: graph update failed; continuing"
                );
                errors += 1;
            }
        }
    }

    // 3. Bump salience by the number of memories processed and trigger
    //    reflections if the threshold is crossed.
    let mut reflections_created = 0usize;
    if total > 0 {
        let salience = bump_salience(state.vault.storage(), total as f64)
            .await
            .unwrap_or(0.0);
        if salience >= state.config.reflection.salience_threshold {
            match reflect(
                &state.vault,
                llm.as_ref(),
                state.config.reflection.max_sources,
            )
            .await
            {
                Ok(created) => {
                    reflections_created = created.len();
                    let _ = reset_salience(state.vault.storage(), chrono::Utc::now()).await;
                    state
                        .events
                        .publish(crate::events::Event::ReflectionCompleted {
                            reflections_created,
                        });
                }
                Err(e) => {
                    tracing::warn!(error = %e, "backfill: reflection pass failed");
                    errors += 1;
                }
            }
        }
    }

    // 4. Run community detection on the newly-populated graph.
    let mut communities_found = 0usize;
    match detect_and_summarize(
        &state.vault,
        llm.as_ref(),
        state.config.community.min_community_size,
    )
    .await
    {
        Ok(summaries) => {
            communities_found = summaries.len();
            if communities_found > 0 {
                state
                    .events
                    .publish(crate::events::Event::CommunityDetected {
                        communities: communities_found,
                    });
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "backfill: community detection failed");
            errors += 1;
        }
    }

    tracing::info!(
        total,
        entities_linked,
        edges_created,
        reflections_created,
        communities_found,
        errors,
        "backfill: complete"
    );

    Ok(Json(json!({
        "memories_processed": total,
        "entities_linked": entities_linked,
        "edges_created": edges_created,
        "reflections_created": reflections_created,
        "communities_found": communities_found,
        "errors": errors,
    })))
}

/// Truncate all entity graph tables (entities, edges, mentions, communities).
/// Memories table is preserved. Use before a full re-backfill.
async fn run_wipe_graph(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    use mnemos_core::error::MnemosError;
    let (conn, _guard) = state.vault.storage().write_conn().await?;

    let del_communities: u64 = conn
        .execute("DELETE FROM entity_communities", ())
        .await
        .map_err(MnemosError::from)?;
    let del_edges: u64 = conn
        .execute("DELETE FROM entity_edges", ())
        .await
        .map_err(MnemosError::from)?;
    let del_mentions: u64 = conn
        .execute("DELETE FROM entity_mentions", ())
        .await
        .map_err(MnemosError::from)?;
    let del_entities: u64 = conn
        .execute("DELETE FROM entities", ())
        .await
        .map_err(MnemosError::from)?;

    tracing::info!(
        del_entities,
        del_edges,
        del_mentions,
        del_communities,
        "wipe-graph: complete"
    );

    Ok(Json(json!({
        "entities_deleted": del_entities,
        "edges_deleted": del_edges,
        "mentions_deleted": del_mentions,
        "communities_deleted": del_communities,
        "status": "wiped",
    })))
}

/// Remove garbage entities and edges:
/// - Single-character entity names (A, B, C, etc.)
/// - Edges with generic "REL" relation label
/// - Orphaned entities (no edges AND no mentions)
async fn run_cleanup_graph(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    use mnemos_core::error::MnemosError;
    let (conn, _guard) = state.vault.storage().write_conn().await?;

    // 1. Delete single-char entities and their edges/mentions
    let del_single_char_edges: u64 = conn
        .execute(
            "DELETE FROM entity_edges WHERE source_entity_id IN (SELECT id FROM entities WHERE LENGTH(name) <= 1)
                OR target_entity_id IN (SELECT id FROM entities WHERE LENGTH(name) <= 1)",
            (),
        )
        .await
        .map_err(MnemosError::from)?;
    let del_single_char_mentions: u64 = conn
        .execute(
            "DELETE FROM entity_mentions WHERE entity_id IN (SELECT id FROM entities WHERE LENGTH(name) <= 1)",
            (),
        )
        .await
        .map_err(MnemosError::from)?;
    let del_single_char_communities: u64 = conn
        .execute(
            "DELETE FROM entity_communities WHERE entity_id IN (SELECT id FROM entities WHERE LENGTH(name) <= 1)",
            (),
        )
        .await
        .map_err(MnemosError::from)?;
    let del_single_char: u64 = conn
        .execute("DELETE FROM entities WHERE LENGTH(name) <= 1", ())
        .await
        .map_err(MnemosError::from)?;

    // 2. Delete edges with generic "REL" relation
    let del_rel_edges: u64 = conn
        .execute("DELETE FROM entity_edges WHERE relation = 'REL'", ())
        .await
        .map_err(MnemosError::from)?;

    // 3. Delete orphaned entities (no edges AND no mentions)
    let del_orphans: u64 = conn
        .execute(
            "DELETE FROM entities WHERE id NOT IN (
                SELECT DISTINCT source_entity_id FROM entity_edges WHERE invalid_at IS NULL
                UNION
                SELECT DISTINCT target_entity_id FROM entity_edges WHERE invalid_at IS NULL
            ) AND id NOT IN (
                SELECT DISTINCT entity_id FROM entity_mentions
            )",
            (),
        )
        .await
        .map_err(MnemosError::from)?;

    tracing::info!(
        del_single_char,
        del_single_char_edges,
        del_rel_edges,
        del_orphans,
        "cleanup-graph: complete"
    );

    Ok(Json(json!({
        "single_char_entities_deleted": del_single_char,
        "single_char_edges_deleted": del_single_char_edges,
        "single_char_mentions_deleted": del_single_char_mentions,
        "single_char_communities_deleted": del_single_char_communities,
        "rel_edges_deleted": del_rel_edges,
        "orphan_entities_deleted": del_orphans,
        "status": "cleaned",
    })))
}

#[derive(Deserialize)]
struct BulkIngestReq {
    directory: String,
}

/// Bulk-ingest markdown files from a directory. Each .md file is split into
/// chunks, inserted as a session, and processed through the full extraction
/// pipeline (extract_facts → resolve_and_apply → link_entities → co_mention →
/// update_graph). This produces distilled knowledge entries rather than raw
/// file copies.
async fn run_bulk_ingest(
    State(state): State<AppState>,
    Json(req): Json<BulkIngestReq>,
) -> Result<Json<Value>, ApiError> {
    use chrono::Utc;
    use mnemos_core::pipeline::co_mention::create_co_mention_edges;
    use mnemos_core::pipeline::extract::extract_facts;
    use mnemos_core::pipeline::resolve::resolve_and_apply;
    use mnemos_core::pipeline::ResolveOp;
    use mnemos_core::types::{Chunk, Provenance};

    let llm = state.llm.clone().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::CONFLICT,
            "no LLM configured; bulk-ingest unavailable",
        )
    })?;

    let dir = std::path::Path::new(&req.directory);
    if !dir.is_dir() {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            format!("not a directory: {}", req.directory),
        ));
    }

    // Collect .md files
    let mut md_files: Vec<std::path::PathBuf> = Vec::new();
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| ApiError::internal(format!("read_dir: {e}")))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| ApiError::internal(format!("next_entry: {e}")))?
    {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "md") && path.is_file() {
            md_files.push(path);
        }
    }
    md_files.sort();

    let total = md_files.len();
    tracing::info!(total, directory = %req.directory, "bulk-ingest: starting");

    let mut facts_extracted = 0usize;
    let mut memories_created = 0usize;
    let mut entities_linked = 0usize;
    let mut edges_created = 0usize;
    let mut errors = 0usize;

    let custom_schema = state.vault.load_custom_schema();

    for (i, path) in md_files.iter().enumerate() {
        let filename = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("file-{i}"));

        let body = match tokio::fs::read_to_string(&path).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "bulk-ingest: read failed");
                errors += 1;
                continue;
            }
        };

        if body.trim().is_empty() {
            continue;
        }

        tracing::info!(
            progress = format!("{}/{}", i + 1, total),
            file = %filename,
            "bulk-ingest: processing"
        );

        // Split the file body into chunks. For conversation logs, split by
        // double-newlines (paragraphs) to create meaningful chunks.
        // Each chunk is at most ~2000 chars to stay within LLM context limits.
        let paragraphs: Vec<&str> = body.split("\n\n").collect();
        let mut chunks: Vec<Chunk> = Vec::new();
        let mut current_chunk = String::new();
        let now = Utc::now();
        let session_id = format!("bulk-ingest-{filename}");

        for para in &paragraphs {
            let trimmed = para.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !current_chunk.is_empty() && current_chunk.len() + trimmed.len() > 2000 {
                // Flush current chunk
                let chunk_id = format!("chunk_{session_id}_{}", chunks.len());
                chunks.push(Chunk {
                    id: chunk_id,
                    session_id: session_id.clone(),
                    speaker: Some("user".to_string()),
                    ordinal: chunks.len() as u32,
                    body: std::mem::take(&mut current_chunk),
                    created_at: now,
                    source_tool: Some("Claude Code conversations bulk import".to_string()),
                    source_meta: None,
                });
            }
            if !current_chunk.is_empty() {
                current_chunk.push_str("\n\n");
            }
            current_chunk.push_str(trimmed);
        }
        // Flush remaining
        if !current_chunk.is_empty() {
            let chunk_id = format!("chunk_{session_id}_{}", chunks.len());
            chunks.push(Chunk {
                id: chunk_id,
                session_id: session_id.clone(),
                speaker: Some("user".to_string()),
                ordinal: chunks.len() as u32,
                body: current_chunk,
                created_at: now,
                source_tool: Some("Claude Code conversations bulk import".to_string()),
                source_meta: None,
            });
        }

        if chunks.is_empty() {
            continue;
        }

        tracing::info!(
            file = %filename,
            chunks = chunks.len(),
            "bulk-ingest: extracting facts"
        );

        // Step 1: Extract facts from chunks using the LLM
        let facts = match extract_facts(&chunks, llm.as_ref(), custom_schema.as_deref()).await {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(
                    file = %filename,
                    error = %e,
                    "bulk-ingest: fact extraction failed"
                );
                errors += 1;
                continue;
            }
        };

        tracing::info!(
            file = %filename,
            facts = facts.len(),
            "bulk-ingest: resolving facts"
        );
        facts_extracted += facts.len();

        let chunk_ids: Vec<String> = chunks.iter().map(|c| c.id.clone()).collect();
        let prov = Provenance {
            session: Some(session_id.clone()),
            chunks: chunk_ids,
        };

        // Step 2: Resolve each fact against existing memories (ADD/UPDATE/DELETE/NOOP)
        for fact in &facts {
            let (op, new_id) =
                match resolve_and_apply(&state.vault, fact, prov.clone(), llm.as_ref()).await {
                    Ok(result) => result,
                    Err(e) => {
                        tracing::warn!(
                            file = %filename,
                            error = %e,
                            "bulk-ingest: resolve_and_apply failed for a fact; skipping"
                        );
                        errors += 1;
                        continue;
                    }
                };

            if let Some(mid) = new_id {
                if matches!(op, ResolveOp::Add | ResolveOp::Update { .. }) {
                    memories_created += 1;
                }

                // Step 3: Entity linking + graph update on the new memory
                if let Ok(mem) = state.vault.get(&mid).await {
                    match link_entities(state.vault.storage(), &mid, &mem.body, llm.as_ref()).await
                    {
                        Ok(ids) => entities_linked += ids.len(),
                        Err(e) => {
                            tracing::warn!(memory_id = %mid, error = %e, "bulk-ingest: entity linking failed");
                        }
                    }

                    match create_co_mention_edges(state.vault.storage(), &mid, mem.valid_at).await {
                        Ok(n) => edges_created += n,
                        Err(e) => {
                            tracing::warn!(memory_id = %mid, error = %e, "bulk-ingest: co-mention failed");
                        }
                    }

                    match update_graph(
                        state.vault.storage(),
                        &mid,
                        &mem.body,
                        mem.valid_at,
                        llm.as_ref(),
                    )
                    .await
                    {
                        Ok(ids) => edges_created += ids.len(),
                        Err(e) => {
                            tracing::warn!(memory_id = %mid, error = %e, "bulk-ingest: graph update failed");
                        }
                    }
                }
            }
        }

        tracing::info!(
            file = %filename,
            "bulk-ingest: file complete"
        );
    }

    tracing::info!(
        total,
        facts_extracted,
        memories_created,
        entities_linked,
        edges_created,
        errors,
        "bulk-ingest: complete"
    );

    Ok(Json(json!({
        "files_found": total,
        "facts_extracted": facts_extracted,
        "memories_created": memories_created,
        "entities_linked": entities_linked,
        "edges_created": edges_created,
        "errors": errors,
    })))
}
