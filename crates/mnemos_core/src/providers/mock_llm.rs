use crate::error::Result;
use crate::providers::{CompletionRequest, LlmProvider};
use async_trait::async_trait;
use serde_json::json;

/// Deterministic, prompt-marker-driven LLM stub for CI.
///
/// Behaviour is selected by a `TASK=<name>` token in the request's **system**
/// prompt. The mock then parses the joined **user** content for task-specific
/// markers and returns canned JSON. This lets pipeline tests embed the
/// "expected" LLM output directly in their input, so the whole pipeline is
/// deterministic without a real model.
///
/// Marker protocol (markers may appear anywhere on a line):
/// * `TASK=extract`   → one fact per occurrence of `FACT:` →
///   `{"facts":[{"text":"<rest of line>"}]}`
/// * `TASK=resolve`   → scan for `OP=<add|noop|update|delete>` (first match
///   wins, default `add`) and optional `TARGET=<id>` →
///   `{"op":"<op>","target_id":"<id-or-null>"}`
/// * `TASK=link`      → one entity per `@<Name>` token →
///   `{"entities":[{"name":"<Name>"}]}`
/// * `TASK=relations` → one edge per `<A>~<REL>~<B>` token →
///   `{"relations":[{"source":"A","relation":"REL","target":"B"}]}`
/// * no recognised marker → echoes the joined user content verbatim.
#[derive(Debug, Clone, Default)]
pub struct MockLlm;

impl MockLlm {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl LlmProvider for MockLlm {
    fn model_id(&self) -> &str {
        "mock-llm"
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<String> {
        let content = req.joined_user_content();
        let out = if req.system.contains("TASK=extract") {
            let facts: Vec<_> = content
                .lines()
                .filter_map(|l| l.find("FACT:").map(|i| &l[i + "FACT:".len()..]))
                .map(|t| json!({ "text": t.trim() }))
                .filter(|v| !v["text"].as_str().unwrap_or("").is_empty())
                .collect();
            json!({ "facts": facts }).to_string()
        } else if req.system.contains("TASK=resolve") {
            let op = ["delete", "update", "noop", "add"]
                .into_iter()
                .find(|o| content.contains(&format!("OP={o}")))
                .unwrap_or("add");
            let target = content
                .split_whitespace()
                .find_map(|tok| tok.strip_prefix("TARGET="))
                .map(|s| s.to_string());
            json!({ "op": op, "target_id": target }).to_string()
        } else if req.system.contains("TASK=link") {
            let entities: Vec<_> = content
                .split_whitespace()
                .filter_map(|tok| tok.strip_prefix('@'))
                .filter(|n| !n.is_empty())
                .map(|name| json!({ "name": name }))
                .collect();
            json!({ "entities": entities }).to_string()
        } else if req.system.contains("TASK=relations") {
            let relations: Vec<_> = content
                .split_whitespace()
                .filter_map(|tok| {
                    let parts: Vec<&str> = tok.split('~').collect();
                    if parts.len() == 3 && parts.iter().all(|p| !p.is_empty()) {
                        Some(json!({
                            "source": parts[0],
                            "relation": parts[1],
                            "target": parts[2]
                        }))
                    } else {
                        None
                    }
                })
                .collect();
            json!({ "relations": relations }).to_string()
        } else {
            content
        };
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{CompletionRequest, LlmProvider};

    #[tokio::test]
    async fn mock_extracts_marked_fact_lines() {
        let llm = MockLlm::new();
        let req = CompletionRequest::new(
            "TASK=extract",
            "user: noise here\nuser: FACT: the sky is blue\nassistant: FACT: water is wet",
        );
        let out = llm.complete(&req).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let facts = v["facts"].as_array().unwrap();
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0]["text"], "the sky is blue");
        assert_eq!(facts[1]["text"], "water is wet");
    }

    #[tokio::test]
    async fn mock_resolve_defaults_to_add_and_reads_markers() {
        let llm = MockLlm::new();
        let add = llm
            .complete(&CompletionRequest::new("TASK=resolve", "a plain new fact"))
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&add).unwrap()["op"],
            "add"
        );
        let upd = llm
            .complete(&CompletionRequest::new(
                "TASK=resolve",
                "refine it OP=update TARGET=mem_123",
            ))
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&upd).unwrap();
        assert_eq!(v["op"], "update");
        assert_eq!(v["target_id"], "mem_123");
    }

    #[tokio::test]
    async fn mock_link_and_relations() {
        let llm = MockLlm::new();
        let ents = llm
            .complete(&CompletionRequest::new(
                "TASK=link",
                "@Shaun uses @Rust daily",
            ))
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&ents).unwrap();
        assert_eq!(v["entities"].as_array().unwrap().len(), 2);
        assert_eq!(v["entities"][0]["name"], "Shaun");

        let rels = llm
            .complete(&CompletionRequest::new(
                "TASK=relations",
                "Shaun~uses~Rust noise",
            ))
            .await
            .unwrap();
        let r: serde_json::Value = serde_json::from_str(&rels).unwrap();
        assert_eq!(r["relations"].as_array().unwrap().len(), 1);
        assert_eq!(r["relations"][0]["relation"], "uses");
    }
}
