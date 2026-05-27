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
        .route("/v1/entities/{id}/graph", get(entity_graph))
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
    use mnemos_core::error::MnemosError;
    let conn = state.vault.storage().conn()?;

    let mut rows = conn
        .query(
            "SELECT id, name, kind, aliases, description FROM entities WHERE id = ?",
            params![id.clone()],
        )
        .await
        .map_err(MnemosError::from)?;
    let row = rows
        .next()
        .await
        .map_err(MnemosError::from)?
        .ok_or_else(|| ApiError::not_found(format!("entity {id}")))?;
    let aliases: Vec<String> =
        serde_json::from_str(&row.get::<String>(3).map_err(MnemosError::from)?).unwrap_or_default();
    let detail = serde_json::json!({
        "id": row.get::<String>(0).map_err(MnemosError::from)?,
        "name": row.get::<String>(1).map_err(MnemosError::from)?,
        "kind": row.get::<String>(2).map_err(MnemosError::from)?,
        "aliases": aliases,
        "description": row.get::<Option<String>>(4).map_err(MnemosError::from)?,
    });
    drop(rows);

    // memory ids that mention this entity
    let mut mrows = conn
        .query(
            "SELECT memory_id FROM entity_mentions WHERE entity_id = ?",
            params![id.clone()],
        )
        .await
        .map_err(MnemosError::from)?;
    let mut memory_ids: Vec<String> = Vec::new();
    while let Some(r) = mrows.next().await.map_err(MnemosError::from)? {
        memory_ids.push(r.get::<String>(0).map_err(MnemosError::from)?);
    }
    drop(mrows);

    // incident active edges
    let mut erows = conn
        .query(
            "SELECT id, source_entity_id, target_entity_id, relation, weight
               FROM entity_edges
              WHERE (source_entity_id = ?1 OR target_entity_id = ?1) AND invalid_at IS NULL",
            params![id.clone()],
        )
        .await
        .map_err(MnemosError::from)?;
    let mut edges: Vec<serde_json::Value> = Vec::new();
    while let Some(r) = erows.next().await.map_err(MnemosError::from)? {
        edges.push(serde_json::json!({
            "id": r.get::<String>(0).map_err(MnemosError::from)?,
            "source": r.get::<String>(1).map_err(MnemosError::from)?,
            "target": r.get::<String>(2).map_err(MnemosError::from)?,
            "relation": r.get::<String>(3).map_err(MnemosError::from)?,
            "weight": r.get::<f64>(4).map_err(MnemosError::from)?,
        }));
    }

    let mut detail = detail;
    detail["mention_count"] = serde_json::json!(memory_ids.len());
    detail["memory_ids"] = serde_json::json!(memory_ids);
    detail["edges"] = serde_json::json!(edges);
    Ok(Json(detail))
}

async fn entity_graph(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    use mnemos_core::error::MnemosError;
    use std::collections::BTreeSet;
    let conn = state.vault.storage().conn()?;

    // incident edges → neighbor ids
    let mut erows = conn
        .query(
            "SELECT id, source_entity_id, target_entity_id, relation, weight
               FROM entity_edges
              WHERE (source_entity_id = ?1 OR target_entity_id = ?1) AND invalid_at IS NULL",
            params![id.clone()],
        )
        .await
        .map_err(MnemosError::from)?;
    let mut edges: Vec<serde_json::Value> = Vec::new();
    let mut ids: BTreeSet<String> = BTreeSet::new();
    ids.insert(id.clone());
    while let Some(r) = erows.next().await.map_err(MnemosError::from)? {
        let src: String = r.get(1).map_err(MnemosError::from)?;
        let tgt: String = r.get(2).map_err(MnemosError::from)?;
        ids.insert(src.clone());
        ids.insert(tgt.clone());
        edges.push(serde_json::json!({
            "id": r.get::<String>(0).map_err(MnemosError::from)?,
            "source": src, "target": tgt,
            "relation": r.get::<String>(3).map_err(MnemosError::from)?,
            "weight": r.get::<f64>(4).map_err(MnemosError::from)?,
        }));
    }
    drop(erows);

    // node detail for self + neighbors
    let mut nodes: Vec<serde_json::Value> = Vec::new();
    for nid in &ids {
        let mut nr = conn
            .query(
                "SELECT id, name, kind FROM entities WHERE id = ?",
                params![nid.clone()],
            )
            .await
            .map_err(MnemosError::from)?;
        if let Some(r) = nr.next().await.map_err(MnemosError::from)? {
            nodes.push(serde_json::json!({
                "id": r.get::<String>(0).map_err(MnemosError::from)?,
                "name": r.get::<String>(1).map_err(MnemosError::from)?,
                "kind": r.get::<String>(2).map_err(MnemosError::from)?,
            }));
        }
    }

    Ok(Json(serde_json::json!({ "nodes": nodes, "edges": edges })))
}
