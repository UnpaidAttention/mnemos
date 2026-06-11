use crate::error::{MnemosError, Result};
use crate::pipeline::{extract_json, CandidateFact};
use crate::providers::{CompletionRequest, LlmProvider};
use crate::types::Chunk;
use serde::Deserialize;

/// System prompt for the extraction stage. The `TASK=extract` marker drives
/// [`MockLlm`](crate::providers::mock_llm::MockLlm); the prose guides real models.
pub const EXTRACT_SYSTEM: &str = "TASK=extract\n\
You are a knowledge-base curator. Your job is to read a conversation transcript \
and extract durable reference entries that would be valuable to recall in future \
sessions. Write each entry as a standalone fact — someone reading it should \
understand the knowledge without seeing the original conversation.\n\n\
CATEGORIES — classify each entry:\n\
- technical: How something works, architecture, APIs, data formats, dependencies\n\
- preference: Personal choices, coding style, tool preferences, opinions\n\
- procedural: Steps to accomplish something, workflows, recipes, commands\n\
- constraint: Limitations, gotchas, things that don't work, deadlines\n\
- decision: Choices that were made and WHY, trade-offs considered\n\n\
GRANULARITY: Each entry should cover one coherent topic. It can contain \
multiple related claims — just don't mash unrelated topics together.\n\n\
VOICE: Write declaratively. State what IS true, not what was discussed.\n\
- NEVER write 'The user said', 'The assistant proposed', 'It was discussed', \
'They agreed', 'It was confirmed', or any narrative framing.\n\
- NEVER describe what happened in the conversation — describe the knowledge.\n\n\
SKIP: Greetings, acknowledgments, chit-chat, troubleshooting back-and-forth \
that didn't reach a conclusion, vague plans with no specifics.\n\n\
EXAMPLES:\n\
{\"text\": \"Apple Music API tokens expire 6 months after creation. Regeneration \
requires the MusicKit private key stored in the developer portal. The token \
format is a JWT signed with ES256.\", \"category\": \"technical\"}\n\
{\"text\": \"Shaun prefers Rust over Go for systems work and uses DM Sans for \
body text in UI projects.\", \"category\": \"preference\"}\n\
{\"text\": \"To deploy Mnemos: bump the version in Cargo.toml, update CHANGELOG.md, \
commit, then push a v*.*.* tag — the release workflow handles .deb and .rpm \
builds automatically.\", \"category\": \"procedural\"}\n\
{\"text\": \"libsql-sys uses Unix-only OsStr::as_bytes() and does not compile on \
Windows. Full Windows support requires libsql to gain Windows compatibility or \
Mnemos to swap storage backends.\", \"category\": \"constraint\"}\n\
{\"text\": \"The project uses the bundled llama.cpp embedder as the default instead \
of Ollama because it eliminates a 200MB dependency and works offline out of the \
box.\", \"category\": \"decision\"}\n\n\
Respond ONLY with JSON: {\"facts\":[{\"text\":\"...\",\"category\":\"...\"}]}.";

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
            category: f.category,
        })
        .filter(|f| !f.text.is_empty())
        .collect())
}

/// System prompt for incremental (mid-session) extraction.
const EXTRACT_INCREMENTAL_SYSTEM: &str = "TASK=extract\n\
You are a knowledge-base curator. Extract durable reference entries from the \
NEW section of a conversation transcript. The CONTEXT section shows earlier \
messages that have already been processed — use them to resolve pronouns and \
references, but do NOT extract facts from them.\n\n\
CATEGORIES — classify each entry:\n\
- technical: How something works, architecture, APIs, data formats, dependencies\n\
- preference: Personal choices, coding style, tool preferences, opinions\n\
- procedural: Steps to accomplish something, workflows, recipes, commands\n\
- constraint: Limitations, gotchas, things that don't work, deadlines\n\
- decision: Choices that were made and WHY, trade-offs considered\n\n\
GRANULARITY: Each entry should cover one coherent topic. It can contain \
multiple related claims — just don't mash unrelated topics together.\n\n\
VOICE: Write declaratively. State what IS true, not what was discussed.\n\
- NEVER write 'The user said', 'The assistant proposed', 'It was discussed', \
'They agreed', 'It was confirmed', or any narrative framing.\n\
- NEVER describe what happened in the conversation — describe the knowledge.\n\n\
SKIP: Greetings, acknowledgments, chit-chat, troubleshooting back-and-forth \
that didn't reach a conclusion, vague plans with no specifics.\n\n\
Respond ONLY with JSON: {\"facts\":[{\"text\":\"...\",\"category\":\"...\"}]}.";

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
    let req = CompletionRequest::new(&system_prompt, transcript);
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
