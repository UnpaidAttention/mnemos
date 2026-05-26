//! `/v1/working` — returns all memories in the Working tier.

use axum::{extract::State, routing::get, Json, Router};
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::Tier;

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/working", get(get_working))
}

async fn get_working(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let memories = state
        .vault
        .list(ListFilter {
            tiers: Some(vec![Tier::Working]),
            workspace: None,
            include_invalid: false,
            limit: Some(64),
        })
        .await?;
    Ok(Json(serde_json::json!({ "memories": memories })))
}
