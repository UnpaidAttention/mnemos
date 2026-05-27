use crate::error::{MnemosError, Result};
use crate::pipeline::extract_json;
use crate::providers::{CompletionRequest, LlmProvider};
use crate::storage::entity_ops::{link_entity_mention, upsert_entity};
use crate::storage::Storage;
use serde::Deserialize;

/// System prompt for the entity-linking stage.
pub const LINK_SYSTEM: &str = "TASK=link\n\
List the named entities (people, projects, organizations, tools, concepts) \
mentioned in the text. Respond ONLY with JSON \
{\"entities\":[{\"name\":\"...\",\"kind\":\"...\"}]}.";

#[derive(Deserialize)]
struct LinkOut {
    #[serde(default)]
    entities: Vec<EntityIn>,
}

#[derive(Deserialize)]
struct EntityIn {
    name: String,
    #[serde(default)]
    kind: Option<String>,
}

/// Extract entities from `body`, upsert them, and link mentions to `memory_id`.
/// Returns the entity ids (deduplicated by name via `upsert_entity`).
pub async fn link_entities(
    storage: &Storage,
    memory_id: &str,
    body: &str,
    llm: &dyn LlmProvider,
) -> Result<Vec<String>> {
    let raw = llm
        .complete(&CompletionRequest::new(LINK_SYSTEM, body))
        .await?;
    let parsed: LinkOut = serde_json::from_str(extract_json(&raw))
        .map_err(|e| MnemosError::Internal(format!("link parse failed: {e}; raw={raw}")))?;
    let mut ids = Vec::new();
    for e in parsed.entities {
        let name = e.name.trim();
        if name.is_empty() {
            continue;
        }
        let kind = e.kind.unwrap_or_else(|| "unknown".into());
        let id = upsert_entity(storage, name, &kind).await?;
        link_entity_mention(storage, memory_id, &id).await?;
        ids.push(id);
    }
    Ok(ids)
}
