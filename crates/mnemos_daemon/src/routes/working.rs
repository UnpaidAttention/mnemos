//! `GET /v1/working` — returns the working set (working-tier memories +
//! hardened reflection rules), optionally scoped to a workspace.
//!
//! Exposes `build_working_set` as the single canonical implementation shared
//! with the MCP `mnemos://working` resource (`mcp::resources`). The workspace
//! query param is the only extension over the MCP path.

use axum::{extract::Query, extract::State, routing::get, Json, Router};
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::Tier;
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiError;
use crate::mcp::resources::HARDENED_CAP;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/working", get(get_working))
}

#[derive(Debug, Deserialize, Default)]
struct WorkingQuery {
    workspace: Option<String>,
}

async fn get_working(
    State(state): State<AppState>,
    Query(q): Query<WorkingQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let payload = build_working_set(&state, q.workspace.as_deref()).await?;
    Ok(Json(payload))
}

/// Build the working-set payload: working-tier memories + hardened rules,
/// optionally filtered to a workspace.
///
/// This is the canonical implementation shared by the HTTP route and the MCP
/// `mnemos://working` resource.
pub(crate) async fn build_working_set(
    state: &AppState,
    workspace: Option<&str>,
) -> Result<serde_json::Value, ApiError> {
    let memories = state
        .vault
        .list(ListFilter {
            tiers: Some(vec![Tier::Working]),
            workspace: workspace.map(str::to_owned),
            include_invalid: false,
            limit: Some(64),
            ..Default::default()
        })
        .await?;

    // P2-11: push the tag filter into SQL instead of hydrating the whole
    // Reflection tier and filtering in Rust.
    let mut hardened = state
        .vault
        .list(ListFilter {
            tiers: Some(vec![Tier::Reflection]),
            include_invalid: false,
            limit: Some(HARDENED_CAP * 4), // over-fetch so sort+truncate has room
            required_tags: vec!["mnemos:hardened".to_owned()],
            ..Default::default()
        })
        .await?;

    // Rank: importance desc, then created_at desc (newest first).
    hardened.sort_by(|a, b| {
        b.importance
            .partial_cmp(&a.importance)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });
    hardened.truncate(HARDENED_CAP);

    Ok(if hardened.is_empty() {
        json!({ "memories": memories })
    } else {
        json!({ "memories": memories, "hardened_rules": hardened })
    })
}
