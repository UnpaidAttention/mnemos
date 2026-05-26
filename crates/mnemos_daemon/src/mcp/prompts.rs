//! MCP prompt templates.
//!
//! `context-for(workspace?)` — composes a system prompt from the working tier
//! (and Plan 5 will add procedural rules + recent reflections).

use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::Tier;
use serde_json::{json, Value};

use crate::state::AppState;

pub fn list_descriptors() -> Vec<Value> {
    vec![json!({
        "name": "context-for",
        "description": "Returns a system prompt with the user's working memory + procedural rules.",
        "arguments": [
            { "name": "workspace", "description": "Optional workspace scope", "required": false }
        ]
    })]
}

pub async fn get(state: &AppState, name: &str, _args: &Value) -> anyhow::Result<Value> {
    match name {
        "context-for" => context_for(state).await,
        other => Err(anyhow::anyhow!("unknown prompt: {other}")),
    }
}

async fn context_for(state: &AppState) -> anyhow::Result<Value> {
    let working = state
        .vault
        .list(ListFilter {
            tiers: Some(vec![Tier::Working]),
            include_invalid: false,
            limit: Some(64),
            ..Default::default()
        })
        .await?;
    let procedural = state
        .vault
        .list(ListFilter {
            tiers: Some(vec![Tier::Procedural]),
            include_invalid: false,
            limit: Some(64),
            ..Default::default()
        })
        .await?;

    let mut text = String::new();
    text.push_str("# Persistent context from Mnemos\n\n");
    if !working.is_empty() {
        text.push_str("## Working memory\n");
        for m in &working {
            text.push_str(&format!(
                "- {} — {}\n",
                m.title,
                m.body.lines().next().unwrap_or("")
            ));
        }
        text.push('\n');
    }
    if !procedural.is_empty() {
        text.push_str("## Procedural rules\n");
        for m in &procedural {
            text.push_str(&format!(
                "- {}: {}\n",
                m.title,
                m.body.lines().next().unwrap_or("")
            ));
        }
        text.push('\n');
    }

    // Append raw bodies for the model to read in full (working tier only, capped).
    text.push_str("---\n");
    for m in &working {
        text.push_str(&format!("\n[{}]\n{}\n", m.title, m.body));
    }

    Ok(json!({
        "messages": [{
            "role": "system",
            "content": { "type": "text", "text": text }
        }]
    }))
}
