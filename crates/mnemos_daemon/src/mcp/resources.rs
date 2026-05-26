//! MCP resource handlers.
//!
//! Plan 3 ships three resources:
//!   - mnemos://working      → full working tier
//!   - mnemos://recent       → last 20 memories created
//!   - mnemos://memory/{id}  → single memory by id (Plan 5+ extends with entity, session)

use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::Tier;
use serde_json::{json, Value};

use crate::state::AppState;

pub fn list_descriptors() -> Vec<Value> {
    vec![
        json!({
            "uri": "mnemos://working",
            "name": "Working memory",
            "description": "Always-loaded memories (identity, current projects, hard constraints).",
            "mimeType": "application/json",
        }),
        json!({
            "uri": "mnemos://recent",
            "name": "Recent memories",
            "description": "Last 20 memories created across all tiers.",
            "mimeType": "application/json",
        }),
    ]
}

pub async fn read(state: &AppState, uri: &str) -> anyhow::Result<Value> {
    if uri == "mnemos://working" {
        let memories = state
            .vault
            .list(ListFilter {
                tiers: Some(vec![Tier::Working]),
                include_invalid: false,
                limit: Some(64),
                ..Default::default()
            })
            .await?;
        return Ok(content_json(uri, json!({ "memories": memories })));
    }
    if uri == "mnemos://recent" {
        let memories = state
            .vault
            .list(ListFilter {
                limit: Some(20),
                ..Default::default()
            })
            .await?;
        return Ok(content_json(uri, json!({ "memories": memories })));
    }
    if let Some(id) = uri.strip_prefix("mnemos://memory/") {
        let mem = state.vault.get(id).await?;
        return Ok(content_json(uri, serde_json::to_value(mem)?));
    }
    Err(anyhow::anyhow!("unknown resource uri: {uri}"))
}

fn content_json(uri: &str, value: Value) -> Value {
    json!({
        "contents": [{
            "uri": uri,
            "mimeType": "application/json",
            "text": value.to_string()
        }]
    })
}
