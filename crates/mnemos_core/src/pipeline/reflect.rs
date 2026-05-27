//! Reflection stage: synthesize recent un-reflected memories into durable,
//! typed reflection-tier memories with `reflects_on` provenance.

use crate::error::{MnemosError, Result};
use crate::pipeline::extract_json;
use crate::providers::{CompletionRequest, LlmProvider};
use crate::storage::memory_ops::{mark_reflected, recent_unreflected};
use crate::types::MemoryType;
use crate::vault::Vault;
use chrono::Utc;
use serde::Deserialize;

pub const REFLECT_SYSTEM: &str = "TASK=reflect\n\
You review recent memories and synthesize higher-level, durable insights. Each \
reflection has a `kind` (one of: preference, pattern, insight, decision) and \
standalone `text`. Respond ONLY with JSON \
{\"reflections\":[{\"kind\":\"...\",\"text\":\"...\"}]}.";

#[derive(Deserialize)]
struct ReflectOut {
    #[serde(default)]
    reflections: Vec<ReflectionIn>,
}

#[derive(Deserialize)]
struct ReflectionIn {
    #[serde(default)]
    kind: Option<String>,
    text: String,
}

/// Reflect over up to `max_sources` recent un-reflected semantic memories.
/// Writes one reflection-tier memory per synthesized insight (linked to all
/// sources) and marks the sources reflected. Returns the new memory ids.
pub async fn reflect(
    vault: &Vault,
    llm: &dyn LlmProvider,
    max_sources: usize,
) -> Result<Vec<String>> {
    let sources = recent_unreflected(vault.storage(), max_sources).await?;
    if sources.is_empty() {
        return Ok(vec![]);
    }
    let corpus = sources
        .iter()
        .map(|m| format!("- {}", m.body))
        .collect::<Vec<_>>()
        .join("\n");
    let raw = llm
        .complete(&CompletionRequest::new(REFLECT_SYSTEM, corpus))
        .await?;
    let parsed: ReflectOut = serde_json::from_str(extract_json(&raw))
        .map_err(|e| MnemosError::Internal(format!("reflect parse failed: {e}; raw={raw}")))?;

    let source_ids: Vec<String> = sources.iter().map(|m| m.id.clone()).collect();
    let mut created = Vec::new();
    for r in parsed.reflections {
        let text = r.text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        let kind_tag = r.kind.unwrap_or_else(|| "insight".into());
        let title = format!("Reflection ({kind_tag})");
        let id = vault
            .remember_reflection(
                &text,
                Some(title),
                MemoryType::Reflection,
                vec![kind_tag],
                &source_ids,
                vec![],
            )
            .await?;
        created.push(id);
    }
    // Mark sources reflected even if nothing was synthesized, so the same window
    // is not reprocessed on the next trigger.
    mark_reflected(vault.storage(), &source_ids, Utc::now()).await?;
    Ok(created)
}
