//! MCP tool implementations. Each wraps the relevant Vault/retrieval call.

use mnemos_core::retrieval::RecallOpts;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::types::MemoryType;
use mnemos_core::vault::RememberOpts;
use mnemos_core::Tier;
use serde_json::{json, Value};
use std::str::FromStr;

use crate::state::AppState;

/// Returns the MCP tool descriptors. Schemas are JSON Schema 2020-12.
pub fn descriptors() -> Vec<Value> {
    vec![
        json!({
            "name": "remember",
            "description": "Store a new memory. Returns its id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "body": { "type": "string" },
                    "title": { "type": "string" },
                    "tier": { "type": "string", "enum": ["working","episodic","semantic","procedural","reflection"], "default": "semantic" },
                    "kind": { "type": "string", "default": "fact" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "importance": { "type": "number" },
                    "workspace": { "type": "string" },
                    "source_tool": { "type": "string" }
                },
                "required": ["body"]
            }
        }),
        json!({
            "name": "recall",
            "description": "Hybrid search (BM25 + dense). Returns ranked hits.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "k": { "type": "integer", "default": 10 },
                    "tier": { "type": "array", "items": { "type": "string" } },
                    "workspace": { "type": "string" },
                    "include_invalid": { "type": "boolean", "default": false },
                    "explain": { "type": "boolean", "default": false },
                    "rerank": { "type": "boolean", "default": false }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "forget",
            "description": "Soft-invalidate a memory by id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string" },
                    "reason": { "type": "string" }
                },
                "required": ["memory_id"]
            }
        }),
        json!({
            "name": "get_memory",
            "description": "Fetch a single memory by id.",
            "inputSchema": {
                "type": "object",
                "properties": { "memory_id": { "type": "string" } },
                "required": ["memory_id"]
            }
        }),
        json!({
            "name": "list_memories",
            "description": "List memories with optional filters.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tier": { "type": "array", "items": { "type": "string" } },
                    "workspace": { "type": "string" },
                    "include_invalid": { "type": "boolean", "default": false },
                    "limit": { "type": "integer", "default": 50 }
                }
            }
        }),
    ]
}

/// Dispatch a tool call. Returns the MCP `content` array.
pub async fn call(state: &AppState, name: &str, args: &Value) -> anyhow::Result<Value> {
    match name {
        "remember" => remember(state, args).await,
        "recall" => recall(state, args).await,
        "forget" => forget(state, args).await,
        "get_memory" => get_memory(state, args).await,
        "list_memories" => list_memories(state, args).await,
        other => Err(anyhow::anyhow!("unknown tool: {other}")),
    }
}

async fn remember(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    let body = args["body"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("body required"))?
        .to_string();
    let tier_str = args["tier"].as_str().unwrap_or("semantic");
    let tier = Tier::from_str(tier_str).map_err(|e| anyhow::anyhow!("invalid tier: {e}"))?;
    let kind_str = args["kind"].as_str().unwrap_or("fact");
    let kind: MemoryType = serde_json::from_str(&format!("\"{kind_str}\""))?;
    let id = state
        .vault
        .remember(
            &body,
            RememberOpts {
                title: args["title"].as_str().map(String::from),
                tier,
                kind,
                tags: args["tags"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                importance: args["importance"].as_f64(),
                workspace: args["workspace"].as_str().map(String::from),
                source_tool: args["source_tool"].as_str().map(String::from),
                provenance: vec![],
            },
        )
        .await?;
    Ok(tool_content_json(json!({ "id": id })))
}

async fn recall(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    let query = args["query"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("query required"))?;
    let k = args["k"].as_u64().unwrap_or(10) as usize;
    let tiers = args["tier"].as_array().map(|a| {
        a.iter()
            .filter_map(|v| v.as_str())
            .filter_map(|s| Tier::from_str(s).ok())
            .collect()
    });
    let opts = RecallOpts {
        k,
        tiers,
        workspace: args["workspace"].as_str().map(String::from),
        include_invalid: args["include_invalid"].as_bool().unwrap_or(false),
        explain: args["explain"].as_bool().unwrap_or(false),
        rerank: args["rerank"].as_bool().unwrap_or(false),
        ..Default::default()
    };
    let hits = crate::routes::recall_helper::recall(state, query, opts).await?;
    Ok(tool_content_json(json!({ "hits": hits })))
}

async fn forget(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    let id = args["memory_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("memory_id required"))?;
    let reason = args["reason"].as_str();
    state.vault.forget(id, reason).await?;
    Ok(tool_content_json(
        json!({ "id": id, "status": "invalidated" }),
    ))
}

async fn get_memory(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    let id = args["memory_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("memory_id required"))?;
    let mem = state.vault.get(id).await?;
    Ok(tool_content_json(serde_json::to_value(mem)?))
}

async fn list_memories(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    let tiers = args["tier"].as_array().map(|a| {
        a.iter()
            .filter_map(|v| v.as_str())
            .filter_map(|s| Tier::from_str(s).ok())
            .collect()
    });
    let memories = state
        .vault
        .list(ListFilter {
            tiers,
            workspace: args["workspace"].as_str().map(String::from),
            include_invalid: args["include_invalid"].as_bool().unwrap_or(false),
            limit: args["limit"].as_u64().map(|n| n as usize),
        })
        .await?;
    Ok(tool_content_json(json!({ "memories": memories })))
}

fn tool_content_json(value: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": value.to_string()
        }]
    })
}
