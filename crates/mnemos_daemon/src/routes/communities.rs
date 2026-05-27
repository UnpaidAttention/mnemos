//! `GET /v1/communities` — detected communities (id + member entities) plus the
//! `community_summary` memories. The UI correlates summaries to communities
//! loosely (no strict FK yet).

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};
use std::collections::BTreeMap;

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/communities", get(get_communities))
}

async fn get_communities(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    use mnemos_core::error::MnemosError;
    let conn = state.vault.storage().conn()?;

    let mut rows = conn
        .query(
            "SELECT ec.community_id, e.id, e.name
               FROM entity_communities ec
               JOIN entities e ON e.id = ec.entity_id
              ORDER BY ec.community_id, e.name",
            (),
        )
        .await
        .map_err(MnemosError::from)?;
    let mut grouped: BTreeMap<i64, Vec<Value>> = BTreeMap::new();
    while let Some(r) = rows.next().await.map_err(MnemosError::from)? {
        let cid: i64 = r.get(0).map_err(MnemosError::from)?;
        let id: String = r.get(1).map_err(MnemosError::from)?;
        let name: String = r.get(2).map_err(MnemosError::from)?;
        grouped
            .entry(cid)
            .or_default()
            .push(json!({ "id": id, "name": name }));
    }
    drop(rows);

    let communities: Vec<Value> = grouped
        .into_iter()
        .map(|(community_id, members)| json!({ "community_id": community_id, "members": members }))
        .collect();

    let summaries = mnemos_core::storage::memory_ops::list_by_kind(
        state.vault.storage(),
        mnemos_core::types::MemoryType::CommunitySummary,
        100,
    )
    .await?;

    Ok(Json(
        json!({ "communities": communities, "summaries": summaries }),
    ))
}
