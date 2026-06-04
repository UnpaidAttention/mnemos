//! Parse a Claude Code transcript (.jsonl) into ordered conversation turns
//! for ingestion. Tolerant: skips lines it can't interpret.

#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub speaker: String, // "user" | "assistant"
    pub body: String,
    pub ordinal: u32,
}

/// Extract turns from JSONL transcript text. Each line is a JSON object; we
/// look for `{"type":"user"|"assistant","message":{"role":..,"content":..}}`
/// shapes and pull plain text. Non-conforming lines are skipped.
///
/// Real Claude Code transcripts contain many event types
/// (tool_use, tool_result, system, summary, file-history-snapshot, thinking,
/// last-prompt, mode, permission-mode, attachment, etc.) — all non-text
/// user/assistant lines are silently skipped.
pub fn parse_transcript(jsonl: &str) -> Vec<Turn> {
    let mut turns = Vec::new();
    let mut ord = 0u32;
    for line in jsonl.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        // role: prefer message.role, fall back to top-level type
        let role = v
            .get("message")
            .and_then(|m| m.get("role"))
            .and_then(|r| r.as_str())
            .or_else(|| v.get("type").and_then(|t| t.as_str()));
        let role = match role {
            Some("user") => "user",
            Some("assistant") => "assistant",
            _ => continue,
        };
        let text = extract_text(&v);
        if text.trim().is_empty() {
            continue;
        }
        turns.push(Turn {
            speaker: role.to_string(),
            body: text,
            ordinal: ord,
        });
        ord += 1;
    }
    turns
}

/// Pull plain text from a transcript line. Content may be:
/// - A plain string (common for user messages)
/// - An array of blocks, each with a "type" field. Only `{"type":"text","text":"..."}`
///   blocks are extracted. Blocks of type `thinking`, `tool_use`, `tool_result`, and
///   all others are silently skipped.
fn extract_text(v: &serde_json::Value) -> String {
    let content = v
        .get("message")
        .and_then(|m| m.get("content"))
        .or_else(|| v.get("content"));
    match content {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(blocks)) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── original plan test ──────────────────────────────────────────────────

    #[test]
    fn parses_user_and_assistant_turns_in_order() {
        let jsonl = r#"{"type":"user","message":{"role":"user","content":"hi there"}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"hello"}]}}
{"type":"system","content":"ignored"}
not json
{"type":"user","message":{"role":"user","content":""}}"#;
        let turns = parse_transcript(jsonl);
        assert_eq!(turns.len(), 2);
        assert_eq!(
            turns[0],
            Turn {
                speaker: "user".into(),
                body: "hi there".into(),
                ordinal: 0
            }
        );
        assert_eq!(turns[1].speaker, "assistant");
        assert_eq!(turns[1].body, "hello");
        assert_eq!(turns[1].ordinal, 1);
    }

    // ── tests derived from real transcript shape ────────────────────────────

    /// A real assistant turn has content as a list of typed blocks.
    /// Only "text" blocks should produce body text; "thinking" blocks must be
    /// excluded (they are internal reasoning, not user-facing output).
    #[test]
    fn skips_thinking_blocks_keeps_text_blocks() {
        // Mirrors real shape: ["thinking", "text", "tool_use"]
        let jsonl = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"<internal>","signature":"sig"},{"type":"text","text":"I'll read the file now."},{"type":"tool_use","id":"tu_1","name":"Read","input":{"file_path":"/foo"}}]}}"#;
        let turns = parse_transcript(jsonl);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].speaker, "assistant");
        assert_eq!(turns[0].body, "I'll read the file now.");
    }

    /// An assistant turn that contains ONLY tool_use (and possibly thinking)
    /// blocks with no "text" block yields an empty body and must be skipped
    /// entirely (no turn emitted).
    #[test]
    fn skips_assistant_turn_with_only_tool_use_blocks() {
        let jsonl = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"tu_2","name":"Bash","input":{"command":"cargo test"}}]}}
{"type":"user","message":{"role":"user","content":"looks good"}}"#;
        let turns = parse_transcript(jsonl);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].speaker, "user");
        assert_eq!(turns[0].body, "looks good");
    }

    /// A user turn whose content is a list with only tool_result blocks
    /// (the pattern used when the user sends back tool output) should be
    /// skipped — it carries no human-authored text.
    #[test]
    fn skips_user_turn_with_only_tool_result_blocks() {
        let jsonl = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tu_1","content":"Exit code 1","is_error":true}]}}
{"type":"user","message":{"role":"user","content":"please fix it"}}"#;
        let turns = parse_transcript(jsonl);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].body, "please fix it");
    }

    /// A user turn whose content list mixes a human text block with a
    /// tool_result block: only the text block should appear in the body.
    #[test]
    fn extracts_text_block_from_mixed_user_content_list() {
        let jsonl = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Here is the output:"},{"type":"tool_result","tool_use_id":"tu_3","content":"ok","is_error":false}]}}"#;
        let turns = parse_transcript(jsonl);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].speaker, "user");
        assert_eq!(turns[0].body, "Here is the output:");
    }

    /// Non-user/assistant event types (system, last-prompt, mode, ai-title,
    /// permission-mode, file-history-snapshot, attachment) must be silently
    /// ignored.
    #[test]
    fn ignores_all_non_conversation_event_types() {
        let jsonl = r#"{"type":"last-prompt","leafUuid":"abc","sessionId":"xyz"}
{"type":"mode","mode":"normal","sessionId":"xyz"}
{"type":"permission-mode","permissionMode":"default","sessionId":"xyz"}
{"type":"ai-title","title":"My session","sessionId":"xyz"}
{"type":"file-history-snapshot","files":[],"sessionId":"xyz"}
{"type":"system","content":"<system prompt>"}
{"type":"user","message":{"role":"user","content":"actual question"}}"#;
        let turns = parse_transcript(jsonl);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].body, "actual question");
        assert_eq!(turns[0].ordinal, 0);
    }

    /// Ordinals are assigned only to emitted turns; skipped lines do not
    /// increment the counter.
    #[test]
    fn ordinals_are_contiguous_across_skipped_lines() {
        let jsonl = r#"{"type":"user","message":{"role":"user","content":"first"}}
{"type":"system","content":"skip"}
not json at all
{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t","name":"Bash","input":{}}]}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"second"}]}}"#;
        let turns = parse_transcript(jsonl);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].ordinal, 0);
        assert_eq!(turns[1].ordinal, 1);
    }

    /// An empty input string returns an empty vec — no panic.
    #[test]
    fn empty_input_returns_empty_vec() {
        assert!(parse_transcript("").is_empty());
    }

    /// Lines that are not valid JSON are silently skipped.
    #[test]
    fn malformed_json_lines_are_skipped() {
        let jsonl = "not json\n{broken\n{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"ok\"}}";
        let turns = parse_transcript(jsonl);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].body, "ok");
    }

    /// Multiple text blocks in a single assistant turn are joined with newlines.
    #[test]
    fn multiple_text_blocks_joined_with_newline() {
        let jsonl = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Line one"},{"type":"text","text":"Line two"}]}}"#;
        let turns = parse_transcript(jsonl);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].body, "Line one\nLine two");
    }
}
