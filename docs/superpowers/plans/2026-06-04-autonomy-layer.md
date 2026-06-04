# Mnemos Autonomy Layer (Claude Code) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After install + the connect wizard, every Claude Code session automatically receives relevant memory (session start + per prompt) and is captured/learned-from at session end — with no manual tool calls and the daemon always running in the background.

**Architecture:** A systemd user service (plus lazy-spawn) keeps the daemon always-on. New `mnemos hook session-start|user-prompt|session-end` CLI subcommands are registered as Claude Code hooks; they inject `additionalContext` and stream the transcript to the existing `/v1/sessions` ingestion → reflect/mine pipeline, which then prunes raw chunks. The connector installs the hooks + service in one click.

**Tech Stack:** Rust (`mnemos_cli`, `mnemos_daemon`, `mnemos_core`), `mnemos_client`, systemd user units, Claude Code hooks, React (app controls).

**Spec:** `docs/superpowers/specs/2026-06-04-autonomy-layer-design.md`

---

## Verified facts (from source)

- **Ingestion API** (`crates/mnemos_daemon/src/routes/sessions.rs`): `POST /v1/sessions {source_tool?, workspace?}` → `{id}`; `POST /v1/sessions/{id}/chunks {body, speaker?, ordinal?, source_meta?}` → `{chunk_id}` (rejects orphan chunks — session must exist); `POST /v1/sessions/{id}/end`.
- **Pipeline trigger** (`crates/mnemos_daemon/src/pipeline_runner.rs`): `process_session` runs on `Event::SessionEnded` → `run_pipeline` (chunks→memories) → `maybe_reflect` (salience-gated) → `maybe_mine_and_harden` (reads the session's **chunks**). Therefore **prune must run AFTER `maybe_mine_and_harden`**.
- **Vector delete** exists: `mnemos_core::storage::vec_ops::delete_chunk_vec(storage, chunk_id)`. No chunk-row delete yet — add one (Task B5).
- **CLI** (`crates/mnemos_cli/src/main.rs`): clap `args.command` match. The `Command` enum + dispatch live in the CLI crate — read it; add `Hook` and `Service` subcommands.
- **Connectors** (merged from PR #2, `crates/mnemos_daemon/src/connectors/`): `ToolConnector { id, edits: Vec<ConfigEdit>, ... }`, `EditStrategy::{JsonMerge, TomlMerge, MarkedBlock}`, `descriptors::registry()`. Claude Code descriptor currently has the MCP `JsonMerge` + the `CLAUDE.md` `MarkedBlock`.
- **Recall** (`mnemos_core`): `dense_recall` / the daemon's recall helper (`crate::routes::recall_helper`) returns `RecallHit { memory, score, .. }`. Working-set builder lives in `mcp/resources.rs` (the `mnemos://working` branch).

---

## File Structure

| File | Responsibility |
|------|----------------|
| `crates/mnemos_cli/src/daemon_ctl.rs` (or extend `commands/daemon.rs`) | `ensure_daemon()` lazy-spawn helper (probe `/health`, spawn detached, wait) |
| `packaging/systemd/mnemosd.service` | systemd **user** unit template |
| `crates/mnemos_cli/src/commands/service.rs` | `mnemos service install|enable|status` (writes unit, `systemctl --user enable --now`) |
| `crates/mnemos_cli/src/commands/hook.rs` | `mnemos hook session-start|user-prompt|session-end` |
| `crates/mnemos_cli/src/transcript.rs` | Claude Code JSONL transcript → `Vec<Turn>` (pure, tested) |
| `crates/mnemos_cli/src/main.rs` | register `Hook` + `Service` subcommands |
| `crates/mnemos_core/src/storage/chunk_ops.rs` | `delete_session_chunks` (row + vec) |
| `crates/mnemos_daemon/src/pipeline_runner.rs` | prune raw chunks after distill (retention-gated) |
| `crates/mnemos_daemon/src/config.rs` | `autonomy` config (capture on/paused, retention, recall budget) |
| `crates/mnemos_daemon/src/connectors/descriptors.rs` | Claude Code connector gains 3 hook edits + service requirement |
| `crates/mnemos_daemon/src/connectors/mod.rs` | service-requirement field + Autonomous status |
| `desktop/src/views/*` | autonomy controls + Knowledge view + wizard glue |

---

# PHASE A — Daemon always-on

## Task A1: `ensure_daemon()` lazy-spawn helper

**Files:**
- Create: `crates/mnemos_cli/src/daemon_ctl.rs`
- Modify: `crates/mnemos_cli/src/main.rs` (or lib) to `mod daemon_ctl;`

- [ ] **Step 1: Implement**

Create `crates/mnemos_cli/src/daemon_ctl.rs`:

```rust
//! Ensure the daemon is running before a hook/CLI action that needs it.
//! Probe /health; if down, spawn `mnemos-daemon` detached and wait briefly.
//! Best-effort: returns Ok(true) if up, Ok(false) if it couldn't be started
//! (callers that are fail-open just proceed).

use std::time::Duration;

const DAEMON_URL: &str = "http://127.0.0.1:7423";

pub async fn is_up() -> bool {
    reqwest::Client::new()
        .get(format!("{DAEMON_URL}/health"))
        .timeout(Duration::from_millis(500))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Ensure the daemon is up. If down, spawn it detached and poll /health up to
/// `timeout`. Returns whether it is up at the end.
pub async fn ensure_daemon(timeout: Duration) -> bool {
    if is_up().await {
        return true;
    }
    // Resolve the daemon binary: prefer the one next to this executable
    // (installed layout: /usr/bin/mnemos + /usr/bin/mnemos-daemon), else PATH.
    let exe = std::env::current_exe().ok();
    let candidate = exe
        .as_ref()
        .and_then(|p| p.parent())
        .map(|d| d.join("mnemos-daemon"));
    let bin = match candidate {
        Some(c) if c.exists() => c.into_os_string(),
        _ => std::ffi::OsString::from("mnemos-daemon"),
    };
    // Detached spawn; ignore failure (fail-open).
    let _ = std::process::Command::new(&bin)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if is_up().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    false
}
```

Add `reqwest` + `tokio` (time) to `crates/mnemos_cli/Cargo.toml` if not already deps (check first; the CLI likely already uses a client — reuse `mnemos_client` if it exposes a health probe).

- [ ] **Step 2: Verify build + a smoke test**

Add a unit test that `is_up()` returns false when nothing is listening (use a guaranteed-unused assumption is unsafe; instead test the binary-resolution logic by factoring it into a pure fn `resolve_daemon_bin(current_exe: Option<PathBuf>) -> OsString` and testing it: when a sibling `mnemos-daemon` exists in a tempdir → returns that path; else → `"mnemos-daemon"`). Run `cargo test -p mnemos_cli daemon_ctl`.

- [ ] **Step 3: Commit**

```bash
git add crates/mnemos_cli/src/daemon_ctl.rs crates/mnemos_cli/src/main.rs crates/mnemos_cli/Cargo.toml
git commit -m "feat(cli): ensure_daemon lazy-spawn helper"
```

## Task A2: systemd user service + `mnemos service` subcommand

**Files:**
- Create: `packaging/systemd/mnemosd.service`
- Create: `crates/mnemos_cli/src/commands/service.rs`
- Modify: `crates/mnemos_cli/src/main.rs` (register `Service` subcommand)

- [ ] **Step 1: Create the unit template**

Create `packaging/systemd/mnemosd.service`:

```ini
[Unit]
Description=Mnemos memory daemon
After=default.target

[Service]
ExecStart=/usr/bin/mnemos-daemon
Restart=always
RestartSec=2
# Bundled embedder assets are found via the packaged /usr/lib/mnemos wrapper.

[Install]
WantedBy=default.target
```

- [ ] **Step 2: Implement `mnemos service`**

Create `crates/mnemos_cli/src/commands/service.rs` with `install`, `enable`, `status` actions:
- `install`: write the unit to `~/.config/systemd/user/mnemosd.service` (resolve via `directories::BaseDirs::config_dir()/systemd/user/`; create dirs). If a packaged copy exists at `/usr/lib/mnemos/mnemosd.service` or `/usr/share/mnemos/`, copy that; else write the embedded template (`include_str!("../../../packaging/systemd/mnemosd.service")`).
- `enable`: run `systemctl --user enable --now mnemosd` (via `std::process::Command`); on failure (no systemd / not a user session) print a clear message and tell the user the daemon will lazy-start instead (non-fatal).
- `status`: `systemctl --user is-active mnemosd` (report active/inactive); fall back to `/health` probe.

Register a `Service { action }` subcommand in the CLI `Command` enum + dispatch in `main.rs`.

> VERIFY: the CLI's clap `Command` enum location + how subcommands dispatch (read `main.rs` + `commands/mod.rs`). Match the existing pattern (e.g. how `daemon` subcommand is structured in `commands/daemon.rs`). `include_str!` path must be correct relative to `service.rs`.

- [ ] **Step 3: Build + commit**

Run `cargo build -p mnemos_cli`. Manually `mnemos service install` writes the unit (verify file appears). Commit:
```bash
git add packaging/systemd/mnemosd.service crates/mnemos_cli/src/commands/service.rs crates/mnemos_cli/src/main.rs
git commit -m "feat(cli): systemd user service + mnemos service install/enable/status"
```

---

# PHASE B — Hooks + capture pipeline

## Task B1: transcript parsing (JSONL → turns)

**Files:**
- Create: `crates/mnemos_cli/src/transcript.rs`
- Modify: `crates/mnemos_cli/src/main.rs` (`mod transcript;`)

- [ ] **Step 1: Failing test + impl**

Claude Code transcripts are JSONL: one JSON object per line, each a conversation event. We extract user/assistant text turns. Create `crates/mnemos_cli/src/transcript.rs`:

```rust
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
        turns.push(Turn { speaker: role.to_string(), body: text, ordinal: ord });
        ord += 1;
    }
    turns
}

/// Pull plain text from a transcript line. content may be a string or an array
/// of blocks `[{"type":"text","text":"..."}]`.
fn extract_text(v: &serde_json::Value) -> String {
    let content = v
        .get("message")
        .and_then(|m| m.get("content"))
        .or_else(|| v.get("content"));
    match content {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_user_and_assistant_turns_in_order() {
        let jsonl = r#"{"type":"user","message":{"role":"user","content":"hi there"}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"hello"}]}}
{"type":"system","content":"ignored"}
not json
{"type":"user","message":{"role":"user","content":""}}"#;
        let turns = parse_transcript(jsonl);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0], Turn { speaker: "user".into(), body: "hi there".into(), ordinal: 0 });
        assert_eq!(turns[1].speaker, "assistant");
        assert_eq!(turns[1].body, "hello");
        assert_eq!(turns[1].ordinal, 1);
    }
}
```

> VERIFY the actual Claude Code transcript JSONL shape against a real transcript (`~/.claude/projects/*/*.jsonl`) — confirm the `type`/`message.role`/`content` fields and adjust `extract_text`/role detection to match. Keep it tolerant (skip unknown lines).

- [ ] **Step 2: Run**

`cargo test -p mnemos_cli transcript`. Commit:
```bash
git add crates/mnemos_cli/src/transcript.rs crates/mnemos_cli/src/main.rs
git commit -m "feat(cli): Claude Code transcript JSONL parser"
```

## Task B2: `mnemos hook session-start`

**Files:**
- Create: `crates/mnemos_cli/src/commands/hook.rs`
- Modify: `crates/mnemos_cli/src/main.rs` (register `Hook { event }`)

- [ ] **Step 1: Implement (fail-open)**

Create `crates/mnemos_cli/src/commands/hook.rs` with a `run(event: &str)` entry that reads stdin JSON and dispatches. For `session-start`:
- Read stdin → JSON; extract `cwd` (→ workspace), `source`.
- `ensure_daemon(Duration::from_secs(5)).await` — if false, print empty output, exit 0.
- GET the working set from the daemon (reuse the `mnemos://working` content: call a daemon endpoint — VERIFY: is there a REST route for the working set, or only the MCP resource? If only MCP, add a small `GET /v1/working?workspace=` route in the daemon that returns the same content, and use it here. Prefer a REST route for the CLI.) Render to text.
- Print `{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext": <text>}}` to stdout, exit 0.
- ANY error → print `{}` (or nothing) + exit 0.

```rust
pub async fn run(event: &str) -> std::process::ExitCode {
    // Always exit 0 (fail-open): a Mnemos problem must never break a session.
    let input = read_stdin_json();
    let out = match event {
        "session-start" => session_start(input).await,
        "user-prompt" => user_prompt(input).await,
        "session-end" => session_end(input).await,
        _ => None,
    };
    if let Some(json) = out {
        println!("{json}");
    }
    std::process::ExitCode::SUCCESS
}
```

Implement `session_start(input) -> Option<String>` per the steps above (returns the hook JSON string, or None on any failure). `read_stdin_json()` returns `serde_json::Value` (or Null on error).

> VERIFY: working-set retrieval path (add `GET /v1/working` to the daemon if absent, mirroring `mcp/resources.rs`'s working branch — that builder already exists; expose it over REST for the hook). Cap the working-set text size (e.g. the existing HARDENED_CAP + a token budget).

- [ ] **Step 2: Test**

Add a test: pipe a `{"cwd":"/x","source":"startup"}` JSON to `session_start` with the daemon down → returns `None` (fail-open), `run` exits 0. (Mock or accept daemon-down path.) Run `cargo test -p mnemos_cli hook`. Commit:
```bash
git add crates/mnemos_cli/src/commands/hook.rs crates/mnemos_cli/src/main.rs crates/mnemos_daemon/src/routes/  # if you added /v1/working
git commit -m "feat(cli): mnemos hook session-start (inject working set, fail-open)"
```

## Task B3: `mnemos hook user-prompt`

**Files:**
- Modify: `crates/mnemos_cli/src/commands/hook.rs`

- [ ] **Step 1: Implement**

`user_prompt(input) -> Option<String>`: extract `prompt` + `cwd`; `ensure_daemon`; POST the daemon recall (`/v1/memories/search {query: prompt, k, workspace}`) with a small `k` (default 6); take hits until a **token budget** (~300 tokens ≈ 1200 chars) is filled; render `additionalContext` listing them; print `{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext": <text>}}`. Empty/short → print nothing. Fail-open.

Add a pure helper `fn budget_hits(hits: &[Hit], max_chars: usize) -> Vec<&Hit>` and test it (stops at the budget). Run `cargo test -p mnemos_cli hook`. Commit `feat(cli): mnemos hook user-prompt (capped per-prompt recall)`.

## Task B4: `mnemos hook session-end` (capture)

**Files:**
- Modify: `crates/mnemos_cli/src/commands/hook.rs`

- [ ] **Step 1: Implement**

`session_end(input) -> Option<String>`: extract `transcript_path` + `cwd` + `session_id`; `ensure_daemon`; read the transcript file → `transcript::parse_transcript`; if empty, return None; POST `/v1/sessions {source_tool:"claude-code", workspace: cwd}` → for each `Turn` POST `/{id}/chunks {body, speaker, ordinal}` → POST `/{id}/end`. Idempotency: record processed `session_id`s in a small state file (`~/.local/state/mnemos/captured-sessions`) and skip if already captured. Always exit 0; capture errors are logged to stderr (not stdout) and swallowed. session-end produces no `additionalContext` (return None).

> VERIFY the exact request JSON field names against `routes/sessions.rs` (AddChunkReq: `body, speaker, ordinal, source_meta`). Use `mnemos_client` if it has session helpers; else raw reqwest with bearer token from `~/.config/mnemos/token`.

- [ ] **Step 2: Test + commit**

Test `session_end` idempotency (second call with same session_id is a no-op) with a fixture transcript + mock/real daemon, or unit-test the idempotency-state logic in isolation. Run tests. Commit `feat(cli): mnemos hook session-end (capture transcript → ingest, idempotent, fail-open)`.

## Task B5: prune-after-distill + autonomy config + redaction

**Files:**
- Create: `crates/mnemos_core/src/storage/chunk_ops.rs` (`delete_session_chunks`)
- Modify: `crates/mnemos_daemon/src/pipeline_runner.rs` (prune after mine)
- Modify: `crates/mnemos_daemon/src/config.rs` (`autonomy` section)

- [ ] **Step 1: `delete_session_chunks` (row + vec)**

Add to a new `chunk_ops.rs` (register in `storage/mod.rs`):
```rust
/// Delete all chunks for a session (and their vectors). Used by the
/// distill-and-prune retention policy after the pipeline has extracted
/// memories + mined corrections from them.
pub async fn delete_session_chunks(storage: &Storage, session_id: &str) -> Result<usize> {
    // 1. select chunk ids for the session
    // 2. delete_chunk_vec for each (reuse vec_ops::delete_chunk_vec)
    // 3. DELETE FROM chunks WHERE session_id = ?
    // return count deleted
}
```
Implement with parameterized SQL (mirror existing `vec_ops`/`memory_ops` patterns). Add a test: insert a session + 2 chunks (+vecs), call delete_session_chunks → 0 chunks remain, vectors gone, the session row + any distilled memories untouched.

- [ ] **Step 2: config**

Add an `autonomy` section to `config.rs` `Config`:
```rust
pub struct AutonomyConfig {
    pub capture: bool,        // default true
    pub retention: String,    // "distill-and-prune" (default) | "keep-raw"
    pub recall_budget_chars: usize, // default 1200
}
```
with `#[serde(default)]` + Default impl (capture=true, retention="distill-and-prune", recall_budget_chars=1200).

- [ ] **Step 3: prune in pipeline_runner**

In `process_session`, AFTER `maybe_mine_and_harden(...)`, if `state.config.autonomy.retention == "distill-and-prune"`, call `delete_session_chunks(state.vault.storage(), session_id)` (log the count; never fail the pipeline on prune error). Confirm ordering: run_pipeline → maybe_reflect → maybe_mine_and_harden → **prune**.

- [ ] **Step 4: redaction guard (in the capture hook)**

In `hook::session_end`, before POSTing a chunk body, run a `redact(body) -> Option<String>` that drops chunks matching obvious secret patterns (e.g. `sk-[A-Za-z0-9]{20,}`, `-----BEGIN .*PRIVATE KEY`, `AKIA[0-9A-Z]{16}`) — skip the chunk entirely if matched. Pure fn + test. (Lives in `hook.rs` or `transcript.rs`.)

- [ ] **Step 5: build/test/commit**

`cargo test -p mnemos_core -p mnemos_daemon -p mnemos_cli`. Commit `feat: distill-and-prune retention + autonomy config + capture redaction guard`.

---

# PHASE C — Connector installs hooks + service; app controls

## Task C1: connector hook edit-set + service + Autonomous status

**Files:**
- Modify: `crates/mnemos_daemon/src/connectors/descriptors.rs`
- Modify: `crates/mnemos_daemon/src/connectors/mod.rs`

- [ ] **Step 1: Extend the model**

Add to `ToolConnector` a way to express the 3 Claude Code settings.json hook entries. Since hooks are nested JSON (`hooks.SessionStart[].hooks[]`), the cleanest is a new `EditStrategy::JsonArrayAppend { pointer: &["hooks","SessionStart"], match_key, value_json }` that appends an object to a JSON array (idempotent: skip if an entry with our marker command already present). Add it alongside `JsonMerge`/`TomlMerge`/`MarkedBlock`, with `is_present`/`rendered`/`removed` arms + tests in `connectors/edits.rs` (mirror the existing JSON helpers; reuse a tempfile test).

The mnemos hook entries (command = the installed `mnemos` on PATH):
```json
{"matcher":"","hooks":[{"type":"command","command":"mnemos hook session-start"}]}
```
for `hooks.SessionStart`, `hooks.UserPromptSubmit` (`mnemos hook user-prompt`), `hooks.SessionEnd` (`mnemos hook session-end`), in `~/.claude/settings.json`.

- [ ] **Step 2: Claude Code descriptor gains the hooks + service flag**

In `descriptors.rs`, add the 3 `JsonArrayAppend` edits (target `~/.claude/settings.json`) to the `claude-code` connector, and a `requires_service: bool` (or a `service` field) on `ToolConnector` set true for claude-code. Add a `connected()` refinement / new `autonomy_status()` returning `Autonomous` (MCP + all hooks present + service active) vs `Connected` (MCP only) vs `None`.

> VERIFY: the connectors `mod.rs` `ConfigEdit`/`EditStrategy`/`ToolConnector` shapes (post-merge) and add the field without breaking existing connectors. The settings.json path is `~/.claude/settings.json` (confirmed). `mnemos` must be on PATH (installed package) for the hook command — note this in the connector (it ensures the service/install).

- [ ] **Step 3: connect/disconnect + status wiring**

The existing `connect`/`disconnect` endpoints apply/remove all edits — the new hook edits flow through automatically. On connect for a `requires_service` tool, also ensure the daemon service (call the same logic as `mnemos service enable`, or return a flag instructing the app/wizard to do it). Surface `autonomy_status` in `GET /v1/connectors`.

- [ ] **Step 4: tests + commit**

Unit-test `JsonArrayAppend` apply/idempotent/remove against a fixture settings.json; test the claude-code descriptor now has MCP + CLAUDE.md + 3 hook edits and reports Autonomous when all present. `cargo test -p mnemos_daemon connectors`. Commit `feat(daemon): connector installs Claude Code hooks + service (Autonomous status)`.

## Task C2: app autonomy controls + Knowledge view + wizard glue

**Files:**
- Modify/Create: `desktop/src/views/*` (Connections status, a Knowledge view, autonomy settings)

- [ ] **Step 1: Connections shows Autonomous**

Update the `Connections` component (TS types + tile) to show the `autonomy_status` (`Autonomous ✓` / `Connected` / `Not installed`) and, for detectable tools, that Connect now enables full autonomy. Update the client type for `connected` to include the autonomy state (or a new `autonomy` field). Test the tile renders the new state.

- [ ] **Step 2: Autonomy settings + Knowledge view**

Add a Settings → Autonomy section bound to the daemon `autonomy` config via the existing `PUT /v1/config` (capture on/pause, retention, recall budget). Add a **Knowledge** view that lists/searches captured memories + hardened rules + corrections (reuse `client.listMemories` / `/v1/corrections`) with delete. Keep components focused; match existing conventions + tests.

- [ ] **Step 3: Wizard glue**

In `FirstRun`, after the embedder step, add a step that calls `mnemos service enable` (via a Tauri command or the daemon) and explains background operation, then the existing Connections step now yields full autonomy. Update FirstRun test for the new step.

- [ ] **Step 4: verify + commit**

`cd desktop && pnpm typecheck && pnpm test && pnpm build`. Commit `feat(ui): autonomy status + controls + Knowledge view + wizard background-service step`.

---

## Task D: manual end-to-end verification (dev)

- [ ] Build CLI + daemon; `mnemos service install` (writes user unit); start daemon. Configure a fake `~/.claude/settings.json` via the connector connect for claude-code; confirm the 3 hooks + MCP entry are present and `GET /v1/connectors` shows Autonomous.
- [ ] Run the hooks manually: `echo '{"cwd":"'$PWD'","source":"startup"}' | mnemos hook session-start` → emits `additionalContext` JSON. `echo '{"prompt":"what am I building","cwd":"'$PWD'"}' | mnemos hook user-prompt` → recall context. Create a small fixture `.jsonl` transcript; `echo '{"transcript_path":"/tmp/t.jsonl","cwd":"'$PWD'","session_id":"s1"}' | mnemos hook session-end` → a session is ingested, a memory forms, and raw chunks are pruned (`GET /v1/sessions/s1` + memory list). Re-run → idempotent (no duplicate).
- [ ] Fail-open: stop the daemon, run each hook → exits 0, emits empty/no context (a real Claude Code session would be unaffected). Record results in the session log.

---

## Self-Review

- **Spec coverage:** daemon always-on = A1 (lazy-spawn) + A2 (systemd); the 3 hooks = B2/B3/B4; capture→distill→prune = B4 (ingest) + B5 (prune after mine) ; workspace scoping = B2/B4 (cwd→workspace); recall cadence (session-start working set + capped per-prompt) = B2 + B3; privacy (capture pause + redaction) = B5; connector installs hooks + service + Autonomous status = C1; app controls + Knowledge view + wizard = C2; fail-open = B2 `run()` + every hook; manual E2E = D. Covered.
- **Verify-against-source notes** are explicit at each integration point (CLI enum, working-set REST exposure, sessions request fields, connectors model, transcript JSONL shape, prune ordering) — must be confirmed against real code, not guessed.
- **Type consistency:** `Turn {speaker,body,ordinal}` (B1) consumed by B4; `ensure_daemon(Duration)->bool` (A1) used by B2/B3/B4; `delete_session_chunks(storage, session_id)->Result<usize>` (B5) called in pipeline_runner (B5 step 3); `EditStrategy::JsonArrayAppend` (C1) consistent with the existing JsonMerge/TomlMerge/MarkedBlock; `autonomy_status` (C1) consumed by the UI (C2). Consistent.
- **Decomposition note:** Phases are sequential (B needs A's `ensure_daemon`; C needs B's hooks). Each phase is independently testable. If executed as separate plans, A → B → C order is required.
- **Bounded exceptions:** a few daemon/UI steps are specified by requirements + a verify-note rather than full literal code (working-set REST route, `mnemos service` systemctl calls, the React views) because they depend on files/exact APIs the implementer must read first; the pure/logic-heavy units (ensure_daemon resolution, transcript parse, budget_hits, delete_session_chunks, redact, JsonArrayAppend) have complete code + tests as the contract.
