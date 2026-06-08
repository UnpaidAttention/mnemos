//! `GET /v1/pipelines` — pipeline status (counters, recent runs, configured model).
//! `POST /v1/maintenance/decay` — trigger an on-demand decay pass.
//! `POST /v1/maintenance/communities` — trigger community detection + summarization.
//! `POST /v1/maintenance/backfill` — retroactively run entity extraction, graph
//!   building, and reflections on all existing semantic memories.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use mnemos_core::pipeline::community::detect_and_summarize;
use mnemos_core::pipeline::decay::DecayConfig;
use mnemos_core::pipeline::entities::link_entities;
use mnemos_core::pipeline::graph::update_graph;
use mnemos_core::pipeline::reflect::reflect;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::storage::reflection_ops::{bump_salience, reset_salience};
use mnemos_core::Tier;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/pipelines", get(status))
        .route("/v1/maintenance/decay", post(run_decay))
        .route("/v1/maintenance/communities", post(run_communities))
        .route("/v1/maintenance/backfill", post(run_backfill))
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

        // 2. Relationship extraction + graph edges
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

