//! Reflection stage: synthesize recent un-reflected memories into durable,
//! typed reflection-tier memories with `reflects_on` provenance.

use crate::error::{MnemosError, Result};
use crate::pipeline::extract_json;
use crate::providers::{CompletionRequest, LlmProvider};
use crate::storage::memory_ops::{list_by_kind, mark_reflected, recent_unreflected};
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

/// System prompt for the correction-hardening LLM call.
pub const HARDEN_SYSTEM: &str = "TASK=harden\n\
You review a cluster of related correction memories sharing a common trigger. \
Synthesize them into ONE concise, actionable rule. \
Respond ONLY with JSON {\"rule\":\"...\"}. \
The rule must be standalone and self-explanatory.";

#[derive(Deserialize)]
struct HardenOut {
    #[serde(default)]
    rule: String,
}

/// Cluster recent un-reflected `Correction` memories by shared trigger tag.
/// For each cluster of `>= min_cluster` members, synthesize one hardened rule
/// (Reflection tier, tagged `mnemos:hardened` and the cluster tag, importance
/// 1.0), link it to the source memories with `reflects_on` edges, and mark
/// the sources reflected so they are not processed again.
///
/// Returns the ids of all newly created hardened-rule memories.
pub async fn harden_corrections(
    vault: &Vault,
    llm: &dyn LlmProvider,
    min_cluster: usize,
) -> Result<Vec<String>> {
    // Load all valid, un-reflected Correction memories.
    // `list_by_kind` queries by kind across all tiers without a `reflected_at`
    // filter, so we apply the reflected filter manually via a direct DB query.
    // Use a generous limit (1000) since correction memories are typically sparse.
    let candidates = list_by_kind(vault.storage(), MemoryType::Correction, 1_000).await?;

    // Keep only those that have not yet been reflected.
    // `list_by_kind` does not filter on `reflected_at`, so we post-filter by
    // loading from the DB column. We do this via a raw query on storage.
    let unreflected = {
        let conn = vault.storage().conn()?;
        let mut rows = conn
            .query(
                "SELECT id FROM memories
                  WHERE kind = 'correction'
                    AND invalid_at IS NULL
                    AND reflected_at IS NULL",
                (),
            )
            .await?;
        let mut ids = std::collections::HashSet::new();
        while let Some(row) = rows.next().await? {
            ids.insert(row.get::<String>(0)?);
        }
        ids
    };

    let corrections: Vec<_> = candidates
        .into_iter()
        .filter(|m| unreflected.contains(&m.id))
        .collect();

    if corrections.is_empty() {
        return Ok(vec![]);
    }

    // Build map: tag → Vec<memory> (excluding the literal "correction" tag).
    let mut tag_map: std::collections::HashMap<String, Vec<&crate::types::Memory>> =
        std::collections::HashMap::new();
    for mem in &corrections {
        for tag in &mem.tags {
            if tag == "correction" {
                continue;
            }
            tag_map.entry(tag.clone()).or_default().push(mem);
        }
    }

    // Process each qualifying cluster. Track used memory ids so each correction
    // is assigned to at most one hardened rule per run.
    let mut used_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut created: Vec<String> = Vec::new();

    // Sort tags for deterministic ordering.
    let mut tags_sorted: Vec<String> = tag_map.keys().cloned().collect();
    tags_sorted.sort();

    for cluster_tag in tags_sorted {
        let members: Vec<&crate::types::Memory> = tag_map[&cluster_tag]
            .iter()
            .filter(|m| !used_ids.contains(&m.id))
            .copied()
            .collect();

        if members.len() < min_cluster {
            continue;
        }

        // Build corpus from member bodies.
        let corpus = members
            .iter()
            .map(|m| format!("- {}", m.body))
            .collect::<Vec<_>>()
            .join("\n");

        // Call LLM and parse response.
        let raw = llm
            .complete(&CompletionRequest::new(HARDEN_SYSTEM, &corpus))
            .await?;
        let parsed: HardenOut = serde_json::from_str(extract_json(&raw)).map_err(|e| {
            MnemosError::Internal(format!("harden_corrections parse failed: {e}; raw={raw}"))
        })?;

        let rule_text = parsed.rule.trim().to_string();
        if rule_text.is_empty() {
            continue;
        }

        let source_ids: Vec<String> = members.iter().map(|m| m.id.clone()).collect();

        // Write the hardened rule as a Reflection-tier memory.
        let rule_id = vault
            .remember_reflection(
                &rule_text,
                Some(format!("Hardened rule ({cluster_tag})")),
                MemoryType::Reflection,
                vec!["mnemos:hardened".into(), cluster_tag.clone()],
                &source_ids,
                vec![],
            )
            .await?;

        // Override the default importance (0.5) with 1.0 to signal high confidence.
        vault.patch(&rule_id, None, Some(1.0)).await?;

        // Mark sources reflected so they won't be re-clustered.
        mark_reflected(vault.storage(), &source_ids, Utc::now()).await?;

        for id in &source_ids {
            used_ids.insert(id.clone());
        }
        created.push(rule_id);
    }

    Ok(created)
}
