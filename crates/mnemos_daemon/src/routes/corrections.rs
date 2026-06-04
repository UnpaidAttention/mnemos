//! POST /v1/corrections — create a correction memory (same validation as the MCP tool).
//! GET  /v1/corrections — list correction memories, newest first.
//!   ?hardened=true lists Reflection-tier memories tagged "mnemos:hardened" instead.

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use mnemos_core::storage::memory_ops::{list_by_kind, ListFilter};
use mnemos_core::types::MemoryType;
use mnemos_core::Tier;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/corrections", get(list).post(create))
}

#[derive(Deserialize)]
struct CreateReq {
    wrong: String,
    right: String,
    why: String,
    trigger: Option<String>,
    supersedes: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateReq>,
) -> Result<Json<Value>, ApiError> {
    let c = mnemos_core::correction::Correction {
        wrong: req.wrong,
        right: req.right,
        why: req.why,
        trigger: req.trigger,
    };
    let id = state
        .vault
        .remember_correction(c, req.supersedes)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
struct ListQ {
    hardened: Option<bool>,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQ>,
) -> Result<Json<Value>, ApiError> {
    if q.hardened == Some(true) {
        // Reflection-tier memories tagged "mnemos:hardened", newest first.
        let all = state
            .vault
            .list(ListFilter {
                tiers: Some(vec![Tier::Reflection]),
                limit: None, // filter in Rust; apply user limit after
                ..Default::default()
            })
            .await?;
        let mut hardened: Vec<_> = all
            .into_iter()
            .filter(|m| m.tags.iter().any(|t| t == "mnemos:hardened"))
            .collect();
        hardened.truncate(q.limit);
        Ok(Json(json!({ "corrections": hardened })))
    } else {
        // Procedural/Correction-kind memories, newest first.
        let corrections =
            list_by_kind(state.vault.storage(), MemoryType::Correction, q.limit).await?;
        Ok(Json(json!({ "corrections": corrections })))
    }
}
