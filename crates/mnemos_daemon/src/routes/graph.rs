//! `GET /v1/graph` — the whole entity graph (nodes + active edges) for the UI
//! graph view. Node `community_id` is -1 when community detection hasn't run.

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/graph", get(get_graph))
}

async fn get_graph(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    use mnemos_core::error::MnemosError;
    let conn = state.vault.storage().conn()?;

    let mut nrows = conn
        .query(
            "SELECT e.id, e.name, e.kind,
                    COALESCE(ec.community_id, -1) AS community_id,
                    (SELECT COUNT(*) FROM entity_mentions m WHERE m.entity_id = e.id) AS mentions
               FROM entities e
               LEFT JOIN entity_communities ec ON ec.entity_id = e.id
              ORDER BY e.created_at",
            (),
        )
        .await
        .map_err(MnemosError::from)?;
    let mut nodes: Vec<Value> = Vec::new();
    while let Some(r) = nrows.next().await.map_err(MnemosError::from)? {
        nodes.push(json!({
            "id": r.get::<String>(0).map_err(MnemosError::from)?,
            "name": r.get::<String>(1).map_err(MnemosError::from)?,
            "kind": r.get::<String>(2).map_err(MnemosError::from)?,
            "community_id": r.get::<i64>(3).map_err(MnemosError::from)?,
            "mentions": r.get::<i64>(4).map_err(MnemosError::from)?,
        }));
    }
    drop(nrows);

    let mut erows = conn
        .query(
            "SELECT id, source_entity_id, target_entity_id, relation, weight
               FROM entity_edges WHERE invalid_at IS NULL",
            (),
        )
        .await
        .map_err(MnemosError::from)?;
    let mut edges: Vec<Value> = Vec::new();
    while let Some(r) = erows.next().await.map_err(MnemosError::from)? {
        edges.push(json!({
            "id": r.get::<String>(0).map_err(MnemosError::from)?,
            "source": r.get::<String>(1).map_err(MnemosError::from)?,
            "target": r.get::<String>(2).map_err(MnemosError::from)?,
            "relation": r.get::<String>(3).map_err(MnemosError::from)?,
            "weight": r.get::<f64>(4).map_err(MnemosError::from)?,
        }));
    }

    Ok(Json(json!({ "nodes": nodes, "edges": edges })))
}
