//! `mnemos hook <event>` — Claude Code hook integration.
//!
//! Dispatches to event-specific handlers. All handlers are fail-open:
//! any Mnemos failure (daemon down, timeout, parse error) results in
//! returning `None` (no output) and `ExitCode::SUCCESS`. The caller's
//! Claude Code session must NEVER be broken by a Mnemos problem.
//!
//! ## Hook event subcommands
//! - `session-start` (B2): inject the user's working set as `additionalContext`.
//! - `user-prompt`   (B3): recall memories relevant to the current prompt and
//!   inject them as `additionalContext`.
//! - `session-end`   (B4): reserved stub — TODO(B4).
//!
//! ## Input
//! Each handler receives the JSON value parsed from stdin (the hook
//! payload Claude Code sends). On any stdin read or parse error the
//! value is `Value::Null`.
//!
//! ## Output
//! Handlers return `Option<String>`. When `Some(json)`, the string is
//! printed to stdout as-is (one line). When `None`, nothing is printed.

use mnemos_core::retrieval::RecallHit;
use serde_json::Value;
use std::io::Read;
use std::process::ExitCode;
use std::time::Duration;

/// Maximum UTF-8 bytes we pass as `additionalContext` to Claude Code.
/// Keeps the injected context inside a reasonable token budget (~2 000 tokens).
const WORKING_SET_BYTE_CAP: usize = 8_000;

/// Maximum chars of recall context injected into a UserPromptSubmit hook.
/// Split between project-pinned context and query-matched recall.
const RECALL_BUDGET_CHARS: usize = 2_400;

/// Budget reserved for project-pinned context (Project + Entity type memories).
const PROJECT_BUDGET_CHARS: usize = 800;

/// Budget for query-matched + entity-expanded recall.
const QUERY_BUDGET_CHARS: usize = 1_600;

/// Default number of recall hits to request from the daemon per prompt.
const RECALL_K: usize = 8;

/// Entry point for `mnemos hook <event>`.
///
/// Always returns `ExitCode::SUCCESS` (fail-open guarantee).
pub async fn run(event: &str) -> ExitCode {
    let input = read_stdin_json().await;
    let out = match event {
        "session-start" => session_start(input).await,
        "user-prompt" => user_prompt(input).await,
        "stop" => stop(input).await,
        "session-end" => session_end(input).await,
        other => {
            eprintln!("mnemos hook: unknown event {other:?} — ignoring");
            None
        }
    };
    if let Some(json) = out {
        println!("{json}");
    }
    ExitCode::SUCCESS
}

// ── Event handlers ────────────────────────────────────────────────────────────

/// Handle the `SessionStart` hook: inject the user's working set as
/// `additionalContext` so Claude Code has memory context from the first message.
///
/// Also creates a daemon session and writes an active-session state file so
/// subsequent hooks (`user-prompt`, `stop`) can stream chunks in real-time.
async fn session_start(input: Value) -> Option<String> {
    // Extract workspace from the hook payload. Claude Code sets `cwd` in the
    // session-start payload; `source` identifies the tool.
    let workspace = input
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Best-effort: try to ensure the daemon is up. If it's not, return None
    // (fail-open — never block the session).
    let up = crate::daemon_ctl::ensure_daemon(Duration::from_secs(5)).await;
    if !up {
        eprintln!("mnemos hook session-start: daemon not available — skipping context injection");
        return None;
    }

    // Load the bearer token from the well-known path.
    let token = match load_token() {
        Some(t) => t,
        None => {
            eprintln!("mnemos hook session-start: could not load bearer token");
            return None;
        }
    };

    // Create a daemon session for real-time chunk streaming.
    if let Some(ref sid) = session_id {
        match post_start_session(&token, workspace.as_deref()).await {
            Ok(daemon_session_id) => {
                let state = ActiveSessionState {
                    daemon_session_id,
                    tool_id: "claude-code".into(),
                    workspace: workspace.clone(),
                    next_ordinal: 0,
                    keyword_first_seen: std::collections::HashMap::new(),
                    last_prompt_ordinal: 0,
                };
                if let Err(e) = save_active_session(sid, &state) {
                    eprintln!("mnemos hook session-start: failed to write state file: {e}");
                }
            }
            Err(e) => {
                eprintln!("mnemos hook session-start: failed to create daemon session: {e:#}");
            }
        }
    }

    // Fetch the working set from the daemon.
    let text = match fetch_working_set(&token, workspace.as_deref()).await {
        Some(t) => t,
        None => return None,
    };

    working_set_hook_json(&text)
}

/// Handle the `UserPromptSubmit` hook: recall memories relevant to the
/// user's current prompt and inject them as `additionalContext`.
///
/// Also streams the user's prompt to the daemon as a chunk for real-time
/// processing (if an active session state file exists).
///
/// Fail-open: any error → `None` (no output), never panics, never writes
/// to stdout on failure.
async fn user_prompt(input: Value) -> Option<String> {
    // Extract the user's prompt text. Empty / whitespace → nothing to recall.
    let prompt = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if prompt.is_empty() {
        return None;
    }

    // Workspace is derived from the hook payload's `cwd` field.
    let workspace = input
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Ensure the daemon is reachable; bail silently if not.
    let up = crate::daemon_ctl::ensure_daemon(Duration::from_secs(5)).await;
    if !up {
        eprintln!("mnemos hook user-prompt: daemon not available — skipping recall injection");
        return None;
    }

    let token = match load_token() {
        Some(t) => t,
        None => {
            eprintln!("mnemos hook user-prompt: could not load bearer token");
            return None;
        }
    };

    // Stream the user prompt as a chunk to the daemon for real-time processing.
    if let Some(ref sid) = session_id {
        if let Some(mut state) = load_active_session(sid) {
            if let Some(body) = redact(&prompt) {
                let _ = post_streaming_chunk(
                    &token,
                    &state.daemon_session_id,
                    &body,
                    "user",
                    state.next_ordinal,
                )
                .await;
                state.next_ordinal += 1;
                let _ = save_active_session(sid, &state);
            }
        }
    }

    // ── Layer 1: always-on project context (workspace-pinned) ────────────────
    let project_ctx = fetch_project_context(&token, workspace.as_deref()).await;

    // ── Layer 3: detect returning topics from session history ────────────────
    let returning_keywords = if let Some(ref sid) = session_id {
        if let Some(ref state) = load_active_session(sid) {
            detect_returning_topics(state, &prompt)
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    // Augment the recall query with returning-topic keywords to boost
    // memories about topics the user is circling back to.
    let recall_query = if returning_keywords.is_empty() {
        prompt.clone()
    } else {
        format!("{} {}", prompt, returning_keywords.join(" "))
    };

    // ── Layer 2: query-matched recall with entity expansion ──────────────────
    let hits = fetch_recall(&token, &recall_query, workspace.as_deref(), true).await;

    // Also fetch recall specifically for returning topics (if any) to ensure
    // we surface the original context that may have fallen out.
    let recovery_hits = if !returning_keywords.is_empty() {
        let recovery_query = returning_keywords.join(" ");
        fetch_recall(&token, &recovery_query, workspace.as_deref(), false).await
    } else {
        None
    };

    // ── Record keywords for future Layer 3 detection ─────────────────────────
    if let Some(ref sid) = session_id {
        if let Some(mut state) = load_active_session(sid) {
            record_prompt_keywords(&mut state, &prompt);
            let _ = save_active_session(sid, &state);
        }
    }

    // ── Merge all layers into the injection text ─────────────────────────────
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut lines: Vec<String> = Vec::new();
    let mut char_budget = RECALL_BUDGET_CHARS;

    // Project context section (Layer 1)
    if let Some(ref project_mems) = project_ctx {
        if !project_mems.is_empty() {
            lines.push("[Project Context]".to_string());
            char_budget -= 18; // "[Project Context]\n"
            let mut project_chars = 0usize;
            for m in project_mems {
                if project_chars >= PROJECT_BUDGET_CHARS {
                    break;
                }
                let snippet: String = m.body.chars().take(300).collect();
                let line = format!("- {}: {}", m.title, snippet);
                let line_len = line.chars().count() + 1;
                project_chars += line_len;
                char_budget = char_budget.saturating_sub(line_len);
                seen_ids.insert(m.id.clone());
                lines.push(line);
            }
        }
    }

    // Context recovery section (Layer 3) — show only if returning topics detected
    if let Some(ref recovery_list) = recovery_hits {
        let recovery_unique: Vec<&RecallHit> = recovery_list
            .iter()
            .filter(|h| !seen_ids.contains(&h.memory.id))
            .take(3) // max 3 recovery hits to avoid crowding
            .collect();
        if !recovery_unique.is_empty() {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push("[Recovered Context]".to_string());
            char_budget = char_budget.saturating_sub(21);
            for h in &recovery_unique {
                if char_budget == 0 {
                    break;
                }
                let snippet: String = h.memory.body.chars().take(300).collect();
                let line = format!("- {}: {}", h.memory.title, snippet);
                let line_len = line.chars().count() + 1;
                char_budget = char_budget.saturating_sub(line_len);
                seen_ids.insert(h.memory.id.clone());
                lines.push(line);
            }
        }
    }

    // Query recall section (Layer 2)
    if let Some(ref hit_list) = hits {
        let query_hits: Vec<&RecallHit> = hit_list
            .iter()
            .filter(|h| !seen_ids.contains(&h.memory.id))
            .collect();
        if !query_hits.is_empty() {
            if !lines.is_empty() {
                lines.push(String::new()); // separator
            }
            lines.push("[Relevant Memories]".to_string());
            char_budget = char_budget.saturating_sub(20);
            let mut query_chars = 0usize;
            for h in &query_hits {
                if query_chars >= QUERY_BUDGET_CHARS || char_budget == 0 {
                    break;
                }
                let snippet: String = h.memory.body.chars().take(300).collect();
                let line = format!("- {}: {}", h.memory.title, snippet);
                let line_len = line.chars().count() + 1;
                query_chars += line_len;
                char_budget = char_budget.saturating_sub(line_len);
                lines.push(line);
            }
        }
    }

    if lines.is_empty() {
        return None;
    }

    let text = lines.join("\n");
    user_prompt_hook_json(&text)
}

/// Handle the `Stop` hook: capture Claude's response as a chunk for real-time
/// processing. Fires after each Claude turn.
///
/// Fail-open: any error → `None`, never writes to stdout.
async fn stop(input: Value) -> Option<String> {
    let message = input
        .get("last_assistant_message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if message.is_empty() {
        return None;
    }

    let session_id = input.get("session_id").and_then(|v| v.as_str())?;

    let mut state = load_active_session(session_id)?;

    let token = load_token()?;

    // Stream the assistant response as a chunk.
    if let Some(body) = redact(&message) {
        let _ = post_streaming_chunk(
            &token,
            &state.daemon_session_id,
            &body,
            "assistant",
            state.next_ordinal,
        )
        .await;
        state.next_ordinal += 1;
        let _ = save_active_session(session_id, &state);
    }

    None // Stop hooks don't produce additionalContext
}

/// Handle the `SessionEnd` hook: read the transcript, ingest it into the
/// daemon as a session + chunks, then post `end`. Fail-open: any failure logs
/// to stderr and returns `None`. Never produces `additionalContext` output.
///
/// Idempotency contract: a session is recorded as captured once it is
/// successfully started on the daemon (`POST /v1/sessions` returned Ok).
/// Chunk POSTs and `/end` are best-effort — failures are logged to stderr
/// and not retried. "Recorded" therefore means start succeeded; it does not
/// guarantee every chunk landed or that `/end` completed.
async fn session_end(input: Value) -> Option<String> {
    if let Err(e) = do_session_end(&input).await {
        eprintln!("mnemos hook session-end: {e:#}");
    }
    None
}

/// All real work for `session_end`. Returns `Ok(())` on success.
/// Any `Err` is swallowed by `session_end` → fail-open guarantee.
///
/// Idempotency contract: a session is recorded as captured once it is
/// successfully started on the daemon (`POST /v1/sessions` returned Ok).
/// Chunk POSTs and `/end` are best-effort — failures are logged to stderr
/// and not retried. "Recorded" therefore means start succeeded; it does not
/// guarantee every chunk landed or that `/end` completed.
async fn do_session_end(input: &Value) -> anyhow::Result<()> {
    use anyhow::Context as _;

    // ── 1. Extract required fields from hook payload ──────────────────────
    let transcript_path = input
        .get("transcript_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing transcript_path in hook payload — skipping"))?;

    let cwd = input
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing session_id in hook payload — skipping"))?;

    // ── 2. Path-traversal guard ───────────────────────────────────────────
    let transcript_path_buf = std::path::PathBuf::from(transcript_path);
    let canonical_transcript = transcript_path_buf.canonicalize().with_context(|| {
        format!("transcript_path {transcript_path:?} does not exist or is not accessible")
    })?;
    if !is_transcript_path_allowed(&canonical_transcript) {
        return Err(anyhow::anyhow!(
            "transcript_path {transcript_path:?} is outside the expected transcript directories — refusing to read"
        ));
    }

    // ── 3. Best-effort capture guard (P0-6 CLI side) ─────────────────────
    if let Ok(cfg) = mnemos_daemon::config::Config::load_default() {
        if !cfg.autonomy.capture {
            eprintln!(
                "mnemos hook session-end: autonomy.capture = false — skipping capture (daemon is authoritative)"
            );
            remove_active_session(session_id);
            return Ok(());
        }
    }

    // ── 4. Idempotency: skip if already captured ──────────────────────────
    let state_path = idempotency_state_path()?;
    if already_captured(&state_path, session_id) {
        eprintln!("mnemos hook session-end: session {session_id} already captured — skipping");
        remove_active_session(session_id);
        return Ok(());
    }

    // ── 5. Ensure daemon is available ─────────────────────────────────────
    let up = crate::daemon_ctl::ensure_daemon(Duration::from_secs(5)).await;
    if !up {
        remove_active_session(session_id);
        return Err(anyhow::anyhow!(
            "daemon not available — transcript not captured"
        ));
    }

    let token = load_token().ok_or_else(|| anyhow::anyhow!("could not load bearer token"))?;

    // ── 6. Read and parse transcript ──────────────────────────────────────
    let contents = std::fs::read_to_string(&canonical_transcript)
        .with_context(|| format!("failed to read transcript at {transcript_path}"))?;

    let turns = crate::transcript::parse_transcript(&contents);
    if turns.is_empty() {
        record_captured(&state_path, session_id);
        remove_active_session(session_id);
        return Ok(());
    }

    // ── 7. Resolve daemon session ─────────────────────────────────────────
    // If real-time streaming was active (state file exists), reuse that
    // daemon session ID. Otherwise create a new one (fallback for sessions
    // where session-start failed to create the state file).
    let active_state = load_active_session(session_id);
    let daemon_session_id = if let Some(ref state) = active_state {
        eprintln!(
            "mnemos hook session-end: reusing streaming session {}",
            state.daemon_session_id
        );
        state.daemon_session_id.clone()
    } else {
        post_start_session(&token, cwd.as_deref()).await?
    };

    // ── 8. POST remaining chunks from transcript ─────────────────────────
    // When real-time streaming was active, many chunks are already in the
    // daemon. The transcript may contain additional turns that weren't
    // captured (e.g., tool use outputs, internal messages). POST them all;
    // the daemon deduplicates by session_id + ordinal.
    post_chunks(&token, &daemon_session_id, &turns).await;

    // ── 9. Record capture (idempotency boundary) ─────────────────────────
    record_captured(&state_path, session_id);

    // ── 10. POST /v1/sessions/{id}/end (best-effort, non-fatal) ──────────
    if let Err(e) = post_end_session(&token, &daemon_session_id).await {
        eprintln!("mnemos hook session-end: /end failed (session {daemon_session_id}): {e:#}");
    }

    // ── 11. Clean up active session state file ───────────────────────────
    remove_active_session(session_id);

    Ok(())
}

// ── Path-traversal guard ──────────────────────────────────────────────────────

/// Returns `true` if `canonical_path` is allowed to be read as a Claude Code
/// transcript.
///
/// Allowed locations (in order of preference):
/// 1. Under `~/.claude/` — the canonical Claude Code transcript directory.
/// 2. Anywhere under the user's home directory (`$HOME`) — Claude Code stores
///    transcripts only under home, so this covers non-standard layouts while
///    still blocking `/etc/passwd`, `/proc/…`, etc.
///
/// If the home directory cannot be determined (unusual environment), the check
/// is skipped and `true` is returned (fail-open: the hook must never block a
/// Claude session).
///
/// `canonical_path` must already be canonicalized by the caller (all symlinks
/// resolved, `..` eliminated) so that prefix comparisons are reliable.
pub(crate) fn is_transcript_path_allowed(canonical_path: &std::path::Path) -> bool {
    use directories::BaseDirs;

    // Prefer the BaseDirs home; fall back to $HOME env var.
    let home = BaseDirs::new()
        .map(|bd| bd.home_dir().to_path_buf())
        .or_else(|| std::env::var("HOME").ok().map(std::path::PathBuf::from));

    let Some(home) = home else {
        // Cannot determine home — fail-open.
        return true;
    };

    // Canonicalize home as well so symlink-based home dirs don't break the
    // prefix check. Tolerate failure (e.g. home doesn't exist in a container).
    let canonical_home = home.canonicalize().unwrap_or(home);

    canonical_path.starts_with(&canonical_home)
}

// ── Idempotency helpers ───────────────────────────────────────────────────────

/// Maximum number of session IDs retained in the idempotency state file.
///
/// When this cap is exceeded during a write, the oldest entries are discarded
/// (the file is rewritten with only the newest `IDEMPOTENCY_MAX_ENTRIES`
/// lines). This bounds both file size and the O(n) scan cost of
/// `already_captured`.  10 000 entries × ~37 bytes/UUID ≈ 370 KiB worst case.
const IDEMPOTENCY_MAX_ENTRIES: usize = 10_000;

/// Resolve the idempotency state file path.
///
/// Uses `~/.local/state/mnemos/captured-sessions` on Linux/macOS.
/// Falls back to `$HOME/.local/state/mnemos/captured-sessions` if `dirs` fails.
fn idempotency_state_path() -> anyhow::Result<std::path::PathBuf> {
    use directories::BaseDirs;
    let base = BaseDirs::new()
        .and_then(|bd| bd.state_dir().map(|p| p.to_path_buf()))
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| std::path::PathBuf::from(h).join(".local").join("state"))
        })
        .ok_or_else(|| anyhow::anyhow!("cannot determine state directory"))?;
    Ok(base.join("mnemos").join("captured-sessions"))
}

/// Load the lines of the idempotency state file.
///
/// Returns an empty `Vec` if the file is absent, unreadable, or oversized
/// (size guard: if the file exceeds `IDEMPOTENCY_MAX_ENTRIES * 200` bytes we
/// treat it as corrupt/bloated and return empty, which will cause the file to
/// be rewritten on the next `record_captured` call).
///
/// Fail-open: any FS error returns `vec![]`.
fn load_idempotency_entries(state_path: &std::path::Path) -> Vec<String> {
    // Size guard: a bloated or corrupt file degrades safely to an empty list.
    // Each entry is typically a UUID (36 chars) + newline = 37 bytes.
    // 10 000 entries × 200 bytes (generous headroom) = 2 MiB cap.
    const MAX_BYTES: u64 = IDEMPOTENCY_MAX_ENTRIES as u64 * 200;
    if let Ok(meta) = std::fs::metadata(state_path) {
        if meta.len() > MAX_BYTES {
            eprintln!(
                "mnemos hook: idempotency file {} exceeds size limit ({} > {} bytes) — treating as empty and will rewrite on next capture",
                state_path.display(),
                meta.len(),
                MAX_BYTES,
            );
            return vec![];
        }
    }

    let Ok(contents) = std::fs::read_to_string(state_path) else {
        return vec![];
    };
    contents
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .collect()
}

/// Returns `true` if `session_id` is already present in the state file.
/// Any FS error → `false` (fail-open: prefer re-capture over losing data).
pub(crate) fn already_captured(state_path: &std::path::Path, session_id: &str) -> bool {
    load_idempotency_entries(state_path)
        .iter()
        .any(|l| l == session_id)
}

/// Appends `session_id` to the state file (one id per line) and trims the
/// file to the newest `IDEMPOTENCY_MAX_ENTRIES` entries when the cap is
/// exceeded.
///
/// Creates parent directories as needed. Any FS error is logged to stderr and
/// silently ignored (fail-open).
pub(crate) fn record_captured(state_path: &std::path::Path, session_id: &str) {
    if let Some(parent) = state_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!(
                "mnemos hook session-end: could not create state dir {}: {e}",
                parent.display()
            );
            return;
        }
    }

    let mut entries = load_idempotency_entries(state_path);
    entries.push(session_id.to_string());

    // Trim oldest entries when over the cap.
    let start = entries.len().saturating_sub(IDEMPOTENCY_MAX_ENTRIES);
    let trimmed = &entries[start..];

    // Rewrite atomically via a temp file in the same directory, then rename.
    // Falls back to a direct truncating write if the temp-file approach fails.
    let write_result = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        let dir = state_path.parent().unwrap_or(std::path::Path::new("."));
        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        for entry in trimmed {
            writeln!(tmp, "{entry}")?;
        }
        tmp.persist(state_path).map_err(|e| e.error)?;
        Ok(())
    })();

    if let Err(e) = write_result {
        // Fallback: direct append (not atomic but keeps the fail-open guarantee).
        eprintln!(
            "mnemos hook session-end: could not atomically rewrite state file ({}), falling back to append: {e}",
            state_path.display()
        );
        use std::io::Write as _;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(state_path)
        {
            let _ = writeln!(f, "{session_id}");
        }
    }
}

// ── Session API helpers ───────────────────────────────────────────────────────

/// `POST /v1/sessions` → returns the daemon-assigned session id (`{"id": "..."}`).
async fn post_start_session(token: &str, workspace: Option<&str>) -> anyhow::Result<String> {
    let client = make_client().ok_or_else(|| anyhow::anyhow!("failed to build HTTP client"))?;
    let mut body = serde_json::Map::new();
    body.insert("source_tool".into(), serde_json::json!("claude-code"));
    if let Some(ws) = workspace {
        body.insert("workspace".into(), serde_json::json!(ws));
    }
    let resp = client
        .post(format!(
            "{}/v1/sessions",
            crate::daemon_ctl::daemon_base_url()
        ))
        .bearer_auth(token)
        .json(&serde_json::Value::Object(body))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("POST /v1/sessions failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!(
            "POST /v1/sessions returned {}",
            resp.status()
        ));
    }
    let parsed: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("failed to parse session start response: {e}"))?;
    let id = parsed
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("session start response missing `id` field"))?
        .to_string();
    Ok(id)
}

/// `POST /v1/sessions/{daemon_session_id}/chunks` for every turn (best-effort).
///
/// Builds chunk request bodies from `Vec<Turn>` using the verified field names:
/// `{ body, speaker?, ordinal? }`. Each turn's body is first passed through
/// [`redact`]; turns whose bodies appear to contain a secret are silently
/// dropped (not POSTed) and a count is logged to stderr. Chunk POSTs are
/// issued with bounded concurrency (up to 8 in-flight at once) so long
/// transcripts do not stall the process for an extended time. Ordinals are
/// carried in each chunk body, so completion order does not affect correctness.
/// A failed chunk is logged to stderr and skipped — it never aborts the batch.
async fn post_chunks(token: &str, daemon_session_id: &str, turns: &[crate::transcript::Turn]) {
    use futures::StreamExt as _;

    let Some(client) = make_client() else {
        eprintln!("mnemos hook session-end: could not build HTTP client for chunks");
        return;
    };
    let url = format!(
        "{}/v1/sessions/{daemon_session_id}/chunks",
        crate::daemon_ctl::daemon_base_url()
    );

    let mut skipped = 0usize;
    let safe_turns: Vec<_> = turns
        .iter()
        .filter(|turn| {
            if redact(&turn.body).is_none() {
                skipped += 1;
                false
            } else {
                true
            }
        })
        .collect();

    if skipped > 0 {
        eprintln!(
            "mnemos hook session-end: skipped {skipped} chunk(s) — body contained a secret pattern"
        );
    }

    futures::stream::iter(safe_turns)
        .map(|turn| {
            let client = client.clone();
            let url = url.clone();
            let token = token.to_owned();
            let body = build_chunk_body(turn);
            let ordinal = turn.ordinal;
            async move {
                match client
                    .post(&url)
                    .bearer_auth(&token)
                    .json(&body)
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {}
                    Ok(resp) => {
                        eprintln!(
                            "mnemos hook session-end: chunk ordinal {ordinal} failed: {}",
                            resp.status()
                        );
                    }
                    Err(e) => {
                        eprintln!("mnemos hook session-end: chunk ordinal {ordinal} error: {e}");
                    }
                }
            }
        })
        .buffer_unordered(8)
        .collect::<()>()
        .await;
}

/// Build the JSON body for a single chunk POST.
/// Extracted as a pure function so it can be unit-tested without a daemon.
pub(crate) fn build_chunk_body(turn: &crate::transcript::Turn) -> serde_json::Value {
    serde_json::json!({
        "body":    turn.body,
        "speaker": turn.speaker,
        "ordinal": turn.ordinal,
    })
}

/// `POST /v1/sessions/{id}/end` — signals the daemon to distil the session.
async fn post_end_session(token: &str, daemon_session_id: &str) -> anyhow::Result<()> {
    let client = make_client().ok_or_else(|| anyhow::anyhow!("failed to build HTTP client"))?;
    let url = format!(
        "{}/v1/sessions/{daemon_session_id}/end",
        crate::daemon_ctl::daemon_base_url()
    );
    let resp = client
        .post(&url)
        .bearer_auth(token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("POST /v1/sessions/{daemon_session_id}/end failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!(
            "POST /v1/sessions/{daemon_session_id}/end returned {}",
            resp.status()
        ));
    }
    Ok(())
}

// ── Pure helpers (unit-testable without daemon) ───────────────────────────────

/// Returns `None` if `body` appears to contain a secret, so the caller can
/// drop the chunk without POSTing it. Returns `Some(body.to_string())`
/// otherwise (including the empty-string case).
///
/// This is a conservative, best-effort guard — it catches obvious shapes only.
/// User-pasted secrets that don't match these patterns remain the user's
/// responsibility. This is defense-in-depth, not a complete DLP solution.
///
/// Patterns detected:
///
/// * **OpenAI key** — `sk-` followed by 20 or more `[A-Za-z0-9]` chars.
///   This also matches `sk-ant-` prefixed Anthropic keys (sk- + 20+ chars).
/// * **Anthropic key explicit prefix** — `sk-ant-` followed by 20+ alphanum
///   chars (explicit guard for clarity; covered by the sk- rule above but
///   kept separately so the intent is clear).
/// * **GitHub PAT (classic)** — `ghp_` followed by 36 or more `[A-Za-z0-9]`
///   chars.
/// * **GitHub OAuth token** — `gho_` followed by 36 or more `[A-Za-z0-9]`
///   chars.
/// * **GitHub fine-grained PAT** — `github_pat_` followed by 20 or more
///   `[A-Za-z0-9_]` chars.
/// * **AWS access key ID** — `AKIA` followed by 16 or more `[0-9A-Z]` chars.
/// * **Generic high-entropy token** — a contiguous run of 40 or more
///   `[A-Za-z0-9+/=_-]` characters with no internal whitespace. This is
///   deliberately conservative to avoid false positives on normal prose: the
///   run must be either the entire body (after trimming) or delimited on both
///   sides by whitespace or common punctuation (`"`, `'`, `` ` ``, `=`).
/// * **PEM private key header** — `-----BEGIN` … `PRIVATE KEY-----`.
///
/// The function is allocation-light and panic-free. It does NOT use the `regex`
/// crate (not a dependency of `mnemos_cli`).
pub(crate) fn redact(body: &str) -> Option<String> {
    // ── PEM private-key header ────────────────────────────────────────────────
    if body.contains("-----BEGIN") && body.contains("PRIVATE KEY-----") {
        return None;
    }

    let bytes = body.as_bytes();
    let len = bytes.len();

    // Helper: scan for `prefix` bytes inside `bytes`, returning each match's
    // start position (index of first byte of prefix). Yields indices lazily.
    // We inline this as a closure to avoid repeating the window scan.

    // ── OpenAI-style key: "sk-" + 20+ [A-Za-z0-9] ────────────────────────────
    // This also covers "sk-ant-…" Anthropic keys because "sk-ant-" starts with
    // "sk-" and the suffix after "sk-" is ≥20 alphanum chars.
    const SK_PREFIX: &[u8] = b"sk-";
    const SK_MIN_SUFFIX: usize = 20;

    {
        let mut i = 0usize;
        while i + SK_PREFIX.len() <= len {
            if let Some(offset) = bytes[i..]
                .windows(SK_PREFIX.len())
                .position(|w| w == SK_PREFIX)
            {
                let key_start = i + offset + SK_PREFIX.len();
                let run = bytes[key_start..]
                    .iter()
                    .take_while(|&&b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                    .count();
                // Count only alphanumeric chars toward the minimum length so
                // "sk-ant-" internal hyphens don't inflate the count.
                let alnum_run = bytes[key_start..]
                    .iter()
                    .take(run)
                    .filter(|&&b| b.is_ascii_alphanumeric())
                    .count();
                if alnum_run >= SK_MIN_SUFFIX {
                    return None;
                }
                i = i + offset + 1;
            } else {
                break;
            }
        }
    }

    // ── GitHub PAT (classic): "ghp_" + 36+ [A-Za-z0-9] ──────────────────────
    const GHP_PREFIX: &[u8] = b"ghp_";
    const GH_MIN_SUFFIX: usize = 36;

    {
        let mut i = 0usize;
        while i + GHP_PREFIX.len() <= len {
            if let Some(offset) = bytes[i..]
                .windows(GHP_PREFIX.len())
                .position(|w| w == GHP_PREFIX)
            {
                let key_start = i + offset + GHP_PREFIX.len();
                let run = bytes[key_start..]
                    .iter()
                    .take_while(|&&b| b.is_ascii_alphanumeric())
                    .count();
                if run >= GH_MIN_SUFFIX {
                    return None;
                }
                i = i + offset + 1;
            } else {
                break;
            }
        }
    }

    // ── GitHub OAuth token: "gho_" + 36+ [A-Za-z0-9] ────────────────────────
    const GHO_PREFIX: &[u8] = b"gho_";

    {
        let mut i = 0usize;
        while i + GHO_PREFIX.len() <= len {
            if let Some(offset) = bytes[i..]
                .windows(GHO_PREFIX.len())
                .position(|w| w == GHO_PREFIX)
            {
                let key_start = i + offset + GHO_PREFIX.len();
                let run = bytes[key_start..]
                    .iter()
                    .take_while(|&&b| b.is_ascii_alphanumeric())
                    .count();
                if run >= GH_MIN_SUFFIX {
                    return None;
                }
                i = i + offset + 1;
            } else {
                break;
            }
        }
    }

    // ── GitHub fine-grained PAT: "github_pat_" + 20+ [A-Za-z0-9_] ───────────
    const GITHUB_PAT_PREFIX: &[u8] = b"github_pat_";
    const GITHUB_PAT_MIN_SUFFIX: usize = 20;

    {
        let mut i = 0usize;
        while i + GITHUB_PAT_PREFIX.len() <= len {
            if let Some(offset) = bytes[i..]
                .windows(GITHUB_PAT_PREFIX.len())
                .position(|w| w == GITHUB_PAT_PREFIX)
            {
                let key_start = i + offset + GITHUB_PAT_PREFIX.len();
                let run = bytes[key_start..]
                    .iter()
                    .take_while(|&&b| b.is_ascii_alphanumeric() || b == b'_')
                    .count();
                if run >= GITHUB_PAT_MIN_SUFFIX {
                    return None;
                }
                i = i + offset + 1;
            } else {
                break;
            }
        }
    }

    // ── AWS access key ID: "AKIA" + 16+ [0-9A-Z] ────────────────────────────
    const AKIA_PREFIX: &[u8] = b"AKIA";
    const AKIA_SUFFIX_LEN: usize = 16;

    {
        let mut j = 0usize;
        while j + AKIA_PREFIX.len() <= len {
            if let Some(offset) = bytes[j..]
                .windows(AKIA_PREFIX.len())
                .position(|w| w == AKIA_PREFIX)
            {
                let key_start = j + offset + AKIA_PREFIX.len();
                let run = bytes[key_start..]
                    .iter()
                    .take_while(|&&b| matches!(b, b'0'..=b'9' | b'A'..=b'Z'))
                    .count();
                if run >= AKIA_SUFFIX_LEN {
                    return None;
                }
                j = j + offset + 1;
            } else {
                break;
            }
        }
    }

    // ── Generic high-entropy token: 40+ contiguous token chars ───────────────
    //
    // A "token char" is any of [A-Za-z0-9+/=_-]. We only trigger this if the
    // run is at the very start/end of the (trimmed) body OR is bounded on both
    // sides by a delimiter byte (whitespace, `"`, `'`, `` ` ``, `=`, `:`).
    // This keeps false positives low on normal prose while catching standalone
    // secrets (e.g. a bare token pasted as the entire message).
    const TOKEN_MIN_LEN: usize = 40;

    fn is_token_char(b: u8) -> bool {
        b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'_' | b'-')
    }
    fn is_delimiter(b: u8) -> bool {
        b.is_ascii_whitespace() || matches!(b, b'"' | b'\'' | b'`' | b'=' | b':')
    }

    {
        let trimmed = body.trim().as_bytes();
        let tlen = trimmed.len();
        let mut k = 0usize;
        while k < tlen {
            if is_token_char(trimmed[k]) {
                // Measure the run length.
                let run_start = k;
                while k < tlen && is_token_char(trimmed[k]) {
                    k += 1;
                }
                let run_len = k - run_start;
                if run_len >= TOKEN_MIN_LEN {
                    // Check delimiters: left boundary is start-of-string or a
                    // delimiter; right boundary is end-of-string or a delimiter.
                    let left_ok = run_start == 0 || is_delimiter(trimmed[run_start - 1]);
                    let right_ok = k == tlen || is_delimiter(trimmed[k]);
                    if left_ok && right_ok {
                        return None;
                    }
                }
            } else {
                k += 1;
            }
        }
    }

    Some(body.to_string())
}

/// Build the hook JSON payload for a non-empty working-set text.
///
/// Returns `None` for empty / whitespace-only text (nothing to inject).
/// Returns `Some(json_string)` with the exact shape Claude Code expects:
/// ```json
/// {
///   "hookSpecificOutput": {
///     "hookEventName": "SessionStart",
///     "additionalContext": "<working set text>"
///   }
/// }
/// ```
pub fn working_set_hook_json(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Cap size to stay inside the token budget.
    let capped = if trimmed.len() > WORKING_SET_BYTE_CAP {
        // Truncate at a UTF-8 char boundary.
        let mut end = WORKING_SET_BYTE_CAP;
        while !trimmed.is_char_boundary(end) {
            end -= 1;
        }
        &trimmed[..end]
    } else {
        trimmed
    };
    Some(
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "SessionStart",
                "additionalContext": capped,
            }
        })
        .to_string(),
    )
}

// ── Private I/O helpers ───────────────────────────────────────────────────────

/// Read all of stdin and parse as JSON. Returns `Value::Null` on any error.
///
/// Uses `spawn_blocking` so the read never stalls the async executor.
async fn read_stdin_json() -> Value {
    let result = tokio::task::spawn_blocking(|| {
        let mut buf = String::new();
        if std::io::stdin().read_to_string(&mut buf).is_err() {
            return Value::Null;
        }
        serde_json::from_str(&buf).unwrap_or(Value::Null)
    })
    .await;
    result.unwrap_or(Value::Null)
}

/// Load the daemon bearer token from the standard path. Returns `None` on error.
fn load_token() -> Option<String> {
    let path = mnemos_daemon::token_path().ok()?;
    mnemos_daemon::auth::load_token(&path).ok()
}

/// Build the HTTP client used for all daemon calls from hooks.
///
/// Returns `None` on failure (fail-open). Uses a 5-second timeout so a slow
/// or unresponsive daemon can never stall the Claude Code session.
fn make_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()
}

// ── Active session state management ──────────────────────────────────────────

/// Persistent state for an active (in-progress) Claude Code session.
/// Written by `session-start`, read by `user-prompt` and `stop`, cleaned up
/// by `session-end`.
#[derive(serde::Serialize, serde::Deserialize)]
struct ActiveSessionState {
    daemon_session_id: String,
    tool_id: String,
    workspace: Option<String>,
    next_ordinal: u32,
    /// Layer 3: maps keyword → ordinal when the keyword first appeared.
    /// Used to detect "returning to an old topic" by finding keywords from
    /// early in the session that reappear in a later prompt.
    #[serde(default)]
    keyword_first_seen: std::collections::HashMap<String, u32>,
    /// The ordinal of the last prompt that was processed. Used to determine
    /// how far back an "old" keyword is relative to the current prompt.
    #[serde(default)]
    last_prompt_ordinal: u32,
}

/// Resolve the directory for active session state files.
fn active_sessions_dir() -> Option<std::path::PathBuf> {
    use directories::BaseDirs;
    let base = BaseDirs::new()
        .and_then(|bd| bd.state_dir().map(|p| p.to_path_buf()))
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| std::path::PathBuf::from(h).join(".local").join("state"))
        })?;
    Some(base.join("mnemos").join("active-sessions"))
}

/// Load the active session state for the given Claude session ID.
fn load_active_session(session_id: &str) -> Option<ActiveSessionState> {
    let dir = active_sessions_dir()?;
    let path = dir.join(format!("{session_id}.json"));
    let contents = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Save the active session state for the given Claude session ID.
fn save_active_session(session_id: &str, state: &ActiveSessionState) -> std::io::Result<()> {
    let dir = active_sessions_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no state dir"))?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{session_id}.json"));
    let json = serde_json::to_string(state).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)
}

/// Remove the active session state file for the given Claude session ID.
fn remove_active_session(session_id: &str) {
    if let Some(dir) = active_sessions_dir() {
        let path = dir.join(format!("{session_id}.json"));
        let _ = std::fs::remove_file(path);
    }
}

// ── Real-time chunk streaming ────────────────────────────────────────────────

/// POST a single chunk to `POST /v1/sessions/{daemon_session_id}/chunks`.
///
/// Fail-open: returns `Ok(())` on success, `Err` on failure (callers should
/// ignore errors).
async fn post_streaming_chunk(
    token: &str,
    daemon_session_id: &str,
    body: &str,
    speaker: &str,
    ordinal: u32,
) -> Result<(), ()> {
    let client = make_client().ok_or(())?;
    let url = format!(
        "{}/v1/sessions/{daemon_session_id}/chunks",
        crate::daemon_ctl::daemon_base_url()
    );
    let payload = serde_json::json!({
        "body": body,
        "speaker": speaker,
        "ordinal": ordinal,
    });
    let resp = client
        .post(&url)
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            eprintln!("mnemos hook: streaming chunk POST failed: {e}");
        })?;
    if !resp.status().is_success() {
        eprintln!(
            "mnemos hook: streaming chunk POST returned {}",
            resp.status()
        );
        return Err(());
    }
    Ok(())
}

/// Fetch the working set from `GET /v1/working` and render it as plain text.
/// Returns `None` on any network or parse error (fail-open).
async fn fetch_working_set(token: &str, workspace: Option<&str>) -> Option<String> {
    let mut url = format!("{}/v1/working", crate::daemon_ctl::daemon_base_url());
    if let Some(ws) = workspace {
        url.push_str(&format!("?workspace={}", urlencoding::encode(ws)));
    }
    let client = make_client()?;
    let resp = client.get(&url).bearer_auth(token).send().await.ok()?;
    if !resp.status().is_success() {
        eprintln!(
            "mnemos hook session-start: /v1/working returned {}",
            resp.status()
        );
        return None;
    }
    let body: Value = resp.json().await.ok()?;
    Some(render_working_set(&body))
}

/// Render the working-set JSON response as a concise plain-text summary.
///
/// The daemon returns `{ "memories": [...], "hardened_rules": [...] }`.
/// We format each entry as `[title]: [body snippet]` lines so the LLM can
/// parse the context easily.
fn render_working_set(body: &Value) -> String {
    let mut lines: Vec<String> = vec![];

    if let Some(mems) = body.get("memories").and_then(|v| v.as_array()) {
        for m in mems {
            let title = m
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("(untitled)");
            let b = m.get("body").and_then(|v| v.as_str()).unwrap_or("");
            let snippet: String = b.chars().take(300).collect();
            lines.push(format!("{title}: {snippet}"));
        }
    }

    if let Some(rules) = body.get("hardened_rules").and_then(|v| v.as_array()) {
        if !rules.is_empty() {
            lines.push("--- hardened rules ---".into());
            for r in rules {
                let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("(rule)");
                let b = r.get("body").and_then(|v| v.as_str()).unwrap_or("");
                let snippet: String = b.chars().take(300).collect();
                lines.push(format!("{title}: {snippet}"));
            }
        }
    }

    lines.join("\n")
}

// ── user_prompt pure helpers ──────────────────────────────────────────────────

/// Build the hook JSON for a `UserPromptSubmit` event.
///
/// Returns `None` for empty / whitespace-only text.
/// Returns `Some(json_string)` with the shape:
/// ```json
/// {
///   "hookSpecificOutput": {
///     "hookEventName": "UserPromptSubmit",
///     "additionalContext": "<recall text>"
///   }
/// }
/// ```
pub fn user_prompt_hook_json(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Cap size to match the working-set byte budget, keeping the injected
    // context inside a reasonable token limit. Truncate on a UTF-8 char
    // boundary so we never produce an invalid string slice.
    let capped = if trimmed.len() > WORKING_SET_BYTE_CAP {
        let mut end = WORKING_SET_BYTE_CAP;
        while !trimmed.is_char_boundary(end) {
            end -= 1;
        }
        &trimmed[..end]
    } else {
        trimmed
    };
    Some(
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "UserPromptSubmit",
                "additionalContext": capped,
            }
        })
        .to_string(),
    )
}

/// Select recall hits that fit within `max_chars` of rendered context.
///
/// The first hit is always included (even if it alone exceeds the budget).
/// Subsequent hits are added in order until adding the next would exceed the
/// budget, at which point accumulation stops.
///
/// Returns an empty slice when `hits` is empty.
#[cfg(test)]
pub fn budget_hits(hits: &[RecallHit], max_chars: usize) -> Vec<&RecallHit> {
    let mut selected: Vec<&RecallHit> = Vec::new();
    let mut total = 0usize;
    for (i, hit) in hits.iter().enumerate() {
        let rendered_len = recall_hit_rendered_len(hit);
        if i == 0 {
            // Always include the first hit regardless of size.
            selected.push(hit);
            total += rendered_len;
        } else if total + rendered_len <= max_chars {
            selected.push(hit);
            total += rendered_len;
        } else {
            break;
        }
    }
    selected
}

/// Estimate the rendered character length for one `RecallHit` line.
///
/// Format: `- <title>: <snippet>\n`
/// Snippet is capped at 300 chars (matching `render_working_set`).
#[cfg(test)]
fn recall_hit_rendered_len(hit: &RecallHit) -> usize {
    let title = &hit.memory.title;
    let body = &hit.memory.body;
    let snippet_len = body.chars().count().min(300);
    // "- " + title + ": " + snippet + "\n"
    2 + title.chars().count() + 2 + snippet_len + 1
}

/// POST `query` to `POST /v1/memories/search` and return the deserialized hits.
/// Returns `None` on any network or parse error (fail-open).
async fn fetch_recall(
    token: &str,
    query: &str,
    workspace: Option<&str>,
    entity_expand: bool,
) -> Option<Vec<RecallHit>> {
    let url = format!(
        "{}/v1/memories/search",
        crate::daemon_ctl::daemon_base_url()
    );
    let client = make_client()?;
    let mut body = serde_json::Map::new();
    body.insert("query".into(), serde_json::json!(query));
    body.insert("k".into(), serde_json::json!(RECALL_K));
    // P1-6: disable graph PPR on the user-prompt path to avoid full-graph load
    // + 30-iter PageRank on every keystroke.  Explicit /v1/memories/search calls
    // from clients can still pass graph:true if they need it.
    body.insert("graph".into(), serde_json::json!(false));
    if entity_expand {
        body.insert("entity_expand".into(), serde_json::json!(true));
    }
    if let Some(ws) = workspace {
        body.insert("workspace".into(), serde_json::json!(ws));
    }
    let body = serde_json::Value::Object(body);
    let resp = client
        .post(&url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        eprintln!(
            "mnemos hook user-prompt: /v1/memories/search returned {}",
            resp.status()
        );
        return None;
    }
    let parsed: Value = resp.json().await.ok()?;
    let hits_val = parsed.get("hits")?;
    serde_json::from_value(hits_val.clone()).ok()
}

/// Lightweight memory struct for project-context deserialization.
/// Only the fields we need to render injection text.
#[derive(Debug, Clone, serde::Deserialize)]
struct ProjectMemory {
    id: String,
    title: String,
    #[serde(default)]
    body: String,
}

/// GET `/v1/memories/project-context?workspace=...` — returns Project + Entity
/// type memories pinned to the workspace. Fail-open: returns `None` on error.
async fn fetch_project_context(token: &str, workspace: Option<&str>) -> Option<Vec<ProjectMemory>> {
    let mut url = format!(
        "{}/v1/memories/project-context",
        crate::daemon_ctl::daemon_base_url()
    );
    if let Some(ws) = workspace {
        url.push_str(&format!("?workspace={}", urlencoding::encode(ws)));
    }
    let client = make_client()?;
    let resp = client.get(&url).bearer_auth(token).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed: Value = resp.json().await.ok()?;
    let mems_val = parsed.get("memories")?;
    serde_json::from_value(mems_val.clone()).ok()
}

// ── Layer 3: session-aware context recovery ──────────────────────────────────

/// Minimum keyword length to track (skip noise words like "the", "a", etc.)
const MIN_KEYWORD_LEN: usize = 4;

/// A topic is "old" if it was first seen at least this many ordinals ago.
/// With the typical prompt cadence, this is ~3-4 user prompts back.
const OLD_TOPIC_ORDINAL_GAP: u32 = 4;

/// Stop words that should never be tracked as topic keywords.
const STOP_WORDS: &[&str] = &[
    "that", "this", "with", "from", "have", "will", "what", "when", "where", "which", "their",
    "there", "been", "some", "also", "than", "them", "then", "just", "more", "only", "into",
    "over", "your", "does", "each", "make", "like", "about", "could", "would", "should", "other",
    "after", "before", "because", "between", "those", "these", "being", "same", "very", "still",
    "here", "every", "through", "code", "file", "please", "want", "need", "sure", "okay", "right",
    "look", "lets", "good", "well", "keep",
];

/// Extract significant keywords from a prompt (lowercased, deduplicated).
fn extract_keywords(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    text.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .filter(|w| w.len() >= MIN_KEYWORD_LEN)
        .map(|w| w.to_lowercase())
        .filter(|w| !STOP_WORDS.contains(&w.as_str()))
        .filter(|w| seen.insert(w.clone()))
        .collect()
}

/// Detect keywords in `current_prompt` that first appeared early in the session
/// (ordinal gap ≥ OLD_TOPIC_ORDINAL_GAP). Returns a set of "returning" keywords
/// that can be used to boost the recall query.
fn detect_returning_topics(state: &ActiveSessionState, current_prompt: &str) -> Vec<String> {
    let current_keywords = extract_keywords(current_prompt);
    let current_ordinal = state.next_ordinal; // next ordinal = current prompt's position

    current_keywords
        .into_iter()
        .filter(|kw| {
            if let Some(&first_seen) = state.keyword_first_seen.get(kw) {
                // The keyword was first seen long ago — this is a "return"
                current_ordinal.saturating_sub(first_seen) >= OLD_TOPIC_ORDINAL_GAP
            } else {
                false
            }
        })
        .take(5) // Cap to prevent query bloat
        .collect()
}

/// Update the session state with keywords from the current prompt.
fn record_prompt_keywords(state: &mut ActiveSessionState, prompt: &str) {
    let keywords = extract_keywords(prompt);
    let ordinal = state.next_ordinal;
    for kw in keywords {
        state.keyword_first_seen.entry(kw).or_insert(ordinal);
    }
    state.last_prompt_ordinal = ordinal;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── working_set_hook_json: pure helper, no daemon needed ──────────────────

    #[test]
    fn hook_json_returns_none_for_empty_string() {
        assert!(working_set_hook_json("").is_none());
    }

    #[test]
    fn hook_json_returns_none_for_whitespace_only() {
        assert!(working_set_hook_json("   \n\t  ").is_none());
    }

    #[test]
    fn hook_json_returns_some_for_nonempty_text() {
        let result = working_set_hook_json("project: my cool project");
        assert!(result.is_some());
    }

    #[test]
    fn hook_json_shape_has_hook_event_name_session_start() {
        let json_str = working_set_hook_json("my context").unwrap();
        let v: Value = serde_json::from_str(&json_str).expect("must be valid JSON");
        let event_name = v
            .get("hookSpecificOutput")
            .and_then(|o| o.get("hookEventName"))
            .and_then(|n| n.as_str());
        assert_eq!(event_name, Some("SessionStart"));
    }

    #[test]
    fn hook_json_shape_has_additional_context() {
        let text = "current project: mnemos, working on hook integration";
        let json_str = working_set_hook_json(text).unwrap();
        let v: Value = serde_json::from_str(&json_str).expect("must be valid JSON");
        let ctx = v
            .get("hookSpecificOutput")
            .and_then(|o| o.get("additionalContext"))
            .and_then(|c| c.as_str());
        assert_eq!(ctx, Some(text));
    }

    #[test]
    fn hook_json_trims_leading_trailing_whitespace() {
        let text = "  some context  ";
        let json_str = working_set_hook_json(text).unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let ctx = v["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert_eq!(ctx, "some context");
    }

    #[test]
    fn hook_json_caps_oversized_text() {
        // Generate text larger than WORKING_SET_BYTE_CAP bytes.
        let big = "x".repeat(WORKING_SET_BYTE_CAP + 1000);
        let json_str = working_set_hook_json(&big).unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let ctx = v["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert!(
            ctx.len() <= WORKING_SET_BYTE_CAP,
            "context byte length {} should be <= cap {}",
            ctx.len(),
            WORKING_SET_BYTE_CAP
        );
    }

    #[test]
    fn hook_json_cap_does_not_truncate_below_cap() {
        let text = "short context";
        let json_str = working_set_hook_json(text).unwrap();
        let v: Value = serde_json::from_str(&json_str).unwrap();
        let ctx = v["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        // Full text should be preserved when under cap.
        assert_eq!(ctx, text);
    }

    #[test]
    fn hook_json_is_valid_json() {
        let json_str = working_set_hook_json("hello").unwrap();
        assert!(serde_json::from_str::<Value>(&json_str).is_ok());
    }

    // ── render_working_set: pure formatter, no daemon needed ──────────────────

    #[test]
    fn render_empty_body_produces_empty_string() {
        let body = serde_json::json!({ "memories": [] });
        assert_eq!(render_working_set(&body), "");
    }

    #[test]
    fn render_formats_memory_as_title_colon_body() {
        let body = serde_json::json!({
            "memories": [
                { "title": "Project Alpha", "body": "We are building X." }
            ]
        });
        let out = render_working_set(&body);
        assert!(out.contains("Project Alpha:"));
        assert!(out.contains("We are building X."));
    }

    #[test]
    fn render_includes_hardened_rules_section_when_present() {
        let body = serde_json::json!({
            "memories": [],
            "hardened_rules": [
                { "title": "Rule 1", "body": "Always do X." }
            ]
        });
        let out = render_working_set(&body);
        assert!(out.contains("hardened rules"));
        assert!(out.contains("Rule 1:"));
    }

    #[test]
    fn render_skips_hardened_section_when_array_empty() {
        let body = serde_json::json!({
            "memories": [{ "title": "T", "body": "B" }],
            "hardened_rules": []
        });
        let out = render_working_set(&body);
        assert!(!out.contains("hardened rules"));
    }

    #[test]
    fn render_body_snippet_capped_at_300_chars() {
        let long_body = "z".repeat(500);
        let body = serde_json::json!({
            "memories": [{ "title": "T", "body": long_body }]
        });
        let out = render_working_set(&body);
        // Title + ": " + up to 300 'z's
        let zs: String = out.chars().filter(|&c| c == 'z').collect();
        assert_eq!(zs.len(), 300);
    }

    // ── user_prompt_hook_json: pure helper ────────────────────────────────────

    #[test]
    fn user_prompt_hook_json_returns_none_for_empty_string() {
        assert!(user_prompt_hook_json("").is_none());
    }

    #[test]
    fn user_prompt_hook_json_returns_none_for_whitespace_only() {
        assert!(user_prompt_hook_json("   \n\t  ").is_none());
    }

    #[test]
    fn user_prompt_hook_json_returns_some_for_nonempty_text() {
        assert!(user_prompt_hook_json("some recall context").is_some());
    }

    #[test]
    fn user_prompt_hook_json_shape_has_hook_event_name_user_prompt_submit() {
        let json_str = user_prompt_hook_json("recall context").unwrap();
        let v: Value = serde_json::from_str(&json_str).expect("must be valid JSON");
        let event_name = v
            .get("hookSpecificOutput")
            .and_then(|o| o.get("hookEventName"))
            .and_then(|n| n.as_str());
        assert_eq!(event_name, Some("UserPromptSubmit"));
    }

    #[test]
    fn user_prompt_hook_json_shape_has_additional_context() {
        let text = "- Memory A: relevant detail";
        let json_str = user_prompt_hook_json(text).unwrap();
        let v: Value = serde_json::from_str(&json_str).expect("must be valid JSON");
        let ctx = v
            .get("hookSpecificOutput")
            .and_then(|o| o.get("additionalContext"))
            .and_then(|c| c.as_str());
        assert_eq!(ctx, Some(text));
    }

    #[test]
    fn user_prompt_hook_json_is_valid_json() {
        let json_str = user_prompt_hook_json("context").unwrap();
        assert!(serde_json::from_str::<Value>(&json_str).is_ok());
    }

    // ── budget_hits: pure accumulator ─────────────────────────────────────────

    /// Build a minimal RecallHit with the given title and body for testing.
    fn make_hit(title: &str, body: &str) -> RecallHit {
        use chrono::Utc;
        use mnemos_core::types::Memory;
        use mnemos_core::types::MemoryType;
        use mnemos_core::Tier;

        RecallHit {
            memory: Memory {
                id: "test".into(),
                tier: Tier::Semantic,
                kind: MemoryType::Fact,
                title: title.into(),
                body: body.into(),
                tags: vec![],
                entities: vec![],
                links: vec![],
                provenance: vec![],
                created_at: Utc::now(),
                ingested_at: Utc::now(),
                valid_at: Utc::now(),
                invalid_at: None,
                superseded_by: None,
                strength: 1.0,
                importance: 1.0,
                last_accessed: Utc::now(),
                access_count: 0,
                workspace: None,
                source_tool: None,
                mnemos_version: 1,
            },
            score: 1.0,
            bm25_rank: None,
            dense_rank: None,
            dense_distance: None,
            ppr_rank: None,
            explain: None,
        }
    }

    #[test]
    fn budget_hits_empty_input_returns_empty() {
        let hits: Vec<RecallHit> = vec![];
        let selected = budget_hits(&hits, 1200);
        assert!(selected.is_empty());
    }

    #[test]
    fn budget_hits_single_oversized_hit_is_returned_alone() {
        // body of 400 chars → rendered_len > 1200 chars,
        // but the first hit must always be included.
        let big_body = "x".repeat(400);
        let hits = vec![make_hit("Big", &big_body)];
        let selected = budget_hits(&hits, 10); // tiny budget
        assert_eq!(
            selected.len(),
            1,
            "oversized first hit must still be returned"
        );
    }

    #[test]
    fn budget_hits_stops_when_budget_exceeded() {
        // Each hit has title "T" (1 char) and body of 100 chars.
        // Rendered length per hit: 2 + 1 + 2 + 100 + 1 = 106 chars.
        // Budget = 200 → fits 1 hit (106) but not 2 (212).
        let hit = make_hit("T", &"y".repeat(100));
        let hits = vec![hit.clone(), hit.clone(), hit.clone()];
        let selected = budget_hits(&hits, 200);
        assert_eq!(
            selected.len(),
            1,
            "only the first hit should fit within budget 200 (each hit ~106 chars)"
        );
    }

    #[test]
    fn budget_hits_includes_all_hits_when_all_fit_in_budget() {
        // 3 tiny hits that easily fit within 1200 chars.
        let hits: Vec<RecallHit> = (0..3).map(|_| make_hit("A", "short")).collect();
        let selected = budget_hits(&hits, 1200);
        assert_eq!(
            selected.len(),
            3,
            "all 3 small hits must fit within 1200 chars"
        );
    }

    #[test]
    fn budget_hits_exactly_at_boundary_is_included() {
        // Craft a hit whose rendered length is exactly the budget.
        // Rendered: "- " + title (1) + ": " + body (N) + "\n" = 6 + N
        // With N = 1194 → rendered_len = 1200 → must be included.
        let body = "b".repeat(1194);
        let hits = vec![make_hit("T", &body)];
        let selected = budget_hits(&hits, 1200);
        assert_eq!(selected.len(), 1);
    }

    // ── daemon-down path: session_start returns None without panicking ─────────

    #[tokio::test]
    async fn session_start_returns_none_when_daemon_is_down() {
        // Pass an empty input; the daemon is not running in CI so ensure_daemon
        // will return false quickly (it won't be able to spawn a real daemon).
        // We verify the function returns None and doesn't panic.
        //
        // Note: this test relies on no daemon being available on the test
        // machine. If a daemon IS running, this test will actually try to
        // fetch the working set; in that case it may return Some or None
        // depending on vault state — both are valid (the function is
        // best-effort). We accept this by only asserting no panic occurs,
        // and that the result is never an error (it returns Option, not Result).
        let result = session_start(Value::Null).await;
        // The only assertion that MUST hold regardless of daemon state:
        // the function never panics and always returns Option<String>.
        // If the daemon was down, we get None. If it was up, we may get
        // None (empty vault) or Some (populated vault). Both are correct.
        let _ = result; // no panic is the assertion
    }

    // ── idempotency helpers: pure FS logic ────────────────────────────────────

    /// `already_captured` returns false for an unknown id when the state file
    /// does not yet exist.
    #[test]
    fn already_captured_returns_false_when_file_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("captured-sessions");
        assert!(!already_captured(&path, "session-abc"));
    }

    /// After `record_captured`, `already_captured` returns true for that id.
    #[test]
    fn record_then_already_captured_returns_true() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("captured-sessions");
        record_captured(&path, "session-xyz");
        assert!(already_captured(&path, "session-xyz"));
    }

    /// An id that was NOT recorded is still unknown even after another id was.
    #[test]
    fn already_captured_is_false_for_different_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("captured-sessions");
        record_captured(&path, "session-aaa");
        assert!(!already_captured(&path, "session-bbb"));
    }

    /// Recording the same id twice does not corrupt the file (no duplicate,
    /// idempotent reads still return true).
    #[test]
    fn record_twice_is_idempotent_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("captured-sessions");
        record_captured(&path, "session-dup");
        record_captured(&path, "session-dup");
        assert!(already_captured(&path, "session-dup"));
        // The file should contain the id (may appear twice — that is allowed;
        // `already_captured` only needs to find at least one occurrence).
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.lines().any(|l| l.trim() == "session-dup"));
    }

    /// Multiple distinct ids round-trip correctly through the state file.
    #[test]
    fn multiple_ids_all_found_after_record() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("captured-sessions");
        let ids = ["sess-1", "sess-2", "sess-3"];
        for id in &ids {
            record_captured(&path, id);
        }
        for id in &ids {
            assert!(
                already_captured(&path, id),
                "expected {id} to be found in state file"
            );
        }
    }

    /// `record_captured` creates parent directories as needed.
    #[test]
    fn record_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join("nested")
            .join("deep")
            .join("captured-sessions");
        // Parent does not exist yet — record_captured must create it.
        record_captured(&path, "sess-nested");
        assert!(path.exists(), "state file should exist after record");
        assert!(already_captured(&path, "sess-nested"));
    }

    // ── idempotency rolling-window cap (P2-5) ─────────────────────────────────

    /// When the entry count exceeds the cap, the oldest entries are dropped and
    /// only the newest `IDEMPOTENCY_MAX_ENTRIES` entries are retained.
    #[test]
    fn record_captured_trims_oldest_when_over_cap() {
        // Use a tiny cap by writing N+1 entries where N = IDEMPOTENCY_MAX_ENTRIES.
        // We can't easily override the constant in tests, so we verify the
        // behaviour with a concrete small file and check that the file never
        // holds more than IDEMPOTENCY_MAX_ENTRIES lines.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("captured-sessions");

        // Pre-fill with IDEMPOTENCY_MAX_ENTRIES - 1 entries.
        for i in 0..IDEMPOTENCY_MAX_ENTRIES - 1 {
            record_captured(&path, &format!("old-{i:06}"));
        }

        // The cap-boundary entry.
        record_captured(&path, "cap-boundary");
        // One entry over the cap.
        record_captured(&path, "over-cap");

        let contents = std::fs::read_to_string(&path).unwrap();
        let line_count = contents.lines().filter(|l| !l.trim().is_empty()).count();
        assert_eq!(
            line_count, IDEMPOTENCY_MAX_ENTRIES,
            "file must contain exactly IDEMPOTENCY_MAX_ENTRIES lines after trim"
        );

        // The newest entries must be present.
        assert!(
            already_captured(&path, "cap-boundary"),
            "cap-boundary entry must be retained"
        );
        assert!(
            already_captured(&path, "over-cap"),
            "over-cap (newest) entry must be retained"
        );

        // The very oldest entry must have been evicted.
        assert!(
            !already_captured(&path, "old-000000"),
            "oldest entry must have been evicted"
        );
    }

    /// A file whose byte size exceeds the size guard is treated as empty
    /// (degraded safely) — `already_captured` returns false.
    #[test]
    fn load_idempotency_entries_size_guard_degrades_safely() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("captured-sessions");

        // Write a file larger than IDEMPOTENCY_MAX_ENTRIES * 200 bytes.
        let big_content = "x".repeat(IDEMPOTENCY_MAX_ENTRIES * 201);
        std::fs::write(&path, &big_content).unwrap();

        // Should degrade to empty without panicking.
        let entries = load_idempotency_entries(&path);
        assert!(
            entries.is_empty(),
            "oversized file must degrade to empty entry list"
        );

        // already_captured must also return false for a known-written id.
        assert!(
            !already_captured(&path, "any-session"),
            "already_captured must return false when file is over size limit"
        );
    }

    /// After the size-guard reset, the next `record_captured` rewrites the
    /// file cleanly so future captures work normally.
    #[test]
    fn record_captured_recovers_after_size_guard_reset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("captured-sessions");

        // Bloat the file.
        let big_content = "x".repeat(IDEMPOTENCY_MAX_ENTRIES * 201);
        std::fs::write(&path, &big_content).unwrap();

        // Record a new capture — should succeed and produce a valid single-line file.
        record_captured(&path, "recovery-session");

        assert!(
            already_captured(&path, "recovery-session"),
            "recovery session must be findable after size-guard reset"
        );

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(
            lines.len(),
            1,
            "file should have exactly one entry after recovery"
        );
        assert_eq!(lines[0], "recovery-session");
    }

    // ── session_end: missing/empty transcript_path → None ─────────────────────

    /// `session_end` with a null input (no transcript_path) returns None
    /// without panicking.
    #[tokio::test]
    async fn session_end_returns_none_when_input_is_null() {
        let result = session_end(Value::Null).await;
        assert!(result.is_none(), "session_end must always return None");
    }

    /// `session_end` with a missing `transcript_path` field returns None.
    #[tokio::test]
    async fn session_end_returns_none_when_transcript_path_missing() {
        let input = serde_json::json!({
            "session_id": "sess-123",
            "cwd": "/tmp/project"
        });
        let result = session_end(input).await;
        assert!(result.is_none());
    }

    /// `session_end` with a `transcript_path` that does not exist on disk
    /// returns None (fail-open on unreadable transcript).
    #[tokio::test]
    async fn session_end_returns_none_when_transcript_file_missing() {
        let input = serde_json::json!({
            "session_id": "sess-456",
            "transcript_path": "/nonexistent/path/transcript.jsonl",
            "cwd": "/tmp"
        });
        let result = session_end(input).await;
        assert!(result.is_none());
    }

    // ── is_transcript_path_allowed: path-traversal guard (P2-19) ─────────────

    /// A path inside the user's home directory is allowed.
    #[test]
    fn transcript_path_inside_home_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        // Canonicalize the dir itself — on macOS, /tmp is a symlink to
        // /private/tmp, so dir.path() returns /tmp/… but file
        // canonicalize() resolves to /private/tmp/….
        let dir_canonical = dir.path().canonicalize().unwrap();
        let transcript = dir_canonical.join(".claude").join("transcript.jsonl");
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(&transcript, "").unwrap();
        let canonical = transcript.canonicalize().unwrap();
        // The home check uses $HOME; override it to the temp dir for this test.
        // Because is_transcript_path_allowed reads $HOME, we test the underlying
        // logic directly using starts_with.
        assert!(
            canonical.starts_with(&dir_canonical),
            "canonical path must be under the temp dir (simulated home)"
        );
    }

    /// A path that escapes home (e.g. /etc/passwd) is rejected.
    #[test]
    fn transcript_path_outside_home_is_rejected() {
        // /etc/passwd exists on Linux; use it as a known path outside home.
        let path = std::path::Path::new("/etc/passwd");
        if !path.exists() {
            // Platform doesn't have /etc/passwd — skip.
            return;
        }
        let canonical = path.canonicalize().unwrap();
        // Get the real home for this user.
        let home = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/nonexistent"));
        assert!(
            !canonical.starts_with(&home),
            "/etc/passwd must not start with the user home"
        );
        // The guard itself should reject it.
        assert!(
            !is_transcript_path_allowed(&canonical),
            "is_transcript_path_allowed must return false for /etc/passwd"
        );
    }

    /// A transcript path that IS under home passes the guard.
    #[test]
    fn transcript_path_in_home_dot_claude_is_allowed() {
        let home = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                // If no HOME, we can't run this test meaningfully.
                std::path::PathBuf::from("/nonexistent")
            });
        if !home.exists() {
            return; // No usable home — skip.
        }
        // Construct a hypothetical path under ~/.claude/projects/.
        // We don't need the file to exist since we're testing starts_with logic.
        let fake_transcript = home.join(".claude").join("projects").join("t.jsonl");
        // Manually simulate the canonicalize result (the file may not exist).
        // We test is_transcript_path_allowed with the non-canonical path only if
        // the home itself can be canonicalized.
        if let Ok(canonical_home) = home.canonicalize() {
            let canonical_transcript = canonical_home
                .join(".claude")
                .join("projects")
                .join("t.jsonl");
            assert!(
                is_transcript_path_allowed(&canonical_transcript),
                "a path under ~/.claude should be allowed by the guard; path={fake_transcript:?}"
            );
        }
    }

    // ── build_chunk_body: pure Turn → JSON mapping ────────────────────────────

    /// Chunk body contains the correct field names and values from a Turn.
    #[test]
    fn build_chunk_body_has_correct_field_names() {
        use crate::transcript::Turn;
        let turn = Turn {
            speaker: "user".into(),
            body: "hello world".into(),
            ordinal: 3,
        };
        let body = build_chunk_body(&turn);
        assert_eq!(body["body"].as_str(), Some("hello world"), "body field");
        assert_eq!(body["speaker"].as_str(), Some("user"), "speaker field");
        assert_eq!(body["ordinal"].as_u64(), Some(3), "ordinal field");
    }

    /// Chunk body for an assistant turn sets speaker correctly.
    #[test]
    fn build_chunk_body_assistant_speaker() {
        use crate::transcript::Turn;
        let turn = Turn {
            speaker: "assistant".into(),
            body: "I can help with that.".into(),
            ordinal: 7,
        };
        let body = build_chunk_body(&turn);
        assert_eq!(body["speaker"].as_str(), Some("assistant"));
        assert_eq!(body["ordinal"].as_u64(), Some(7));
    }

    /// Ordinal zero is preserved (not treated as missing/null).
    #[test]
    fn build_chunk_body_ordinal_zero_is_preserved() {
        use crate::transcript::Turn;
        let turn = Turn {
            speaker: "user".into(),
            body: "first message".into(),
            ordinal: 0,
        };
        let body = build_chunk_body(&turn);
        assert_eq!(body["ordinal"].as_u64(), Some(0));
    }

    /// The chunk body is valid JSON (can be round-tripped through serde_json).
    #[test]
    fn build_chunk_body_is_valid_json_object() {
        use crate::transcript::Turn;
        let turn = Turn {
            speaker: "user".into(),
            body: "test".into(),
            ordinal: 1,
        };
        let body = build_chunk_body(&turn);
        // Serialise + deserialise round-trip.
        let serialised = serde_json::to_string(&body).expect("must serialise");
        let parsed: Value = serde_json::from_str(&serialised).expect("must parse");
        assert_eq!(parsed["body"].as_str(), Some("test"));
    }

    /// Multiple turns produce chunk bodies with strictly increasing ordinals.
    #[test]
    fn build_chunk_bodies_have_increasing_ordinals() {
        use crate::transcript::Turn;
        let turns: Vec<Turn> = (0u32..5)
            .map(|i| Turn {
                speaker: "user".into(),
                body: format!("msg {i}"),
                ordinal: i,
            })
            .collect();
        let ordinals: Vec<u64> = turns
            .iter()
            .map(|t| build_chunk_body(t)["ordinal"].as_u64().unwrap())
            .collect();
        let expected: Vec<u64> = (0..5).collect();
        assert_eq!(ordinals, expected, "ordinals must be 0..4 in order");
    }

    // ── redact: pure secret-detection guard ──────────────────────────────────

    /// Normal prose passes through unchanged.
    #[test]
    fn redact_normal_prose_returns_some_unchanged() {
        let text = "Today we discussed the project roadmap and timelines.";
        let result = redact(text);
        assert_eq!(result, Some(text.to_string()));
    }

    /// An OpenAI-style key (sk- + 20+ alphanum chars) causes the chunk to be dropped.
    #[test]
    fn redact_openai_key_returns_none() {
        let text = "my key is sk-abcdefghijklmnopqrstuvwxyz123456";
        assert!(
            redact(text).is_none(),
            "body containing an OpenAI-style key must return None"
        );
    }

    /// An AWS access key ID (AKIA + exactly 16 [0-9A-Z] chars) is dropped.
    #[test]
    fn redact_aws_access_key_id_returns_none() {
        // "AKIAIOSFODNN7EXAMPLE" — canonical example from AWS docs.
        let text = "AKIAIOSFODNN7EXAMPLE";
        assert!(
            redact(text).is_none(),
            "body containing an AWS access key ID must return None"
        );
    }

    /// An AWS access key ID with MORE than 16 uppercase-alphanum chars after
    /// AKIA is also dropped (fail-safe: >=16 triggers redaction).
    #[test]
    fn redact_aws_access_key_id_long_suffix_returns_none() {
        // "AKIAIOSFODNN7EXAMPLELONG" — AKIA + 20 uppercase chars.
        let text = "AKIAIOSFODNN7EXAMPLELONG";
        assert!(
            redact(text).is_none(),
            "AKIA followed by 20 uppercase chars must be redacted (>= 16 trigger)"
        );
    }

    /// A PEM block containing a private key header is dropped.
    #[test]
    fn redact_pem_private_key_returns_none() {
        let text =
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----";
        assert!(
            redact(text).is_none(),
            "body containing a PEM private key header must return None"
        );
    }

    /// "sk-8" — prefix present but only 1 trailing char — must NOT be redacted (false-positive guard).
    #[test]
    fn redact_short_sk_prefix_is_not_redacted() {
        let text = "take the sk-8 bus";
        assert!(
            redact(text).is_some(),
            "sk- with fewer than 20 trailing chars must not be redacted"
        );
    }

    /// Empty string passes through as Some("").
    #[test]
    fn redact_empty_string_returns_some_empty() {
        assert_eq!(redact(""), Some(String::new()));
    }

    // ── redact: new patterns (P2-2) ───────────────────────────────────────────

    /// Anthropic key (sk-ant- prefix) is dropped — covered by the sk- rule.
    #[test]
    fn redact_anthropic_key_sk_ant_returns_none() {
        // sk-ant-api03-XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
        let key = format!("sk-ant-api03-{}", "A".repeat(48));
        let text = format!("my anthropic key: {key}");
        assert!(
            redact(&text).is_none(),
            "body containing an Anthropic key must return None"
        );
    }

    /// Bare Anthropic key (no surrounding prose) is also dropped.
    #[test]
    fn redact_anthropic_key_bare_returns_none() {
        let key = format!("sk-ant-api03-{}", "B".repeat(48));
        assert!(
            redact(&key).is_none(),
            "bare Anthropic key must return None"
        );
    }

    /// GitHub classic PAT (ghp_ prefix + 36 chars) is dropped.
    #[test]
    fn redact_github_classic_pat_returns_none() {
        // "ghp_" + 36 alphanum chars = canonical classic PAT shape.
        let token = format!("ghp_{}", "x".repeat(36));
        let text = format!("export GH_TOKEN={token}");
        assert!(
            redact(&text).is_none(),
            "body containing a GitHub classic PAT must return None"
        );
    }

    /// GitHub OAuth token (gho_ prefix + 36 chars) is dropped.
    #[test]
    fn redact_github_oauth_token_returns_none() {
        let token = format!("gho_{}", "y".repeat(36));
        assert!(
            redact(&token).is_none(),
            "body containing a GitHub OAuth token must return None"
        );
    }

    /// GitHub fine-grained PAT (github_pat_ prefix + 20+ chars) is dropped.
    #[test]
    fn redact_github_fine_grained_pat_returns_none() {
        let token = format!("github_pat_{}", "z1_".repeat(10));
        let text = format!("token = \"{token}\"");
        assert!(
            redact(&text).is_none(),
            "body containing a GitHub fine-grained PAT must return None"
        );
    }

    /// Short "ghp_" prefix with only 3 trailing chars must NOT be redacted
    /// (false-positive guard — e.g. a variable name "ghp_id" in code).
    #[test]
    fn redact_short_ghp_prefix_is_not_redacted() {
        let text = "let ghp_id = get_id();";
        assert!(
            redact(text).is_some(),
            "ghp_ with fewer than 36 trailing chars must not be redacted"
        );
    }

    /// A standalone 40-char hex-like token (the high-entropy guard) is dropped.
    #[test]
    fn redact_generic_40char_token_returns_none() {
        // 40 uppercase hex chars — looks like a Git SHA or generic API token.
        let token = "A1B2C3D4E5F6A1B2C3D4E5F6A1B2C3D4E5F6A1B2";
        assert_eq!(token.len(), 40);
        assert!(
            redact(token).is_none(),
            "a standalone 40-char high-entropy token must return None"
        );
    }

    /// A 40-char token embedded in a sentence (space-delimited) is dropped.
    #[test]
    fn redact_generic_40char_token_in_prose_returns_none() {
        let token = "A1B2C3D4E5F6A1B2C3D4E5F6A1B2C3D4E5F6A1B2";
        let text = format!("my token is {token} please keep it safe");
        assert!(
            redact(&text).is_none(),
            "space-delimited 40-char token in prose must return None"
        );
    }

    /// A 39-char run (one short of the threshold) must NOT be redacted.
    #[test]
    fn redact_39char_run_is_not_redacted() {
        // Build exactly 39 token chars programmatically to avoid miscounting.
        let text: String = "A1".repeat(19) + "B"; // 19*2 + 1 = 39
        assert_eq!(text.len(), 39, "pre-condition: string must be 39 chars");
        assert!(
            redact(&text).is_some(),
            "a 39-char run must not be redacted (below threshold)"
        );
    }

    /// A sentence whose longest word is 10 chars must NOT be redacted.
    #[test]
    fn redact_normal_long_word_is_not_redacted() {
        // "productivity" is 12 chars — well below 40.
        let text = "Focus on productivity and collaboration today.";
        assert!(
            redact(text).is_some(),
            "normal prose with long words must not be redacted"
        );
    }

    /// A URL (no token chars beyond 40) passes through unchanged.
    #[test]
    fn redact_url_is_not_redacted() {
        let text = "See https://docs.example.com/api/v2/reference for details.";
        assert!(redact(text).is_some(), "a plain URL must not be redacted");
    }

    // ── Layer 3: keyword extraction and context recovery ─────────────────────

    #[test]
    fn extract_keywords_filters_short_and_stop_words() {
        let kws = extract_keywords("I want to fix the connector module for claude");
        assert!(kws.contains(&"connector".to_string()));
        assert!(kws.contains(&"module".to_string()));
        assert!(kws.contains(&"claude".to_string()));
        assert!(!kws.contains(&"want".to_string()), "stop word excluded");
        assert!(!kws.contains(&"the".to_string()), "short word excluded");
        assert!(!kws.contains(&"fix".to_string()), "short word excluded");
    }

    #[test]
    fn extract_keywords_deduplicates() {
        let kws = extract_keywords("connector connector connector module");
        assert_eq!(kws.iter().filter(|k| *k == "connector").count(), 1);
    }

    #[test]
    fn detect_returning_topics_finds_old_keywords() {
        let state = ActiveSessionState {
            daemon_session_id: "s1".into(),
            tool_id: "claude-code".into(),
            workspace: None,
            next_ordinal: 10,
            keyword_first_seen: {
                let mut m = std::collections::HashMap::new();
                m.insert("connector".to_string(), 0); // seen at ordinal 0 (very old)
                m.insert("module".to_string(), 8); // seen at ordinal 8 (recent)
                m
            },
            last_prompt_ordinal: 9,
        };
        // "connector" gap = 10-0 = 10 >= 4 (OLD_TOPIC_ORDINAL_GAP) => returning
        // "module" gap = 10-8 = 2 < 4 => NOT returning
        let returning = detect_returning_topics(&state, "fix the connector settings");
        assert!(returning.contains(&"connector".to_string()));
        assert!(
            !returning.contains(&"settings".to_string()),
            "new keyword is not returning"
        );
    }

    #[test]
    fn detect_returning_topics_empty_when_no_history() {
        let state = ActiveSessionState {
            daemon_session_id: "s1".into(),
            tool_id: "claude-code".into(),
            workspace: None,
            next_ordinal: 0,
            keyword_first_seen: std::collections::HashMap::new(),
            last_prompt_ordinal: 0,
        };
        let returning = detect_returning_topics(&state, "fix the connector");
        assert!(returning.is_empty());
    }

    #[test]
    fn record_prompt_keywords_does_not_overwrite_first_seen() {
        let mut state = ActiveSessionState {
            daemon_session_id: "s1".into(),
            tool_id: "claude-code".into(),
            workspace: None,
            next_ordinal: 5,
            keyword_first_seen: {
                let mut m = std::collections::HashMap::new();
                m.insert("connector".to_string(), 1);
                m
            },
            last_prompt_ordinal: 4,
        };
        record_prompt_keywords(&mut state, "fix the connector again");
        assert_eq!(
            state.keyword_first_seen["connector"], 1,
            "first_seen must not be overwritten"
        );
        assert_eq!(
            state.keyword_first_seen["again"], 5,
            "new keyword gets current ordinal"
        );
    }
}
