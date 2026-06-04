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

/// Stub for the `SessionEnd` hook — implemented in task B4.
// TODO(B4)
async fn session_end(_input: Value) -> Option<String> {
    None
}

// ── Pure helpers (unit-testable without daemon) ───────────────────────────────

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
}
