use crate::error::{MnemosError, Result};
use crate::pipeline::extract_json;
use crate::providers::{CompletionRequest, LlmProvider};
use crate::storage::entity_ops::{upsert_edge, upsert_entity};
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use serde::Deserialize;

/// System prompt for the graph-update stage.
pub const RELATIONS_SYSTEM: &str = "TASK=relations\n\
Extract relationships between entities as subject–relation–object triples. \
The relation should be a specific, descriptive verb phrase (e.g. \"is built with\", \
\"stores data in\", \"was created by\", \"prefers over\") — never use generic labels \
like \"REL\" or \"has\". Respond ONLY with JSON \
{\"relations\":[{\"source\":\"A\",\"relation\":\"descriptive verb phrase\",\"target\":\"B\"}]}.";

#[derive(Deserialize)]
struct RelOut {
    #[serde(default)]
    relations: Vec<Triple>,
}

#[derive(Deserialize)]
struct Triple {
    source: String,
    relation: String,
    target: String,
}

/// Extract relationship triples from `body` and upsert the corresponding
/// entities and edges. `valid_at` stamps newly-created edges (bi-temporal).
/// Returns the edge ids touched.
pub async fn update_graph(
    storage: &Storage,
    memory_id: &str,
    body: &str,
    valid_at: DateTime<Utc>,
    llm: &dyn LlmProvider,
) -> Result<Vec<String>> {
    let raw = llm
        .complete(&CompletionRequest::new(RELATIONS_SYSTEM, body))
        .await?;
    let parsed: RelOut = serde_json::from_str(extract_json(&raw))
        .map_err(|e| MnemosError::Internal(format!("relations parse failed: {e}; raw={raw}")))?;
    let mut edge_ids = Vec::new();
    for t in parsed.relations {
        let (s, r, o) = (t.source.trim(), t.relation.trim(), t.target.trim());
        if s.is_empty() || r.is_empty() || o.is_empty() {
            continue;
        }
        let src = upsert_entity(storage, s, "unknown", None).await?;
        let tgt = upsert_entity(storage, o, "unknown", None).await?;
        let edge = upsert_edge(storage, &src, &tgt, r, memory_id, valid_at).await?;
        edge_ids.push(edge);
    }
    Ok(edge_ids)
}
