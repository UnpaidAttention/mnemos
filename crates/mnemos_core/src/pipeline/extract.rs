use crate::error::{MnemosError, Result};
use crate::pipeline::{extract_json, CandidateFact};
use crate::providers::{CompletionRequest, LlmProvider};
use crate::types::Chunk;
use serde::Deserialize;
use serde_json::json;

/// System prompt for the extraction stage. The `TASK=extract` marker drives
/// [`MockLlm`](crate::providers::mock_llm::MockLlm); the prose guides real models.
///
/// Simplified for reliability with small models (3-4B parameters). The JSON
/// Schema enforcement via `EXTRACT_FORMAT_SCHEMA` handles structural validity;
/// this prompt focuses on content quality.
pub const EXTRACT_SYSTEM: &str = "TASK=extract\n\
You extract facts from conversation transcripts. Each fact is standalone — \
someone reading it should understand without seeing the original conversation.\n\n\
Write declaratively. State what IS true. Never narrate the conversation.\n\
Never write 'The user said' or 'It was discussed' — describe the knowledge.\n\n\
CATEGORIES: technical, preference, procedural, constraint, decision\n\n\
EXAMPLES:\n\
{\"text\": \"Apple Music API tokens expire 6 months after creation and use \
ES256 JWT. The MusicKit private key in the developer portal is needed for \
regeneration.\", \"category\": \"technical\"}\n\
{\"text\": \"The project uses bundled llama.cpp as the default embedder because \
it eliminates a 200MB dependency and works offline.\", \"category\": \"decision\"}\n\n\
Skip greetings, chit-chat, and troubleshooting that reached no conclusion.\n\n\
Respond with JSON: {\"facts\":[{\"text\":\"...\",\"category\":\"...\"}]}.";

/// JSON Schema for grammar-constrained extraction output. Passed to Ollama/OpenAI
/// to enforce structural validity at the token decoding level.
pub fn extraction_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "facts": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" },
                        "category": {
                            "type": "string",
                            "enum": ["technical", "preference", "procedural", "constraint", "decision"]
                        }
                    },
                    "required": ["text", "category"]
                }
            }
        },
        "required": ["facts"]
    })
}

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
    let req = CompletionRequest::new(&system_prompt, transcript)
        .with_schema(extraction_schema());
    let raw = llm.complete(&req).await?;
    let parsed: ExtractOut = serde_json::from_str(extract_json(&raw))
        .map_err(|e| MnemosError::Internal(format!("extract parse failed: {e}; raw={raw}")))?;
    Ok(parsed
        .facts
        .into_iter()
        .map(|f| CandidateFact {
            text: f.text.trim().to_string(),
            category: f.category,
        })
        .filter(|f| !f.text.is_empty())
        .collect())
}

/// System prompt for incremental (mid-session) extraction.
/// Same simplification as EXTRACT_SYSTEM for small-model reliability.
const EXTRACT_INCREMENTAL_SYSTEM: &str = "TASK=extract\n\
Extract facts from the NEW section only. The CONTEXT section is for resolving \
pronouns and references — do NOT extract facts from it.\n\n\
Write declaratively. State what IS true. Never narrate the conversation.\n\n\
CATEGORIES: technical, preference, procedural, constraint, decision\n\n\
Skip greetings, chit-chat, and troubleshooting that reached no conclusion.\n\n\
Respond with JSON: {\"facts\":[{\"text\":\"...\",\"category\":\"...\"}]}.";

/// Run fact extraction over new chunks with full session context.
///
/// `context_chunks` are already-processed chunks (for reference only).
/// `new_chunks` are the chunks to extract from.
/// Returns an empty vector when there are no new chunks.
pub async fn extract_facts_incremental(
    context_chunks: &[Chunk],
    new_chunks: &[Chunk],
    llm: &dyn LlmProvider,
    custom_schema: Option<&str>,
) -> Result<Vec<CandidateFact>> {
    if new_chunks.is_empty() {
        return Ok(vec![]);
    }
    let mut transcript = String::new();
    if !context_chunks.is_empty() {
        transcript.push_str(
            "CONTEXT (already processed — for reference only, do NOT extract from these):\n",
        );
        for c in context_chunks {
            let who = c.speaker.as_deref().unwrap_or("unknown");
            transcript.push_str(&format!("{who}: {}\n", c.body));
        }
        transcript.push('\n');
    }
    transcript.push_str("NEW (extract facts from ONLY these messages):\n");
    for c in new_chunks {
        let who = c.speaker.as_deref().unwrap_or("unknown");
        transcript.push_str(&format!("{who}: {}\n", c.body));
    }
    let mut system_prompt = EXTRACT_INCREMENTAL_SYSTEM.to_string();
    if let Some(schema) = custom_schema {
        system_prompt.push_str("\n\nCustom Schema Guidelines:\n");
        system_prompt.push_str(schema);
    }
    let req = CompletionRequest::new(&system_prompt, transcript)
        .with_schema(extraction_schema());
    let raw = llm.complete(&req).await?;
    let parsed: ExtractOut = serde_json::from_str(extract_json(&raw))
        .map_err(|e| MnemosError::Internal(format!("extract parse failed: {e}; raw={raw}")))?;
    Ok(parsed
        .facts
        .into_iter()
        .map(|f| CandidateFact {
            text: f.text.trim().to_string(),
            category: f.category,
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
