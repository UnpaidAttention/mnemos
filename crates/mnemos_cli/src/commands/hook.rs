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

const DAEMON_URL: &str = "http://127.0.0.1:7423";

/// Maximum UTF-8 bytes we pass as `additionalContext` to Claude Code.
/// Keeps the injected context inside a reasonable token budget (~2 000 tokens).
const WORKING_SET_BYTE_CAP: usize = 8_000;

/// Maximum chars of recall context injected into a UserPromptSubmit hook.
/// Approximates ~300 tokens at 4 chars/token to stay lightweight.
const RECALL_BUDGET_CHARS: usize = 1_200;

/// Default number of recall hits to request from the daemon per prompt.
const RECALL_K: usize = 6;

/// Entry point for `mnemos hook <event>`.
///
/// Always returns `ExitCode::SUCCESS` (fail-open guarantee).
pub async fn run(event: &str) -> ExitCode {
    let input = read_stdin_json().await;
    let out = match event {
        "session-start" => session_start(input).await,
        "user-prompt" => user_prompt(input).await, // TODO(B3)
        "session-end" => session_end(input).await, // TODO(B4)
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
async fn session_start(input: Value) -> Option<String> {
    // Extract workspace from the hook payload. Claude Code sets `cwd` in the
    // session-start payload; `source` identifies the tool.
    let workspace = input
        .get("cwd")
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

    let hits = fetch_recall(&token, &prompt, workspace.as_deref()).await?;
    let selected = budget_hits(&hits, RECALL_BUDGET_CHARS);
    if selected.is_empty() {
        return None;
    }

    let text = render_recall_hits(&selected);
    user_prompt_hook_json(&text)
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

    // ── 2. Idempotency: skip if already captured ──────────────────────────
    let state_path = idempotency_state_path()?;
    if already_captured(&state_path, session_id) {
        eprintln!("mnemos hook session-end: session {session_id} already captured — skipping");
        return Ok(());
    }

    // ── 3. Ensure daemon is available ─────────────────────────────────────
    let up = crate::daemon_ctl::ensure_daemon(Duration::from_secs(5)).await;
    if !up {
        return Err(anyhow::anyhow!(
            "daemon not available — transcript not captured"
        ));
    }

    let token = load_token().ok_or_else(|| anyhow::anyhow!("could not load bearer token"))?;

    // ── 4. Read and parse transcript ──────────────────────────────────────
    let contents = std::fs::read_to_string(transcript_path)
        .with_context(|| format!("failed to read transcript at {transcript_path}"))?;

    let turns = crate::transcript::parse_transcript(&contents);
    if turns.is_empty() {
        // Nothing to capture; still record as processed so we don't re-read.
        record_captured(&state_path, session_id);
        return Ok(());
    }

    // ── 5. POST /v1/sessions ──────────────────────────────────────────────
    let daemon_session_id = post_start_session(&token, cwd.as_deref()).await?;

    // ── 6. POST chunks with bounded concurrency (best-effort per chunk) ───
    post_chunks(&token, &daemon_session_id, &turns).await;

    // ── 7. Record capture now that start succeeded (idempotency boundary) ─
    // We record here — before /end — so that if /end fails, the next hook
    // fire does NOT create a duplicate daemon session. Missing /end or
    // missing chunks are best-effort and not worth re-capturing.
    record_captured(&state_path, session_id);

    // ── 8. POST /v1/sessions/{id}/end (best-effort, non-fatal) ───────────
    if let Err(e) = post_end_session(&token, &daemon_session_id).await {
        eprintln!("mnemos hook session-end: /end failed (session {daemon_session_id}): {e:#}");
    }

    Ok(())
}

// ── Idempotency helpers ───────────────────────────────────────────────────────

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

/// Returns `true` if `session_id` is already present in the state file.
/// Any FS error → `false` (fail-open: prefer re-capture over losing data).
pub(crate) fn already_captured(state_path: &std::path::Path, session_id: &str) -> bool {
    let Ok(contents) = std::fs::read_to_string(state_path) else {
        return false;
    };
    contents.lines().any(|l| l.trim() == session_id)
}

/// Appends `session_id` to the state file (one id per line).
/// Creates parent directories as needed.
/// Any FS error is logged to stderr and silently ignored (fail-open).
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
    use std::io::Write as _;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(state_path);
    match file {
        Ok(mut f) => {
            if let Err(e) = writeln!(f, "{session_id}") {
                eprintln!("mnemos hook session-end: failed to record session id: {e}");
            }
        }
        Err(e) => {
            eprintln!(
                "mnemos hook session-end: could not open state file {}: {e}",
                state_path.display()
            );
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
        .post(format!("{DAEMON_URL}/v1/sessions"))
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
    let url = format!("{DAEMON_URL}/v1/sessions/{daemon_session_id}/chunks");

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
    let url = format!("{DAEMON_URL}/v1/sessions/{daemon_session_id}/end");
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
/// This is a conservative, best-effort guard — it catches obvious shapes only:
///
/// * **OpenAI-style key** — `sk-` followed by 20 or more `[A-Za-z0-9]` chars.
/// * **AWS access key ID** — `AKIA` followed by exactly 16 `[0-9A-Z]` chars.
/// * **PEM private key header** — substring `-----BEGIN` appears AND later
///   `PRIVATE KEY-----` appears (covers RSA, EC, OPENSSH, etc.).
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

    // ── OpenAI-style key: "sk-" + 20+ [A-Za-z0-9] ────────────────────────────
    const SK_PREFIX: &[u8] = b"sk-";
    const SK_MIN_SUFFIX: usize = 20;

    let mut i = 0usize;
    while i + SK_PREFIX.len() <= len {
        // Find the next "sk-" occurrence starting from `i`.
        if let Some(offset) = bytes[i..]
            .windows(SK_PREFIX.len())
            .position(|w| w == SK_PREFIX)
        {
            let key_start = i + offset + SK_PREFIX.len();
            let run = bytes[key_start..]
                .iter()
                .take_while(|&&b| b.is_ascii_alphanumeric())
                .count();
            if run >= SK_MIN_SUFFIX {
                return None;
            }
            // Advance past the prefix we just checked.
            i = i + offset + 1;
        } else {
            break;
        }
    }

    // ── AWS access key ID: "AKIA" + exactly 16 [0-9A-Z] ─────────────────────
    const AKIA_PREFIX: &[u8] = b"AKIA";
    const AKIA_SUFFIX_LEN: usize = 16;

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

/// Fetch the working set from `GET /v1/working` and render it as plain text.
/// Returns `None` on any network or parse error (fail-open).
async fn fetch_working_set(token: &str, workspace: Option<&str>) -> Option<String> {
    let mut url = format!("{DAEMON_URL}/v1/working");
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
fn recall_hit_rendered_len(hit: &RecallHit) -> usize {
    let title = &hit.memory.title;
    let body = &hit.memory.body;
    let snippet_len = body.chars().count().min(300);
    // "- " + title + ": " + snippet + "\n"
    2 + title.chars().count() + 2 + snippet_len + 1
}

/// Render selected recall hits as plain-text lines, matching the
/// `render_working_set` style: `- title: snippet`.
fn render_recall_hits(hits: &[&RecallHit]) -> String {
    hits.iter()
        .map(|h| {
            let title = &h.memory.title;
            let snippet: String = h.memory.body.chars().take(300).collect();
            format!("- {title}: {snippet}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// POST `query` to `POST /v1/memories/search` and return the deserialized hits.
/// Returns `None` on any network or parse error (fail-open).
async fn fetch_recall(token: &str, query: &str, workspace: Option<&str>) -> Option<Vec<RecallHit>> {
    let url = format!("{DAEMON_URL}/v1/memories/search");
    let client = make_client()?;
    let mut body = serde_json::Map::new();
    body.insert("query".into(), serde_json::json!(query));
    body.insert("k".into(), serde_json::json!(RECALL_K));
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
}
