//! Entity routes. Plan 3 ships the surface; Plan 4 (entity-linking pipeline)
//! and Plan 5 (PPR retrieval) populate it. For Plan 3 the list endpoint queries
//! the `entities` table directly — empty until Plan 4 starts writing rows.

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use libsql::params;
use mnemos_core::types::Entity;
use serde::Deserialize;

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/entities", get(list_entities))
        .route("/v1/entities/{id}", get(get_entity))
        .route("/v1/entities/{id}/graph", get(entity_graph_stub))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

async fn list_entities(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state.vault.storage().conn()?;
    let mut rows = conn
        .query(
            "SELECT id, name, kind, aliases, description, file_path, created_at \
             FROM entities ORDER BY created_at DESC LIMIT ?",
            params![q.limit as i64],
        )
        .await
        .map_err(mnemos_core::error::MnemosError::from)?;

    let mut entities: Vec<Entity> = Vec::new();
    while let Some(r) = rows
        .next()
        .await
        .map_err(mnemos_core::error::MnemosError::from)?
    {
        let aliases_str: String = r.get(3).map_err(mnemos_core::error::MnemosError::from)?;
        let aliases: Vec<String> = serde_json::from_str(&aliases_str).unwrap_or_default();
        let created_at_str: String = r.get(6).map_err(mnemos_core::error::MnemosError::from)?;
        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|d| d.with_timezone(&chrono::Utc))
            .map_err(|e| ApiError::internal(e.to_string()))?;
        entities.push(Entity {
            id: r.get(0).map_err(mnemos_core::error::MnemosError::from)?,
            name: r.get(1).map_err(mnemos_core::error::MnemosError::from)?,
            kind: r.get(2).map_err(mnemos_core::error::MnemosError::from)?,
            aliases,
            description: r.get(4).map_err(mnemos_core::error::MnemosError::from)?,
            file_path: r.get(5).map_err(mnemos_core::error::MnemosError::from)?,
            created_at,
        });
    }
    Ok(Json(serde_json::json!({ "entities": entities })))
}

async fn get_entity(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state.vault.storage().conn()?;
    let mut rows = conn
        .query(
            "SELECT id, name, kind, aliases, description, file_path, created_at \
             FROM entities WHERE id = ?",
            params![id.clone()],
        )
        .await
        .map_err(mnemos_core::error::MnemosError::from)?;
    match rows
        .next()
        .await
        .map_err(mnemos_core::error::MnemosError::from)?
    {
        Some(r) => Ok(Json(serde_json::json!({
            "id":   r.get::<String>(0).map_err(mnemos_core::error::MnemosError::from)?,
            "name": r.get::<String>(1).map_err(mnemos_core::error::MnemosError::from)?,
            "kind": r.get::<String>(2).map_err(mnemos_core::error::MnemosError::from)?,
        }))),
        None => Err(ApiError::not_found(format!("entity {id}"))),
    }
}

async fn entity_graph_stub(
    State(_): State<AppState>,
    Path(_): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({ "nodes": [], "edges": [] })))
}
