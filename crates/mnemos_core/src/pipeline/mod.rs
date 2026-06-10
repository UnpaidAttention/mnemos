//! Async learning pipeline: extract → resolve → entity-link → graph-update,
//! plus the decay pass. Each stage is a pure-ish function taking `&dyn
//! LlmProvider`; the daemon's `PipelineRunner` orchestrates them off the
//! `SessionEnded` event.

pub mod co_mention;
pub mod community;
pub mod decay;
pub mod entities;
pub mod extract;
pub mod graph;
pub mod index_log;
pub mod lint;
pub mod reflect;
pub mod resolve;

use serde::{Deserialize, Serialize};

/// A fact extracted from conversation chunks, before resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateFact {
    pub text: String,
}

/// What resolution decided to do with a candidate fact relative to existing
/// memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveOp {
    /// Store as a brand-new memory.
    Add,
    /// Already known — do nothing.
    Noop { reason: String },
    /// Supersede an existing memory with this refined version.
    Update { target_id: String },
    /// The new fact negates an existing memory; invalidate it.
    Delete { target_id: String },
}

/// Extract the JSON payload from an LLM response. LLMs frequently wrap JSON in
/// prose or ```json fences; this returns the substring from the first opening
/// bracket to the last closing bracket. Returns the whole string unchanged if
/// no brackets are found.
pub fn extract_json(s: &str) -> &str {
    let start = s.find(['{', '[']);
    let end = s.rfind(['}', ']']);
    match (start, end) {
        (Some(a), Some(b)) if b >= a => &s[a..=b],
        _ => s,
    }
}

/// Truncate a string to at most `max_chars` characters, appending "…" if
/// truncated.  Unlike byte-based slicing this is always safe for multi-byte
/// UTF-8 (emoji, CJK, accented characters).
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_strips_fences_and_prose() {
        let s = "Here you go:\n```json\n{\"facts\": []}\n```\nhope that helps";
        assert_eq!(extract_json(s), "{\"facts\": []}");
    }

    #[test]
    fn extract_json_passthrough_when_no_brackets() {
        assert_eq!(extract_json("no json here"), "no json here");
    }
}
