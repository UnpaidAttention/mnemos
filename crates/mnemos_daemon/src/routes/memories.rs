//! REST endpoints over the memory CRUD + retrieval surface.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use mnemos_core::retrieval::RecallOpts;
use mnemos_core::storage::audit::list_audit;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::types::MemoryType;
use mnemos_core::vault::RememberOpts;
use mnemos_core::Tier;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/memories", post(post_memory).get(list_memories))
        .route("/v1/memories/search", post(search))
        .route("/v1/memories/time-travel", post(time_travel))
        .route(
            "/v1/memories/{id}",
            get(get_memory).patch(patch_memory).delete(delete_memory),
        )
        .route("/v1/memories/{id}/audit", get(audit))
}

#[derive(Debug, Deserialize)]
struct PostMemoryReq {
    body: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default = "default_tier")]
    tier: String,
    #[serde(default = "default_kind")]
    kind: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    importance: Option<f64>,
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    source_tool: Option<String>,
}

fn default_tier() -> String {
    "semantic".into()
}
fn default_kind() -> String {
    "fact".into()
}

#[derive(Debug, Serialize)]
struct PostMemoryResp {
    id: String,
}

async fn post_memory(
    State(state): State<AppState>,
    Json(req): Json<PostMemoryReq>,
) -> Result<(StatusCode, Json<PostMemoryResp>), ApiError> {
    let tier = Tier::from_str(&req.tier)
        .map_err(|e| ApiError::bad_request(format!("invalid tier: {e}")))?;
    let kind: MemoryType = serde_json::from_str(&format!("\"{}\"", req.kind))
        .map_err(|e| ApiError::bad_request(format!("invalid kind: {e}")))?;
    let id = state
        .vault
        .remember(
            &req.body,
            RememberOpts {
                title: req.title,
                tier,
                kind,
                tags: req.tags,
                importance: req.importance,
                workspace: req.workspace,
                source_tool: req.source_tool,
                provenance: vec![],
            },
        )
        .await?;
    // Fetch the stored record so we can emit accurate title + tier.
    if let Ok(mem) = state.vault.get(&id).await {
        state.events.publish(crate::events::Event::MemoryCreated {
            id: id.clone(),
            title: mem.title.clone(),
            tier: mem.tier.as_str().to_string(),
        });
    }
    Ok((StatusCode::CREATED, Json(PostMemoryResp { id })))
}

async fn get_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<mnemos_core::types::Memory>, ApiError> {
    let mem = state.vault.get(&id).await?;
    Ok(Json(mem))
}

#[derive(Debug, Deserialize)]
struct PatchMemoryReq {
    // Fields land in Plan 4; struct accepted now so clients don't get 422.
    #[serde(default)]
    #[allow(dead_code)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    #[allow(dead_code)]
    importance: Option<f64>,
}

async fn patch_memory(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
    Json(_req): Json<PatchMemoryReq>,
) -> Result<StatusCode, ApiError> {
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        "PATCH lands in Plan 4 — use file edits for now",
    ))
}

#[derive(Debug, Deserialize)]
struct DeleteQuery {
    #[serde(default)]
    reason: Option<String>,
}

async fn delete_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<DeleteQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.vault.forget(&id, q.reason.as_deref()).await?;
    state
        .events
        .publish(crate::events::Event::MemoryInvalidated {
            id: id.clone(),
            reason: q.reason.clone(),
        });
    Ok(Json(
        serde_json::json!({ "id": id, "status": "invalidated" }),
    ))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default)]
    tier: Option<Vec<String>>,
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    include_invalid: bool,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

async fn list_memories(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tiers = q.tier.as_ref().map(|ts| {
        ts.iter()
            .filter_map(|t| Tier::from_str(t).ok())
            .collect::<Vec<_>>()
    });
    let memories = state
        .vault
        .list(ListFilter {
            tiers,
            workspace: q.workspace,
            include_invalid: q.include_invalid,
            limit: Some(q.limit),
        })
        .await?;
    Ok(Json(serde_json::json!({ "memories": memories })))
}

#[derive(Debug, Deserialize)]
struct SearchReq {
    query: String,
    #[serde(default = "default_k")]
    k: usize,
    #[serde(default)]
    tier: Option<Vec<String>>,
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    include_invalid: bool,
    #[serde(default)]
    explain: bool,
    #[serde(default)]
    rerank: bool,
}

fn default_k() -> usize {
    10
}

async fn search(
    State(state): State<AppState>,
    Json(req): Json<SearchReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tiers = req.tier.as_ref().map(|ts| {
        ts.iter()
            .filter_map(|t| Tier::from_str(t).ok())
            .collect::<Vec<_>>()
    });
    let opts = RecallOpts {
        k: req.k,
        tiers,
        workspace: req.workspace,
        include_invalid: req.include_invalid,
        explain: req.explain,
        rerank: req.rerank,
        ..Default::default()
    };
    let hits = crate::routes::recall_helper::recall(&state, &req.query, opts).await?;
    Ok(Json(serde_json::json!({ "hits": hits })))
}

#[derive(Debug, Deserialize)]
struct TimeTravelReq {
    #[allow(dead_code)]
    query: String,
    #[allow(dead_code)]
    as_of: String,
    #[serde(default = "default_k")]
    #[allow(dead_code)]
    k: usize,
}

async fn time_travel(
    State(_state): State<AppState>,
    Json(_req): Json<TimeTravelReq>,
) -> Result<StatusCode, ApiError> {
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        "time-travel lands in Plan 4",
    ))
}

async fn audit(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let entries = list_audit(state.vault.storage(), Some(&id)).await?;
    Ok(Json(serde_json::json!({ "entries": entries })))
}
