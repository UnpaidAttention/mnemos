//! Entity routes. Plan 3 ships the surface; Plan 4 (entity-linking pipeline)
//! and Plan 5 (PPR retrieval) populate it. For Plan 3 the list endpoint queries
//! the `entities` table directly — empty until Plan 4 starts writing rows.

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use libsql::params;
use mnemos_core::storage::entity_ops::merge_entities;
use mnemos_core::types::Entity;
use serde::Deserialize;

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/entities", get(list_entities))
        .route("/v1/entities/merge", post(merge_route))
        .route("/v1/entities/{id}", get(get_entity))
        .route("/v1/entities/{id}/graph", get(entity_graph))
}

#[derive(Debug, Deserialize)]
struct MergeReq {
    source: String,
    target: String,
}

async fn merge_route(
    State(state): State<AppState>,
    Json(req): Json<MergeReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    merge_entities(state.vault.storage(), &req.source, &req.target).await?;
    mnemos_core::storage::audit::write_audit(
        state.vault.storage(),
        "mnemos-cli",
        "entity_merge",
        None,
        Some(serde_json::json!({ "source": req.source, "target": req.target })),
    )
    .await?;
    Ok(Json(serde_json::json!({
        "source": req.source,
        "target": req.target,
        "status": "merged",
    })))
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
            "SELECT id, name, kind, aliases, description, created_at FROM entities WHERE id = ?",
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
    let created_at: String = row.get::<String>(5).map_err(MnemosError::from)?;
    let mut detail = serde_json::json!({
        "id": row.get::<String>(0).map_err(MnemosError::from)?,
        "name": row.get::<String>(1).map_err(MnemosError::from)?,
        "kind": row.get::<String>(2).map_err(MnemosError::from)?,
        "aliases": aliases,
        "description": row.get::<Option<String>>(4).map_err(MnemosError::from)?,
        "created_at": created_at,
    });
    drop(rows);

    // memories that mention this entity — fetch title + body preview + tier + created_at
    let mut mrows = conn
        .query(
            "SELECT m.id, m.title, m.body, m.tier, m.created_at FROM memories m \
             JOIN entity_mentions em ON em.memory_id = m.id \
             WHERE em.entity_id = ? AND m.invalid_at IS NULL",
            params![id.clone()],
        )
        .await
        .map_err(MnemosError::from)?;
    let mut memories: Vec<serde_json::Value> = Vec::new();
    let mut memory_ids: Vec<String> = Vec::new();
    while let Some(r) = mrows.next().await.map_err(MnemosError::from)? {
        let mid: String = r.get::<String>(0).map_err(MnemosError::from)?;
        let title: String = r.get::<String>(1).map_err(MnemosError::from)?;
        let body: String = r.get::<String>(2).map_err(MnemosError::from)?;
        let tier: String = r.get::<String>(3).map_err(MnemosError::from)?;
        let mem_created_at: String = r.get::<String>(4).map_err(MnemosError::from)?;
        let preview = if body.len() > 300 {
            format!(
                "{}…",
                &body[..body
                    .char_indices()
                    .take_while(|(i, _)| *i <= 297)
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0)]
            )
        } else {
            body
        };
        memory_ids.push(mid.clone());
        memories.push(serde_json::json!({
            "id": mid,
            "title": title,
            "body_preview": preview,
            "tier": tier,
            "created_at": mem_created_at,
        }));
    }
    drop(mrows);

    // Incident active edges — with source/target names and kinds
    let mut erows = conn
        .query(
            "SELECT ee.id, ee.source_entity_id, ee.target_entity_id, ee.relation, ee.weight,
                    es.name AS source_name, es.kind AS source_kind,
                    et.name AS target_name, et.kind AS target_kind
               FROM entity_edges ee
               JOIN entities es ON es.id = ee.source_entity_id
               JOIN entities et ON et.id = ee.target_entity_id
              WHERE (ee.source_entity_id = ?1 OR ee.target_entity_id = ?1) AND ee.invalid_at IS NULL",
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
            "source_name": r.get::<String>(5).map_err(MnemosError::from)?,
            "source_kind": r.get::<String>(6).map_err(MnemosError::from)?,
            "target_name": r.get::<String>(7).map_err(MnemosError::from)?,
            "target_kind": r.get::<String>(8).map_err(MnemosError::from)?,
        }));
    }
    drop(erows);

    // Co-mentioned entities: entities that share at least one memory with this entity
    let mut co_rows = conn
        .query(
            "SELECT e.id, e.name, e.kind, COUNT(DISTINCT em2.memory_id) AS shared_count
               FROM entity_mentions em1
               JOIN entity_mentions em2 ON em1.memory_id = em2.memory_id
               JOIN entities e ON e.id = em2.entity_id
              WHERE em1.entity_id = ?1 AND em2.entity_id != ?1
              GROUP BY e.id, e.name, e.kind
              ORDER BY shared_count DESC
              LIMIT 50",
            params![id.clone()],
        )
        .await
        .map_err(MnemosError::from)?;
    let mut co_mentioned: Vec<serde_json::Value> = Vec::new();
    while let Some(r) = co_rows.next().await.map_err(MnemosError::from)? {
        co_mentioned.push(serde_json::json!({
            "id": r.get::<String>(0).map_err(MnemosError::from)?,
            "name": r.get::<String>(1).map_err(MnemosError::from)?,
            "kind": r.get::<String>(2).map_err(MnemosError::from)?,
            "shared_memory_count": r.get::<i64>(3).map_err(MnemosError::from)?,
        }));
    }
    drop(co_rows);

    // Community info
    let mut com_rows = conn
        .query(
            "SELECT community_id FROM entity_communities WHERE entity_id = ?",
            params![id.clone()],
        )
        .await
        .map_err(MnemosError::from)?;
    let community = if let Some(r) = com_rows.next().await.map_err(MnemosError::from)? {
        let cid: i64 = r.get(0).map_err(MnemosError::from)?;
        drop(com_rows);
        // Try to find a community summary memory
        let mut sum_rows = conn
            .query(
                "SELECT m.body FROM memories m WHERE m.kind = 'community-summary'
                 AND m.body LIKE '%community ' || ?1 || '%' AND m.invalid_at IS NULL LIMIT 1",
                params![cid.to_string()],
            )
            .await
            .map_err(MnemosError::from)?;
        let summary = if let Some(sr) = sum_rows.next().await.map_err(MnemosError::from)? {
            Some(sr.get::<String>(0).map_err(MnemosError::from)?)
        } else {
            None
        };
        Some(serde_json::json!({ "id": cid, "summary": summary }))
    } else {
        drop(com_rows);
        None
    };

    detail["mention_count"] = serde_json::json!(memory_ids.len());
    detail["memory_ids"] = serde_json::json!(memory_ids);
    detail["memories"] = serde_json::json!(memories);
    detail["edges"] = serde_json::json!(edges);
    detail["co_mentioned_entities"] = serde_json::json!(co_mentioned);
    detail["community"] = serde_json::json!(community);
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
