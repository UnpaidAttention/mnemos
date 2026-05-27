use crate::error::{MnemosError, Result};
use crate::pipeline::{extract_json, CandidateFact, ResolveOp};
use crate::providers::{CompletionRequest, LlmProvider};
use crate::retrieval::hybrid::hybrid_recall;
use crate::retrieval::RecallOpts;
use crate::storage::memory_ops::{link_memory_chunks, supersede_memory};
use crate::tier::Tier;
use crate::types::{MemoryType, Provenance};
use crate::vault::{RememberOpts, Vault};
use chrono::Utc;
use serde::Deserialize;

/// System prompt for the resolution stage.
pub const RESOLVE_SYSTEM: &str = "TASK=resolve\n\
Decide how a new candidate fact relates to the listed existing memories. \
Respond ONLY with JSON {\"op\":\"add|noop|update|delete\",\"target_id\":\"<existing id or null>\"}. \
Use `noop` if the fact is already represented; `update` if it refines or \
replaces a specific existing memory (give its id as target_id); `delete` if it \
negates one; otherwise `add`.";

/// How many existing memories to surface to the resolver as context.
const RESOLVE_CONTEXT_K: usize = 5;

#[derive(Deserialize)]
struct ResolveOut {
    op: String,
    #[serde(default)]
    target_id: Option<String>,
}

/// Resolve a candidate fact against existing memory and apply the decision.
///
/// Returns the chosen [`ResolveOp`] and, when a new memory was written
/// (Add/Update), its id. The new memory is stored in the semantic tier with the
/// supplied provenance and is linked to its source chunks.
pub async fn resolve_and_apply(
    vault: &Vault,
    candidate: &CandidateFact,
    provenance: Provenance,
    llm: &dyn LlmProvider,
) -> Result<(ResolveOp, Option<String>)> {
    let op = decide(vault, candidate, llm).await?;
    match &op {
        ResolveOp::Noop { .. } => Ok((op, None)),
        ResolveOp::Delete { target_id } => {
            vault
                .forget(target_id, Some("negated by extracted fact"))
                .await?;
            Ok((op, None))
        }
        ResolveOp::Add => {
            let id = store(vault, candidate, &provenance).await?;
            Ok((op, Some(id)))
        }
        ResolveOp::Update { target_id } => {
            let id = store(vault, candidate, &provenance).await?;
            supersede_memory(vault.storage(), target_id, &id, Utc::now()).await?;
            Ok((op, Some(id)))
        }
    }
}

/// Build the resolver prompt, call the LLM, and parse the decision.
async fn decide(
    vault: &Vault,
    candidate: &CandidateFact,
    llm: &dyn LlmProvider,
) -> Result<ResolveOp> {
    let embedder = vault.embedder().map(|a| a.as_ref());
    let hits = hybrid_recall(
        vault.storage(),
        embedder,
        &candidate.text,
        RecallOpts {
            k: RESOLVE_CONTEXT_K,
            ..Default::default()
        },
    )
    .await?;
    let existing = hits
        .iter()
        .map(|h| format!("- id={} title={}", h.memory.id, h.memory.title))
        .collect::<Vec<_>>()
        .join("\n");
    let user = format!(
        "Candidate fact:\n{}\n\nExisting memories:\n{}",
        candidate.text,
        if existing.is_empty() {
            "(none)"
        } else {
            &existing
        }
    );
    let raw = llm
        .complete(&CompletionRequest::new(RESOLVE_SYSTEM, user))
        .await?;
    let parsed: ResolveOut = serde_json::from_str(extract_json(&raw))
        .map_err(|e| MnemosError::Internal(format!("resolve parse failed: {e}; raw={raw}")))?;
    Ok(match parsed.op.as_str() {
        "noop" => ResolveOp::Noop {
            reason: "already known".into(),
        },
        "update" => match parsed.target_id {
            Some(t) => ResolveOp::Update { target_id: t },
            None => ResolveOp::Add, // model said update but gave no target → treat as add
        },
        "delete" => match parsed.target_id {
            Some(t) => ResolveOp::Delete { target_id: t },
            None => ResolveOp::Noop {
                reason: "delete with no target".into(),
            },
        },
        _ => ResolveOp::Add,
    })
}

/// Persist a candidate fact as a new semantic memory with provenance + chunk links.
async fn store(
    vault: &Vault,
    candidate: &CandidateFact,
    provenance: &Provenance,
) -> Result<String> {
    let chunks = provenance.chunks.clone();
    let id = vault
        .remember(
            &candidate.text,
            RememberOpts {
                tier: Tier::Semantic,
                kind: MemoryType::Fact,
                provenance: vec![provenance.clone()],
                source_tool: Some("mnemos-pipeline".into()),
                ..Default::default()
            },
        )
        .await?;
    link_memory_chunks(vault.storage(), &id, &chunks).await?;
    Ok(id)
}
