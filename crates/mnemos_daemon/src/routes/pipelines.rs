//! `GET /v1/pipelines` — pipeline status (counters, recent runs, configured model).
//! `POST /v1/maintenance/decay` — trigger an on-demand decay pass.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use mnemos_core::pipeline::decay::DecayConfig;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/pipelines", get(status))
        .route("/v1/maintenance/decay", post(run_decay))
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
