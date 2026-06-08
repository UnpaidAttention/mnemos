use crate::error::{MnemosError, Result};
use crate::pipeline::{extract_json, CandidateFact};
use crate::providers::{CompletionRequest, LlmProvider};
use crate::types::Chunk;
use serde::Deserialize;

/// System prompt for the extraction stage. The `TASK=extract` marker drives
/// [`MockLlm`](crate::providers::mock_llm::MockLlm); the prose guides real models.
pub const EXTRACT_SYSTEM: &str = "TASK=extract\n\
You extract atomic, standalone facts worth remembering from a conversation \
transcript. Each fact must be self-contained — resolve pronouns and context so \
it stands alone. Ignore greetings and chit-chat. Respond ONLY with JSON of the \
form {\"facts\":[{\"text\":\"...\"}]}.";

#[derive(Deserialize)]
struct ExtractOut {
    #[serde(default)]
    facts: Vec<CandidateFact>,
}

/// Run fact extraction over a session's chunks.
///
/// Returns an empty vector when there are no chunks (no LLM call is made).
pub async fn extract_facts(
    chunks: &[Chunk],
    llm: &dyn LlmProvider,
    custom_schema: Option<&str>,
) -> Result<Vec<CandidateFact>> {
    if chunks.is_empty() {
        return Ok(vec![]);
    }
    let transcript = chunks
        .iter()
        .map(|c| {
            let who = c.speaker.as_deref().unwrap_or("unknown");
            format!("{who}: {}", c.body)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut system_prompt = EXTRACT_SYSTEM.to_string();
    if let Some(schema) = custom_schema {
        system_prompt.push_str("\n\nCustom Schema Guidelines:\n");
        system_prompt.push_str(schema);
    }
    let req = CompletionRequest::new(&system_prompt, transcript);
    let raw = llm.complete(&req).await?;
    let parsed: ExtractOut = serde_json::from_str(extract_json(&raw))
        .map_err(|e| MnemosError::Internal(format!("extract parse failed: {e}; raw={raw}")))?;
    Ok(parsed
        .facts
        .into_iter()
        .map(|f| CandidateFact {
            text: f.text.trim().to_string(),
        })
        .filter(|f| !f.text.is_empty())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::mock_llm::MockLlm;
    use crate::types::Chunk;
    use chrono::Utc;

    fn chunk(speaker: &str, body: &str) -> Chunk {
        Chunk {
            id: format!("chunk_{speaker}_{}", body.len()),
            session_id: "sess_test".into(),
            speaker: Some(speaker.into()),
            ordinal: 0,
            body: body.into(),
            created_at: Utc::now(),
            source_tool: None,
            source_meta: None,
        }
    }

    #[tokio::test]
    async fn extracts_marked_facts() {
        let chunks = vec![
            chunk("user", "FACT: Shaun prefers Rust over Go"),
            chunk("assistant", "noted, no fact here"),
        ];
        let facts = extract_facts(&chunks, &MockLlm::new(), None).await.unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].text, "Shaun prefers Rust over Go");
    }

    #[tokio::test]
    async fn empty_chunks_yield_no_facts() {
        let facts = extract_facts(&[], &MockLlm::new(), None).await.unwrap();
        assert!(facts.is_empty());
    }
}
