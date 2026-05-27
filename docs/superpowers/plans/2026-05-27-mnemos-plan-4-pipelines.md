# Mnemos Plan 4 — Async Learning Pipelines

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn raw session chunks into durable, deduplicated semantic memories automatically. When a session ends, the daemon extracts atomic facts from its chunks (LLM), resolves each against existing memories (ADD/UPDATE/DELETE/NOOP), links entities, builds bi-temporal relationship edges, and an hourly decay worker fades unused memories. End state: a user talks to Claude Code through Mnemos, ends the session, and durable facts appear in the semantic tier — with provenance back to the verbatim chunks — without any manual `remember` calls.

**Architecture:** A `LlmProvider` trait (Ollama default, OpenAI/Anthropic API, plus a deterministic `MockLlm` for CI) sits alongside the existing `Embedder`. The daemon runs a `PipelineRunner` that subscribes to the `SessionEnded` event the bus already emits; each event enqueues an extraction job processed on a bounded worker pool. The pipeline is a fixed sequence: extract → resolve → entity-link → graph-update, each a pure-ish `mnemos_core` function taking `&dyn LlmProvider`. A separate hourly tokio interval runs the Ebbinghaus decay pass. The 501 stubs from Plan 3 (`PATCH /v1/memories/{id}`, time-travel) get real implementations. Three Plan 3 carry-forwards are closed: orphan-chunk FK, recall-logic dedup, graceful-shutdown task-group join.

**Tech Stack:** Rust 2021, the existing stack (libsql, axum, tokio), plus reqwest for the Ollama chat API. No new heavy deps. `MockLlm` is prompt-marker-driven so the full pipeline is deterministic in CI without any model.

---

## Plan sequence context

Plan 4 of 7. Subsequent:
- Plan 5: HippoRAG Personalized PageRank retriever + importance-triggered reflection + community detection (consumes the entity graph this plan builds)
- Plan 6: Tauri + React desktop UI (visualizes the pipeline status + the graph)
- Plan 7: sync backends, additional adapters, packaging

Plan 4 produces **v0.3.0** — memories form themselves from conversations. Schema migration is additive (v4 adds pipeline bookkeeping columns); v0.2.0 vaults upgrade transparently.

---

## What this plan deliberately defers

| Capability | Why deferred | Target |
|---|---|---|
| MCP `sampling/createMessage` (extraction via the calling client's LLM) | Async pipelines run AFTER the triggering MCP request returns — there is no request-scoped client connection to sample from. Server-initiated sampling also needs a bidirectional/SSE transport our request-response `/mcp` does not have. Building it half-way is worse than a clean configured-LLM path. | Revisit when/if a streaming MCP transport lands (Plan 5+ or a dedicated increment). For now extraction uses the configured `LlmProvider`. |
| HippoRAG Personalized PageRank retriever | Depends on the entity graph this plan *builds*; retrieval over it is its own concern | Plan 5 |
| Reflection (importance-triggered self-summarization) | Builds on having a populated semantic tier + entity graph | Plan 5 |
| Community detection (Leiden + summaries) | Same — needs the graph populated first | Plan 5 |
| LLM extraction-quality eval suite (precision/recall on a gold dataset) | Needs a labelled corpus; quality tuning is iterative post-MVP | A later increment; Plan 4 ships `MockLlm` correctness tests, not quality benchmarks |

---

## Hard prerequisites

- Plan 3 (`v0.2.0`) shipped; CI green on Linux + macOS.
- Rust stable (pinned).
- Ollama optional — every test uses `MockLlm` / `MockEmbedder`. The Ollama LLM path has `#[ignore]`-d tests like the embedder.

---

## File structure produced by this plan

```
crates/mnemos_core/src/
├── providers/
│   ├── mod.rs                 # MODIFIED: add LlmProvider trait + CompletionRequest
│   ├── mock_llm.rs            # NEW: MockLlm — deterministic, prompt-marker-driven
│   └── ollama_llm.rs          # NEW: OllamaLlm — POST /api/chat
├── pipeline/                  # NEW module — the async learning pipeline (pure logic)
│   ├── mod.rs                 # CandidateFact, ResolveOp, PipelineError, re-exports
│   ├── extract.rs             # extract_facts(chunks, &dyn LlmProvider) -> Vec<CandidateFact>
│   ├── resolve.rs             # resolve(&Vault, &candidate, &dyn LlmProvider) -> ResolveOp + apply
│   ├── entities.rs            # link_entities(&Vault, &memory, &dyn LlmProvider)
│   ├── graph.rs               # update_graph(&Vault, &memory, &dyn LlmProvider)
│   └── decay.rs               # decay_pass(&Storage, &DecayConfig) -> DecayStats
├── storage/
│   ├── entity_ops.rs          # MODIFIED (was stub): entity CRUD + fuzzy match + edge ops
│   ├── memory_ops.rs          # MODIFIED: link_chunks (memory_chunks rows) + time-travel query + patch
│   └── migrations.rs          # MODIFIED: v4 (chunks FK pragma note + memory bookkeeping)
└── vault.rs                   # MODIFIED: supersede(), patch_memory(), recall_as_of() conveniences

crates/mnemos_daemon/src/
├── config.rs                  # MODIFIED: [llm] section
├── llm.rs                     # NEW: build_llm_for_daemon(&Config) -> Option<Arc<dyn LlmProvider>>
├── pipeline_runner.rs         # NEW: subscribes to SessionEnded, runs the pipeline on a worker pool
├── state.rs                   # MODIFIED: AppState gains llm + pipeline handle
├── lib.rs                     # MODIFIED: build_app_full(config, vault, reranker, llm); spawn runner; shutdown join
├── main.rs                    # MODIFIED: build llm, pass to build_app_full
├── routes/
│   ├── memories.rs            # MODIFIED: implement PATCH + time-travel (replace 501s); use shared recall helper
│   ├── sessions.rs            # MODIFIED: add_chunk validates session exists (orphan-chunk fix); end emits trigger
│   ├── pipelines.rs           # NEW: GET /v1/pipelines status
│   └── recall_helper.rs       # NEW: shared recall(state, query, opts) used by REST + MCP
├── mcp/tools.rs               # MODIFIED: recall uses the shared helper (dedup)
└── routes/mod.rs              # MODIFIED: mount pipelines router

crates/mnemos_cli/src/
├── cli.rs                     # MODIFIED: `decay` + `pipelines` subcommands
└── commands/{decay.rs,pipelines.rs}  # NEW

README.md / CHANGELOG.md       # MODIFIED: v0.3.0
```

---

## Conventions (same as Plans 1-3)

- TDD: failing test → confirm fail → implement → confirm pass → commit. Pure-config/doc tasks skip the failing-test step.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` green at every commit.
- Commit message `<type>: <subject>`; reference Plan 4 / Task N in the body.
- All paths relative to `/home/jons/AntiGravityProjects/mnemos/`.
- Tokio async throughout. axum 0.8 path syntax `{id}`.
- CI runs default features only (no `--all-features`); all tests use `MockLlm` / `MockEmbedder` — never require Ollama. Ollama-backed tests are `#[ignore]`.

---

## Task 1: `LlmProvider` trait + `MockLlm`

The LLM abstraction parallels the existing `Embedder` trait. `MockLlm` is the deterministic, prompt-marker-driven stub that makes the whole pipeline testable in CI without a model.

**Files:**
- Modify: `crates/mnemos_core/src/providers/mod.rs`
- Create: `crates/mnemos_core/src/providers/mock_llm.rs`

- [ ] **Step 1: Write the failing test** — append to `crates/mnemos_core/src/providers/mock_llm.rs` (created in Step 3, so write the test there alongside the impl in one file). For now create the file with ONLY the test module:

```rust
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
            .complete(&CompletionRequest::new("TASK=link", "@Shaun uses @Rust daily"))
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&ents).unwrap();
        assert_eq!(v["entities"].as_array().unwrap().len(), 2);
        assert_eq!(v["entities"][0]["name"], "Shaun");

        let rels = llm
            .complete(&CompletionRequest::new("TASK=relations", "Shaun~uses~Rust noise"))
            .await
            .unwrap();
        let r: serde_json::Value = serde_json::from_str(&rels).unwrap();
        assert_eq!(r["relations"].as_array().unwrap().len(), 1);
        assert_eq!(r["relations"][0]["relation"], "uses");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --lib providers::mock_llm`
Expected: FAIL — `MockLlm`, `CompletionRequest`, `LlmProvider` not found / module not declared.

- [ ] **Step 3: Add the trait + types to `providers/mod.rs`**

Add `pub mod mock_llm;` near the other `pub mod` lines, then append:

```rust
/// Role of a single chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmRole {
    System,
    User,
    Assistant,
}

/// One chat message in a completion request.
#[derive(Debug, Clone)]
pub struct LlmMessage {
    pub role: LlmRole,
    pub content: String,
}

/// A chat completion request: a system prompt plus a sequence of messages.
#[derive(Debug, Clone, Default)]
pub struct CompletionRequest {
    pub system: String,
    pub messages: Vec<LlmMessage>,
    /// Hint that the provider should bias toward strict JSON output.
    pub json: bool,
}

impl CompletionRequest {
    /// Convenience constructor: a system prompt and a single user message,
    /// with JSON mode enabled (the pipeline always wants JSON back).
    pub fn new(system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            system: system.into(),
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: user.into(),
            }],
            json: true,
        }
    }

    /// Concatenate all user-message contents with newlines. Deterministic
    /// providers parse markers out of this.
    pub fn joined_user_content(&self) -> String {
        self.messages
            .iter()
            .filter(|m| m.role == LlmRole::User)
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Generates a text completion from a chat-style request.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Stable identifier for the underlying model.
    fn model_id(&self) -> &str {
        "unknown"
    }

    /// Run the completion and return the assistant's text response.
    async fn complete(&self, req: &CompletionRequest) -> Result<String>;
}
```

- [ ] **Step 4: Prepend the `MockLlm` implementation above the test module in `mock_llm.rs`**

```rust
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --lib providers::mock_llm`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/providers/mod.rs crates/mnemos_core/src/providers/mock_llm.rs
git commit -m "feat: add LlmProvider trait and deterministic MockLlm (Plan 4 Task 1)"
```

---

## Task 2: `OllamaLlm` provider

Real LLM path via Ollama's `/api/chat`. Live test is `#[ignore]`-d (CI has no Ollama), mirroring the Ollama embedder.

**Files:**
- Modify: `crates/mnemos_core/src/providers/mod.rs` (add `pub mod ollama_llm;`)
- Create: `crates/mnemos_core/src/providers/ollama_llm.rs`

- [ ] **Step 1: Write the failing test** — create `ollama_llm.rs` with the test module only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::LlmProvider;

    fn cfg() -> OllamaLlmConfig {
        OllamaLlmConfig {
            base_url: "http://localhost:11434".into(),
            model: "llama3.2".into(),
            timeout_secs: 60,
        }
    }

    #[test]
    fn reports_model_id() {
        assert_eq!(OllamaLlm::new(cfg()).model_id(), "llama3.2");
    }

    #[tokio::test]
    #[ignore = "requires a running Ollama with the model pulled"]
    async fn completes_live() {
        use crate::providers::CompletionRequest;
        let llm = OllamaLlm::new(cfg());
        let req = CompletionRequest::new(
            "You reply with strict JSON only.",
            "Return the JSON object {\"ok\": true}",
        );
        let out = llm.complete(&req).await.unwrap();
        assert!(out.contains("ok"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --lib providers::ollama_llm`
Expected: FAIL — `OllamaLlm` / `OllamaLlmConfig` not found.

- [ ] **Step 3: Prepend the implementation above the test module**

```rust
use crate::error::{MnemosError, Result};
use crate::providers::{CompletionRequest, LlmProvider, LlmRole};
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;

/// Configuration for [`OllamaLlm`].
#[derive(Debug, Clone)]
pub struct OllamaLlmConfig {
    pub base_url: String,
    pub model: String,
    pub timeout_secs: u64,
}

/// LLM provider backed by Ollama's `POST /api/chat` endpoint.
#[derive(Debug, Clone)]
pub struct OllamaLlm {
    cfg: OllamaLlmConfig,
    client: reqwest::Client,
}

impl OllamaLlm {
    pub fn new(cfg: OllamaLlmConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(cfg.timeout_secs.max(1)))
            .build()
            .expect("failed to build reqwest client");
        Self { cfg, client }
    }
}

#[derive(Deserialize)]
struct ChatResp {
    message: ChatMsg,
}

#[derive(Deserialize)]
struct ChatMsg {
    content: String,
}

#[async_trait]
impl LlmProvider for OllamaLlm {
    fn model_id(&self) -> &str {
        &self.cfg.model
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<String> {
        let mut messages = vec![serde_json::json!({
            "role": "system",
            "content": req.system,
        })];
        for m in &req.messages {
            let role = match m.role {
                LlmRole::System => "system",
                LlmRole::User => "user",
                LlmRole::Assistant => "assistant",
            };
            messages.push(serde_json::json!({ "role": role, "content": m.content }));
        }
        let mut body = serde_json::json!({
            "model": self.cfg.model,
            "messages": messages,
            "stream": false,
        });
        if req.json {
            body["format"] = serde_json::json!("json");
        }
        let url = format!("{}/api/chat", self.cfg.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| MnemosError::Internal(format!("ollama chat request failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(MnemosError::Internal(format!(
                "ollama chat returned HTTP {}",
                resp.status()
            )));
        }
        let parsed: ChatResp = resp
            .json()
            .await
            .map_err(|e| MnemosError::Internal(format!("ollama chat decode failed: {e}")))?;
        Ok(parsed.message.content)
    }
}
```

- [ ] **Step 4: Add the module declaration** in `providers/mod.rs`:

```rust
pub mod ollama_llm;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --lib providers::ollama_llm`
Expected: PASS (1 test; the live test is ignored).

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/providers/mod.rs crates/mnemos_core/src/providers/ollama_llm.rs
git commit -m "feat: add OllamaLlm provider over /api/chat (Plan 4 Task 2)"
```

---

## Task 3: `pipeline` module + `extract_facts`

The pipeline module holds the pure learning logic. This task creates the module, the shared `extract_json` helper (LLMs wrap JSON in prose/fences), the `CandidateFact` / `ResolveOp` types, and the first stage: extraction.

**Files:**
- Modify: `crates/mnemos_core/src/lib.rs` (add `pub mod pipeline;`)
- Create: `crates/mnemos_core/src/pipeline/mod.rs`
- Create: `crates/mnemos_core/src/pipeline/extract.rs`

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/src/pipeline/extract.rs` with the test module only:

```rust
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
        let facts = extract_facts(&chunks, &MockLlm::new()).await.unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].text, "Shaun prefers Rust over Go");
    }

    #[tokio::test]
    async fn empty_chunks_yield_no_facts() {
        let facts = extract_facts(&[], &MockLlm::new()).await.unwrap();
        assert!(facts.is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --lib pipeline::extract`
Expected: FAIL — `pipeline` module / `extract_facts` not found.

- [ ] **Step 3: Create `pipeline/mod.rs`**

```rust
//! Async learning pipeline: extract → resolve → entity-link → graph-update,
//! plus the decay pass. Each stage is a pure-ish function taking `&dyn
//! LlmProvider`; the daemon's `PipelineRunner` orchestrates them off the
//! `SessionEnded` event.

pub mod extract;
// Later pipeline stages are declared by the tasks that create their files, so
// the crate keeps compiling between tasks:
//   resolve (Task 5), entities (Task 7), graph (Task 8), decay (Task 9).

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
```

- [ ] **Step 4: Prepend the implementation above the test module in `extract.rs`**

```rust
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
pub async fn extract_facts(chunks: &[Chunk], llm: &dyn LlmProvider) -> Result<Vec<CandidateFact>> {
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
    let req = CompletionRequest::new(EXTRACT_SYSTEM, transcript);
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
```

- [ ] **Step 5: Add the module to `lib.rs`**

Add `pub mod pipeline;` alongside the other `pub mod` declarations (e.g. after `pub mod paths;`).

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --lib pipeline`
Expected: PASS (extract: 2, mod: 2).

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/lib.rs crates/mnemos_core/src/pipeline/mod.rs crates/mnemos_core/src/pipeline/extract.rs
git commit -m "feat: add pipeline module, extract_json, and extract_facts (Plan 4 Task 3)"
```

---

## Task 4: `memory_ops` extensions + `RememberOpts.provenance` + `Vault::patch`

Resolution needs to (a) store provenance on new memories, (b) link memories to their source chunks, (c) patch metadata, and (d) query as-of a timestamp. This task adds all four primitives so Task 5 can use them.

**Files:**
- Modify: `crates/mnemos_core/src/storage/memory_ops.rs` (add `link_memory_chunks`, `recall_as_of`)
- Modify: `crates/mnemos_core/src/vault.rs` (add `provenance` to `RememberOpts`, use it in `remember`, add `Vault::patch`)
- Modify: `crates/mnemos_daemon/src/routes/memories.rs` and `crates/mnemos_daemon/src/mcp/tools.rs` (add `provenance: vec![]` to the two `RememberOpts` literals)
- Test: `crates/mnemos_core/tests/pipeline_memory_ops.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/pipeline_memory_ops.rs`:

```rust
use mnemos_core::paths::Paths;
use mnemos_core::storage::memory_ops::{link_memory_chunks, recall_as_of};
use mnemos_core::types::Provenance;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::Tier;
use tempfile::TempDir;

#[tokio::test]
async fn remember_persists_provenance_and_chunk_links() {
    let tmp = TempDir::new().unwrap();
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    let id = vault
        .remember(
            "Shaun loves Rust",
            RememberOpts {
                tier: Tier::Semantic,
                provenance: vec![Provenance {
                    session: Some("sess_1".into()),
                    chunks: vec!["chunk_a".into(), "chunk_b".into()],
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let mem = vault.get(&id).await.unwrap();
    assert_eq!(mem.provenance.len(), 1);
    assert_eq!(mem.provenance[0].session.as_deref(), Some("sess_1"));

    link_memory_chunks(vault.storage(), &id, &["chunk_a".into(), "chunk_b".into()])
        .await
        .unwrap();
    let conn = vault.storage().conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM memory_chunks WHERE memory_id = ?",
            libsql::params![id.clone()],
        )
        .await
        .unwrap();
    let n: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(n, 2);
}

#[tokio::test]
async fn patch_updates_tags_and_importance() {
    let tmp = TempDir::new().unwrap();
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let id = vault
        .remember("patch me", RememberOpts::default())
        .await
        .unwrap();

    let updated = vault
        .patch(&id, Some(vec!["x".into(), "y".into()]), Some(0.9))
        .await
        .unwrap();
    assert_eq!(updated.tags, vec!["x".to_string(), "y".to_string()]);
    assert!((updated.importance - 0.9).abs() < 1e-9);

    // Round-trips through the file too.
    let reloaded = vault.get(&id).await.unwrap();
    assert_eq!(reloaded.tags, vec!["x".to_string(), "y".to_string()]);
}

#[tokio::test]
async fn recall_as_of_respects_temporal_window() {
    let tmp = TempDir::new().unwrap();
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let id = vault
        .remember("alpha temporal beacon", RememberOpts::default())
        .await
        .unwrap();
    let mem = vault.get(&id).await.unwrap();

    let future = mem.valid_at + chrono::Duration::days(1);
    let past = mem.valid_at - chrono::Duration::days(1);

    let hits_future = recall_as_of(vault.storage(), "alpha", future, 10).await.unwrap();
    assert_eq!(hits_future.len(), 1);
    assert_eq!(hits_future[0].id, id);

    let hits_past = recall_as_of(vault.storage(), "alpha", past, 10).await.unwrap();
    assert!(hits_past.is_empty(), "memory not yet valid in the past");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test pipeline_memory_ops`
Expected: FAIL — `provenance` field, `link_memory_chunks`, `recall_as_of`, `Vault::patch` do not exist.

- [ ] **Step 3: Add `provenance` to `RememberOpts` and use it in `remember`** (`vault.rs`)

In the `RememberOpts` struct, add the field (keep `#[derive(Default)]` working — `Vec` defaults to empty):

```rust
#[derive(Debug, Clone, Default)]
pub struct RememberOpts {
    pub title: Option<String>,
    pub tier: Tier,
    pub kind: MemoryType,
    pub tags: Vec<String>,
    pub importance: Option<f64>,
    pub workspace: Option<String>,
    pub source_tool: Option<String>,
    /// Provenance links (session + chunk ids) for memories derived by the
    /// async pipeline. Empty for manually-created memories.
    pub provenance: Vec<Provenance>,
}
```

In `remember`, replace the hard-coded `provenance: vec![],` line in the `Memory { .. }` literal with:

```rust
            provenance: opts.provenance,
```

Add `Provenance` to the `use crate::types::...` import at the top of `vault.rs`:

```rust
use crate::types::{Memory, MemoryType, Provenance};
```

- [ ] **Step 4: Add `Vault::patch`** — insert this method inside `impl Vault` in `vault.rs` (e.g. after `list`):

```rust
    /// Patch mutable metadata (tags and/or importance) on a memory.
    ///
    /// Updates the DB row, rewrites the markdown file so disk remains the
    /// source of truth, writes an `update` audit entry, and returns the
    /// refreshed memory. `title` and `body` are not patchable here — those go
    /// through file edits + reindex.
    pub async fn patch(
        &self,
        id: &str,
        tags: Option<Vec<String>>,
        importance: Option<f64>,
    ) -> Result<Memory> {
        let mut mem = get_memory(&self.storage, id).await?;
        if let Some(t) = tags {
            mem.tags = t;
        }
        if let Some(i) = importance {
            mem.importance = i;
        }
        let new_path = write_memory_file(&self.paths, &mem).await?;
        let new_hash = content_hash(&mem.body);
        {
            let (conn, _g) = self.storage.write_conn().await?;
            conn.execute(
                "UPDATE memories SET tags_json = ?, importance = ?, file_path = ?, content_hash = ? WHERE id = ?",
                libsql::params![
                    serde_json::to_string(&mem.tags)?,
                    mem.importance,
                    new_path.to_string_lossy().to_string(),
                    new_hash,
                    id.to_string()
                ],
            )
            .await?;
        }
        write_audit(
            &self.storage,
            opts_actor(),
            "update",
            Some(id),
            Some(json!({ "tags": mem.tags, "importance": mem.importance })),
        )
        .await?;
        get_memory(&self.storage, id).await
    }
```

- [ ] **Step 5: Add `link_memory_chunks` and `recall_as_of`** — append to `memory_ops.rs`:

```rust
/// Record provenance links from a memory to the chunks it was derived from.
/// Idempotent (`INSERT OR IGNORE`). No-op for an empty chunk list.
pub async fn link_memory_chunks(
    storage: &Storage,
    memory_id: &str,
    chunk_ids: &[String],
) -> Result<()> {
    if chunk_ids.is_empty() {
        return Ok(());
    }
    let (conn, _guard) = storage.write_conn().await?;
    for cid in chunk_ids {
        conn.execute(
            "INSERT OR IGNORE INTO memory_chunks (memory_id, chunk_id) VALUES (?, ?)",
            params![memory_id.to_string(), cid.clone()],
        )
        .await?;
    }
    Ok(())
}

/// Time-travel recall: full-text match `query` restricted to memories that were
/// valid at `as_of` (`valid_at <= as_of < invalid_at`). Ordered by FTS rank.
///
/// The query is treated as a single FTS phrase (quotes stripped) so arbitrary
/// user input cannot produce an FTS syntax error.
pub async fn recall_as_of(
    storage: &Storage,
    query: &str,
    as_of: DateTime<Utc>,
    k: usize,
) -> Result<Vec<Memory>> {
    let phrase = format!("\"{}\"", query.replace('"', " "));
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT m.id, m.tier, m.kind, m.title, m.body,
                    m.tags_json, m.entities_json, m.links_json, m.provenance_json,
                    m.created_at, m.ingested_at, m.valid_at, m.invalid_at, m.superseded_by,
                    m.strength, m.importance, m.last_accessed, m.access_count,
                    m.workspace, m.source_tool, m.mnemos_version
               FROM memory_fts f
               JOIN memories m ON m.id = f.memory_id
              WHERE memory_fts MATCH ?1
                AND m.valid_at <= ?2
                AND (m.invalid_at IS NULL OR m.invalid_at > ?2)
              ORDER BY rank
              LIMIT ?3",
            params![phrase, as_of.to_rfc3339(), k as i64],
        )
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(row_to_memory(&row)?);
    }
    Ok(out)
}
```

- [ ] **Step 6: Fix the two `RememberOpts` literals** so they still compile with the new field. In `crates/mnemos_daemon/src/routes/memories.rs` (`post_memory`) and `crates/mnemos_daemon/src/mcp/tools.rs` (`remember`), add this line inside the `RememberOpts { .. }` literal (after `source_tool: ...`):

```rust
                provenance: vec![],
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test pipeline_memory_ops && cargo build -p mnemos_daemon`
Expected: PASS (3 tests); daemon still builds.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/mnemos_core/src/storage/memory_ops.rs crates/mnemos_core/src/vault.rs crates/mnemos_daemon/src/routes/memories.rs crates/mnemos_daemon/src/mcp/tools.rs crates/mnemos_core/tests/pipeline_memory_ops.rs
git commit -m "feat: provenance on RememberOpts, chunk links, patch, recall_as_of (Plan 4 Task 4)"
```

---

## Task 5: `resolve_and_apply` (ADD / UPDATE / DELETE / NOOP)

The resolution stage decides what to do with each candidate fact against existing memory and applies it. This is the heart of continuous learning.

**Files:**
- Create: `crates/mnemos_core/src/pipeline/resolve.rs`
- Modify: `crates/mnemos_core/src/pipeline/mod.rs` (add `pub mod resolve;`)
- Test: `crates/mnemos_core/tests/pipeline_resolve.rs` (new)

> Note: Task 3 deliberately did NOT declare `pub mod resolve;` (so the crate kept compiling). This task creates the file AND adds the declaration in Step 4.

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/pipeline_resolve.rs`:

```rust
use mnemos_core::paths::Paths;
use mnemos_core::pipeline::resolve::resolve_and_apply;
use mnemos_core::pipeline::{CandidateFact, ResolveOp};
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::types::Provenance;
use mnemos_core::vault::{RememberOpts, Vault};
use tempfile::TempDir;

async fn vault() -> Vault {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    Vault::open(Paths::with_root(tmp.path())).await.unwrap()
}

fn prov() -> Provenance {
    Provenance {
        session: Some("sess_1".into()),
        chunks: vec!["chunk_1".into()],
    }
}

#[tokio::test]
async fn add_creates_new_memory() {
    let v = vault().await;
    let cand = CandidateFact {
        text: "Shaun loves Rust".into(),
    };
    let (op, new_id) = resolve_and_apply(&v, &cand, prov(), &MockLlm::new())
        .await
        .unwrap();
    assert_eq!(op, ResolveOp::Add);
    let id = new_id.expect("add returns id");
    assert_eq!(v.get(&id).await.unwrap().body, "Shaun loves Rust");
}

#[tokio::test]
async fn update_supersedes_existing() {
    let v = vault().await;
    let old = v
        .remember("Shaun uses vim", RememberOpts::default())
        .await
        .unwrap();
    let cand = CandidateFact {
        text: format!("Shaun now uses Helix OP=update TARGET={old}"),
    };
    let (op, new_id) = resolve_and_apply(&v, &cand, prov(), &MockLlm::new())
        .await
        .unwrap();
    assert!(matches!(op, ResolveOp::Update { .. }));
    let new_id = new_id.unwrap();
    // old is invalidated and superseded by the new one
    let old_mem = v.get(&old).await.unwrap();
    assert!(old_mem.invalid_at.is_some());
    assert_eq!(old_mem.superseded_by.as_deref(), Some(new_id.as_str()));
}

#[tokio::test]
async fn delete_invalidates_target() {
    let v = vault().await;
    let target = v
        .remember("temporary fact", RememberOpts::default())
        .await
        .unwrap();
    let cand = CandidateFact {
        text: format!("that is no longer true OP=delete TARGET={target}"),
    };
    let (op, new_id) = resolve_and_apply(&v, &cand, prov(), &MockLlm::new())
        .await
        .unwrap();
    assert!(matches!(op, ResolveOp::Delete { .. }));
    assert!(new_id.is_none());
    assert!(v.get(&target).await.unwrap().invalid_at.is_some());
}

#[tokio::test]
async fn noop_creates_nothing() {
    let v = vault().await;
    let before = v.list(ListFilter::default()).await.unwrap().len();
    let cand = CandidateFact {
        text: "already known OP=noop".into(),
    };
    let (op, new_id) = resolve_and_apply(&v, &cand, prov(), &MockLlm::new())
        .await
        .unwrap();
    assert!(matches!(op, ResolveOp::Noop { .. }));
    assert!(new_id.is_none());
    let after = v.list(ListFilter::default()).await.unwrap().len();
    assert_eq!(before, after);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test pipeline_resolve`
Expected: FAIL — `resolve_and_apply` not found.

- [ ] **Step 3: Create `crates/mnemos_core/src/pipeline/resolve.rs`**

```rust
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
    let raw = llm.complete(&CompletionRequest::new(RESOLVE_SYSTEM, user)).await?;
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
async fn store(vault: &Vault, candidate: &CandidateFact, provenance: &Provenance) -> Result<String> {
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
```

- [ ] **Step 4: Declare the module** — add to `crates/mnemos_core/src/pipeline/mod.rs` (next to `pub mod extract;`):

```rust
pub mod resolve;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test pipeline_resolve`
Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/pipeline/mod.rs crates/mnemos_core/src/pipeline/resolve.rs crates/mnemos_core/tests/pipeline_resolve.rs
git commit -m "feat: resolve_and_apply with ADD/UPDATE/DELETE/NOOP (Plan 4 Task 5)"
```

---

## Task 6: `entity_ops` storage primitives

The entity graph storage layer: upsert entities by unique name, record mentions, and upsert relationship edges (bi-temporal, weighted).

**Files:**
- Replace: `crates/mnemos_core/src/storage/entity_ops.rs` (currently a one-line stub)
- Test: `crates/mnemos_core/tests/entity_ops.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/entity_ops.rs`:

```rust
use mnemos_core::paths::Paths;
use mnemos_core::storage::entity_ops::{
    find_entity_by_name, link_entity_mention, upsert_edge, upsert_entity,
};
use mnemos_core::vault::Vault;
use chrono::Utc;
use tempfile::TempDir;

async fn vault() -> Vault {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    Vault::open(Paths::with_root(tmp.path())).await.unwrap()
}

#[tokio::test]
async fn upsert_entity_is_idempotent_by_name() {
    let v = vault().await;
    let a = upsert_entity(v.storage(), "Rust", "tool").await.unwrap();
    let b = upsert_entity(v.storage(), "Rust", "tool").await.unwrap();
    assert_eq!(a, b);
    let found = find_entity_by_name(v.storage(), "Rust").await.unwrap();
    assert_eq!(found.unwrap().id, a);
}

#[tokio::test]
async fn link_mention_is_idempotent() {
    let v = vault().await;
    let e = upsert_entity(v.storage(), "Shaun", "person").await.unwrap();
    link_entity_mention(v.storage(), "mem_1", &e).await.unwrap();
    link_entity_mention(v.storage(), "mem_1", &e).await.unwrap();
    let conn = v.storage().conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM entity_mentions WHERE entity_id = ?",
            libsql::params![e.clone()],
        )
        .await
        .unwrap();
    let n: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn upsert_edge_merges_and_bumps_weight() {
    let v = vault().await;
    let s = upsert_entity(v.storage(), "Shaun", "person").await.unwrap();
    let t = upsert_entity(v.storage(), "Rust", "tool").await.unwrap();
    let e1 = upsert_edge(v.storage(), &s, &t, "uses", "mem_1", Utc::now())
        .await
        .unwrap();
    let e2 = upsert_edge(v.storage(), &s, &t, "uses", "mem_2", Utc::now())
        .await
        .unwrap();
    assert_eq!(e1, e2, "same (source,target,relation) edge is reused");
    let conn = v.storage().conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT weight, source_memory_ids FROM entity_edges WHERE id = ?",
            libsql::params![e1.clone()],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let weight: f64 = row.get(0).unwrap();
    let mids_json: String = row.get(1).unwrap();
    assert!((weight - 2.0).abs() < 1e-9);
    let mids: Vec<String> = serde_json::from_str(&mids_json).unwrap();
    assert_eq!(mids.len(), 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test entity_ops`
Expected: FAIL — functions not found.

- [ ] **Step 3: Replace `crates/mnemos_core/src/storage/entity_ops.rs`**

```rust
//! Entity + edge storage primitives backing the knowledge graph.

use crate::error::Result;
use crate::id::{new_edge_id, new_entity_id};
use crate::storage::Storage;
use crate::types::Entity;
use chrono::{DateTime, Utc};
use libsql::params;

/// Insert an entity by unique `name`, or return the id of the existing one.
pub async fn upsert_entity(storage: &Storage, name: &str, kind: &str) -> Result<String> {
    let (conn, _guard) = storage.write_conn().await?;
    let mut rows = conn
        .query("SELECT id FROM entities WHERE name = ?", params![name.to_string()])
        .await?;
    if let Some(r) = rows.next().await? {
        return Ok(r.get::<String>(0)?);
    }
    drop(rows);
    let id = new_entity_id();
    conn.execute(
        "INSERT INTO entities (id, name, kind, aliases, description, file_path, created_at)
             VALUES (?, ?, ?, '[]', NULL, NULL, ?)",
        params![
            id.clone(),
            name.to_string(),
            kind.to_string(),
            Utc::now().to_rfc3339()
        ],
    )
    .await?;
    Ok(id)
}

/// Look up an entity by exact name.
pub async fn find_entity_by_name(storage: &Storage, name: &str) -> Result<Option<Entity>> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT id, name, kind, aliases, description, file_path, created_at
                 FROM entities WHERE name = ?",
            params![name.to_string()],
        )
        .await?;
    match rows.next().await? {
        None => Ok(None),
        Some(r) => Ok(Some(Entity {
            id: r.get(0)?,
            name: r.get(1)?,
            kind: r.get(2)?,
            aliases: serde_json::from_str(&r.get::<String>(3)?)?,
            description: r.get(4)?,
            file_path: r.get(5)?,
            created_at: DateTime::parse_from_rfc3339(&r.get::<String>(6)?)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| crate::error::MnemosError::Validation(format!("bad ts: {e}")))?,
        })),
    }
}

/// Record that `memory_id` mentions `entity_id`. Idempotent.
pub async fn link_entity_mention(storage: &Storage, memory_id: &str, entity_id: &str) -> Result<()> {
    let (conn, _guard) = storage.write_conn().await?;
    conn.execute(
        "INSERT OR IGNORE INTO entity_mentions (memory_id, entity_id) VALUES (?, ?)",
        params![memory_id.to_string(), entity_id.to_string()],
    )
    .await?;
    Ok(())
}

/// Insert a relationship edge, or reinforce the existing active one.
///
/// "Active" = same `(source, target, relation)` with `invalid_at IS NULL`. When
/// found, the edge's `weight` is bumped and `source_memory_id` is appended to
/// its provenance list. Returns the edge id either way.
pub async fn upsert_edge(
    storage: &Storage,
    source_id: &str,
    target_id: &str,
    relation: &str,
    source_memory_id: &str,
    valid_at: DateTime<Utc>,
) -> Result<String> {
    let (conn, _guard) = storage.write_conn().await?;
    let mut rows = conn
        .query(
            "SELECT id, source_memory_ids FROM entity_edges
              WHERE source_entity_id = ? AND target_entity_id = ?
                AND relation = ? AND invalid_at IS NULL",
            params![
                source_id.to_string(),
                target_id.to_string(),
                relation.to_string()
            ],
        )
        .await?;
    if let Some(r) = rows.next().await? {
        let id: String = r.get(0)?;
        let mids_json: String = r.get(1)?;
        drop(rows);
        let mut mids: Vec<String> = serde_json::from_str(&mids_json).unwrap_or_default();
        if !mids.iter().any(|m| m == source_memory_id) {
            mids.push(source_memory_id.to_string());
        }
        conn.execute(
            "UPDATE entity_edges SET weight = weight + 1.0, source_memory_ids = ? WHERE id = ?",
            params![serde_json::to_string(&mids)?, id.clone()],
        )
        .await?;
        return Ok(id);
    }
    drop(rows);
    let id = new_edge_id();
    let mids = serde_json::to_string(&vec![source_memory_id.to_string()])?;
    conn.execute(
        "INSERT INTO entity_edges
            (id, source_entity_id, target_entity_id, relation, created_at, valid_at, invalid_at, weight, source_memory_ids)
         VALUES (?, ?, ?, ?, ?, ?, NULL, 1.0, ?)",
        params![
            id.clone(),
            source_id.to_string(),
            target_id.to_string(),
            relation.to_string(),
            Utc::now().to_rfc3339(),
            valid_at.to_rfc3339(),
            mids
        ],
    )
    .await?;
    Ok(id)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test entity_ops`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/storage/entity_ops.rs crates/mnemos_core/tests/entity_ops.rs
git commit -m "feat: entity + edge storage primitives (Plan 4 Task 6)"
```

---

## Task 7: `link_entities` pipeline stage

Given a stored memory's body, ask the LLM for named entities, upsert them, and record mentions. Builds the node set of the knowledge graph.

**Files:**
- Create: `crates/mnemos_core/src/pipeline/entities.rs`
- Modify: `crates/mnemos_core/src/pipeline/mod.rs` (add `pub mod entities;`)
- Test: `crates/mnemos_core/tests/pipeline_entities.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/pipeline_entities.rs`:

```rust
use mnemos_core::paths::Paths;
use mnemos_core::pipeline::entities::link_entities;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::vault::Vault;
use tempfile::TempDir;

#[tokio::test]
async fn links_entities_and_records_mentions() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    let ids = link_entities(v.storage(), "mem_99", "@Shaun ships @Rust code", &MockLlm::new())
        .await
        .unwrap();
    assert_eq!(ids.len(), 2);

    let conn = v.storage().conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM entity_mentions WHERE memory_id = ?",
            libsql::params!["mem_99".to_string()],
        )
        .await
        .unwrap();
    let n: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(n, 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test pipeline_entities`
Expected: FAIL — `link_entities` not found.

- [ ] **Step 3: Create `crates/mnemos_core/src/pipeline/entities.rs`**

```rust
use crate::error::{MnemosError, Result};
use crate::pipeline::extract_json;
use crate::providers::{CompletionRequest, LlmProvider};
use crate::storage::entity_ops::{link_entity_mention, upsert_entity};
use crate::storage::Storage;
use serde::Deserialize;

/// System prompt for the entity-linking stage.
pub const LINK_SYSTEM: &str = "TASK=link\n\
List the named entities (people, projects, organizations, tools, concepts) \
mentioned in the text. Respond ONLY with JSON \
{\"entities\":[{\"name\":\"...\",\"kind\":\"...\"}]}.";

#[derive(Deserialize)]
struct LinkOut {
    #[serde(default)]
    entities: Vec<EntityIn>,
}

#[derive(Deserialize)]
struct EntityIn {
    name: String,
    #[serde(default)]
    kind: Option<String>,
}

/// Extract entities from `body`, upsert them, and link mentions to `memory_id`.
/// Returns the entity ids (deduplicated by name via `upsert_entity`).
pub async fn link_entities(
    storage: &Storage,
    memory_id: &str,
    body: &str,
    llm: &dyn LlmProvider,
) -> Result<Vec<String>> {
    let raw = llm.complete(&CompletionRequest::new(LINK_SYSTEM, body)).await?;
    let parsed: LinkOut = serde_json::from_str(extract_json(&raw))
        .map_err(|e| MnemosError::Internal(format!("link parse failed: {e}; raw={raw}")))?;
    let mut ids = Vec::new();
    for e in parsed.entities {
        let name = e.name.trim();
        if name.is_empty() {
            continue;
        }
        let kind = e.kind.unwrap_or_else(|| "unknown".into());
        let id = upsert_entity(storage, name, &kind).await?;
        link_entity_mention(storage, memory_id, &id).await?;
        ids.push(id);
    }
    Ok(ids)
}
```

- [ ] **Step 4: Declare the module** — add to `crates/mnemos_core/src/pipeline/mod.rs`:

```rust
pub mod entities;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test pipeline_entities`
Expected: PASS (1 test).

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/pipeline/mod.rs crates/mnemos_core/src/pipeline/entities.rs crates/mnemos_core/tests/pipeline_entities.rs
git commit -m "feat: entity-linking pipeline stage (Plan 4 Task 7)"
```

---

## Task 8: `update_graph` pipeline stage

Extract relationship triples from a memory and upsert weighted, bi-temporal edges between entities.

**Files:**
- Create: `crates/mnemos_core/src/pipeline/graph.rs`
- Modify: `crates/mnemos_core/src/pipeline/mod.rs` (add `pub mod graph;`)
- Test: `crates/mnemos_core/tests/pipeline_graph.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/pipeline_graph.rs`:

```rust
use mnemos_core::paths::Paths;
use mnemos_core::pipeline::graph::update_graph;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::vault::Vault;
use chrono::Utc;
use tempfile::TempDir;

#[tokio::test]
async fn builds_edges_and_entities_from_triples() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    let edges = update_graph(
        v.storage(),
        "mem_1",
        "Shaun~uses~Rust and Shaun~works_at~Armellini",
        Utc::now(),
        &MockLlm::new(),
    )
    .await
    .unwrap();
    assert_eq!(edges.len(), 2);

    let conn = v.storage().conn().unwrap();
    let mut er = conn
        .query("SELECT COUNT(*) FROM entity_edges", ())
        .await
        .unwrap();
    let edge_count: i64 = er.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(edge_count, 2);

    let mut nr = conn.query("SELECT COUNT(*) FROM entities", ()).await.unwrap();
    let node_count: i64 = nr.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(node_count, 3, "Shaun, Rust, Armellini");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test pipeline_graph`
Expected: FAIL — `update_graph` not found.

- [ ] **Step 3: Create `crates/mnemos_core/src/pipeline/graph.rs`**

```rust
use crate::error::{MnemosError, Result};
use crate::pipeline::extract_json;
use crate::providers::{CompletionRequest, LlmProvider};
use crate::storage::entity_ops::{upsert_edge, upsert_entity};
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use serde::Deserialize;

/// System prompt for the graph-update stage.
pub const RELATIONS_SYSTEM: &str = "TASK=relations\n\
Extract relationships between entities as subject–relation–object triples. \
Respond ONLY with JSON \
{\"relations\":[{\"source\":\"A\",\"relation\":\"REL\",\"target\":\"B\"}]}.";

#[derive(Deserialize)]
struct RelOut {
    #[serde(default)]
    relations: Vec<Triple>,
}

#[derive(Deserialize)]
struct Triple {
    source: String,
    relation: String,
    target: String,
}

/// Extract relationship triples from `body` and upsert the corresponding
/// entities and edges. `valid_at` stamps newly-created edges (bi-temporal).
/// Returns the edge ids touched.
pub async fn update_graph(
    storage: &Storage,
    memory_id: &str,
    body: &str,
    valid_at: DateTime<Utc>,
    llm: &dyn LlmProvider,
) -> Result<Vec<String>> {
    let raw = llm
        .complete(&CompletionRequest::new(RELATIONS_SYSTEM, body))
        .await?;
    let parsed: RelOut = serde_json::from_str(extract_json(&raw))
        .map_err(|e| MnemosError::Internal(format!("relations parse failed: {e}; raw={raw}")))?;
    let mut edge_ids = Vec::new();
    for t in parsed.relations {
        let (s, r, o) = (t.source.trim(), t.relation.trim(), t.target.trim());
        if s.is_empty() || r.is_empty() || o.is_empty() {
            continue;
        }
        let src = upsert_entity(storage, s, "unknown").await?;
        let tgt = upsert_entity(storage, o, "unknown").await?;
        let edge = upsert_edge(storage, &src, &tgt, r, memory_id, valid_at).await?;
        edge_ids.push(edge);
    }
    Ok(edge_ids)
}
```

- [ ] **Step 4: Declare the module** — add to `crates/mnemos_core/src/pipeline/mod.rs`:

```rust
pub mod graph;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test pipeline_graph`
Expected: PASS (1 test).

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/pipeline/mod.rs crates/mnemos_core/src/pipeline/graph.rs crates/mnemos_core/tests/pipeline_graph.rs
git commit -m "feat: graph-update pipeline stage (Plan 4 Task 8)"
```

---

## Task 9: `decay_pass` + `Vault::run_decay`

Ebbinghaus strength decay. Pure `decay_pass` (takes an explicit `now` for testability) updates strengths in the DB and reports which memories crossed the invalidation floor; `Vault::run_decay` applies those invalidations through `forget` so files stay authoritative.

**Files:**
- Create: `crates/mnemos_core/src/pipeline/decay.rs`
- Modify: `crates/mnemos_core/src/pipeline/mod.rs` (add `pub mod decay;`)
- Modify: `crates/mnemos_core/src/vault.rs` (add `run_decay`)
- Test: `crates/mnemos_core/tests/pipeline_decay.rs` (new)

> Design note: `decay_pass` writes only the volatile `strength` column to the DB — it does NOT rewrite files every pass (that would churn the entire vault hourly). A full `rebuild` therefore resets strength to the file's frontmatter value; this is an accepted trade-off (strength is a derived/volatile signal). Invalidations, however, ARE persisted to files via `forget`, so decayed-out memories are not resurrected on rebuild.

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/pipeline_decay.rs`:

```rust
use mnemos_core::paths::Paths;
use mnemos_core::pipeline::decay::{decay_pass, DecayConfig};
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::Tier;
use chrono::{Duration, Utc};
use tempfile::TempDir;

async fn backdate_last_accessed(v: &Vault, id: &str, days_ago: i64) {
    let when = (Utc::now() - Duration::days(days_ago)).to_rfc3339();
    let (conn, _g) = v.storage().write_conn().await.unwrap();
    conn.execute(
        "UPDATE memories SET last_accessed = ? WHERE id = ?",
        libsql::params![when, id.to_string()],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn working_memory_decays_below_floor_and_is_flagged() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let id = v
        .remember(
            "ephemeral working note",
            RememberOpts {
                tier: Tier::Working,
                importance: Some(0.0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    backdate_last_accessed(&v, &id, 30).await;

    let stats = decay_pass(v.storage(), Utc::now(), &DecayConfig::default())
        .await
        .unwrap();
    assert_eq!(stats.scanned, 1);
    assert_eq!(stats.decayed, 1);
    assert!(stats.to_invalidate.contains(&id));
}

#[tokio::test]
async fn run_decay_invalidates_and_persists() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let id = v
        .remember(
            "ephemeral",
            RememberOpts {
                tier: Tier::Working,
                importance: Some(0.0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    backdate_last_accessed(&v, &id, 30).await;

    let stats = v.run_decay(&DecayConfig::default()).await.unwrap();
    assert!(stats.to_invalidate.contains(&id));
    assert!(v.get(&id).await.unwrap().invalid_at.is_some());
}

#[tokio::test]
async fn semantic_memory_with_high_importance_survives() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let id = v
        .remember(
            "durable identity fact",
            RememberOpts {
                tier: Tier::Semantic,
                importance: Some(1.0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    backdate_last_accessed(&v, &id, 30).await;
    let stats = decay_pass(v.storage(), Utc::now(), &DecayConfig::default())
        .await
        .unwrap();
    assert!(!stats.to_invalidate.contains(&id));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test pipeline_decay`
Expected: FAIL — `decay` module / `run_decay` not found.

- [ ] **Step 3: Create `crates/mnemos_core/src/pipeline/decay.rs`**

```rust
use crate::error::{MnemosError, Result};
use crate::storage::Storage;
use crate::tier::Tier;
use chrono::{DateTime, Utc};
use libsql::params;
use std::str::FromStr;

/// Tunable decay parameters (half-lives in days per decaying tier, plus the
/// strength floor below which working/episodic memories are invalidated).
#[derive(Debug, Clone)]
pub struct DecayConfig {
    pub working_half_life_days: f64,
    pub episodic_half_life_days: f64,
    pub semantic_half_life_days: f64,
    pub floor: f64,
}

impl Default for DecayConfig {
    fn default() -> Self {
        Self {
            working_half_life_days: 1.0,
            episodic_half_life_days: 7.0,
            semantic_half_life_days: 90.0,
            floor: 0.05,
        }
    }
}

/// Outcome of a decay pass.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct DecayStats {
    pub scanned: usize,
    pub decayed: usize,
    /// Working/episodic memory ids that fell below the floor and should be
    /// invalidated by the caller (`Vault::run_decay`).
    pub to_invalidate: Vec<String>,
}

fn half_life_for(tier: Tier, cfg: &DecayConfig) -> Option<f64> {
    match tier {
        Tier::Working => Some(cfg.working_half_life_days),
        Tier::Episodic => Some(cfg.episodic_half_life_days),
        Tier::Semantic => Some(cfg.semantic_half_life_days),
        // procedural & reflection are not subject to time decay
        _ => None,
    }
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| MnemosError::Validation(format!("bad timestamp '{s}': {e}")))
}

/// Apply Ebbinghaus decay to every active, decaying-tier memory.
///
/// `strength' = strength * 0.5 ^ (idle_days / effective_half_life)` where
/// `effective_half_life = half_life * (1 + importance)` (important memories
/// fade slower). Updates the `strength` column; returns the ids of
/// working/episodic memories that dropped below `cfg.floor`.
pub async fn decay_pass(
    storage: &Storage,
    now: DateTime<Utc>,
    cfg: &DecayConfig,
) -> Result<DecayStats> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT id, tier, strength, last_accessed, importance
               FROM memories WHERE invalid_at IS NULL",
            (),
        )
        .await?;

    let mut updates: Vec<(String, f64)> = Vec::new();
    let mut stats = DecayStats::default();
    while let Some(r) = rows.next().await? {
        stats.scanned += 1;
        let id: String = r.get(0)?;
        let tier = Tier::from_str(&r.get::<String>(1)?)?;
        let strength: f64 = r.get(2)?;
        let last = parse_ts(&r.get::<String>(3)?)?;
        let importance: f64 = r.get(4)?;
        let Some(hl) = half_life_for(tier, cfg) else {
            continue;
        };
        let idle_days = (now - last).num_seconds() as f64 / 86_400.0;
        if idle_days <= 0.0 {
            continue;
        }
        let eff_hl = hl * (1.0 + importance);
        let new_strength = (strength * 0.5_f64.powf(idle_days / eff_hl)).clamp(0.0, 1.0);
        if (new_strength - strength).abs() < 1e-6 {
            continue;
        }
        updates.push((id.clone(), new_strength));
        stats.decayed += 1;
        if matches!(tier, Tier::Working | Tier::Episodic) && new_strength < cfg.floor {
            stats.to_invalidate.push(id);
        }
    }
    drop(rows);

    let (conn, _guard) = storage.write_conn().await?;
    for (id, s) in updates {
        conn.execute(
            "UPDATE memories SET strength = ? WHERE id = ?",
            params![s, id],
        )
        .await?;
    }
    Ok(stats)
}
```

- [ ] **Step 4: Add `Vault::run_decay`** — insert into `impl Vault` in `vault.rs` (after `patch`), and add the import at the top:

```rust
use crate::pipeline::decay::{decay_pass, DecayConfig, DecayStats};
```

```rust
    /// Run a decay pass and invalidate any memories that fell below the floor.
    /// Invalidation goes through `forget` so the change is persisted to disk.
    pub async fn run_decay(&self, cfg: &DecayConfig) -> Result<DecayStats> {
        let stats = decay_pass(&self.storage, Utc::now(), cfg).await?;
        for id in &stats.to_invalidate {
            if let Err(e) = self.forget(id, Some("decayed below strength floor")).await {
                tracing::warn!(memory_id = %id, error = %e, "decay invalidation failed");
            }
        }
        Ok(stats)
    }
```

- [ ] **Step 5: Declare the module** — add to `crates/mnemos_core/src/pipeline/mod.rs`:

```rust
pub mod decay;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test pipeline_decay`
Expected: PASS (3 tests).

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/pipeline/mod.rs crates/mnemos_core/src/pipeline/decay.rs crates/mnemos_core/src/vault.rs crates/mnemos_core/tests/pipeline_decay.rs
git commit -m "feat: Ebbinghaus decay pass and Vault::run_decay (Plan 4 Task 9)"
```

---

## Task 10: Schema migration v4 (`sessions.processed_at`)

The pipeline runner must not reprocess a session twice. Add a `processed_at` column it can stamp.

**Files:**
- Modify: `crates/mnemos_core/src/storage/migrations.rs`
- Test: `crates/mnemos_core/tests/schema_v4.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/schema_v4.rs`:

```rust
use mnemos_core::storage::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn migration_v4_adds_processed_at() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("v4.db")).await.unwrap();
    assert!(storage.schema_version().await.unwrap() >= 4);

    let conn = storage.conn().unwrap();
    // Column exists and is queryable (NULL by default).
    conn.execute(
        "INSERT INTO sessions (id, started_at) VALUES ('sess_x', '2026-01-01T00:00:00+00:00')",
        (),
    )
    .await
    .unwrap();
    let mut rows = conn
        .query("SELECT processed_at FROM sessions WHERE id = 'sess_x'", ())
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    assert!(row.get::<Option<String>>(0).unwrap().is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test schema_v4`
Expected: FAIL — `no such column: processed_at` and/or `schema_version < 4`.

- [ ] **Step 3: Add the v4 migration** in `migrations.rs`. In `apply_migrations`, after the `current < 3` block, add:

```rust
        if current < 4 {
            migration_v4(&conn).await?;
            conn.execute(
                "INSERT OR IGNORE INTO schema_migrations (version) VALUES (4)",
                (),
            )
            .await?;
        }
```

Then add the migration function + statements near the others:

```rust
async fn migration_v4(conn: &libsql::Connection) -> Result<()> {
    for stmt in V4_STATEMENTS {
        conn.execute(stmt, ()).await?;
    }
    Ok(())
}

const V4_STATEMENTS: &[&str] = &[
    // Stamped by the pipeline runner once a session's chunks have been
    // processed into memories, so SessionEnded is idempotent.
    "ALTER TABLE sessions ADD COLUMN processed_at TEXT",
];
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test schema_v4`
Expected: PASS.

- [ ] **Step 5: Verify the full core suite still passes** (migrations are foundational)

Run: `cargo test -p mnemos_core`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/storage/migrations.rs crates/mnemos_core/tests/schema_v4.rs
git commit -m "feat: schema v4 adds sessions.processed_at (Plan 4 Task 10)"
```

---

## Task 11: Config `[llm]` section

Daemon config gains an `[llm]` section parallel to `[embedder]`, with env overrides.

**Files:**
- Modify: `crates/mnemos_daemon/src/config.rs`
- Test: add a unit test in `config.rs` (or extend `crates/mnemos_daemon/tests/config.rs`)

- [ ] **Step 1: Write the failing test** — append to `crates/mnemos_daemon/tests/config.rs`:

```rust
#[test]
fn llm_defaults_to_ollama_llama() {
    use mnemos_daemon::config::{Config, LlmKind};
    let cfg = Config::default();
    assert_eq!(cfg.llm.kind, LlmKind::Ollama);
    assert_eq!(cfg.llm.model, "llama3.2");
    assert!(cfg.llm.url.contains("11434"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test config llm_defaults`
Expected: FAIL — `LlmKind` / `cfg.llm` not found.

- [ ] **Step 3: Add the config types** to `config.rs`. Add the field to `Config`:

```rust
    pub llm: LlmConfig,
```

(insert after `pub embedder: EmbedderConfig,`). Then add the struct + enum + default:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    pub kind: LlmKind,
    pub url: String,
    pub model: String,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LlmKind {
    Ollama,
    Mock,
    None,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            kind: LlmKind::Ollama,
            url: "http://localhost:11434".into(),
            model: "llama3.2".into(),
            timeout_secs: 120,
        }
    }
}
```

- [ ] **Step 4: Add env overrides** — inside `apply_env_overrides`, add:

```rust
    if let Ok(v) = std::env::var("MNEMOS_LLM") {
        cfg.llm.kind = match v.as_str() {
            "mock" => LlmKind::Mock,
            "none" => LlmKind::None,
            _ => LlmKind::Ollama,
        };
    }
    if let Ok(v) = std::env::var("MNEMOS_LLM_URL") {
        cfg.llm.url = v;
    }
    if let Ok(v) = std::env::var("MNEMOS_LLM_MODEL") {
        cfg.llm.model = v;
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test config`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/config.rs crates/mnemos_daemon/tests/config.rs
git commit -m "feat: add [llm] config section with env overrides (Plan 4 Task 11)"
```

---

## Task 12: Daemon foundation — events, pipeline status, AppState, LLM builder, `build_app_full`

Wire the LLM and pipeline-observability state into the daemon. The actual runner loop lands in Task 13; this task creates everything it plugs into so the next task is purely the loop.

**Files:**
- Modify: `crates/mnemos_daemon/src/events.rs` (add `PipelineCompleted`, `PipelineFailed`)
- Create: `crates/mnemos_daemon/src/pipeline_status.rs`
- Modify: `crates/mnemos_daemon/src/state.rs` (add `llm`, `pipeline_status`)
- Create: `crates/mnemos_daemon/src/llm.rs` (`build_llm_for_daemon`)
- Modify: `crates/mnemos_daemon/src/lib.rs` (declare modules; add `build_app_full`; make `build_app`/`build_app_with_reranker` delegate)
- Test: extend `crates/mnemos_daemon/tests/serve.rs` (a smoke test that `build_app_full` with `llm = None` returns no handle)

- [ ] **Step 1: Write the failing test** — append to `crates/mnemos_daemon/tests/serve.rs`:

```rust
#[tokio::test]
async fn build_app_full_without_llm_has_no_pipeline_handle() {
    use mnemos_core::paths::Paths;
    use mnemos_core::vault::Vault;
    use mnemos_daemon::{build_app_full, config::Config};
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (_app, state, handle) = build_app_full(Config::default(), vault, None, None)
        .await
        .unwrap();
    assert!(handle.is_none(), "no llm → no runner");
    assert!(state.llm.is_none());
    // pipeline status starts empty
    let (counters, recent) = state.pipeline_status.snapshot().await;
    assert_eq!(counters.completed, 0);
    assert!(recent.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test serve build_app_full_without_llm`
Expected: FAIL — `build_app_full`, `state.llm`, `pipeline_status` not found.

- [ ] **Step 3: Add the two events** to `events.rs` `Event` enum (inside the enum, after `SessionEnded`):

```rust
    PipelineCompleted {
        session_id: String,
        facts_added: usize,
    },
    PipelineFailed {
        session_id: String,
        error: String,
    },
```

- [ ] **Step 4: Create `crates/mnemos_daemon/src/pipeline_status.rs`**

```rust
//! Observable, in-memory pipeline status surfaced by `GET /v1/pipelines`.

use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

const RECENT_CAP: usize = 20;

#[derive(Debug, Default, Clone, Serialize)]
pub struct PipelineCounters {
    pub completed: u64,
    pub failed: u64,
    pub facts_added: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecentRun {
    pub session_id: String,
    pub facts_added: usize,
    pub ok: bool,
    pub at: String,
}

#[derive(Debug, Default)]
struct Inner {
    counters: PipelineCounters,
    recent: VecDeque<RecentRun>,
}

/// Cloneable handle to pipeline run statistics.
#[derive(Clone, Default)]
pub struct PipelineStatus {
    inner: Arc<Mutex<Inner>>,
}

impl PipelineStatus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the outcome of a pipeline run.
    pub async fn record(&self, run: RecentRun) {
        let mut g = self.inner.lock().await;
        if run.ok {
            g.counters.completed += 1;
            g.counters.facts_added += run.facts_added as u64;
        } else {
            g.counters.failed += 1;
        }
        g.recent.push_front(run);
        while g.recent.len() > RECENT_CAP {
            g.recent.pop_back();
        }
    }

    /// Snapshot the counters and the recent-runs list (newest first).
    pub async fn snapshot(&self) -> (PipelineCounters, Vec<RecentRun>) {
        let g = self.inner.lock().await;
        (g.counters.clone(), g.recent.iter().cloned().collect())
    }
}
```

- [ ] **Step 5: Extend `AppState`** in `state.rs`:

```rust
use mnemos_core::providers::{LlmProvider, Reranker};
use mnemos_core::vault::Vault;
use std::sync::Arc;

use crate::config::Config;
use crate::events::EventBus;
use crate::pipeline_status::PipelineStatus;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub vault: Vault,
    pub token: String,
    pub events: EventBus,
    pub reranker: Option<Arc<dyn Reranker>>,
    pub llm: Option<Arc<dyn LlmProvider>>,
    pub pipeline_status: PipelineStatus,
}
```

- [ ] **Step 6: Create `crates/mnemos_daemon/src/llm.rs`**

```rust
//! Builds the configured `LlmProvider` for the daemon.

use crate::config::{Config, LlmKind};
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::providers::ollama_llm::{OllamaLlm, OllamaLlmConfig};
use mnemos_core::providers::LlmProvider;
use std::sync::Arc;

/// Construct the LLM provider from config, or `None` when `kind = none`.
pub fn build_llm_for_daemon(cfg: &Config) -> Option<Arc<dyn LlmProvider>> {
    match cfg.llm.kind {
        LlmKind::None => None,
        LlmKind::Mock => Some(Arc::new(MockLlm::new())),
        LlmKind::Ollama => Some(Arc::new(OllamaLlm::new(OllamaLlmConfig {
            base_url: cfg.llm.url.clone(),
            model: cfg.llm.model.clone(),
            timeout_secs: cfg.llm.timeout_secs,
        }))),
    }
}
```

- [ ] **Step 7: Rewire `lib.rs`** — declare the new modules and add `build_app_full`. Add to the `pub mod` list:

```rust
pub mod llm;
pub mod pipeline_runner;
pub mod pipeline_status;
```

> `pipeline_runner` is created in Task 13. To keep this task compiling, create a placeholder file now: `crates/mnemos_daemon/src/pipeline_runner.rs` containing only the handle type + a `spawn` that is filled in Task 13:

```rust
//! Background pipeline runner. Loop body lands in Task 13.

use crate::state::AppState;
use tokio::sync::watch;

/// Handle to the background pipeline runner; `shutdown` stops it and joins.
pub struct PipelineHandle {
    pub(crate) join: tokio::task::JoinHandle<()>,
    pub(crate) shutdown: watch::Sender<bool>,
}

impl PipelineHandle {
    /// Signal the runner to stop and await its completion.
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(true);
        let _ = self.join.await;
    }
}

/// Spawn the runner. Filled in Task 13; placeholder idles until shutdown.
pub fn spawn(_state: AppState) -> PipelineHandle {
    let (tx, mut rx) = watch::channel(false);
    let join = tokio::spawn(async move {
        let _ = rx.changed().await;
    });
    PipelineHandle { join, shutdown: tx }
}
```

Now replace the `build_app*` functions in `lib.rs`:

```rust
pub async fn build_app(config: Config, vault: Vault) -> Result<(axum::Router, AppState)> {
    build_app_with_reranker(config, vault, None).await
}

pub async fn build_app_with_reranker(
    config: Config,
    vault: Vault,
    reranker: Option<Arc<dyn mnemos_core::providers::Reranker>>,
) -> Result<(axum::Router, AppState)> {
    let (app, state, _handle) = build_app_full(config, vault, reranker, None).await?;
    Ok((app, state))
}

/// Full constructor: also wires the LLM and spawns the pipeline runner when an
/// LLM is configured. Returns the runner handle (for graceful shutdown) when a
/// runner was spawned.
pub async fn build_app_full(
    config: Config,
    vault: Vault,
    reranker: Option<Arc<dyn mnemos_core::providers::Reranker>>,
    llm: Option<Arc<dyn mnemos_core::providers::LlmProvider>>,
) -> Result<(
    axum::Router,
    AppState,
    Option<crate::pipeline_runner::PipelineHandle>,
)> {
    let token_path = config_token_path()?;
    let token = auth::ensure_token(&token_path)?;
    let state = AppState {
        config: Arc::new(config),
        vault,
        token,
        events: events::EventBus::new(),
        reranker,
        llm,
        pipeline_status: pipeline_status::PipelineStatus::new(),
    };
    let app = routes::build_router(state.clone());
    let handle = if state.llm.is_some() {
        Some(pipeline_runner::spawn(state.clone()))
    } else {
        None
    };
    Ok((app, state, handle))
}
```

Add `pub use pipeline_status::PipelineStatus;` is optional — tests reach it via `state.pipeline_status`.

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test serve`
Expected: PASS. Also `cargo build -p mnemos_daemon` to confirm the whole crate compiles with the new AppState fields (existing handlers construct `AppState` only via `build_app_full`, so no other literal needs updating).

- [ ] **Step 9: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/events.rs crates/mnemos_daemon/src/pipeline_status.rs crates/mnemos_daemon/src/state.rs crates/mnemos_daemon/src/llm.rs crates/mnemos_daemon/src/pipeline_runner.rs crates/mnemos_daemon/src/lib.rs crates/mnemos_daemon/tests/serve.rs
git commit -m "feat: daemon LLM/pipeline state + build_app_full (Plan 4 Task 12)"
```

---

## Task 13: `PipelineRunner` loop

Replace the Task 12 placeholder with the real runner: subscribe to the event bus, run the full pipeline on `SessionEnded`, mark the session processed, update status, and publish `PipelineCompleted`/`PipelineFailed`.

**Files:**
- Replace: `crates/mnemos_daemon/src/pipeline_runner.rs`
- Test: `crates/mnemos_daemon/tests/pipeline_runner.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_daemon/tests/pipeline_runner.rs`:

```rust
use mnemos_core::paths::Paths;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::vault::Vault;
use mnemos_core::Tier;
use mnemos_daemon::config::Config;
use mnemos_daemon::events::Event;
use mnemos_daemon::build_app_full;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn runner_turns_session_end_into_semantic_memory() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (_app, state, handle) =
        build_app_full(Config::default(), vault, None, Some(Arc::new(MockLlm::new())))
            .await
            .unwrap();
    let handle = handle.expect("runner spawned when llm present");
    let mut rx = state.events.subscribe();

    {
        let (conn, _g) = state.vault.storage().write_conn().await.unwrap();
        conn.execute(
            "INSERT INTO sessions (id, started_at) VALUES ('sess_p', '2026-01-01T00:00:00+00:00')",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO chunks (id, session_id, speaker, ordinal, body, created_at)
                 VALUES ('chunk_p', 'sess_p', 'user', 0, 'FACT: Shaun ships Rust', '2026-01-01T00:00:00+00:00')",
            (),
        )
        .await
        .unwrap();
    }

    state.events.publish(Event::SessionEnded { id: "sess_p".into() });

    let facts_added = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match rx.recv().await {
                Ok(Event::PipelineCompleted { session_id, facts_added }) if session_id == "sess_p" => {
                    return facts_added
                }
                Ok(_) => continue,
                Err(_) => continue,
            }
        }
    })
    .await
    .expect("pipeline completes within 5s");
    assert!(facts_added >= 1);

    let mems = state
        .vault
        .list(ListFilter {
            tiers: Some(vec![Tier::Semantic]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(mems.iter().any(|m| m.body == "Shaun ships Rust"));

    // SessionEnded is idempotent: processed_at is stamped.
    let conn = state.vault.storage().conn().unwrap();
    let mut rows = conn
        .query("SELECT processed_at FROM sessions WHERE id = 'sess_p'", ())
        .await
        .unwrap();
    let pa: Option<String> = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert!(pa.is_some());

    handle.shutdown().await;
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test pipeline_runner`
Expected: FAIL — runner is the Task 12 placeholder (idles; never processes the event), so the timeout fires.

- [ ] **Step 3: Replace `crates/mnemos_daemon/src/pipeline_runner.rs`**

```rust
//! Background pipeline runner: subscribes to `SessionEnded` and turns a
//! session's chunks into durable memories + graph edges.

use crate::events::Event;
use crate::pipeline_status::RecentRun;
use crate::state::AppState;
use chrono::{DateTime, Utc};
use libsql::params;
use mnemos_core::pipeline::entities::link_entities;
use mnemos_core::pipeline::extract::extract_facts;
use mnemos_core::pipeline::graph::update_graph;
use mnemos_core::pipeline::resolve::resolve_and_apply;
use mnemos_core::pipeline::ResolveOp;
use mnemos_core::providers::LlmProvider;
use mnemos_core::types::{Chunk, Provenance};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;

/// Handle to the background pipeline runner; `shutdown` stops it and joins.
pub struct PipelineHandle {
    pub(crate) join: tokio::task::JoinHandle<()>,
    pub(crate) shutdown: watch::Sender<bool>,
}

impl PipelineHandle {
    /// Signal the runner to stop and await its completion.
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(true);
        let _ = self.join.await;
    }
}

/// Spawn the runner. It processes `SessionEnded` events until told to stop.
pub fn spawn(state: AppState) -> PipelineHandle {
    let (tx, mut rx) = watch::channel(false);
    let join = tokio::spawn(async move {
        let mut events = state.events.subscribe();
        loop {
            tokio::select! {
                _ = rx.changed() => {
                    if *rx.borrow() { break; }
                }
                ev = events.recv() => match ev {
                    Ok(Event::SessionEnded { id }) => process_session(&state, &id).await,
                    Ok(_) => {}
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "pipeline runner lagged; some events dropped");
                    }
                    Err(RecvError::Closed) => break,
                },
            }
        }
    });
    PipelineHandle { join, shutdown: tx }
}

async fn process_session(state: &AppState, session_id: &str) {
    let Some(llm) = state.llm.clone() else {
        return;
    };
    match run_pipeline(state, session_id, llm.as_ref()).await {
        Ok(n) => {
            state
                .pipeline_status
                .record(RecentRun {
                    session_id: session_id.to_string(),
                    facts_added: n,
                    ok: true,
                    at: Utc::now().to_rfc3339(),
                })
                .await;
            state.events.publish(Event::PipelineCompleted {
                session_id: session_id.to_string(),
                facts_added: n,
            });
        }
        Err(e) => {
            tracing::error!(session_id = %session_id, error = %e, "pipeline failed");
            state
                .pipeline_status
                .record(RecentRun {
                    session_id: session_id.to_string(),
                    facts_added: 0,
                    ok: false,
                    at: Utc::now().to_rfc3339(),
                })
                .await;
            state.events.publish(Event::PipelineFailed {
                session_id: session_id.to_string(),
                error: e.to_string(),
            });
        }
    }
}

async fn run_pipeline(
    state: &AppState,
    session_id: &str,
    llm: &dyn LlmProvider,
) -> anyhow::Result<usize> {
    if is_processed(state, session_id).await? {
        return Ok(0);
    }
    let chunks = load_chunks(state, session_id).await?;
    if chunks.is_empty() {
        mark_processed(state, session_id).await?;
        return Ok(0);
    }
    let chunk_ids: Vec<String> = chunks.iter().map(|c| c.id.clone()).collect();
    let facts = extract_facts(&chunks, llm).await?;
    let prov = Provenance {
        session: Some(session_id.to_string()),
        chunks: chunk_ids,
    };
    let mut added = 0usize;
    for fact in &facts {
        let (op, new_id) = resolve_and_apply(&state.vault, fact, prov.clone(), llm).await?;
        if let Some(mid) = new_id {
            if matches!(op, ResolveOp::Add | ResolveOp::Update { .. }) {
                added += 1;
            }
            if let Ok(mem) = state.vault.get(&mid).await {
                state.events.publish(Event::MemoryCreated {
                    id: mid.clone(),
                    title: mem.title.clone(),
                    tier: mem.tier.as_str().to_string(),
                });
                if let Err(e) = link_entities(state.vault.storage(), &mid, &mem.body, llm).await {
                    tracing::warn!(memory_id = %mid, error = %e, "entity linking failed");
                }
                if let Err(e) =
                    update_graph(state.vault.storage(), &mid, &mem.body, mem.valid_at, llm).await
                {
                    tracing::warn!(memory_id = %mid, error = %e, "graph update failed");
                }
            }
        }
    }
    mark_processed(state, session_id).await?;
    Ok(added)
}

async fn is_processed(state: &AppState, session_id: &str) -> anyhow::Result<bool> {
    let conn = state.vault.storage().conn()?;
    let mut rows = conn
        .query(
            "SELECT processed_at FROM sessions WHERE id = ?",
            params![session_id.to_string()],
        )
        .await?;
    match rows.next().await? {
        Some(r) => Ok(r.get::<Option<String>>(0)?.is_some()),
        None => Ok(true), // unknown session — nothing to do
    }
}

async fn mark_processed(state: &AppState, session_id: &str) -> anyhow::Result<()> {
    let (conn, _g) = state.vault.storage().write_conn().await?;
    conn.execute(
        "UPDATE sessions SET processed_at = ? WHERE id = ?",
        params![Utc::now().to_rfc3339(), session_id.to_string()],
    )
    .await?;
    Ok(())
}

async fn load_chunks(state: &AppState, session_id: &str) -> anyhow::Result<Vec<Chunk>> {
    let conn = state.vault.storage().conn()?;
    let mut rows = conn
        .query(
            "SELECT id, session_id, speaker, ordinal, body, created_at, source_tool, source_meta
               FROM chunks WHERE session_id = ? ORDER BY ordinal ASC",
            params![session_id.to_string()],
        )
        .await?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().await? {
        let source_meta_raw: Option<String> = r.get(7)?;
        let created: String = r.get(5)?;
        out.push(Chunk {
            id: r.get(0)?,
            session_id: r.get(1)?,
            speaker: r.get(2)?,
            ordinal: r.get::<i64>(3)? as u32,
            body: r.get(4)?,
            created_at: DateTime::parse_from_rfc3339(&created)?.with_timezone(&Utc),
            source_tool: r.get(6)?,
            source_meta: source_meta_raw
                .map(|s| serde_json::from_str(&s))
                .transpose()?,
        });
    }
    Ok(out)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test pipeline_runner`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/pipeline_runner.rs crates/mnemos_daemon/tests/pipeline_runner.rs
git commit -m "feat: pipeline runner processes SessionEnded into memories (Plan 4 Task 13)"
```

---

## Task 14: Wire the runner into `main` + graceful shutdown join

Builds the LLM, uses `build_app_full`, and joins the runner on shutdown (closes the Plan 3 graceful-shutdown carry-forward). This is a binary entrypoint, so verification is build + manual signal test, not a unit test.

**Files:**
- Modify: `crates/mnemos_daemon/src/main.rs`

- [ ] **Step 1: Swap the import**

Change:

```rust
use mnemos_daemon::build_app_with_reranker;
```

to:

```rust
use mnemos_daemon::build_app_full;
```

- [ ] **Step 2: Build the LLM and use `build_app_full` in `serve_cmd`**

Replace the body of `serve_cmd` from the embedder line through the `build_app_with_reranker` call and serve block with:

```rust
    let paths = Paths::with_root(&cfg.vault.root);
    let embedder = build_embedder_for_daemon(&cfg)?;
    let reranker = build_reranker_for_daemon(&cfg)?;
    let llm = mnemos_daemon::llm::build_llm_for_daemon(&cfg);
    let vault = Vault::open_with_embedder(paths, embedder)
        .await
        .context("opening vault")?;
    let bind = format!("{}:{}", cfg.daemon.host, cfg.daemon.port);
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("bind {bind}"))?;
    tracing::info!(addr = %listener.local_addr()?, "mnemosd listening");

    let pid_path = mnemos_daemon::pid_path()?;
    let _pid = mnemos_daemon::pid::PidFile::acquire(&pid_path)
        .with_context(|| format!("acquire PID file {}", pid_path.display()))?;
    tracing::info!(pid_file = %pid_path.display(), pid = std::process::id(), "PID file acquired");

    let (app, _state, pipeline) = build_app_full(cfg, vault, reranker, llm).await?;

    let shutdown = async {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");
            let mut int = signal(SignalKind::interrupt()).expect("install SIGINT handler");
            tokio::select! {
                _ = term.recv() => {},
                _ = int.recv() => {},
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
    };

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown)
        .await?;

    // Graceful shutdown: stop the background pipeline runner and join it before
    // the PID file (`_pid`) is dropped.
    if let Some(handle) = pipeline {
        tracing::info!("stopping pipeline runner");
        handle.shutdown().await;
    }
    Ok(())
```

- [ ] **Step 3: Build to verify it compiles**

Run: `cargo build -p mnemos_daemon`
Expected: builds clean. (`build_app_with_reranker` is still exported for tests; main no longer uses it.)

- [ ] **Step 4: Manual verification**

```bash
MNEMOS_LLM=mock MNEMOS_EMBEDDER=mock cargo run -p mnemos_daemon -- serve &
sleep 1
kill -TERM %1
wait
```
Expected: log shows `stopping pipeline runner` then clean exit; PID file removed.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/main.rs
git commit -m "feat: wire pipeline runner into daemon with graceful shutdown join (Plan 4 Task 14)"
```

---

## Task 15: `GET /v1/pipelines` status endpoint

Expose pipeline observability (counters + recent runs + configured model) over REST. The Tauri UI (Plan 6) and `curl` consume this.

**Files:**
- Create: `crates/mnemos_daemon/src/routes/pipelines.rs`
- Modify: `crates/mnemos_daemon/src/routes/mod.rs` (declare + mount)
- Test: `crates/mnemos_daemon/tests/pipelines.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_daemon/tests/pipelines.rs`:

```rust
use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

#[tokio::test]
async fn pipelines_status_returns_disabled_without_llm() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();

    let (s, b) = call(app, "GET", "/v1/pipelines", Some(&state.token), "").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["enabled"], false);
    assert_eq!(v["counters"]["completed"], 0);
    assert!(v["recent"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn pipelines_status_requires_auth() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (app, _state) = build_app(Config::default(), vault).await.unwrap();
    let (s, _) = call(app, "GET", "/v1/pipelines", None, "").await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

async fn call(
    app: axum::Router,
    method: &str,
    uri: &str,
    auth: Option<&str>,
    body: &str,
) -> (StatusCode, String) {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = auth {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let req = req.body(Body::from(body.to_string())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let s = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (s, String::from_utf8_lossy(&bytes).to_string())
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test pipelines`
Expected: FAIL — route returns 404 (not mounted).

- [ ] **Step 3: Create `crates/mnemos_daemon/src/routes/pipelines.rs`**

```rust
//! `GET /v1/pipelines` — pipeline status (counters, recent runs, configured model).

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/pipelines", get(status))
}

async fn status(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let (counters, recent) = state.pipeline_status.snapshot().await;
    let model = state.llm.as_ref().map(|l| l.model_id().to_string());
    Ok(Json(json!({
        "enabled": state.llm.is_some(),
        "llm_model": model,
        "counters": counters,
        "recent": recent,
    })))
}
```

- [ ] **Step 4: Mount it** in `routes/mod.rs` — add `pub mod pipelines;` to the module list and `.merge(pipelines::router())` to the `authed` router chain (alongside `memories::router()` etc.).

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test pipelines`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/pipelines.rs crates/mnemos_daemon/src/routes/mod.rs crates/mnemos_daemon/tests/pipelines.rs
git commit -m "feat: GET /v1/pipelines status endpoint (Plan 4 Task 15)"
```

---

## Task 16: Carry-forward — reject orphan chunks

Plan 3 left `add_chunk` inserting a chunk for any `session_id`, even a nonexistent one. Validate the session exists and return 404 otherwise.

**Files:**
- Modify: `crates/mnemos_daemon/src/routes/sessions.rs`
- Test: add to `crates/mnemos_daemon/tests/sessions.rs`

- [ ] **Step 1: Write the failing test** — append to `crates/mnemos_daemon/tests/sessions.rs` (the `fixture` and `call` helpers already exist in that file):

```rust
#[tokio::test]
async fn add_chunk_to_missing_session_is_404() {
    let (app, token) = fixture().await;
    let (s, b) = call(
        app,
        "POST",
        "/v1/sessions/sess_does_not_exist/chunks",
        Some(&token),
        r#"{"speaker":"user","body":"orphan"}"#,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{b}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test sessions add_chunk_to_missing_session`
Expected: FAIL — currently returns 201 CREATED for an orphan chunk.

- [ ] **Step 3: Add the existence check** in `add_chunk` (`sessions.rs`), immediately after destructuring args and before computing `chunk_id`:

```rust
    // Reject orphan chunks: the parent session must exist.
    {
        let conn = state.vault.storage().conn()?;
        let mut rows = conn
            .query(
                "SELECT 1 FROM sessions WHERE id = ?",
                params![session_id.clone()],
            )
            .await
            .map_err(mnemos_core::error::MnemosError::from)?;
        let exists = rows
            .next()
            .await
            .map_err(mnemos_core::error::MnemosError::from)?
            .is_some();
        if !exists {
            return Err(ApiError::not_found(format!("session {session_id}")));
        }
    }
```

(`ApiError` and `params` are already imported in `sessions.rs`. `state.vault.storage().conn()` returns a `mnemos_core::error::Result`; the `?` converts to `ApiError` via the existing `From<MnemosError>` impl.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test sessions`
Expected: PASS (both the existing lifecycle test and the new orphan test).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/sessions.rs crates/mnemos_daemon/tests/sessions.rs
git commit -m "fix: reject chunks for nonexistent sessions (Plan 4 Task 16, Plan 3 carry-forward)"
```

---

## Task 17: Carry-forward — dedupe recall logic

Plan 3 duplicated the embedder-ref + rerank-branch logic in both `routes/memories.rs::search` and `mcp/tools.rs::recall`. Extract a single shared helper.

**Files:**
- Create: `crates/mnemos_daemon/src/routes/recall_helper.rs`
- Modify: `crates/mnemos_daemon/src/routes/mod.rs` (declare module)
- Modify: `crates/mnemos_daemon/src/routes/memories.rs` (`search` uses helper)
- Modify: `crates/mnemos_daemon/src/mcp/tools.rs` (`recall` uses helper)

> This is a refactor: behavior is unchanged, so the "tests" are the existing `tests/memories.rs` (search) and `tests/mcp.rs` (recall) suites continuing to pass.

- [ ] **Step 1: Create `crates/mnemos_daemon/src/routes/recall_helper.rs`**

```rust
//! Shared recall path used by both the REST search endpoint and the MCP recall
//! tool, so the embedder-ref + rerank branching lives in exactly one place.

use mnemos_core::error::Result;
use mnemos_core::retrieval::hybrid::{hybrid_recall, hybrid_recall_with_rerank};
use mnemos_core::retrieval::{RecallHit, RecallOpts};

use crate::state::AppState;

/// Run hybrid recall, applying the cross-encoder reranker when requested and
/// configured. Returns ranked hits.
pub async fn recall(state: &AppState, query: &str, opts: RecallOpts) -> Result<Vec<RecallHit>> {
    let embedder = state.vault.embedder().cloned();
    let embedder_ref = embedder.as_ref().map(|a| a.as_ref());
    if opts.rerank && state.reranker.is_some() {
        let rr = state.reranker.clone().unwrap();
        hybrid_recall_with_rerank(state.vault.storage(), embedder_ref, Some(rr.as_ref()), query, opts).await
    } else {
        hybrid_recall(state.vault.storage(), embedder_ref, query, opts).await
    }
}
```

- [ ] **Step 2: Declare the module** in `routes/mod.rs`:

```rust
pub mod recall_helper;
```

- [ ] **Step 3: Replace the body of `search` in `memories.rs`** (the block from `let embedder = ...` through the `hits` assignment) with:

```rust
    let hits = crate::routes::recall_helper::recall(&state, &req.query, opts).await?;
```

Then fix the import line at the top of `memories.rs`:

```rust
use mnemos_core::retrieval::RecallOpts;
```

(drop `hybrid::hybrid_recall` from that `use`; the helper owns it now).

- [ ] **Step 4: Replace the body of `recall` in `mcp/tools.rs`** (the block from `let embedder = ...` through the `hits` assignment) with:

```rust
    let hits = crate::routes::recall_helper::recall(state, query, opts).await?;
```

Then fix the import line at the top of `tools.rs`:

```rust
use mnemos_core::retrieval::RecallOpts;
```

(drop `hybrid::hybrid_recall`). The `?` works because `recall` returns `mnemos_core::error::Result`, and the function is `anyhow::Result<Value>` (anyhow converts `MnemosError` via its `Error` impl).

- [ ] **Step 5: Run the affected suites to verify behavior is unchanged**

Run: `cargo test -p mnemos_daemon --test memories && cargo test -p mnemos_daemon --test mcp`
Expected: PASS (no behavioral change).

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/recall_helper.rs crates/mnemos_daemon/src/routes/mod.rs crates/mnemos_daemon/src/routes/memories.rs crates/mnemos_daemon/src/mcp/tools.rs
git commit -m "refactor: extract shared recall helper for REST + MCP (Plan 4 Task 17, Plan 3 carry-forward)"
```

---

## Task 18: Implement `PATCH /v1/memories/{id}`

Replace the Plan 3 `501` stub with a real metadata patch backed by `Vault::patch` (Task 4).

**Files:**
- Modify: `crates/mnemos_daemon/src/routes/memories.rs`
- Test: add to `crates/mnemos_daemon/tests/memories.rs`

- [ ] **Step 1: Write the failing test** — append to `crates/mnemos_daemon/tests/memories.rs` (reuse the file's existing `fixture`/`call` helpers; adapt names if they differ):

```rust
#[tokio::test]
async fn patch_updates_tags_and_importance() {
    let (app, token) = fixture().await;
    let (s, b) = call(
        app.clone(),
        "POST",
        "/v1/memories",
        Some(&token),
        r#"{"body":"patch target","tier":"semantic"}"#,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let id = serde_json::from_str::<serde_json::Value>(&b).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (s2, b2) = call(
        app.clone(),
        "PATCH",
        &format!("/v1/memories/{id}"),
        Some(&token),
        r#"{"tags":["urgent","work"],"importance":0.95}"#,
    )
    .await;
    assert_eq!(s2, StatusCode::OK, "{b2}");
    let v: serde_json::Value = serde_json::from_str(&b2).unwrap();
    assert_eq!(v["tags"][0], "urgent");
    assert!((v["importance"].as_f64().unwrap() - 0.95).abs() < 1e-9);
}
```

> If `tests/memories.rs` does not already define `fixture`/`call`, copy the `call` helper from `tests/sessions.rs` and a `fixture()` that returns `(app, state.token)` from `build_app`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test memories patch_updates`
Expected: FAIL — handler returns `501 NOT_IMPLEMENTED`.

- [ ] **Step 3: Replace the `PatchMemoryReq` struct and `patch_memory` handler** in `memories.rs`:

```rust
#[derive(Debug, Deserialize)]
struct PatchMemoryReq {
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    importance: Option<f64>,
}

async fn patch_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<PatchMemoryReq>,
) -> Result<Json<mnemos_core::types::Memory>, ApiError> {
    let mem = state.vault.patch(&id, req.tags, req.importance).await?;
    state
        .events
        .publish(crate::events::Event::MemoryUpdated { id: id.clone() });
    Ok(Json(mem))
}
```

(Remove the old `#[allow(dead_code)]` attributes — the fields are used now.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test memories`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/memories.rs crates/mnemos_daemon/tests/memories.rs
git commit -m "feat: implement PATCH /v1/memories/{id} metadata patch (Plan 4 Task 18)"
```

---

## Task 19: Implement `POST /v1/memories/time-travel`

Replace the Plan 3 `501` stub with real time-travel recall backed by `recall_as_of` (Task 4).

**Files:**
- Modify: `crates/mnemos_daemon/src/routes/memories.rs`
- Test: add to `crates/mnemos_daemon/tests/memories.rs`

- [ ] **Step 1: Write the failing test** — append to `crates/mnemos_daemon/tests/memories.rs`:

```rust
#[tokio::test]
async fn time_travel_respects_as_of_window() {
    let (app, token) = fixture().await;
    let (s, b) = call(
        app.clone(),
        "POST",
        "/v1/memories",
        Some(&token),
        r#"{"body":"timetravel beacon alpha","tier":"semantic"}"#,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");

    let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
    let (s2, b2) = call(
        app.clone(),
        "POST",
        "/v1/memories/time-travel",
        Some(&token),
        &format!(r#"{{"query":"beacon","as_of":"{future}","k":10}}"#),
    )
    .await;
    assert_eq!(s2, StatusCode::OK, "{b2}");
    let v: serde_json::Value = serde_json::from_str(&b2).unwrap();
    assert!(!v["memories"].as_array().unwrap().is_empty());

    let past = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    let (s3, b3) = call(
        app,
        "POST",
        "/v1/memories/time-travel",
        Some(&token),
        &format!(r#"{{"query":"beacon","as_of":"{past}","k":10}}"#),
    )
    .await;
    assert_eq!(s3, StatusCode::OK);
    let v3: serde_json::Value = serde_json::from_str(&b3).unwrap();
    assert!(v3["memories"].as_array().unwrap().is_empty(), "not valid yet in the past");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test memories time_travel`
Expected: FAIL — handler returns `501 NOT_IMPLEMENTED`.

- [ ] **Step 3: Replace the `TimeTravelReq` struct and `time_travel` handler** in `memories.rs`:

```rust
#[derive(Debug, Deserialize)]
struct TimeTravelReq {
    query: String,
    as_of: String,
    #[serde(default = "default_k")]
    k: usize,
}

async fn time_travel(
    State(state): State<AppState>,
    Json(req): Json<TimeTravelReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let as_of = chrono::DateTime::parse_from_rfc3339(&req.as_of)
        .map(|d| d.with_timezone(&chrono::Utc))
        .map_err(|e| ApiError::bad_request(format!("invalid as_of timestamp: {e}")))?;
    let memories =
        mnemos_core::storage::memory_ops::recall_as_of(state.vault.storage(), &req.query, as_of, req.k)
            .await?;
    Ok(Json(serde_json::json!({ "as_of": req.as_of, "memories": memories })))
}
```

(Remove the old `#[allow(dead_code)]` attributes from the struct fields.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test memories`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/memories.rs crates/mnemos_daemon/tests/memories.rs
git commit -m "feat: implement time-travel recall endpoint (Plan 4 Task 19)"
```

---

## Task 20: Hourly decay worker + `POST /v1/maintenance/decay`

The daemon runs a decay pass every hour; a REST endpoint triggers one on demand.

**Files:**
- Modify: `crates/mnemos_daemon/src/routes/pipelines.rs` (add the maintenance route)
- Modify: `crates/mnemos_daemon/src/main.rs` (spawn the hourly worker, join on shutdown)
- Test: add to `crates/mnemos_daemon/tests/pipelines.rs`

- [ ] **Step 1: Write the failing test** — append to `crates/mnemos_daemon/tests/pipelines.rs`:

```rust
#[tokio::test]
async fn manual_decay_endpoint_returns_stats() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();

    let (s, b) = call(app, "POST", "/v1/maintenance/decay", Some(&state.token), "{}").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["scanned"], 0);
    assert_eq!(v["invalidated"], 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test pipelines manual_decay`
Expected: FAIL — route not mounted (404).

- [ ] **Step 3: Add the maintenance route** to `routes/pipelines.rs`. Update the imports and router, and add the handler:

```rust
use axum::routing::{get, post};
use mnemos_core::pipeline::decay::DecayConfig;
```

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/pipelines", get(status))
        .route("/v1/maintenance/decay", post(run_decay))
}
```

```rust
async fn run_decay(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let stats = state.vault.run_decay(&DecayConfig::default()).await?;
    Ok(Json(json!({
        "scanned": stats.scanned,
        "decayed": stats.decayed,
        "invalidated": stats.to_invalidate.len(),
        "invalidated_ids": stats.to_invalidate,
    })))
}
```

- [ ] **Step 4: Spawn the hourly worker in `main.rs`**. In `serve_cmd`, clone the vault BEFORE it is moved into `build_app_full`, then spawn the worker and join it on shutdown. Update the relevant lines:

```rust
    let decay_vault = vault.clone();
    let (app, _state, pipeline) = build_app_full(cfg, vault, reranker, llm).await?;

    // Hourly decay worker.
    let (decay_tx, mut decay_rx) = tokio::sync::watch::channel(false);
    let decay_handle = tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
        tick.tick().await; // consume the immediate first tick
        loop {
            tokio::select! {
                _ = decay_rx.changed() => {
                    if *decay_rx.borrow() { break; }
                }
                _ = tick.tick() => {
                    match decay_vault
                        .run_decay(&mnemos_core::pipeline::decay::DecayConfig::default())
                        .await
                    {
                        Ok(s) => tracing::info!(
                            scanned = s.scanned,
                            decayed = s.decayed,
                            invalidated = s.to_invalidate.len(),
                            "decay pass complete"
                        ),
                        Err(e) => tracing::warn!(error = %e, "decay pass failed"),
                    }
                }
            }
        }
    });
```

Then, after `axum::serve(...).with_graceful_shutdown(shutdown).await?;`, stop the worker before stopping the pipeline runner:

```rust
    let _ = decay_tx.send(true);
    let _ = decay_handle.await;
    if let Some(handle) = pipeline {
        tracing::info!("stopping pipeline runner");
        handle.shutdown().await;
    }
    Ok(())
```

- [ ] **Step 5: Run tests + build to verify**

Run: `cargo test -p mnemos_daemon --test pipelines && cargo build -p mnemos_daemon`
Expected: PASS + clean build.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/pipelines.rs crates/mnemos_daemon/src/main.rs crates/mnemos_daemon/tests/pipelines.rs
git commit -m "feat: hourly decay worker + POST /v1/maintenance/decay (Plan 4 Task 20)"
```

---

## Task 21: CLI `mnemos decay`

A local manual decay trigger that opens the vault directly (consistent with `rebuild`/`embed`).

**Files:**
- Modify: `crates/mnemos_cli/src/cli.rs` (add `Decay` subcommand)
- Modify: `crates/mnemos_cli/src/commands/mod.rs` (declare module)
- Create: `crates/mnemos_cli/src/commands/decay.rs`
- Modify: `crates/mnemos_cli/src/main.rs` (dispatch)

> Note: `mnemos decay` opens the vault file directly. If the daemon is also running it shares the same SQLite file — the same cross-process condition that already applies to `mnemos rebuild`/`remember`. The daemon's hourly worker is the primary mechanism; this command is a manual/ops convenience.

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_cli/src/commands/decay.rs` with the implementation AND a smoke test:

```rust
use crate::commands::open_vault;
use anyhow::Result;
use mnemos_core::pipeline::decay::DecayConfig;
use std::path::PathBuf;

pub async fn run(vault: Option<PathBuf>, json: bool) -> Result<()> {
    let vault = open_vault(vault).await?;
    let stats = vault.run_decay(&DecayConfig::default()).await?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "scanned": stats.scanned,
                "decayed": stats.decayed,
                "invalidated": stats.to_invalidate.len(),
            })
        );
    } else {
        println!(
            "decay pass — scanned: {}  decayed: {}  invalidated: {}",
            stats.scanned,
            stats.decayed,
            stats.to_invalidate.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn decay_runs_on_empty_vault() {
        std::env::set_var("MNEMOS_EMBEDDER", "none");
        let tmp = TempDir::new().unwrap();
        run(Some(tmp.path().to_path_buf()), true).await.unwrap();
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_cli commands::decay`
Expected: FAIL — `commands::decay` module not declared.

- [ ] **Step 3: Declare the module** in `crates/mnemos_cli/src/commands/mod.rs`:

```rust
pub mod decay;
```

- [ ] **Step 4: Add the subcommand** to `crates/mnemos_cli/src/cli.rs` `Cmd` enum:

```rust
    /// Run a memory decay pass now (Ebbinghaus strength decay).
    Decay,
```

- [ ] **Step 5: Dispatch it** in `crates/mnemos_cli/src/main.rs` `match args.command`:

```rust
        Cmd::Decay => commands::decay::run(args.vault, args.json).await,
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p mnemos_cli commands::decay && cargo build -p mnemos_cli`
Expected: PASS + clean build.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_cli --all-targets -- -D warnings
git add crates/mnemos_cli/src/cli.rs crates/mnemos_cli/src/commands/mod.rs crates/mnemos_cli/src/commands/decay.rs crates/mnemos_cli/src/main.rs
git commit -m "feat: add 'mnemos decay' CLI command (Plan 4 Task 21)"
```

---

## Task 22: End-to-end HTTP integration test

Prove the whole loop over HTTP: start session → add a `FACT:` chunk → end session → runner extracts → memory becomes searchable → status reflects the run. Uses `MockLlm` + `MockEmbedder`, fully deterministic.

**Files:**
- Test: `crates/mnemos_daemon/tests/pipeline_e2e.rs` (new)

- [ ] **Step 1: Write the test** — create `crates/mnemos_daemon/tests/pipeline_e2e.rs`:

```rust
use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::providers::mock::MockEmbedder;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app_full, config::Config, events::Event};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn session_end_produces_searchable_memory_over_http() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open_with_embedder(
        Paths::with_root(tmp.path()),
        Some(Arc::new(MockEmbedder::new(768))),
    )
    .await
    .unwrap();
    let (app, state, handle) =
        build_app_full(Config::default(), vault, None, Some(Arc::new(MockLlm::new())))
            .await
            .unwrap();
    let handle = handle.expect("runner present");
    let token = state.token.clone();
    let mut rx = state.events.subscribe();

    let (s, b) = call(
        app.clone(),
        "POST",
        "/v1/sessions",
        Some(&token),
        r#"{"source_tool":"claude-code"}"#,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let sid = serde_json::from_str::<serde_json::Value>(&b).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (s2, _) = call(
        app.clone(),
        "POST",
        &format!("/v1/sessions/{sid}/chunks"),
        Some(&token),
        r#"{"speaker":"user","body":"FACT: Shaun loves Rust"}"#,
    )
    .await;
    assert_eq!(s2, StatusCode::CREATED);

    let (s3, _) = call(
        app.clone(),
        "POST",
        &format!("/v1/sessions/{sid}/end"),
        Some(&token),
        r#"{}"#,
    )
    .await;
    assert_eq!(s3, StatusCode::OK);

    let sid2 = sid.clone();
    let added = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match rx.recv().await {
                Ok(Event::PipelineCompleted { session_id, facts_added }) if session_id == sid2 => {
                    return facts_added
                }
                _ => continue,
            }
        }
    })
    .await
    .expect("pipeline completes within 5s");
    assert!(added >= 1);

    let (s4, b4) = call(
        app.clone(),
        "POST",
        "/v1/memories/search",
        Some(&token),
        r#"{"query":"Rust","k":10}"#,
    )
    .await;
    assert_eq!(s4, StatusCode::OK, "{b4}");
    assert!(b4.contains("Shaun loves Rust"), "memory should be searchable: {b4}");

    let (s5, b5) = call(app, "GET", "/v1/pipelines", Some(&token), "").await;
    assert_eq!(s5, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&b5).unwrap();
    assert_eq!(v["enabled"], true);
    assert!(v["counters"]["completed"].as_u64().unwrap() >= 1);

    handle.shutdown().await;
}

async fn call(
    app: axum::Router,
    method: &str,
    uri: &str,
    auth: Option<&str>,
    body: &str,
) -> (StatusCode, String) {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = auth {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let req = req.body(Body::from(body.to_string())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let s = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (s, String::from_utf8_lossy(&bytes).to_string())
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p mnemos_daemon --test pipeline_e2e`
Expected: PASS.

- [ ] **Step 3: Run the full workspace suite** (regression check across all crates)

Run: `cargo test --workspace`
Expected: PASS (all crates green; Ollama-backed tests remain `#[ignore]`).

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/mnemos_daemon/tests/pipeline_e2e.rs
git commit -m "test: end-to-end session→memory pipeline over HTTP (Plan 4 Task 22)"
```

---

## Task 23: Release v0.3.0 — version bump, README, CHANGELOG, tag

**Files:**
- Modify: `Cargo.toml` (workspace `version = "0.3.0"`)
- Modify: `README.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Bump the workspace version** in `Cargo.toml`:

```toml
version = "0.3.0"
```

- [ ] **Step 2: Update `README.md`** — add a section documenting the learning pipeline. Insert after the existing daemon/MCP section:

```markdown
## Automatic learning pipeline (v0.3.0)

When a session ends, the daemon turns its conversation chunks into durable
memories automatically — no manual `remember` calls required:

1. **Extract** — atomic facts are pulled from the session transcript.
2. **Resolve** — each fact is ADDed, used to UPDATE (supersede) an existing
   memory, DELETE (invalidate) a contradicted one, or skipped as a NOOP.
3. **Entity-link** — named entities are upserted and linked to the memory.
4. **Graph-update** — relationship edges between entities are recorded.

A background worker also runs an hourly **Ebbinghaus decay** pass: unused
working/episodic memories lose strength and are eventually invalidated, while
important and semantic memories persist far longer.

### Configuring the LLM

```toml
[llm]
kind = "ollama"        # "ollama" | "mock" | "none"
url = "http://localhost:11434"
model = "llama3.2"
timeout_secs = 120
```

Env overrides: `MNEMOS_LLM`, `MNEMOS_LLM_URL`, `MNEMOS_LLM_MODEL`.
Set `kind = "none"` to disable automatic learning (manual `remember` still works).

### New endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `GET`  | `/v1/pipelines` | Pipeline status: counters, recent runs, configured model |
| `POST` | `/v1/maintenance/decay` | Trigger a decay pass now |
| `PATCH`| `/v1/memories/{id}` | Patch a memory's tags / importance |
| `POST` | `/v1/memories/time-travel` | Recall as of a past timestamp |

### CLI

```bash
mnemos decay        # run a decay pass locally
```
```

- [ ] **Step 3: Update `CHANGELOG.md`** — add at the top:

```markdown
## [0.3.0] - 2026-05-27

### Added
- `LlmProvider` trait with `OllamaLlm` (default) and deterministic `MockLlm` for CI.
- Async learning pipeline triggered on `SessionEnded`: extract → resolve
  (ADD/UPDATE/DELETE/NOOP) → entity-link → graph-update.
- Hourly Ebbinghaus decay worker + `POST /v1/maintenance/decay` + `mnemos decay`.
- `GET /v1/pipelines` status endpoint (counters, recent runs, configured model).
- `PATCH /v1/memories/{id}` (tags/importance) and `POST /v1/memories/time-travel`
  (replacing the Plan 3 `501` stubs).
- `[llm]` config section with `MNEMOS_LLM*` env overrides.
- Schema v4: `sessions.processed_at` for idempotent pipeline processing.

### Fixed
- Reject chunks posted to a nonexistent session (was silently creating orphans).
- Daemon graceful shutdown now joins the background pipeline + decay workers.

### Changed
- Extracted a shared recall helper used by both the REST search endpoint and the
  MCP recall tool (removing duplicated logic).

### Deferred
- MCP `sampling/createMessage` (extraction via the calling client's LLM): async
  pipelines run after the triggering request returns, so there is no
  request-scoped connection to sample from. Revisit with a streaming transport.
```

- [ ] **Step 4: Final verification**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all green.

- [ ] **Step 5: Commit and tag**

```bash
git add Cargo.toml README.md CHANGELOG.md
git commit -m "chore: release v0.3.0 — async learning pipelines (Plan 4 Task 23)"
git tag -a v0.3.0 -m "v0.3.0 — automatic learning pipelines, decay, time-travel"
```

(Do not push the tag until the user confirms — pushing is a shared-state action.)

---

## Done

After all tasks: the daemon, given an LLM, turns ended sessions into durable, deduplicated, entity-linked semantic memories with provenance back to the source chunks, decays unused memories over time, and exposes pipeline status + time-travel + metadata patching over REST. All of it is deterministic in CI via `MockLlm` + `MockEmbedder`.

**Next:** Plan 5 (HippoRAG Personalized PageRank over the entity graph this plan builds + importance-triggered reflection + community detection).
