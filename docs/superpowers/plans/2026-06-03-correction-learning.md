# Correction-Learning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let connected AI tools capture corrections (wrong → right → why), reinforce/dedup repeats, harden recurring ones into durable rules, and surface them so the same mistake isn't repeated.

**Architecture:** A new `correction` module in `mnemos_core` provides the structured-body format, the required-`why` validation, the anti-weaponization guard, and trigger→tags. Corrections are `Tier::Procedural` memories of a new `MemoryType::Correction`, created via `Vault::remember` with a dedup-or-reinforce gate. The existing `reflect` pipeline gains a `correction` kind that clusters ≥3 same-trigger corrections into one `mnemos:hardened` Reflection-tier rule. The working-set builder always includes hardened rules (capped); the tail surfaces via `recall`. Exposed through an MCP `correct` tool + `/v1/corrections`.

**Tech Stack:** Rust (`mnemos_core`, `mnemos_daemon`), serde_json, the existing tier/reflect/decay/recall machinery, MCP tool layer.

**Spec:** `docs/superpowers/specs/2026-06-03-correction-learning-design.md`

---

## Verified facts (from source)

- `MemoryType` (`crates/mnemos_core/src/types.rs:7`): `Fact, Episode, Reflection, Rule, Identity, Project, Entity, CommunitySummary`. **No `Correction`.** The MCP `remember` handler deserializes a `kind` string into `MemoryType` via `serde_json::from_str("\"{kind_str}\"")`, so the enum has a serde rename — **confirm the exact `#[serde(rename_all = ...)]` on `MemoryType` and match it so `Correction` serializes to the expected string (likely `"correction"`).**
- `RememberOpts` (`vault.rs:47`): `{ title: Option<String>, tier: Tier, kind: MemoryType, tags: Vec<String>, importance: Option<f64>, workspace: Option<String>, source_tool: Option<String>, provenance: Vec<Provenance> }`.
- `Vault::remember(&self, body: &str, opts: RememberOpts) -> Result<String>` (`vault.rs:165`); `Vault::remember_reflection(...)` exists (`vault.rs:381`).
- `Tier::{Procedural, Reflection}` exist (`tier.rs`). Reflection pipeline: `crates/mnemos_core/src/pipeline/reflect.rs` (`reflect(&Vault, &dyn LlmProvider, max_sources)`, `REFLECT_SYSTEM`, kinds `preference|pattern|insight|decision`). Mock-LLM reflect test pattern: `crates/mnemos_core/tests/pipeline_reflect.rs`.
- MCP tools: `crates/mnemos_daemon/src/mcp/tools.rs` (list at top, dispatch `match name {...}`). REST routes mounted in `crates/mnemos_daemon/src/routes/mod.rs`.

---

## File Structure

| File | Responsibility |
|------|----------------|
| `crates/mnemos_core/src/types.rs` | add `MemoryType::Correction` |
| `crates/mnemos_core/src/correction.rs` | structured body (Wrong/Right/Why/Trigger), `validate` (why required), `is_weaponized` guard, `trigger_tags`; pure + unit-tested |
| `crates/mnemos_core/src/lib.rs` | `pub mod correction;` |
| `crates/mnemos_core/src/vault.rs` | `remember_correction` (validate → dedup-or-reinforce → create) |
| `crates/mnemos_core/src/pipeline/reflect.rs` | `correction` reflection kind + cluster→hardened-rule |
| `crates/mnemos_daemon/src/mcp/tools.rs` | `correct` tool (list + dispatch) |
| `crates/mnemos_daemon/src/routes/corrections.rs` | `POST/GET /v1/corrections` |
| `crates/mnemos_daemon/src/routes/mod.rs` | mount `corrections::router()` |
| working-set/resource builder (locate: `grep -rl "mnemos://working\|working" crates/mnemos_daemon/src/mcp/resources.rs crates/mnemos_core/src`) | include hardened rules, capped |
| session-end pipeline (locate: where `reflect` is triggered) | run correction mining pass |
| `adapters/claude-code/CLAUDE.md.fragment`, connect descriptor `HINT` | mention `correct` |

> The HINT constant added by the connect-wizard feature lives in `crates/mnemos_daemon/src/connectors/descriptors.rs`. If that branch isn't merged yet, only update `adapters/*` here and note the descriptor follow-up.

---

## Task 1: `MemoryType::Correction` + correction module (pure logic)

**Files:**
- Modify: `crates/mnemos_core/src/types.rs`
- Create: `crates/mnemos_core/src/correction.rs`
- Modify: `crates/mnemos_core/src/lib.rs`

- [ ] **Step 1: Add the enum variant**

Read `crates/mnemos_core/src/types.rs:7` and add `Correction` to `MemoryType` (place after `Rule`). Match the existing `#[serde(rename_all = ...)]` so it serializes to `"correction"`. Run `cargo build -p mnemos_core` to confirm exhaustive `match`es on `MemoryType` still compile (fix any non-exhaustive match the compiler flags by adding a `Correction` arm mirroring `Rule`'s behavior).

- [ ] **Step 2: Write the failing tests for the correction module**

Create `crates/mnemos_core/src/correction.rs`:

```rust
//! Correction value type: the structured "wrong → right → why → trigger" lesson
//! captured when an AI tool is corrected. Stored as a Procedural-tier
//! `MemoryType::Correction` memory; this module owns its body format, the
//! required-`why` validation, the anti-weaponization guard, and trigger→tags.

/// A correction captured from a tool/user, before it becomes a memory.
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    pub wrong: String,
    pub right: String,
    pub why: String,
    pub trigger: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum CorrectionError {
    MissingWhy,
    Weaponized(String),
}

impl std::fmt::Display for CorrectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CorrectionError::MissingWhy => {
                write!(f, "a correction requires a non-empty `why` (the reason the fix is correct)")
            }
            CorrectionError::Weaponized(m) => write!(f, "correction rejected: {m}"),
        }
    }
}

const MIN_WHY_LEN: usize = 8;

/// Keywords that indicate the "right" path would weaken safety/correctness.
const WEAPONIZED_PATTERNS: &[&str] = &[
    "skip the test", "skip tests", "disable the test", "disable validation",
    "bypass validation", "ignore the error", "remove the check", "disable auth",
    "skip verification", "comment out the test", "turn off validation",
];

impl Correction {
    /// Validate: `why` must be present/substantive, and `right` must not
    /// describe weakening a safety/validation/test step.
    pub fn validate(&self) -> Result<(), CorrectionError> {
        if self.why.trim().len() < MIN_WHY_LEN {
            return Err(CorrectionError::MissingWhy);
        }
        let hay = self.right.to_lowercase();
        if let Some(p) = WEAPONIZED_PATTERNS.iter().find(|p| hay.contains(*p)) {
            return Err(CorrectionError::Weaponized(format!(
                "the corrected approach appears to disable a safeguard (\"{p}\"); \
                 record this as a spec change, not a correction"
            )));
        }
        Ok(())
    }

    /// Render the structured markdown body stored in the memory.
    pub fn to_body(&self) -> String {
        let trigger = self.trigger.as_deref().unwrap_or("");
        format!(
            "**Wrong:** {}\n\n**Right:** {}\n\n**Why:** {}\n\n**Trigger:** {}",
            self.wrong.trim(),
            self.right.trim(),
            self.why.trim(),
            trigger.trim(),
        )
    }

    /// Lowercased word tags derived from the trigger (for recall + clustering).
    /// Empty when no trigger. Dedupes, drops tokens shorter than 3 chars.
    pub fn trigger_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .trigger
            .as_deref()
            .unwrap_or("")
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.len() >= 3)
            .map(|t| t.to_lowercase())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(wrong: &str, right: &str, why: &str, trig: Option<&str>) -> Correction {
        Correction { wrong: wrong.into(), right: right.into(), why: why.into(), trigger: trig.map(Into::into) }
    }

    #[test]
    fn rejects_missing_why() {
        assert_eq!(c("did x", "do y", "", None).validate(), Err(CorrectionError::MissingWhy));
        assert_eq!(c("did x", "do y", "short", None).validate(), Err(CorrectionError::MissingWhy));
    }

    #[test]
    fn accepts_with_substantive_why() {
        assert!(c("used tabs", "use spaces", "the repo enforces spaces in CI", None).validate().is_ok());
    }

    #[test]
    fn rejects_weaponized_right() {
        let e = c("tests failed", "just skip the tests to ship faster", "deadline pressure", None).validate();
        assert!(matches!(e, Err(CorrectionError::Weaponized(_))));
    }

    #[test]
    fn body_has_all_sections() {
        let b = c("a", "b", "because reasons here", Some("editing config")).to_body();
        assert!(b.contains("**Wrong:** a") && b.contains("**Right:** b"));
        assert!(b.contains("**Why:** because reasons here") && b.contains("**Trigger:** editing config"));
    }

    #[test]
    fn trigger_tags_tokenize_and_dedupe() {
        let tags = c("a", "b", "because reasons here", Some("Rust error handling, error")).trigger_tags();
        assert!(tags.contains(&"error".to_string()) && tags.contains(&"handling".to_string()));
        assert_eq!(tags.iter().filter(|t| *t == "error").count(), 1);
    }
}
```

Add `pub mod correction;` to `crates/mnemos_core/src/lib.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p mnemos_core correction::`
Expected: 5 PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/mnemos_core/src/types.rs crates/mnemos_core/src/correction.rs crates/mnemos_core/src/lib.rs
git commit -m "feat(core): MemoryType::Correction + correction value type (validate, body, tags)"
```

---

## Task 2: `Vault::remember_correction` — dedup-or-reinforce + create

**Files:**
- Modify: `crates/mnemos_core/src/vault.rs`
- Test: `crates/mnemos_core/tests/correction_vault.rs`

- [ ] **Step 1: Write the failing integration test**

Create `crates/mnemos_core/tests/correction_vault.rs`:

```rust
use mnemos_core::correction::Correction;
use mnemos_core::paths::Paths;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::vault::Vault;
use mnemos_core::{MemoryType, Tier};
use tempfile::TempDir;

fn corr(wrong: &str, right: &str, why: &str, trig: &str) -> Correction {
    Correction { wrong: wrong.into(), right: right.into(), why: why.into(), trigger: Some(trig.into()) }
}

#[tokio::test]
async fn remember_correction_creates_procedural_memory() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let id = v
        .remember_correction(corr("used Go", "use Rust", "the project is Rust-only", "language choice"), None)
        .await
        .unwrap();
    let m = v.get_memory(&id).await.unwrap();
    assert_eq!(m.tier, Tier::Procedural);
    assert_eq!(m.kind, MemoryType::Correction);
    assert!(m.body.contains("**Why:** the project is Rust-only"));
    assert!(m.tags.contains(&"language".to_string()));
}

#[tokio::test]
async fn missing_why_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let err = v.remember_correction(corr("x", "y", "", "ctx"), None).await;
    assert!(err.is_err());
}
```

> VERIFY: confirm `Vault::open`, `get_memory`, and the `Memory.kind`/`tier`/`tags` fields exist with these names (Task 1 + the earlier `reflection_helpers.rs` test use `Vault::open`/`remember`). Adjust the test to the real `get_memory` accessor if it differs (it may be `storage::memory_ops::get`).

- [ ] **Step 2: Implement `remember_correction`**

Add to `impl Vault` in `crates/mnemos_core/src/vault.rs`:

```rust
/// Capture a correction. Validates (`why` required, not weaponized), then
/// either reinforces a near-duplicate existing correction or creates a new
/// Procedural `Correction` memory. `supersedes` optionally invalidates a prior
/// belief. Returns the correction memory id (existing one if reinforced).
pub async fn remember_correction(
    &self,
    correction: crate::correction::Correction,
    supersedes: Option<String>,
) -> crate::error::Result<String> {
    correction
        .validate()
        .map_err(|e| crate::error::MnemosError::Invalid(e.to_string()))?;

    // Dedup: if a very similar existing correction is found, reinforce it.
    if let Some(existing) = self.find_duplicate_correction(&correction).await? {
        self.reinforce(&existing).await?;
        return Ok(existing);
    }

    let mut tags = correction.trigger_tags();
    tags.push("correction".to_string());
    let id = self
        .remember(
            &correction.to_body(),
            crate::vault::RememberOpts {
                title: Some(truncate_title(&correction.right)),
                tier: crate::Tier::Procedural,
                kind: crate::MemoryType::Correction,
                tags,
                importance: Some(0.8),
                workspace: None,
                source_tool: None,
                provenance: vec![],
            },
        )
        .await?;
    if let Some(old) = supersedes {
        // Best-effort: don't fail the capture if the target is missing.
        let _ = self.supersede(&old, &id).await;
    }
    Ok(id)
}

fn truncate_title(s: &str) -> String {
    let t = s.trim();
    if t.len() <= 72 { t.to_string() } else { format!("{}…", &t[..71]) }
}
```

> VERIFY each referenced method against `vault.rs`/`storage`:
> - `MnemosError::Invalid` — use the real variant for bad input (the codebase uses `MnemosError::Internal`/similar; pick the one for invalid user input, or add `Invalid` if none exists).
> - `reinforce(&id)` — there is an access→strength reinforcement path used by recall; find it (`grep -rn "fn reinforce\|access_count\|bump.*strength" crates/mnemos_core/src`) and call it, or inline the same UPDATE (bump strength/importance, set last_accessed, increment access_count, append provenance).
> - `supersede(old, new)` — find how `superseded_by`/`invalid_at` is set (`grep -rn "superseded_by\|supersede" crates/mnemos_core/src`); reuse it.
> - `find_duplicate_correction` — implement in Step 3.

- [ ] **Step 3: Implement the dedup finder**

Add to `impl Vault`:

```rust
/// Find an existing `Correction` memory that is a near-duplicate of `c`,
/// using semantic recall over correction-tier memories filtered by tag
/// overlap. Returns the id to reinforce, or None to create fresh.
async fn find_duplicate_correction(
    &self,
    c: &crate::correction::Correction,
) -> crate::error::Result<Option<String>> {
    // Query text = the trigger + right (what the lesson is about).
    let query = format!("{} {}", c.trigger.as_deref().unwrap_or(""), c.right);
    let hits = self.recall(&query, 5).await?; // VERIFY recall signature/returns
    for hit in hits {
        if hit.memory.kind == crate::MemoryType::Correction
            && hit.score >= DUP_THRESHOLD
        {
            return Ok(Some(hit.memory.id));
        }
    }
    Ok(None)
}
```

Add `const DUP_THRESHOLD: f64 = 0.9;` near the top of `vault.rs` (tune later).

> VERIFY: the `recall` method's real name/signature/return type (the daemon `/v1/memories/search` calls into core recall — `grep -rn "pub async fn recall\|fn search" crates/mnemos_core/src`). Map `hit.memory.kind`/`hit.score` to the real hit struct fields. If recall requires an embedder and the vault was opened without one, dedup is a no-op (return None) — guard for that so the no-embedder test path still creates corrections.

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_core --test correction_vault`
Expected: 2 PASS. Then `cargo test -p mnemos_core` (no regressions) + `cargo build -p mnemos_core`.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/vault.rs crates/mnemos_core/tests/correction_vault.rs
git commit -m "feat(core): Vault::remember_correction (validate, dedup-or-reinforce, supersede)"
```

---

## Task 3: Reflection hardening (cluster ≥3 same-trigger corrections → hardened rule)

**Files:**
- Modify: `crates/mnemos_core/src/pipeline/reflect.rs`
- Test: `crates/mnemos_core/tests/correction_harden.rs`

- [ ] **Step 1: Read the reflect pipeline**

Read `crates/mnemos_core/src/pipeline/reflect.rs` fully and `crates/mnemos_core/tests/pipeline_reflect.rs`. Note how it lists sources, calls the LLM, writes reflection-tier memories, and marks sources reflected. The hardening pass mirrors this but operates on `Correction` memories.

- [ ] **Step 2: Write the failing test**

Create `crates/mnemos_core/tests/correction_harden.rs`:

```rust
use mnemos_core::correction::Correction;
use mnemos_core::paths::Paths;
use mnemos_core::pipeline::reflect::harden_corrections;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::vault::Vault;
use mnemos_core::Tier;
use tempfile::TempDir;

#[tokio::test]
async fn three_same_trigger_corrections_harden_into_one_rule() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    for i in 0..3 {
        v.remember_correction(
            Correction {
                wrong: format!("variant {i}"),
                right: "always run cargo fmt before commit".into(),
                why: "CI rejects unformatted code".into(),
                trigger: Some("git commit formatting".into()),
            },
            None,
        ).await.unwrap();
    }
    let created = harden_corrections(&v, &MockLlm::new(), 3).await.unwrap();
    assert_eq!(created.len(), 1, "one hardened rule from the cluster");
    let rule = v.get_memory(&created[0]).await.unwrap();
    assert_eq!(rule.tier, Tier::Reflection);
    assert!(rule.tags.contains(&"mnemos:hardened".to_string()));
}
```

> VERIFY `MockLlm` location/constructor (`crates/mnemos_core/src/providers/mock_llm.rs`, used in `pipeline_reflect.rs`). If MockLlm needs a `REFLECT:`-style marker to produce output, seed the correction bodies accordingly or extend MockLlm minimally — match the existing reflect test's approach.

- [ ] **Step 3: Implement `harden_corrections`**

Add to `reflect.rs`:

```rust
/// Cluster recent `Correction` memories by shared trigger tags; when a cluster
/// has >= `min_cluster` members, synthesize one hardened rule (Reflection tier,
/// tagged `mnemos:hardened`, elevated importance) that `reflects_on` the
/// sources, and mark the sources reflected. Returns new rule ids.
pub async fn harden_corrections(
    vault: &crate::vault::Vault,
    llm: &dyn crate::providers::LlmProvider,
    min_cluster: usize,
) -> crate::error::Result<Vec<String>> {
    // 1. Load recent un-reflected Correction memories.
    // 2. Group by dominant shared trigger tag (exclude the literal "correction" tag).
    // 3. For each group with len >= min_cluster: build a corpus of their bodies,
    //    ask the LLM to synthesize ONE durable rule (system prompt below),
    //    write it via vault.remember(... Tier::Reflection, kind Reflection,
    //    tags ["mnemos:hardened", <trigger tag>], importance 1.0,
    //    provenance/links = source ids), then mark_reflected(source ids).
    // 4. Return the new rule ids.
    todo!("implement per the steps above; see HARDEN_SYSTEM and the reflect() reference")
}

pub const HARDEN_SYSTEM: &str = "TASK=harden\n\
You are given several corrections the assistant received about the same kind of \
situation. Synthesize ONE durable rule that prevents the mistake going forward. \
Respond ONLY with JSON {\"rule\":\"<imperative rule text>\"}.";
```

Replace the `todo!` with the real implementation following the numbered steps, reusing `recent_unreflected`/`mark_reflected`/`add_memory_link` (from `storage::memory_ops`, seen in `reflection_helpers.rs`) and the `extract_json` helper `reflect()` already uses. Filter the loaded memories to `kind == MemoryType::Correction`.

> VERIFY: `recent_unreflected` returns memories across kinds — filter to corrections. Confirm `mark_reflected`, `add_memory_link`, `extract_json`, and `LlmProvider`/`CompletionRequest` names from the existing `reflect()` body. Reuse them; do not invent new helpers.

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_core --test correction_harden && cargo test -p mnemos_core`
Expected: PASS, no regressions.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/pipeline/reflect.rs crates/mnemos_core/tests/correction_harden.rs
git commit -m "feat(core): harden_corrections — cluster recurring corrections into a hardened rule"
```

---

## Task 4: Working-set surfacing — always include hardened rules (capped)

**Files:**
- Modify: the working-set/`mnemos://working` resource builder
- Test: alongside it

- [ ] **Step 1: Locate the builder**

Run `grep -rn "mnemos://working\|working_set\|fn working" crates/mnemos_daemon/src crates/mnemos_core/src`. Read the function that assembles the working resource (likely `mcp/resources.rs` calling a core helper).

- [ ] **Step 2: Write a failing test**

Add a test (in the builder's crate) that: opens a vault, creates a `mnemos:hardened` Reflection memory (via `remember` with that tag), builds the working set, and asserts the hardened rule's text appears in the output. (If the builder is daemon-side and hard to unit-test, add the test at the core helper boundary you introduce in Step 3.)

- [ ] **Step 3: Implement**

In the working-set builder, after the existing content, query memories tagged `mnemos:hardened` (Reflection tier), ranked by `importance` then recency, capped at `HARDENED_CAP` (const = 10), scoped to the current `workspace` when the builder has one. Append them under a clear heading (e.g. `## Learned rules`). Add `const HARDENED_CAP: usize = 10;`.

> VERIFY: how the builder filters/queries memories (it likely uses `storage::memory_ops::ListFilter` or a recall). Use a tag filter + tier filter consistent with existing queries. Keep the section omitted entirely when there are no hardened rules (no empty heading).

- [ ] **Step 4: Test + commit**

Run the new test + `cargo test -p mnemos_daemon` (or `-p mnemos_core`). Then:
```bash
git add -A
git commit -m "feat: surface hardened correction rules in the working set (capped)"
```

---

## Task 5: MCP `correct` tool

**Files:**
- Modify: `crates/mnemos_daemon/src/mcp/tools.rs`
- Test: `crates/mnemos_daemon/tests/mcp.rs` (or a new `mcp_correct.rs`)

- [ ] **Step 1: Add the tool to the list**

In `tools.rs`, add to the tools-list JSON (near `remember`/`recall`/`reflect`):

```json
{
  "name": "correct",
  "description": "Record a correction after you did something wrong and were corrected. Stores wrong→right→why so the mistake isn't repeated. `why` is required.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "wrong": {"type": "string", "description": "What you did incorrectly"},
      "right": {"type": "string", "description": "The correct approach going forward"},
      "why": {"type": "string", "description": "Why the correct approach is right (required)"},
      "trigger": {"type": "string", "description": "The situation this applies to"},
      "supersedes": {"type": "string", "description": "Optional id of a prior memory this invalidates"}
    },
    "required": ["wrong", "right", "why"]
  }
}
```

- [ ] **Step 2: Add the dispatch arm + handler**

In the `match name { ... }` add `"correct" => correct(state, args).await,` and implement (mirroring the `remember` handler shape):

```rust
async fn correct(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    let get = |k: &str| args[k].as_str().map(String::from);
    let correction = mnemos_core::correction::Correction {
        wrong: get("wrong").ok_or_else(|| anyhow::anyhow!("wrong required"))?,
        right: get("right").ok_or_else(|| anyhow::anyhow!("right required"))?,
        why: get("why").unwrap_or_default(),
        trigger: get("trigger"),
    };
    let id = state
        .vault
        .remember_correction(correction, get("supersedes"))
        .await?; // validation error (missing why / weaponized) surfaces here as a tool error
    Ok(tool_content_json(json!({ "id": id })))
}
```

- [ ] **Step 3: Test**

Add an MCP test that calls `tools/call` with `name: "correct"`, valid args → returns an id; and missing `why` → returns an error. Mirror the existing `mcp.rs` test harness.

Run: `cargo test -p mnemos_daemon --test mcp` (or your new test file). Expected PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/mnemos_daemon/src/mcp/tools.rs crates/mnemos_daemon/tests/
git commit -m "feat(daemon): MCP correct tool"
```

---

## Task 6: REST `POST/GET /v1/corrections`

**Files:**
- Create: `crates/mnemos_daemon/src/routes/corrections.rs`
- Modify: `crates/mnemos_daemon/src/routes/mod.rs`

- [ ] **Step 1: Implement the routes**

Create `routes/corrections.rs` mirroring `routes/firstrun.rs`/`routes/memories.rs` patterns:

```rust
//! `POST /v1/corrections` (structured create, same validation as the MCP tool)
//! and `GET /v1/corrections` (list correction memories, newest first, optional
//! `?hardened=true`).

use axum::{extract::{Query, State}, routing::{get, post}, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/corrections", get(list).post(create))
}

#[derive(Deserialize)]
struct CreateReq { wrong: String, right: String, why: String, trigger: Option<String>, supersedes: Option<String> }

async fn create(State(state): State<AppState>, Json(req): Json<CreateReq>) -> Result<Json<Value>, ApiError> {
    let c = mnemos_core::correction::Correction { wrong: req.wrong, right: req.right, why: req.why, trigger: req.trigger };
    let id = state.vault.remember_correction(c, req.supersedes).await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
struct ListQ { hardened: Option<bool> }

async fn list(State(state): State<AppState>, Query(q): Query<ListQ>) -> Result<Json<Value>, ApiError> {
    // List Correction-kind memories (and, when hardened=true, mnemos:hardened
    // Reflection memories). VERIFY the real list API (storage::memory_ops::ListFilter
    // + the daemon's existing list handler in routes/memories.rs) and map to it.
    let items = state.vault /* ... list by kind=Correction / tag=mnemos:hardened ... */ ;
    Ok(Json(json!({ "corrections": items })))
}
```

> VERIFY + complete `list` against the real list/query API used by `routes/memories.rs` (it already lists memories with filters). Return the same memory JSON shape that endpoint returns. `ApiError::bad_request` maps the validation error (missing why / weaponized) to a 400.

- [ ] **Step 2: Mount + test**

Add `mod corrections;` + `.merge(corrections::router())` in `routes/mod.rs` (authed chain). Add an integration test (mirror `tests/connectors_api.rs`/`tests/reflections.rs`): POST a valid correction → 200 + id; POST missing `why` → 400; GET lists it.

Run: `cargo test -p mnemos_daemon` + `cargo build -p mnemos_daemon`. Expected PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/mnemos_daemon/src/routes/corrections.rs crates/mnemos_daemon/src/routes/mod.rs crates/mnemos_daemon/tests/
git commit -m "feat(daemon): /v1/corrections create + list"
```

---

## Task 7: Session-end mining pass

**Files:**
- Modify: the session-end pipeline (where `reflect`/`harden` runs)

- [ ] **Step 1: Locate the session-end trigger**

Run `grep -rn "reflect(\|salience\|session.*end\|bump_salience\|run_pipeline" crates/mnemos_daemon/src crates/mnemos_core/src/pipeline`. Find where the daemon decides to run reflection at session end (the salience-triggered path noted in the spec).

- [ ] **Step 2: Add a mining pass**

Add a `mine_corrections(vault, llm, session_id)` to `pipeline` that: loads the session's conversation chunks, asks the LLM (system prompt `MINE_SYSTEM` below) to extract `{wrong,right,why,trigger}` tuples, and for each tuple with a non-empty `why` calls `vault.remember_correction(...)` (which dedups/validates). Skip tuples the LLM can't give a `why` for.

```rust
pub const MINE_SYSTEM: &str = "TASK=mine-corrections\n\
Review this conversation and extract moments where the user corrected the \
assistant. For each, output the mistake, the correct approach, the reason, and \
the triggering situation. Only include corrections with a clear reason. \
Respond ONLY with JSON {\"corrections\":[{\"wrong\":\"\",\"right\":\"\",\"why\":\"\",\"trigger\":\"\"}]}.";
```

Wire `mine_corrections` + `harden_corrections` into the existing session-end/reflection trigger so both run. Guard: if the LLM is unavailable or errors, log and skip — never block session end.

- [ ] **Step 3: Test**

Add a core test using MockLlm seeded to emit one correction tuple from a session's chunks → assert a Correction memory is created. Mirror `pipeline_reflect.rs`.

Run the test + `cargo test -p mnemos_core -p mnemos_daemon`. Commit:
```bash
git add -A
git commit -m "feat(core): session-end correction mining pass (LLM extract + dedup)"
```

---

## Task 8: Instruction-file hint + docs

**Files:**
- Modify: `adapters/claude-code/CLAUDE.md.fragment` (and `adapters/codex/` if it has an instructions fragment)
- Modify: `crates/mnemos_daemon/src/connectors/descriptors.rs` `HINT` (only if the connect-wizard branch is merged into this base; else note as follow-up)
- Modify: `README.md` (MCP tools list)

- [ ] **Step 1: Extend the hint**

Append to `adapters/claude-code/CLAUDE.md.fragment`:

```markdown
When the user corrects you and explains why, call `correct(wrong, right, why, trigger)`
so the fix persists and the same mistake isn't repeated.
```

Add `correct` to the README's MCP tools list. If `connectors/descriptors.rs` `HINT` is present in this base, append the same sentence there.

- [ ] **Step 2: Commit**

```bash
git add adapters/ README.md crates/mnemos_daemon/src/connectors/descriptors.rs 2>/dev/null
git commit -m "docs: instruct tools to call correct(); document the correct MCP tool"
```

---

## Task 9: Manual end-to-end verification (dev)

**Files:** none (verification only)

- [ ] **Step 1: Run daemon (trial vault, bundled embedder) and exercise the loop**

```bash
export MNEMOS_CONFIG_PATH=/tmp/mnemos-corr/config.toml MNEMOS_VAULT=/tmp/mnemos-corr/vault LD_LIBRARY_PATH="$PWD/assets"
mkdir -p /tmp/mnemos-corr/vault; printf '[vault]\nroot = "/tmp/mnemos-corr/vault"\n[embedder]\nkind="bundled"\n' > /tmp/mnemos-corr/config.toml
cargo build -p mnemos_daemon && ./target/debug/mnemosd &
TOKEN=$(cat ~/.config/mnemos/token); H=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")
# create a correction
curl -s "${H[@]}" -X POST localhost:7423/v1/corrections -d '{"wrong":"used tabs","right":"use spaces","why":"repo CI enforces spaces","trigger":"formatting rust"}'
# missing why → 400
curl -s -o /dev/null -w "%{http_code}\n" "${H[@]}" -X POST localhost:7423/v1/corrections -d '{"wrong":"x","right":"y","why":""}'
# list
curl -s "${H[@]}" localhost:7423/v1/corrections | python3 -m json.tool
# recall surfaces it
curl -s "${H[@]}" -X POST localhost:7423/v1/memories/search -d '{"query":"how should I format rust code","k":3}' | python3 -m json.tool
```

Expected: create returns id; missing-why returns 400; list shows it; recall surfaces it by trigger.

- [ ] **Step 2: Hardening + working set**

Create 3 corrections with the same trigger, run the reflection trigger (or call the harden path), then fetch the `mnemos://working` resource (MCP `resources/read`) and confirm a `mnemos:hardened` rule appears. Note results in the session log. Tear down: kill the daemon, `rm -rf /tmp/mnemos-corr`.

---

## Self-Review

- **Spec coverage:** data model + `MemoryType::Correction` (Task 1); required-why + anti-weaponization (Task 1, enforced in 2/5/6); dedup-or-reinforce + supersede (Task 2); dual capture = MCP tool (5) + session-end mining (7); hardening (3); hybrid surfacing = working set (4) + recall (relies on corrections being normal procedural memories, verified in Task 2 test); decay/reinforcement reuse (Task 2 reinforce); REST (6); hint/docs (8); manual E2E (9). Violation detection correctly absent (v2). Covered.
- **Verify-against-source notes** are explicit on every integration point (MemoryType serde, recall signature, reinforce/supersede helpers, MockLlm marker, working-set builder, session trigger, list API) — these are real uncertainties best resolved against source, not guessed.
- **Type consistency:** `correction::Correction { wrong, right, why, trigger }` used identically in Tasks 1/2/3/5/6/7; `remember_correction(Correction, Option<String>) -> Result<String>` consistent across 2/5/6; `harden_corrections(&Vault, &dyn LlmProvider, usize) -> Result<Vec<String>>` (3) parallels existing `reflect`; tag `mnemos:hardened` consistent across 3/4; `MemoryType::Correction` + `Tier::Procedural`/`Reflection` consistent.
- **Placeholder note:** Task 3 Step 3 intentionally provides a numbered-spec + `todo!()` skeleton plus the real helper names to reuse, rather than fabricating the exact body of a function that depends on `reflect()` internals the implementer must read first — the test (Step 2) is the contract. Task 4's builder body is described against an unread file with a locate step. These are bounded, contract-pinned exceptions, consistent with this codebase's "verify against source" reality; all other code steps are complete.
