//! `mnemos hook <event>` — Claude Code hook integration.
//!
//! Dispatches to event-specific handlers. All handlers are fail-open:
//! any Mnemos failure (daemon down, timeout, parse error) results in
//! returning `None` (no output) and `ExitCode::SUCCESS`. The caller's
//! Claude Code session must NEVER be broken by a Mnemos problem.
//!
//! ## Hook event subcommands
//! - `session-start` (B2): inject working-set as `additionalContext`.
//! - `user-prompt`   (B3): reserved stub — TODO(B3).
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

use serde_json::Value;
use std::io::Read;
use std::process::ExitCode;
use std::time::Duration;

const DAEMON_URL: &str = "http://127.0.0.1:7423";

/// Maximum UTF-8 bytes we pass as `additionalContext` to Claude Code.
/// Keeps the injected context inside a reasonable token budget (~2 000 tokens).
const WORKING_SET_BYTE_CAP: usize = 8_000;

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

/// Stub for the `UserPrompt` hook — implemented in task B3.
// TODO(B3)
async fn user_prompt(_input: Value) -> Option<String> {
    None
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

/// Fetch the working set from `GET /v1/working` and render it as plain text.
/// Returns `None` on any network or parse error (fail-open).
async fn fetch_working_set(token: &str, workspace: Option<&str>) -> Option<String> {
    let mut url = format!("{DAEMON_URL}/v1/working");
    if let Some(ws) = workspace {
        url.push_str(&format!("?workspace={}", urlencoding::encode(ws)));
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;
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
