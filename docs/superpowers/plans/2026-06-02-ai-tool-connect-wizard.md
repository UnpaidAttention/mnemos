# AI-Tool Auto-Connect Wizard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Detect AI tools installed on the machine and let the user connect each to Mnemos with one confirmation — the daemon writes the MCP config (and session-start hint) into each tool's files, with preview, backup, idempotent merge, and disconnect.

**Architecture:** A `connectors` module in `crates/mnemos_daemon` holds a registry of `ToolConnector` descriptors. Each detectable tool has one or more `ConfigEdit`s (a `JsonMerge` into its MCP config and/or a `MarkedBlock` append into its instruction file). REST endpoints expose list/preview/connect/disconnect; a React `Connections` component (used in the first-run wizard and Settings) is a thin client.

**Tech Stack:** Rust (axum, serde_json, `toml` already present), the existing daemon route/`ApiError` pattern, React + TypeScript (Vitest).

**Spec:** `docs/superpowers/specs/2026-06-02-ai-tool-connect-wizard-design.md`

---

## Cross-cutting conventions (read once)

- **Path resolution is `$HOME`-relative and override-friendly** so tests run in tempdirs: every descriptor's file paths derive from `std::env::var("HOME")` (and XDG vars where relevant). Tests set `HOME` to a tempdir. No absolute literals.
- **`ApiError`** (`crates/mnemos_daemon/src/error.rs`) provides `::bad_request`, `::not_found`, `::internal`; handlers return `Result<Json<Value>, ApiError>`. Mirror `routes/firstrun.rs`.
- **Route modules** export `pub fn router() -> Router<AppState>` and are `.merge()`d in `routes/mod.rs` under bearer auth.
- **No secret in written configs:** edits reference the `mnemos-mcp-stdio` command, which auto-reads `~/.config/mnemos/token`.
- **All writes are atomic** (temp file in the same dir + rename) and **backed up** to `<file>.mnemos.bak` before first modification.

## File Structure

| File | Responsibility |
|------|----------------|
| `crates/mnemos_daemon/src/connectors/mod.rs` | Types (`ToolConnector`, `ConfigEdit`, `EditStrategy`, `ToolKind`, `Installed`, `Connected`), registry accessor, status evaluation |
| `crates/mnemos_daemon/src/connectors/edits.rs` | `backup`, `atomic_write`, JSON-merge + marked-block apply/is_present/remove |
| `crates/mnemos_daemon/src/connectors/detect.rs` | `binary_on_path`, `path_exists` detection probes |
| `crates/mnemos_daemon/src/connectors/descriptors.rs` | The real registry: claude-code, codex, antigravity-cli, gemini-cli (deprecated), manual tiles |
| `crates/mnemos_daemon/src/routes/connectors.rs` | REST: list / preview / connect / disconnect |
| `crates/mnemos_daemon/src/lib.rs` (or `main.rs` mod tree) | `pub mod connectors;` registration |
| `crates/mnemos_daemon/src/routes/mod.rs` | `.merge(connectors::router())` |
| `adapters/antigravity-cli/{README.md,mcp_config.json}` | New adapter content for Antigravity CLI |
| `desktop/src/api/client.ts` | `listConnectors/previewConnector/connectConnector/disconnectConnector` |
| `desktop/src/views/Connections.tsx` + `.test.tsx` | The Connections UI |
| `desktop/src/views/Settings.tsx` | Mount `<Connections />` |
| `desktop/src/views/FirstRun.tsx` (+ test) | Replace static step 3 with `<Connections />` |

---

## Task 1: Edit primitives — backup + atomic write + JSON merge

**Files:**
- Create: `crates/mnemos_daemon/src/connectors/edits.rs`
- Modify: `crates/mnemos_daemon/src/connectors/mod.rs` (create with `pub mod edits;`)
- Modify: `crates/mnemos_daemon/src/lib.rs` — add `pub mod connectors;`

- [ ] **Step 1: Write failing tests**

Create `crates/mnemos_daemon/src/connectors/edits.rs`:

```rust
//! File-edit primitives for tool connectors: safe backup + atomic write, and
//! the JSON-merge / marked-block strategies used to add or remove the mnemos
//! entry from a tool's config files.

use std::path::Path;

/// Back up `path` to `<path>.mnemos.bak` if it exists and no backup exists yet.
pub fn backup(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let bak = path.with_extension(format!(
        "{}.mnemos.bak",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    if !bak.exists() {
        std::fs::copy(path, &bak).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Write `contents` to `path` atomically (temp file in same dir + rename).
pub fn atomic_write(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("mnemos.tmp");
    std::fs::write(&tmp, contents).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Insert `value` at `pointer`/`key` in a JSON document string, creating
/// intermediate objects as needed. Returns the new document string. Idempotent
/// (replaces an existing key, never duplicates).
pub fn json_merge(doc: &str, pointer: &[&str], key: &str, value: &serde_json::Value) -> Result<String, String> {
    let mut root: serde_json::Value =
        if doc.trim().is_empty() { serde_json::json!({}) } else { serde_json::from_str(doc).map_err(|e| e.to_string())? };
    if !root.is_object() {
        return Err("config root is not a JSON object".into());
    }
    let mut cur = &mut root;
    for seg in pointer {
        cur = cur
            .as_object_mut()
            .ok_or_else(|| format!("`{seg}` parent is not an object"))?
            .entry(seg.to_string())
            .or_insert_with(|| serde_json::json!({}));
    }
    cur.as_object_mut()
        .ok_or_else(|| "target is not an object".to_string())?
        .insert(key.to_string(), value.clone());
    serde_json::to_string_pretty(&root).map_err(|e| e.to_string())
}

/// True if `pointer`/`key` exists in the JSON document.
pub fn json_has(doc: &str, pointer: &[&str], key: &str) -> bool {
    let root: serde_json::Value = match serde_json::from_str(doc) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mut cur = &root;
    for seg in pointer {
        match cur.get(seg) {
            Some(v) => cur = v,
            None => return false,
        }
    }
    cur.get(key).is_some()
}

/// Remove `pointer`/`key` from the JSON document; returns new string. No-op if absent.
pub fn json_remove(doc: &str, pointer: &[&str], key: &str) -> Result<String, String> {
    let mut root: serde_json::Value =
        if doc.trim().is_empty() { return Ok(doc.to_string()); } else { serde_json::from_str(doc).map_err(|e| e.to_string())? };
    let mut cur = &mut root;
    for seg in pointer {
        match cur.as_object_mut().and_then(|o| o.get_mut(*seg)) {
            Some(v) => cur = v,
            None => return serde_json::to_string_pretty(&root).map_err(|e| e.to_string()),
        }
    }
    if let Some(o) = cur.as_object_mut() {
        o.remove(key);
    }
    serde_json::to_string_pretty(&root).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_merge_inserts_nested_and_is_idempotent() {
        let v = json!({"command":"mnemos-mcp-stdio"});
        let once = json_merge("{}", &["mcp", "servers"], "mnemos", &v).unwrap();
        assert!(json_has(&once, &["mcp", "servers"], "mnemos"));
        let twice = json_merge(&once, &["mcp", "servers"], "mnemos", &v).unwrap();
        // idempotent: still exactly one mnemos under mcp.servers
        let parsed: serde_json::Value = serde_json::from_str(&twice).unwrap();
        assert_eq!(parsed["mcp"]["servers"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn json_merge_preserves_existing_keys() {
        let start = r#"{"mcpServers":{"other":{"command":"x"}}}"#;
        let out = json_merge(start, &["mcpServers"], "mnemos", &json!({"command":"mnemos-mcp-stdio"})).unwrap();
        assert!(json_has(&out, &["mcpServers"], "other"));
        assert!(json_has(&out, &["mcpServers"], "mnemos"));
    }

    #[test]
    fn json_remove_strips_only_mnemos() {
        let start = r#"{"mcpServers":{"other":{},"mnemos":{}}}"#;
        let out = json_remove(start, &["mcpServers"], "mnemos").unwrap();
        assert!(!json_has(&out, &["mcpServers"], "mnemos"));
        assert!(json_has(&out, &["mcpServers"], "other"));
    }

    #[test]
    fn backup_and_atomic_write_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("c.json");
        std::fs::write(&f, "{}").unwrap();
        backup(&f).unwrap();
        assert!(f.with_extension("json.mnemos.bak").exists());
        atomic_write(&f, "{\"a\":1}").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "{\"a\":1}");
    }
}
```

Create `crates/mnemos_daemon/src/connectors/mod.rs` with just `pub mod edits;` for now. Add `pub mod connectors;` to `crates/mnemos_daemon/src/lib.rs` (near the other `pub mod` lines). Confirm `tempfile` is a dev-dependency of `mnemos_daemon` — if not, add it under `[dev-dependencies]` in `crates/mnemos_daemon/Cargo.toml`.

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon connectors::edits`
Expected: 4 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/mnemos_daemon/src/connectors/ crates/mnemos_daemon/src/lib.rs crates/mnemos_daemon/Cargo.toml
git commit -m "feat(daemon): connector edit primitives (backup, atomic write, json merge)"
```

---

## Task 2: Marked-block edit strategy (markdown instruction-file hints)

**Files:**
- Modify: `crates/mnemos_daemon/src/connectors/edits.rs`

- [ ] **Step 1: Write failing tests**

Append to `edits.rs` (above the `tests` module):

```rust
pub const BLOCK_START: &str = "<!-- mnemos:start -->";
pub const BLOCK_END: &str = "<!-- mnemos:end -->";

/// Insert or replace the marked block containing `body` in `doc`. The block is
/// delimited by BLOCK_START/BLOCK_END so it can be detected and removed cleanly.
pub fn marked_block_apply(doc: &str, body: &str) -> String {
    let block = format!("{BLOCK_START}\n{}\n{BLOCK_END}", body.trim_end());
    if let (Some(s), Some(e)) = (doc.find(BLOCK_START), doc.find(BLOCK_END)) {
        let end = e + BLOCK_END.len();
        let mut out = String::with_capacity(doc.len());
        out.push_str(&doc[..s]);
        out.push_str(&block);
        out.push_str(&doc[end..]);
        out
    } else {
        let sep = if doc.is_empty() || doc.ends_with('\n') { "" } else { "\n" };
        format!("{doc}{sep}\n{block}\n")
    }
}

/// True if the marked block is present.
pub fn marked_block_present(doc: &str) -> bool {
    doc.contains(BLOCK_START) && doc.contains(BLOCK_END)
}

/// Remove the marked block (and a trailing blank line it introduced). No-op if absent.
pub fn marked_block_remove(doc: &str) -> String {
    match (doc.find(BLOCK_START), doc.find(BLOCK_END)) {
        (Some(s), Some(e)) => {
            let end = (e + BLOCK_END.len()).min(doc.len());
            let mut out = String::new();
            out.push_str(doc[..s].trim_end_matches('\n'));
            let tail = &doc[end..];
            out.push_str(tail.trim_start_matches('\n').is_empty().then_some("").unwrap_or(tail));
            if out.is_empty() { out } else { out }
        }
        _ => doc.to_string(),
    }
}
```

Add to the `tests` module:

```rust
    #[test]
    fn marked_block_apply_is_idempotent_and_removable() {
        let original = "# My CLAUDE.md\n\nmy own notes\n";
        let once = marked_block_apply(original, "hint body");
        assert!(marked_block_present(&once));
        assert!(once.contains("my own notes"), "preserves user content");
        let twice = marked_block_apply(&once, "hint body");
        assert_eq!(once.matches(BLOCK_START).count(), 1);
        assert_eq!(twice.matches(BLOCK_START).count(), 1, "no duplicate block");
        let removed = marked_block_remove(&twice);
        assert!(!marked_block_present(&removed));
        assert!(removed.contains("my own notes"), "user content survives removal");
    }
```

- [ ] **Step 2: Run to verify pass**

Run: `cargo test -p mnemos_daemon connectors::edits`
Expected: 5 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/mnemos_daemon/src/connectors/edits.rs
git commit -m "feat(daemon): marked-block edit strategy for instruction-file hints"
```

---

## Task 3: Detection probes

**Files:**
- Create: `crates/mnemos_daemon/src/connectors/detect.rs`
- Modify: `crates/mnemos_daemon/src/connectors/mod.rs` (`pub mod detect;`)

- [ ] **Step 1: Write failing tests**

Create `crates/mnemos_daemon/src/connectors/detect.rs`:

```rust
//! Detection probes: is a tool installed? Either its binary is on PATH or one
//! of its known config paths exists.

use std::path::Path;

/// True if `name` resolves to an executable on `PATH`.
pub fn binary_on_path(name: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else { return false };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(name);
        candidate.is_file()
    })
}

/// True if any of the given paths exists.
pub fn any_path_exists(paths: &[&Path]) -> bool {
    paths.iter().any(|p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn binary_on_path_finds_seeded_binary() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("faketool");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        let prev = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", dir.path());
        assert!(binary_on_path("faketool"));
        assert!(!binary_on_path("definitely-not-a-real-tool-xyz"));
        std::env::set_var("PATH", prev);
    }

    #[test]
    fn any_path_exists_detects_present_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("config");
        std::fs::write(&f, "x").unwrap();
        let missing = PathBuf::from("/no/such/path/xyz");
        assert!(any_path_exists(&[&f, &missing]));
        assert!(!any_path_exists(&[&missing]));
    }
}
```

Add `pub mod detect;` to `connectors/mod.rs`.

- [ ] **Step 2: Run to verify pass**

Run: `cargo test -p mnemos_daemon connectors::detect`
Expected: 2 tests PASS.

> NOTE: these tests mutate the process-global `PATH` env. If the daemon test suite runs tests in parallel and another test reads `PATH`, add `#[serial_test::serial]` (check whether `serial_test` is already a dev-dep; if not, the two tests here are self-contained enough — they restore PATH — but flag it). Prefer not adding a dep unless a flake appears.

- [ ] **Step 3: Commit**

```bash
git add crates/mnemos_daemon/src/connectors/detect.rs crates/mnemos_daemon/src/connectors/mod.rs
git commit -m "feat(daemon): connector detection probes (PATH + config paths)"
```

---

## Task 4: Connector types + status evaluation

**Files:**
- Modify: `crates/mnemos_daemon/src/connectors/mod.rs`

- [ ] **Step 1: Write the types + status logic with tests**

Replace `connectors/mod.rs` contents with (keeping the `pub mod` lines):

```rust
pub mod detect;
pub mod edits;
pub mod descriptors;

use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    /// Detectable and writable (we can auto-connect).
    Detectable,
    /// SDK/wrapper integration shown as a manual tile.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Connected {
    Full,
    Partial,
    None,
}

/// One file edit a connector performs.
pub struct ConfigEdit {
    /// Resolves the target file path (HOME-relative). Boxed fn for testability.
    pub target: fn() -> PathBuf,
    pub strategy: EditStrategy,
}

pub enum EditStrategy {
    /// Insert `value` at `pointer`/`key` in a JSON config.
    JsonMerge { pointer: &'static [&'static str], key: &'static str, value_json: &'static str },
    /// Insert a marked block of `body` into a markdown/text file.
    MarkedBlock { body: &'static str },
}

pub struct ToolConnector {
    pub id: &'static str,
    pub display_name: &'static str,
    pub kind: ToolKind,
    /// Some(reason) if the tool is deprecated.
    pub deprecated: Option<&'static str>,
    /// Detection probe.
    pub detect: fn() -> bool,
    /// Edits (empty for Manual).
    pub edits: Vec<ConfigEdit>,
    /// For Manual tiles (and fallback display): a copy-paste snippet + target hint.
    pub manual_snippet: Option<(&'static str, &'static str)>, // (target_hint, snippet)
}

impl ConfigEdit {
    /// Read the current target file ("" if missing).
    pub fn read(&self) -> String {
        let p = (self.target)();
        std::fs::read_to_string(&p).unwrap_or_default()
    }
    pub fn path(&self) -> PathBuf { (self.target)() }
    pub fn is_present(&self) -> bool {
        let doc = self.read();
        match &self.strategy {
            EditStrategy::JsonMerge { pointer, key, .. } => edits::json_has(&doc, pointer, key),
            EditStrategy::MarkedBlock { .. } => edits::marked_block_present(&doc),
        }
    }
    /// Compute the post-apply contents without writing.
    pub fn rendered(&self) -> Result<String, String> {
        let doc = self.read();
        match &self.strategy {
            EditStrategy::JsonMerge { pointer, key, value_json } => {
                let v: serde_json::Value = serde_json::from_str(value_json).map_err(|e| e.to_string())?;
                edits::json_merge(&doc, pointer, key, &v)
            }
            EditStrategy::MarkedBlock { body } => Ok(edits::marked_block_apply(&doc, body)),
        }
    }
    pub fn removed(&self) -> Result<String, String> {
        let doc = self.read();
        match &self.strategy {
            EditStrategy::JsonMerge { pointer, key, .. } => edits::json_remove(&doc, pointer, key),
            EditStrategy::MarkedBlock { .. } => Ok(edits::marked_block_remove(&doc)),
        }
    }
}

impl ToolConnector {
    pub fn installed(&self) -> bool { (self.detect)() }
    pub fn connected(&self) -> Connected {
        if self.edits.is_empty() { return Connected::None; }
        let present = self.edits.iter().filter(|e| e.is_present()).count();
        if present == 0 { Connected::None }
        else if present == self.edits.len() { Connected::Full }
        else { Connected::Partial }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_target() -> PathBuf {
        PathBuf::from(std::env::var("MNEMOS_TEST_EDIT_FILE").unwrap())
    }

    #[test]
    fn connected_reflects_edit_presence() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("c.json");
        std::env::set_var("MNEMOS_TEST_EDIT_FILE", &f);
        let c = ToolConnector {
            id: "t", display_name: "T", kind: ToolKind::Detectable, deprecated: None,
            detect: || true,
            edits: vec![ConfigEdit {
                target: tmp_target,
                strategy: EditStrategy::JsonMerge { pointer: &["mcpServers"], key: "mnemos", value_json: "{\"command\":\"mnemos-mcp-stdio\"}" },
            }],
            manual_snippet: None,
        };
        assert_eq!(c.connected(), Connected::None);
        std::fs::write(&f, c.edits[0].rendered().unwrap()).unwrap();
        assert_eq!(c.connected(), Connected::Full);
    }
}
```

- [ ] **Step 2: Run to verify pass**

Run: `cargo test -p mnemos_daemon connectors::tests`
Expected: PASS. Then `cargo build -p mnemos_daemon` (descriptors module referenced but not yet created will fail — create a stub `descriptors.rs` with `use super::*; pub fn registry() -> Vec<ToolConnector> { vec![] }` to compile, replaced in Task 5).

- [ ] **Step 3: Commit**

```bash
git add crates/mnemos_daemon/src/connectors/mod.rs crates/mnemos_daemon/src/connectors/descriptors.rs
git commit -m "feat(daemon): connector types + connected-status evaluation"
```

---

## Task 5: The connector registry (real descriptors)

**Files:**
- Modify: `crates/mnemos_daemon/src/connectors/descriptors.rs`

> VERIFY against each tool's real docs before finalizing paths/JSON shapes — do not guess. Known starting points (from `adapters/*`): Claude Code MCP JSON uses top-level `mcpServers`; Codex uses `mcp.servers`; Gemini uses `mcpServers`. **Confirm the actual on-disk config file each tool reads** (e.g. Claude Code: `~/.claude.json` vs `~/.config/claude-code/...`; Antigravity CLI: its `mcp_config.json` location) by checking each tool's current documentation. Adjust `target` paths + JSON `pointer` accordingly and note what you confirmed.

- [ ] **Step 1: Implement the registry**

Replace `descriptors.rs`:

```rust
use super::{ConfigEdit, EditStrategy, ToolConnector, ToolKind};
use super::detect::{any_path_exists, binary_on_path};
use std::path::PathBuf;

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

// ---- path resolvers (HOME-relative; VERIFY each against tool docs) ----
fn claude_mcp_path() -> PathBuf { home().join(".claude.json") }
fn claude_md_path() -> PathBuf { home().join(".claude").join("CLAUDE.md") }
fn codex_config_path() -> PathBuf { home().join(".codex").join("config.json") }
fn codex_agents_path() -> PathBuf { home().join(".codex").join("AGENTS.md") }
fn gemini_settings_path() -> PathBuf { home().join(".gemini").join("settings.json") }
fn antigravity_mcp_path() -> PathBuf { home().join(".antigravity").join("mcp_config.json") }

const CLAUDE_HINT: &str = "## Mnemos persistent memory\n\nThis session has a persistent memory server (Mnemos) registered as an MCP provider. At the start of every session, read the `mnemos://working` resource — it contains identity facts and active project context.\n\nWhen the user states a durable preference, project context, or rule not obvious from the codebase, call `remember(...)` so it persists across sessions.";

pub fn registry() -> Vec<ToolConnector> {
    vec![
        ToolConnector {
            id: "claude-code",
            display_name: "Claude Code",
            kind: ToolKind::Detectable,
            deprecated: None,
            detect: || binary_on_path("claude") || any_path_exists(&[&claude_mcp_path(), &home().join(".claude")]),
            edits: vec![
                ConfigEdit {
                    target: claude_mcp_path,
                    strategy: EditStrategy::JsonMerge {
                        pointer: &["mcpServers"],
                        key: "mnemos",
                        value_json: r#"{"command":"mnemos-mcp-stdio","args":[],"env":{"MNEMOS_DAEMON_URL":"http://127.0.0.1:7423"}}"#,
                    },
                },
                ConfigEdit { target: claude_md_path, strategy: EditStrategy::MarkedBlock { body: CLAUDE_HINT } },
            ],
            manual_snippet: None,
        },
        ToolConnector {
            id: "codex",
            display_name: "Codex",
            kind: ToolKind::Detectable,
            deprecated: None,
            detect: || binary_on_path("codex") || any_path_exists(&[&home().join(".codex")]),
            edits: vec![
                ConfigEdit {
                    target: codex_config_path,
                    strategy: EditStrategy::JsonMerge {
                        pointer: &["mcp", "servers"],
                        key: "mnemos",
                        value_json: r#"{"command":"mnemos-mcp-stdio","args":[]}"#,
                    },
                },
                ConfigEdit { target: codex_agents_path, strategy: EditStrategy::MarkedBlock { body: CLAUDE_HINT } },
            ],
            manual_snippet: None,
        },
        ToolConnector {
            id: "antigravity-cli",
            display_name: "Antigravity CLI",
            kind: ToolKind::Detectable,
            deprecated: None,
            detect: || binary_on_path("antigravity") || any_path_exists(&[&home().join(".antigravity")]),
            edits: vec![ConfigEdit {
                target: antigravity_mcp_path,
                strategy: EditStrategy::JsonMerge {
                    pointer: &["mcpServers"],
                    key: "mnemos",
                    value_json: r#"{"command":"mnemos-mcp-stdio"}"#,
                },
            }],
            manual_snippet: None,
        },
        ToolConnector {
            id: "gemini-cli",
            display_name: "Gemini CLI",
            kind: ToolKind::Detectable,
            deprecated: Some("Gemini CLI shuts down 2026-06-18 — migrate to Antigravity CLI"),
            detect: || binary_on_path("gemini") || any_path_exists(&[&gemini_settings_path()]),
            edits: vec![ConfigEdit {
                target: gemini_settings_path,
                strategy: EditStrategy::JsonMerge {
                    pointer: &["mcpServers"],
                    key: "mnemos",
                    value_json: r#"{"command":"mnemos-mcp-stdio"}"#,
                },
            }],
            manual_snippet: None,
        },
        // ---- Manual tiles ----
        manual("generic-mcp", "Generic MCP client", "your MCP client config", r#"{"mcpServers":{"mnemos":{"command":"mnemos-mcp-stdio"}}}"#),
        manual("openai-functions", "OpenAI function-calling", "adapters/openai-functions/schema.json", "see adapters/openai-functions/schema.json"),
        manual("hermes", "Hermes agent", "adapters/hermes-agent/", "see adapters/hermes-agent/README.md"),
        manual("openclaw", "OpenClaw", "adapters/openclaw/", "see adapters/openclaw/README.md"),
    ]
}

fn manual(id: &'static str, name: &'static str, target_hint: &'static str, snippet: &'static str) -> ToolConnector {
    ToolConnector {
        id, display_name: name, kind: ToolKind::Manual, deprecated: None,
        detect: || false, edits: vec![], manual_snippet: Some((target_hint, snippet)),
    }
}

pub fn by_id(id: &str) -> Option<ToolConnector> {
    registry().into_iter().find(|c| c.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_expected_tools_and_kinds() {
        let r = registry();
        assert!(r.iter().any(|c| c.id == "claude-code" && c.kind == ToolKind::Detectable));
        assert!(r.iter().any(|c| c.id == "antigravity-cli"));
        let gem = r.iter().find(|c| c.id == "gemini-cli").unwrap();
        assert!(gem.deprecated.is_some(), "gemini-cli is deprecated");
        assert!(r.iter().any(|c| c.id == "generic-mcp" && c.kind == ToolKind::Manual));
        // claude-code has two edits (MCP + CLAUDE.md hint)
        assert_eq!(r.iter().find(|c| c.id == "claude-code").unwrap().edits.len(), 2);
    }
}
```

- [ ] **Step 2: Run to verify pass**

Run: `cargo test -p mnemos_daemon connectors::descriptors`
Expected: 1 test PASS. Then `cargo build -p mnemos_daemon`.

- [ ] **Step 3: Commit**

```bash
git add crates/mnemos_daemon/src/connectors/descriptors.rs
git commit -m "feat(daemon): connector registry (claude-code, codex, antigravity, gemini[deprecated], manual tiles)"
```

---

## Task 6: REST — list + apply/remove engine

**Files:**
- Create: `crates/mnemos_daemon/src/routes/connectors.rs`
- Modify: `crates/mnemos_daemon/src/routes/mod.rs` (`.merge(connectors::router())` + `mod connectors;`)

- [ ] **Step 1: Implement the endpoints**

Create `crates/mnemos_daemon/src/routes/connectors.rs`:

```rust
//! `GET /v1/connectors`, `POST /v1/connectors/{id}/preview|connect|disconnect`.
//! Detects installed AI tools and writes/removes the mnemos MCP entry (and
//! session-start hint) in each tool's config, with backup + atomic writes.

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use crate::connectors::{descriptors, edits, Connected};
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/connectors", get(list))
        .route("/v1/connectors/{id}/preview", post(preview))
        .route("/v1/connectors/{id}/connect", post(connect))
        .route("/v1/connectors/{id}/disconnect", post(disconnect))
}

fn connected_str(c: Connected) -> &'static str {
    match c { Connected::Full => "full", Connected::Partial => "partial", Connected::None => "none" }
}

async fn list(State(_): State<AppState>) -> Result<Json<Value>, ApiError> {
    let items: Vec<Value> = descriptors::registry().iter().map(|c| {
        json!({
            "id": c.id,
            "display_name": c.display_name,
            "kind": c.kind,
            "deprecated": c.deprecated,
            "installed": c.installed(),
            "connected": connected_str(c.connected()),
            "manual_snippet": c.manual_snippet.map(|(t, s)| json!({"target": t, "snippet": s})),
            "edits": c.edits.iter().map(|e| json!({
                "path": e.path().to_string_lossy(),
                "present": e.is_present(),
            })).collect::<Vec<_>>(),
        })
    }).collect();
    Ok(Json(json!({ "connectors": items })))
}

async fn preview(Path(id): Path<String>, State(_): State<AppState>) -> Result<Json<Value>, ApiError> {
    let c = descriptors::by_id(&id).ok_or_else(|| ApiError::not_found(format!("unknown connector {id}")))?;
    if c.edits.is_empty() {
        return Err(ApiError::bad_request(format!("{id} is a manual integration; no automatic config to preview")));
    }
    let mut previews = Vec::new();
    for e in &c.edits {
        let before = e.read();
        let after = e.rendered().map_err(ApiError::bad_request)?;
        previews.push(json!({
            "path": e.path().to_string_lossy(),
            "before": before,
            "after": after,
            "already_present": e.is_present(),
        }));
    }
    Ok(Json(json!({ "id": id, "edits": previews })))
}

async fn connect(Path(id): Path<String>, State(_): State<AppState>) -> Result<Json<Value>, ApiError> {
    let c = descriptors::by_id(&id).ok_or_else(|| ApiError::not_found(format!("unknown connector {id}")))?;
    if c.edits.is_empty() {
        return Err(ApiError::bad_request(format!("{id} is a manual integration")));
    }
    // Apply each edit; on failure, restore from the backups written this call.
    let mut applied: Vec<std::path::PathBuf> = Vec::new();
    for e in &c.edits {
        let path = e.path();
        let rendered = e.rendered().map_err(ApiError::bad_request)?;
        if let Err(err) = (|| -> Result<(), String> {
            edits::backup(&path)?;
            edits::atomic_write(&path, &rendered)
        })() {
            // rollback previously applied edits from their .mnemos.bak
            for p in &applied {
                let bak = p.with_extension(format!("{}.mnemos.bak", p.extension().and_then(|x| x.to_str()).unwrap_or("")));
                if bak.exists() { let _ = std::fs::copy(&bak, p); }
            }
            return Err(ApiError::internal(format!("connect {id} failed at {}: {err}", path.display())));
        }
        applied.push(path);
    }
    Ok(Json(json!({ "id": id, "connected": connected_str(c.connected()) })))
}

async fn disconnect(Path(id): Path<String>, State(_): State<AppState>) -> Result<Json<Value>, ApiError> {
    let c = descriptors::by_id(&id).ok_or_else(|| ApiError::not_found(format!("unknown connector {id}")))?;
    for e in &c.edits {
        if !e.path().exists() { continue; }
        let removed = e.removed().map_err(ApiError::bad_request)?;
        edits::backup(&e.path()).map_err(ApiError::internal)?;
        edits::atomic_write(&e.path(), &removed).map_err(ApiError::internal)?;
    }
    Ok(Json(json!({ "id": id, "connected": connected_str(c.connected()) })))
}
```

Add `mod connectors;` to `routes/mod.rs` and `.merge(connectors::router())` in the `authed` chain. NOTE: confirm the axum version's path-param syntax — this codebase's other routes (e.g. `memories::router()`) show whether it's `:id` or `{id}`. Match the existing style (the firstrun/memories routes are the reference).

- [ ] **Step 2: Build**

Run: `cargo build -p mnemos_daemon`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/mnemos_daemon/src/routes/connectors.rs crates/mnemos_daemon/src/routes/mod.rs
git commit -m "feat(daemon): /v1/connectors list/preview/connect/disconnect endpoints"
```

---

## Task 7: Endpoint integration test (fixture HOME)

**Files:**
- Create: `crates/mnemos_daemon/tests/connectors_api.rs`

- [ ] **Step 1: Write the test**

This test drives the connector logic through a fixture `HOME`, exercising connect → present → disconnect for Claude Code (both edits). It tests the module functions directly (the registry reads `HOME` at call time), which is the same code the handlers call.

Create `crates/mnemos_daemon/tests/connectors_api.rs`:

```rust
// Connect/disconnect a tool against a fixture HOME and assert files change correctly.
use mnemos_daemon::connectors::{descriptors, edits};

#[test]
fn claude_code_connect_then_disconnect_roundtrip() {
    let home = tempfile::tempdir().unwrap();
    std::env::set_var("HOME", home.path());
    // Pre-seed an existing CLAUDE.md with user content + an existing MCP config with another server.
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    std::fs::write(home.path().join(".claude/CLAUDE.md"), "# mine\n\nkeep me\n").unwrap();
    std::fs::write(home.path().join(".claude.json"), r#"{"mcpServers":{"other":{"command":"x"}}}"#).unwrap();

    let c = descriptors::by_id("claude-code").unwrap();

    // connect: apply each edit
    for e in &c.edits {
        let rendered = e.rendered().unwrap();
        edits::backup(&e.path()).unwrap();
        edits::atomic_write(&e.path(), &rendered).unwrap();
    }
    assert_eq!(format!("{:?}", c.connected()), "Full");
    let mcp = std::fs::read_to_string(home.path().join(".claude.json")).unwrap();
    assert!(mcp.contains("mnemos") && mcp.contains("other"), "added mnemos, kept other");
    let md = std::fs::read_to_string(home.path().join(".claude/CLAUDE.md")).unwrap();
    assert!(md.contains("keep me") && md.contains("mnemos:start"));

    // disconnect: remove each edit
    for e in &c.edits {
        let removed = e.removed().unwrap();
        edits::atomic_write(&e.path(), &removed).unwrap();
    }
    assert_eq!(format!("{:?}", c.connected()), "None");
    let md2 = std::fs::read_to_string(home.path().join(".claude/CLAUDE.md")).unwrap();
    assert!(md2.contains("keep me") && !md2.contains("mnemos:start"), "user content kept, block gone");
}
```

This requires `Connected` to derive `Debug` (it does) and `mnemos_daemon::connectors` to be public (Task 1 made it `pub mod`). `descriptors::by_id`, `ConfigEdit::{rendered,path,removed,is_present}`, and `edits::{backup,atomic_write}` must be `pub` (they are).

- [ ] **Step 2: Run**

Run: `cargo test -p mnemos_daemon --test connectors_api`
Expected: PASS.
NOTE: sets `HOME` globally — if other integration tests in the same binary rely on HOME, isolate via a separate test binary (this is its own file = its own binary, so safe).

- [ ] **Step 3: Commit**

```bash
git add crates/mnemos_daemon/tests/connectors_api.rs
git commit -m "test(daemon): connector connect/disconnect roundtrip against fixture HOME"
```

---

## Task 8: Antigravity CLI adapter content

**Files:**
- Create: `adapters/antigravity-cli/README.md`, `adapters/antigravity-cli/mcp_config.json`

- [ ] **Step 1: Create the adapter**

> VERIFY the real Antigravity CLI `mcp_config.json` path + schema from its current docs before finalizing. If the schema differs from the assumed `mcpServers` shape, update both this file AND the `antigravity-cli` descriptor's `pointer`/`value_json` in Task 5 to match, and note what you confirmed.

Create `adapters/antigravity-cli/mcp_config.json`:

```json
{
  "mcpServers": {
    "mnemos": {
      "command": "mnemos-mcp-stdio"
    }
  }
}
```

Create `adapters/antigravity-cli/README.md`:

```markdown
# Mnemos × Antigravity CLI

Google's Antigravity CLI (the successor to Gemini CLI, which shuts down
2026-06-18) supports MCP servers via a dedicated `mcp_config.json`.

## Setup (automatic)

Use the Mnemos desktop app → **Settings → Connections** → **Antigravity CLI →
Connect**. It writes the `mnemos` entry below into Antigravity's
`mcp_config.json` for you (with a backup + preview).

## Setup (manual)

Add the `mnemos` server from `mcp_config.json` in this directory to Antigravity's
MCP config. The `mnemos-mcp-stdio` command reads the daemon token from
`~/.config/mnemos/token` automatically.

> Verify the exact config path against the current Antigravity CLI docs — the
> tool is new and its layout may change.
```

- [ ] **Step 2: Commit**

```bash
git add adapters/antigravity-cli/
git commit -m "docs: add Antigravity CLI adapter (Gemini CLI successor)"
```

---

## Task 9: Frontend API client methods

**Files:**
- Modify: `desktop/src/api/client.ts`

- [ ] **Step 1: Add typed methods**

Read `desktop/src/api/client.ts` and add these methods to the `MnemosClient` class (mirroring the existing `req<T>` helper), plus exported types near the other type defs:

```typescript
export interface ConnectorEdit { path: string; present: boolean }
export interface Connector {
  id: string;
  display_name: string;
  kind: "detectable" | "manual";
  deprecated: string | null;
  installed: boolean;
  connected: "full" | "partial" | "none";
  manual_snippet: { target: string; snippet: string } | null;
  edits: ConnectorEdit[];
}
export interface ConnectorPreview {
  id: string;
  edits: { path: string; before: string; after: string; already_present: boolean }[];
}
```

Methods on `MnemosClient`:

```typescript
  async listConnectors(): Promise<Connector[]> {
    return (await this.req<{ connectors: Connector[] }>("GET", "/v1/connectors")).connectors;
  }
  previewConnector(id: string): Promise<ConnectorPreview> {
    return this.req<ConnectorPreview>("POST", `/v1/connectors/${id}/preview`);
  }
  connectConnector(id: string): Promise<{ id: string; connected: string }> {
    return this.req<{ id: string; connected: string }>("POST", `/v1/connectors/${id}/connect`);
  }
  disconnectConnector(id: string): Promise<{ id: string; connected: string }> {
    return this.req<{ id: string; connected: string }>("POST", `/v1/connectors/${id}/disconnect`);
  }
```

- [ ] **Step 2: Typecheck**

Run: `cd desktop && pnpm typecheck`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add desktop/src/api/client.ts
git commit -m "feat(ui): connector API client methods + types"
```

---

## Task 10: Connections component

**Files:**
- Create: `desktop/src/views/Connections.tsx`, `desktop/src/views/Connections.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `desktop/src/views/Connections.test.tsx`:

```typescript
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { Connections } from "./Connections";
import { client } from "../api/client";

vi.mock("../api/client", () => ({
  client: {
    listConnectors: vi.fn(),
    previewConnector: vi.fn(),
    connectConnector: vi.fn(),
    disconnectConnector: vi.fn(),
  },
}));

const base = {
  id: "claude-code", display_name: "Claude Code", kind: "detectable" as const,
  deprecated: null, installed: true, connected: "none" as const,
  manual_snippet: null, edits: [{ path: "~/.claude.json", present: false }],
};

describe("Connections", () => {
  beforeEach(() => {
    vi.mocked(client.listConnectors).mockResolvedValue([base]);
    vi.mocked(client.previewConnector).mockResolvedValue({ id: "claude-code", edits: [{ path: "~/.claude.json", before: "{}", after: "{...}", already_present: false }] });
    vi.mocked(client.connectConnector).mockResolvedValue({ id: "claude-code", connected: "full" });
  });

  it("lists detected tools with status", async () => {
    render(<Connections />);
    expect(await screen.findByText("Claude Code")).toBeInTheDocument();
    expect(screen.getByText(/installed/i)).toBeInTheDocument();
  });

  it("previews then connects on confirm", async () => {
    render(<Connections />);
    fireEvent.click(await screen.findByRole("button", { name: /^connect$/i }));
    // preview shown, then confirm
    fireEvent.click(await screen.findByRole("button", { name: /apply/i }));
    await waitFor(() => expect(client.connectConnector).toHaveBeenCalledWith("claude-code"));
  });
});
```

- [ ] **Step 2: Run to verify fail**

Run: `cd desktop && pnpm test -- Connections`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement the component**

Create `desktop/src/views/Connections.tsx`. Read `desktop/src/design/primitives` first to confirm `Button`/`Card`; use `text-tier-procedural` for the deprecated/error emphasis (as established in StorageSettings). Requirements the component must meet (write it to satisfy the tests + spec, matching existing view conventions):
- On mount: `client.listConnectors()`; loading + error states (error → `role="alert"`).
- Each connector → a tile: `display_name`, a status badge (Installed / Connected ✓ / Partially connected / Deprecated ⚠ / Not installed), and actions.
- Detectable + installed + not fully connected → **Connect** button → calls `previewConnector(id)`, shows the per-edit `path` + a before/after preview, with **Apply** (→ `connectConnector(id)`) and **Cancel**.
- Connected (full/partial) → **Disconnect** button → `disconnectConnector(id)`.
- `deprecated` → show the deprecation note with `text-tier-procedural`.
- Manual kind → show `manual_snippet.target` + `manual_snippet.snippet` in a `<pre>`, no Connect.
- After connect/disconnect, refresh the list (re-call `listConnectors`).
Keep it one component focused on rendering connector state + the preview/confirm flow. No extra features.

- [ ] **Step 4: Run to verify pass**

Run: `cd desktop && pnpm test -- Connections && pnpm typecheck`
Expected: 2 tests PASS, typecheck clean.

- [ ] **Step 5: Commit**

```bash
git add desktop/src/views/Connections.tsx desktop/src/views/Connections.test.tsx
git commit -m "feat(ui): Connections component (detect, preview, connect, disconnect)"
```

---

## Task 11: Mount in Settings + first-run wizard

**Files:**
- Modify: `desktop/src/views/Settings.tsx`, `desktop/src/views/FirstRun.tsx` (+ its test if it asserts old copy)

- [ ] **Step 1: Settings → Connections**

In `desktop/src/views/Settings.tsx`, `import { Connections } from "./Connections";` and render `<Connections />` as a section (alongside `<StorageSettings />`, above the `SCHEMA.map(...)` config sections). Read the file to match placement/layout.

- [ ] **Step 2: First-run wizard step 3 uses the live detector**

In `desktop/src/views/FirstRun.tsx`, replace the static "Connect your AI tools" snippet block in step 2 (the `<details>` snippets) with `<Connections />`. Keep the step title and the Back/Finish buttons. If `FirstRun.test.tsx` asserts on the old snippet text, update those assertions to the new behavior (the step now renders the Connections list; assert the heading remains).

- [ ] **Step 3: Verify**

Run: `cd desktop && pnpm typecheck && pnpm test && pnpm build`
Expected: all PASS.

- [ ] **Step 4: Commit**

```bash
git add desktop/src/views/Settings.tsx desktop/src/views/FirstRun.tsx desktop/src/views/FirstRun.test.tsx
git commit -m "feat(ui): surface Connections in Settings + first-run wizard"
```

---

## Task 12: Manual end-to-end verification (dev)

**Files:** none (verification only)

- [ ] **Step 1: Run daemon + UI against a fixture HOME**

Use a throwaway HOME so real tool configs aren't touched:

```bash
export TRIAL=/tmp/mnemos-connect-trial
mkdir -p "$TRIAL/.claude"
echo '{}' > "$TRIAL/.claude.json"
# Make a tool "detectable": create its dir
# (HOME override applies to the daemon process and what the connectors read)
HOME="$TRIAL" MNEMOS_CONFIG_PATH=/tmp/mnemos-trial/config.toml LD_LIBRARY_PATH="$PWD/assets" ./target/debug/mnemosd &
TOKEN=$(cat ~/.config/mnemos/token)
curl -s -H "Authorization: Bearer $TOKEN" http://localhost:7423/v1/connectors | python3 -m json.tool
```

Expected: `claude-code` shows `installed: true, connected: "none"`.

- [ ] **Step 2: Preview + connect + verify files**

```bash
curl -s -H "Authorization: Bearer $TOKEN" -X POST http://localhost:7423/v1/connectors/claude-code/preview | python3 -m json.tool
curl -s -H "Authorization: Bearer $TOKEN" -X POST http://localhost:7423/v1/connectors/claude-code/connect
cat "$TRIAL/.claude.json"            # contains mnemos under mcpServers
cat "$TRIAL/.claude/CLAUDE.md"       # contains the mnemos:start..end block
ls "$TRIAL"/.claude.json.mnemos.bak  # backup exists
```

- [ ] **Step 3: Disconnect + verify clean removal**

```bash
curl -s -H "Authorization: Bearer $TOKEN" -X POST http://localhost:7423/v1/connectors/claude-code/disconnect
grep -c mnemos "$TRIAL/.claude.json"   # 0
```

- [ ] **Step 4: GUI check**

Launch the desktop app, open **Settings → Connections**, confirm the tiles render with correct badges and the preview→Apply→Disconnect flow works. Note results in the session log.

---

## Self-Review

- **Spec coverage:** registry/descriptor kinds (Task 4–5), multi-edit incl. CLAUDE.md hint (Task 2, 5, 7), JsonMerge+MarkedBlock strategies (Task 1–2), detection (Task 3), backup+atomic+idempotent (Task 1, 6), REST list/preview/connect/disconnect (Task 6), Antigravity first-class + Gemini deprecated (Task 5, 8), manual tiles (Task 5, 10), UI in wizard + settings (Task 10–11), connected full/partial/none (Task 4), error handling incl. rollback (Task 6), token-not-in-file (descriptor values use `mnemos-mcp-stdio`), testing layers (unit 1–5, integration 7, manual 12). Covered.
- **Verify-against-real-source notes** are explicit in Tasks 5, 6, 8 (config paths, axum path-param syntax, Antigravity schema) — implementer must confirm, not guess. This is intentional given recent/closed-source tools.
- **Type consistency:** `Connected` (full/partial/none) ↔ TS `connected` union; `ToolKind` (detectable/manual) ↔ TS `kind`; endpoint shapes in Task 6 match the TS types in Task 9 (`connectors`, `edits[].present`, `manual_snippet{target,snippet}`, preview `edits[].{path,before,after,already_present}`). `descriptors::by_id`, `registry`, `ConfigEdit::{rendered,removed,path,is_present,read}`, `edits::{backup,atomic_write,json_merge,json_has,json_remove,marked_block_*}` consistent across Tasks 1–7.
- **Placeholder scan:** Task 10 step 3 describes the component via requirements rather than full literal code (the test pins behavior; the rendering is conventional and must match existing primitives) — this is a deliberate, bounded exception so the implementer matches the real design system, with the test as the contract. All other code steps are complete.
