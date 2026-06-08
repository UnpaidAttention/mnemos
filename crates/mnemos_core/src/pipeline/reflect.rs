//! Reflection stage: synthesize recent un-reflected memories into durable,
//! typed reflection-tier memories with `reflects_on` provenance.

use crate::correction::Correction;
use crate::error::{MnemosError, Result};
use crate::pipeline::extract_json;
use crate::providers::{CompletionRequest, LlmProvider};
use crate::storage::memory_ops::{list_by_kind, mark_reflected, recent_unreflected};
use crate::types::MemoryType;
use crate::vault::Vault;
use chrono::Utc;
use libsql::params;
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
    // Build corpus from source memories, truncating to fit within the LLM
    // context window.  ~4 chars per token; system prompt is ~200 tokens;
    // leave headroom → target ~6000 tokens of user content ≈ 24_000 chars.
    const MAX_CORPUS_CHARS: usize = 24_000;
    const MAX_BODY_CHARS: usize = 500;
    let mut corpus = sources
        .iter()
        .map(|m| format!("- {}", m.body))
        .collect::<Vec<_>>()
        .join("\n");
    // If over budget, truncate each memory body.
    if corpus.len() > MAX_CORPUS_CHARS {
        corpus = sources
            .iter()
            .map(|m| {
                let body = if m.body.len() > MAX_BODY_CHARS {
                    format!("{}…", &m.body[..MAX_BODY_CHARS])
                } else {
                    m.body.clone()
                };
                format!("- {body}")
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    // If still over budget, keep only the first N memories that fit.
    if corpus.len() > MAX_CORPUS_CHARS {
        corpus.truncate(MAX_CORPUS_CHARS);
    }
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

        // Call the LLM; skip this cluster (don't abort the whole run) on failure
        // so one bad response doesn't block hardening the other clusters.
        let raw = match llm
            .complete(&CompletionRequest::new(HARDEN_SYSTEM, &corpus))
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(cluster_tag = %cluster_tag, error = %e, "harden_corrections LLM call failed; skipping cluster");
                continue;
            }
        };
        let parsed: HardenOut = match serde_json::from_str(extract_json(&raw)) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(cluster_tag = %cluster_tag, error = %e, "harden_corrections parse failed; skipping cluster");
                continue;
            }
        };

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

/// System prompt for the correction-mining LLM call.
pub const MINE_SYSTEM: &str = "TASK=mine-corrections\n\
Review this conversation and extract moments where the user corrected the \
assistant. For each, output the mistake, the correct approach, the reason, and \
the triggering situation. Only include corrections with a clear reason. \
Respond ONLY with JSON {\"corrections\":[{\"wrong\":\"\",\"right\":\"\",\"why\":\"\",\"trigger\":\"\"}]}.";

#[derive(Deserialize)]
struct MineOut {
    #[serde(default)]
    corrections: Vec<MinedCorrection>,
}

#[derive(Deserialize)]
struct MinedCorrection {
    #[serde(default)]
    wrong: String,
    #[serde(default)]
    right: String,
    #[serde(default)]
    why: String,
    #[serde(default)]
    trigger: String,
}

/// Scan a session's raw conversation chunks with the LLM and extract corrections
/// the model did not log explicitly.
///
/// For each tuple with a non-empty `why`, builds a [`Correction`] and calls
/// [`Vault::remember_correction`] (which deduplicates against tool-logged ones
/// automatically). Individual validation failures are skipped — they do not
/// abort the whole pass. LLM errors are silenced and return `Ok(vec![])` so
/// that a mining failure never blocks session-end processing.
///
/// Returns the ids of all newly created correction memories.
pub async fn mine_corrections(
    vault: &Vault,
    llm: &dyn LlmProvider,
    session_id: &str,
) -> Result<Vec<String>> {
    // Load the session's chunks from the `chunks` table, ordered by ordinal.
    let chunks = {
        let conn = vault.storage().conn()?;
        let mut rows = conn
            .query(
                "SELECT speaker, body FROM chunks
                  WHERE session_id = ?
                  ORDER BY ordinal ASC",
                params![session_id.to_string()],
            )
            .await?;
        let mut out: Vec<(String, String)> = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push((row.get::<String>(0)?, row.get::<String>(1)?));
        }
        out
    };

    if chunks.is_empty() {
        return Ok(vec![]);
    }

    // Build a readable transcript to send to the LLM.
    let corpus = chunks
        .iter()
        .map(|(speaker, body)| format!("{speaker}: {body}"))
        .collect::<Vec<_>>()
        .join("\n");

    // Call LLM; silence errors so that a transient failure never blocks
    // session-end processing.
    let raw = match llm
        .complete(&CompletionRequest::new(MINE_SYSTEM, corpus))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(session_id = %session_id, error = %e, "mine_corrections LLM call failed; skipping");
            return Ok(vec![]);
        }
    };

    let parsed: MineOut = match serde_json::from_str(extract_json(&raw)) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(session_id = %session_id, error = %e, "mine_corrections JSON parse failed; skipping");
            return Ok(vec![]);
        }
    };

    let mut created = Vec::new();
    for mc in parsed.corrections {
        // Skip tuples without a substantive `why`.
        if mc.why.trim().is_empty() {
            continue;
        }
        let trigger = if mc.trigger.trim().is_empty() {
            None
        } else {
            Some(mc.trigger.trim().to_string())
        };
        let correction = Correction {
            wrong: mc.wrong.trim().to_string(),
            right: mc.right.trim().to_string(),
            why: mc.why.trim().to_string(),
            trigger,
        };
        match vault.remember_correction(correction, None).await {
            Ok(id) => created.push(id),
            Err(e) => {
                // Validation errors (e.g. `why` too short, weaponized) are
                // expected for noisy LLM output — skip the tuple.
                tracing::debug!(error = %e, "mine_corrections skipping invalid correction tuple");
            }
        }
    }

    Ok(created)
}
