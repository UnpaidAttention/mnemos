//! Community detection stage: Louvain over the entity graph, persist membership,
//! and write one `community_summary` memory per community (>= min size).

use crate::error::{MnemosError, Result};
use crate::graph::community::louvain;
use crate::graph::MemoryGraph;
use crate::pipeline::extract_json;
use crate::providers::{CompletionRequest, LlmProvider};
use crate::storage::community_ops::store_communities;
use crate::storage::entity_ops::entity_names;
use crate::types::MemoryType;
use crate::vault::Vault;
use chrono::Utc;
use serde::Deserialize;
use std::collections::BTreeMap;

pub const COMMUNITY_SYSTEM: &str = "TASK=community\n\
You are given the named entities of one knowledge-graph community. Write a \
concise summary of the theme that connects them. Respond ONLY with JSON \
{\"title\":\"...\",\"summary\":\"...\"}.";

#[derive(Deserialize)]
struct CommunityOut {
    #[serde(default)]
    title: Option<String>,
    summary: String,
}

/// Detect communities, persist membership, and summarize each community of at
/// least `min_size` entities into a `community_summary` reflection memory.
/// Returns the new summary memory ids.
pub async fn detect_and_summarize(
    vault: &Vault,
    llm: &dyn LlmProvider,
    min_size: usize,
) -> Result<Vec<String>> {
    let graph = MemoryGraph::load(vault.storage()).await?;
    if graph.is_empty() {
        return Ok(vec![]);
    }
    let comm = louvain(&graph);

    // Persist membership for every entity.
    let assignments: Vec<(String, usize)> = comm
        .iter()
        .enumerate()
        .map(|(i, &c)| (graph.entity_id(i).to_string(), c))
        .collect();
    store_communities(vault.storage(), &assignments, Utc::now()).await?;

    // Group node indices by community.
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, &c) in comm.iter().enumerate() {
        groups.entry(c).or_default().push(i);
    }

    let mut created = Vec::new();
    for (cid, members) in groups {
        if members.len() < min_size {
            continue;
        }
        let ids: Vec<String> = members
            .iter()
            .map(|&i| graph.entity_id(i).to_string())
            .collect();
        let names = entity_names(vault.storage(), &ids).await?;
        if names.is_empty() {
            continue;
        }
        let prompt = format!("Community {cid} entities: {}", names.join(", "));
        let raw = llm
            .complete(&CompletionRequest::new(COMMUNITY_SYSTEM, prompt))
            .await?;
        let parsed: CommunityOut = serde_json::from_str(extract_json(&raw)).map_err(|e| {
            MnemosError::Internal(format!("community parse failed: {e}; raw={raw}"))
        })?;
        let summary = parsed.summary.trim().to_string();
        if summary.is_empty() {
            continue;
        }
        let title = parsed.title.unwrap_or_else(|| format!("Community {cid}"));
        let id = vault
            .remember_reflection(
                &summary,
                Some(title),
                MemoryType::CommunitySummary,
                vec!["community".into()],
                &[],
                vec![],
            )
            .await?;
        created.push(id);
    }
    Ok(created)
}
