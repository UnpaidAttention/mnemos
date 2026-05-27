//! `GET /v1/pipelines` — pipeline status (counters, recent runs, configured model).

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/pipelines", get(status))
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
