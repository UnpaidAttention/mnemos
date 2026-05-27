//! Reflection endpoints: trigger a reflection pass + list reflection memories.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use mnemos_core::pipeline::reflect::reflect;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::Tier;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/reflections", post(run_reflect).get(list_reflections))
}

async fn run_reflect(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let llm = state.llm.clone().ok_or_else(|| {
        ApiError::new(
            StatusCode::CONFLICT,
            "no LLM configured; reflection unavailable",
        )
    })?;
    let created = reflect(
        &state.vault,
        llm.as_ref(),
        state.config.reflection.max_sources,
    )
    .await?;
    if !created.is_empty() {
        let _ = mnemos_core::storage::reflection_ops::reset_salience(
            state.vault.storage(),
            chrono::Utc::now(),
        )
        .await;
    }
    state
        .events
        .publish(crate::events::Event::ReflectionCompleted {
            reflections_created: created.len(),
        });
    Ok(Json(json!({ "created": created })))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

async fn list_reflections(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let reflections = state
        .vault
        .list(ListFilter {
            tiers: Some(vec![Tier::Reflection]),
            limit: Some(q.limit),
            ..Default::default()
        })
        .await?;
    Ok(Json(json!({ "reflections": reflections })))
}
