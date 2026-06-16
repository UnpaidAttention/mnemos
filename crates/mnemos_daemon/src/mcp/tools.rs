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
            "description": "Store a durable memory that persists across sessions. Call this proactively whenever you learn: user preferences or rules, project architecture decisions, environment or tooling details, debugging insights, important facts not obvious from the codebase, corrections to prior memories (use 'correct' instead for updates). Use 'semantic' tier for facts, 'procedural' for how-tos, 'episodic' for session events, 'working' for active-context items. Always include a concise title.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "body": { "type": "string", "description": "The memory content. Be specific and detailed — include names, versions, paths, decisions, and rationale." },
                    "title": { "type": "string", "description": "Short descriptive title (required for quality). Example: 'User prefers DM Sans font'." },
                    "tier": { "type": "string", "enum": ["working","episodic","semantic","procedural","reflection"], "default": "semantic" },
                    "kind": { "type": "string", "default": "fact", "description": "Memory kind: fact, preference, rule, decision, insight, procedure, observation." },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Relevant tags for categorization." },
                    "importance": { "type": "number", "description": "0.0 to 1.0. User preferences and project rules are high (0.7+). Routine observations are low (0.3)." },
                    "workspace": { "type": "string" },
                    "source_tool": { "type": "string" }
                },
                "required": ["body"]
            }
        }),
        json!({
            "name": "recall",
            "description": "Search persistent memory for relevant context. Call this at session start to load project context, when the user references something discussed in a previous session, when you need background on a topic, or before making decisions that might conflict with stored preferences or rules. Returns ranked hits combining keyword, semantic, and graph-based retrieval.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural language query describing what you want to recall." },
                    "k": { "type": "integer", "default": 10 },
                    "tier": { "type": "array", "items": { "type": "string" } },
                    "workspace": { "type": "string" },
                    "include_invalid": { "type": "boolean", "default": false },
                    "explain": { "type": "boolean", "default": false },
                    "rerank": { "type": "boolean", "default": false },
                    "graph": { "type": "boolean", "default": true },
                    "global": { "type": "boolean", "default": false }
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
        json!({
            "name": "reflect",
            "description": "Run a reflection pass now: synthesize recent memories into typed reflections.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "list_reflections",
            "description": "List reflection-tier memories.",
            "inputSchema": {
                "type": "object",
                "properties": { "limit": { "type": "integer", "default": 50 } }
            }
        }),
        json!({
            "name": "correct",
            "description": "Record a correction after you did something wrong and were corrected. Stores wrong→right→why so the mistake isn't repeated. `why` is required.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "wrong": {"type": "string", "description": "What you did incorrectly"},
                    "right": {"type": "string", "description": "The correct approach going forward"},
                    "why": {"type": "string", "description": "Why the correct approach is right (required)"},
                    "trigger": {"type": "string", "description": "The situation this applies to"},
                    "supersedes": {"type": "string", "description": "Optional id of a prior memory this invalidates"}
                },
                "required": ["wrong", "right", "why"]
            }
        }),
        json!({
            "name": "persist_synthesis",
            "description": "Save a synthesized answer, summary, or comparison back to the vault.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "content": { "type": "string" },
                    "sources": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["title", "content", "sources"]
            }
        }),
        json!({
            "name": "ingest_source",
            "description": "Ingest a raw file or document, summarize it, and extract key facts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "lint_vault",
            "description": "Trigger a semantic linter health check on the vault.",
            "inputSchema": {
                "type": "object",
                "properties": {}
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
        "reflect" => reflect_tool(state, args).await,
        "list_reflections" => list_reflections_tool(state, args).await,
        "correct" => correct(state, args).await,
        "persist_synthesis" => persist_synthesis(state, args).await,
        "ingest_source" => ingest_source(state, args).await,
        "lint_vault" => lint_vault(state, args).await,
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

    if args["global"].as_bool().unwrap_or(false) {
        let hits = crate::routes::recall_helper::global(state, query, k).await?;
        return Ok(tool_content_json(json!({ "hits": hits })));
    }

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
        graph: args["graph"].as_bool().unwrap_or(true),
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
            ..Default::default()
        })
        .await?;
    Ok(tool_content_json(json!({ "memories": memories })))
}

async fn reflect_tool(state: &AppState, _args: &Value) -> anyhow::Result<Value> {
    let llm = state
        .llm
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no LLM configured; reflection unavailable"))?;
    let created = mnemos_core::pipeline::reflect::reflect(
        &state.vault,
        llm.as_ref(),
        state.config.reflection.max_sources,
    )
    .await?;
    Ok(tool_content_json(json!({ "created": created })))
}

async fn list_reflections_tool(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    use mnemos_core::storage::memory_ops::ListFilter;
    let limit = args["limit"].as_u64().map(|n| n as usize);
    let reflections = state
        .vault
        .list(ListFilter {
            tiers: Some(vec![mnemos_core::Tier::Reflection]),
            limit,
            ..Default::default()
        })
        .await?;
    Ok(tool_content_json(json!({ "reflections": reflections })))
}

async fn correct(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    let get = |k: &str| args[k].as_str().map(String::from);
    let correction = mnemos_core::correction::Correction {
        wrong: get("wrong").ok_or_else(|| anyhow::anyhow!("wrong required"))?,
        right: get("right").ok_or_else(|| anyhow::anyhow!("right required"))?,
        why: get("why").ok_or_else(|| anyhow::anyhow!("why required"))?,
        trigger: get("trigger"),
    };
    let id = state
        .vault
        .remember_correction(correction, get("supersedes"))
        .await?;
    Ok(tool_content_json(json!({ "id": id })))
}

fn tool_content_json(value: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": value.to_string()
        }]
    })
}

async fn persist_synthesis(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    let title = args["title"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("title required"))?
        .to_string();
    let content = args["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("content required"))?
        .to_string();
    let sources_json = args["sources"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("sources array required"))?;

    let mut sources = Vec::new();
    for v in sources_json {
        if let Some(s) = v.as_str() {
            sources.push(s.to_string());
        }
    }

    let id = state
        .vault
        .remember_synthesis(
            &content,
            Some(title),
            vec!["synthesis".to_string()],
            &sources,
            vec![],
        )
        .await?;

    Ok(tool_content_json(json!({
        "status": "success",
        "synthesis_id": id
    })))
}

async fn ingest_source(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    let path_str = args["path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("path required"))?;
    let path = std::path::Path::new(path_str);

    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| anyhow::anyhow!("failed to read source file: {e}"))?;

    let llm = state
        .llm
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no LLM configured; ingestion unavailable"))?;

    let chunk = mnemos_core::types::Chunk {
        id: mnemos_core::id::new_memory_id(),
        session_id: "sess_source_ingest".into(),
        speaker: Some("source_document".into()),
        ordinal: 0,
        body: content.clone(),
        created_at: chrono::Utc::now(),
        source_tool: Some("ingest_source".into()),
        source_meta: Some(serde_json::json!({ "file_path": path_str })),
    };

    let custom_schema = state.vault.load_custom_schema();
    let facts = mnemos_core::pipeline::extract::extract_facts(
        &[chunk],
        llm.as_ref(),
        custom_schema.as_deref(),
    )
    .await?;

    let prov = mnemos_core::types::Provenance {
        session: Some("sess_source_ingest".into()),
        chunks: vec![path_str.to_string()],
    };

    let mut added_ids = Vec::new();
    for fact in &facts {
        let (_op, new_id) = mnemos_core::pipeline::resolve::resolve_and_apply(
            &state.vault,
            fact,
            prov.clone(),
            llm.as_ref(),
        )
        .await?;
        if let Some(id) = new_id {
            added_ids.push(id);
        }
    }

    let summary_title = format!(
        "Summary of {}",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    let summary_prompt = mnemos_core::providers::CompletionRequest::new(
        "Generate a concise one-paragraph summary of the following document content.",
        &content,
    );
    let summary_body = llm.complete(&summary_prompt).await?;
    let summary_id = state
        .vault
        .remember(
            &summary_body,
            mnemos_core::vault::RememberOpts {
                title: Some(summary_title),
                tier: mnemos_core::Tier::Reflection,
                kind: mnemos_core::types::MemoryType::SourceSummary,
                tags: vec!["source-summary".to_string()],
                source_tool: Some("ingest_source".into()),
                ..Default::default()
            },
        )
        .await?;

    Ok(tool_content_json(json!({
        "status": "success",
        "summary_id": summary_id,
        "facts_extracted": facts.len(),
        "facts_added": added_ids
    })))
}

async fn lint_vault(state: &AppState, _args: &Value) -> anyhow::Result<Value> {
    let llm = state
        .llm
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no LLM configured; linting unavailable"))?;

    let result = mnemos_core::pipeline::lint::run_lint(state.vault.storage(), llm.as_ref()).await?;
    Ok(tool_content_json(serde_json::to_value(result)?))
}
