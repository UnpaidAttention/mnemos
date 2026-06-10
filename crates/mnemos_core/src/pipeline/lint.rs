use crate::error::{MnemosError, Result};
use crate::pipeline::extract_json;
use crate::providers::{CompletionRequest, LlmProvider};
use crate::storage::Storage;
use serde::{Deserialize, Serialize};

pub const LINT_SYSTEM: &str = "TASK=lint\n\
You are a knowledge base auditor. You analyze a list of semantic memories (each with id, title, and body) and identify:\n\
1. Contradictions: Memories that directly conflict with or contradict each other.\n\
2. Gaps: Logical gaps or missing connections between related concepts.\n\
Respond ONLY with a JSON object of the form:\n\
{\n\
  \"contradictions\": [\n\
    {\"id_a\": \"mem_id_1\", \"id_b\": \"mem_id_2\", \"reason\": \"Detailed explanation of why they contradict\"}\n\
  ],\n\
  \"gaps\": [\n\
    {\"topic\": \"Topic name\", \"reason\": \"Detailed explanation of the missing context\"}\n\
  ]\n\
}";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    pub id_a: String,
    pub id_b: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gap {
    pub topic: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintResult {
    pub contradictions: Vec<Contradiction>,
    pub gaps: Vec<Gap>,
    pub orphans: Vec<String>,
}

#[derive(Deserialize)]
struct LintLlmOut {
    #[serde(default)]
    contradictions: Vec<Contradiction>,
    #[serde(default)]
    gaps: Vec<Gap>,
}

/// Run a semantic audit over the vault.
pub async fn run_lint(storage: &Storage, llm: &dyn LlmProvider) -> Result<LintResult> {
    let conn = storage.conn()?;

    // 1. Query all active semantic memories
    let mut rows = conn
        .query(
            "SELECT id, title, body FROM memories WHERE invalid_at IS NULL AND tier = 'semantic'",
            (),
        )
        .await?;

    let mut memories_list = Vec::new();
    while let Some(row) = rows.next().await? {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let body: String = row.get(2)?;
        memories_list.push(format!("ID: {}\nTitle: {}\nBody: {}\n---", id, title, body));
    }

    // 2. Query orphan memories (no inbound links, no outbound links, no entity mentions)
    let mut orphan_rows = conn
        .query(
            "SELECT id FROM memories \
             WHERE invalid_at IS NULL \
               AND id NOT IN (SELECT source_id FROM memory_links) \
               AND id NOT IN (SELECT target_id FROM memory_links) \
               AND id NOT IN (SELECT memory_id FROM entity_mentions)",
            (),
        )
        .await?;

    let mut orphans = Vec::new();
    while let Some(row) = orphan_rows.next().await? {
        orphans.push(row.get::<String>(0)?);
    }

    if memories_list.is_empty() {
        return Ok(LintResult {
            contradictions: vec![],
            gaps: vec![],
            orphans,
        });
    }

    // Cap the number of memories to prevent context window overflow.
    // With many memories, the full corpus would exceed LLM limits.
    const MAX_LINT_MEMORIES: usize = 50;
    if memories_list.len() > MAX_LINT_MEMORIES {
        memories_list.truncate(MAX_LINT_MEMORIES);
    }

    // 3. Prompt the LLM for contradictions and gaps
    let memories_input = memories_list.join("\n");
    let req = CompletionRequest::new(LINT_SYSTEM, memories_input);
    let raw = llm.complete(&req).await?;

    let parsed: LintLlmOut = serde_json::from_str(extract_json(&raw))
        .map_err(|e| MnemosError::Internal(format!("lint parse failed: {e}; raw={raw}")))?;

    Ok(LintResult {
        contradictions: parsed.contradictions,
        gaps: parsed.gaps,
        orphans,
    })
}
