//! MCP stdio transport. Reads newline-delimited JSON-RPC from stdin (the MCP
//! stdio framing used by Claude Code and other MCP clients), forwards each
//! request to the daemon's /mcp HTTP endpoint, and writes each response back
//! as one newline-terminated JSON line.
//!
//! Error handling contract
//! -----------------------
//! * Stdin EOF → clean exit.
//! * Non-2xx HTTP response → emit a JSON-RPC error frame (code -32603) keyed
//!   to the request id; continue the loop. 401/403 get a human-readable hint.
//! * Transport error (connection refused, timeout, etc.) → retry up to
//!   `MAX_RETRIES` times with a short back-off; after exhausting retries, emit
//!   a JSON-RPC error frame and continue. Only stdin EOF breaks the loop.
//! * Nothing is written to stdout except well-formed JSON-RPC frames.
//! * All diagnostic messages go to stderr.
//!
//! Usage: set `MNEMOS_DAEMON_URL` (default `http://127.0.0.1:7423`) and either
//! `MNEMOS_DAEMON_TOKEN` or rely on the token file at `~/.config/mnemos/token`.

use anyhow::{Context, Result};
use std::io::{BufRead, Write};
use std::process::ExitCode;

/// How many times to retry a single request on a connection-level error before
/// giving up and emitting a JSON-RPC error frame for that request.
const MAX_RETRIES: u32 = 2;
/// Base back-off between retries (doubles each attempt).
const RETRY_BASE_MS: u64 = 50;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mnemos-mcp-stdio: {e:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    let url = std::env::var("MNEMOS_DAEMON_URL").unwrap_or_else(|_| "http://127.0.0.1:7423".into());
    let token = match std::env::var("MNEMOS_DAEMON_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            let path = mnemos_daemon::token_path()?;
            mnemos_daemon::auth::load_token(&path)
                .with_context(|| format!("read token from {}", path.display()))?
        }
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    let mcp_url = format!("{}/mcp", url.trim_end_matches('/'));

    let stdin = std::io::stdin();
    let stdout_handle = std::io::stdout();

    loop {
        // ── Read one request frame from stdin ──────────────────────────────
        let frame = match read_frame(&mut stdin.lock()) {
            Ok(Some(f)) => f,
            Ok(None) => break, // EOF — graceful exit
            Err(e) => return Err(e),
        };

        // Best-effort: extract the request id so errors can be correlated.
        let request_id = extract_id(&frame);

        // ── Forward to the daemon with bounded retries ─────────────────────
        let result = send_with_retry(&client, &mcp_url, &token, &frame).await;

        match result {
            Ok(body) => {
                // ── Non-2xx HTTP: emit a JSON-RPC error frame ──────────────
                if let Some(error_frame) = body.error_frame {
                    write_frame(&mut stdout_handle.lock(), error_frame.as_bytes())?;
                } else {
                    write_frame(&mut stdout_handle.lock(), &body.bytes)?;
                }
            }
            Err(transport_err) => {
                // ── Transport/network error: emit a JSON-RPC error frame ───
                eprintln!("mnemos-mcp-stdio: transport error: {transport_err:#}");
                let frame = make_error_frame(
                    request_id,
                    -32_603,
                    &format!(
                        "mnemos daemon unreachable — check that mnemosd is running ({transport_err})"
                    ),
                );
                write_frame(&mut stdout_handle.lock(), frame.as_bytes())?;
                // Keep running — the client can retry or disconnect gracefully.
            }
        }
    }
    Ok(())
}

// ── HTTP dispatch with retry ──────────────────────────────────────────────────

struct SendResult {
    /// Raw response bytes from the daemon (set when HTTP was 2xx).
    bytes: Vec<u8>,
    /// Pre-rendered JSON-RPC error frame (set when HTTP was non-2xx).
    error_frame: Option<String>,
}

async fn send_with_retry(
    client: &reqwest::Client,
    mcp_url: &str,
    token: &str,
    frame: &[u8],
) -> Result<SendResult> {
    let mut last_err: Option<anyhow::Error> = None;

    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            let delay = std::time::Duration::from_millis(RETRY_BASE_MS * (1 << (attempt - 1)));
            tokio::time::sleep(delay).await;
        }

        match client
            .post(mcp_url)
            .bearer_auth(token)
            .header("content-type", "application/json")
            .body(frame.to_vec())
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                let body_bytes = resp.bytes().await.unwrap_or_default();

                if status.is_success() {
                    return Ok(SendResult {
                        bytes: body_bytes.to_vec(),
                        error_frame: None,
                    });
                }

                // Non-2xx: do NOT retry; emit an error frame immediately.
                let snippet = String::from_utf8_lossy(&body_bytes);
                let snippet = snippet.trim();
                let snippet = if snippet.len() > 200 {
                    &snippet[..200]
                } else {
                    snippet
                };

                let message = if status == reqwest::StatusCode::UNAUTHORIZED
                    || status == reqwest::StatusCode::FORBIDDEN
                {
                    format!("authentication to mnemos daemon failed — check token (HTTP {status})")
                } else {
                    format!("daemon returned HTTP {status}: {snippet}")
                };

                // Extract request id from the frame we sent.
                let id = extract_id(frame);
                return Ok(SendResult {
                    bytes: Vec::new(),
                    error_frame: Some(make_error_frame(id, -32_603, &message)),
                });
            }
            Err(e) => {
                // Connection-level error — retry if budget remains.
                eprintln!(
                    "mnemos-mcp-stdio: attempt {}/{}: {e}",
                    attempt + 1,
                    MAX_RETRIES + 1
                );
                last_err = Some(anyhow::anyhow!("{e}"));
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("unknown transport error")))
}

// ── JSON-RPC helpers ──────────────────────────────────────────────────────────

/// Extract the `id` field from a raw JSON-RPC frame, returning a JSON
/// `Value`. Returns `serde_json::Value::Null` when the id is absent or the
/// frame cannot be parsed (per JSON-RPC 2.0: id is null for parse errors).
fn extract_id(frame: &[u8]) -> serde_json::Value {
    serde_json::from_slice::<serde_json::Value>(frame)
        .ok()
        .and_then(|v| v.get("id").cloned())
        .unwrap_or(serde_json::Value::Null)
}

/// Build a compact JSON-RPC 2.0 error response string (no trailing newline).
fn make_error_frame(id: serde_json::Value, code: i64, message: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
    .to_string()
}

// ── Framing ───────────────────────────────────────────────────────────────────

/// Read one newline-delimited JSON-RPC message (MCP stdio framing).
///
/// Each message is a single line of JSON terminated by `\n` (MCP messages
/// MUST NOT contain embedded newlines). Blank lines are skipped. Returns
/// `Ok(None)` on EOF.
fn read_frame<R: BufRead>(r: &mut R) -> Result<Option<Vec<u8>>> {
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line)?;
        if n == 0 {
            return Ok(None); // EOF — clean exit
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue; // skip blank lines between messages
        }
        return Ok(Some(trimmed.as_bytes().to_vec()));
    }
}

/// Write one response as a single newline-terminated JSON line (MCP stdio
/// framing). The daemon emits compact JSON, so the body is a single line.
fn write_frame<W: Write>(w: &mut W, body: &[u8]) -> Result<()> {
    w.write_all(body)?;
    w.write_all(b"\n")?;
    w.flush()?;
    Ok(())
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── extract_id ────────────────────────────────────────────────────────────

    #[test]
    fn extract_id_returns_numeric_id() {
        let frame = br#"{"jsonrpc":"2.0","id":42,"method":"initialize","params":{}}"#;
        assert_eq!(extract_id(frame), serde_json::json!(42));
    }

    #[test]
    fn extract_id_returns_string_id() {
        let frame = br#"{"jsonrpc":"2.0","id":"req-abc","method":"tools/list"}"#;
        assert_eq!(extract_id(frame), serde_json::json!("req-abc"));
    }

    #[test]
    fn extract_id_returns_null_when_absent() {
        let frame = br#"{"jsonrpc":"2.0","method":"notifications/foo"}"#;
        assert_eq!(extract_id(frame), serde_json::Value::Null);
    }

    #[test]
    fn extract_id_returns_null_on_garbage() {
        assert_eq!(extract_id(b"not json at all"), serde_json::Value::Null);
    }

    // ── make_error_frame ──────────────────────────────────────────────────────

    #[test]
    fn make_error_frame_is_valid_json_rpc() {
        let frame = make_error_frame(
            serde_json::json!(1),
            -32_603,
            "daemon returned HTTP 401: unauthorized",
        );
        let v: serde_json::Value = serde_json::from_str(&frame).expect("valid JSON");
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["id"], 1);
        assert_eq!(v["error"]["code"], -32_603);
        assert!(v["error"]["message"].as_str().unwrap().contains("HTTP 401"));
    }

    #[test]
    fn make_error_frame_401_contains_auth_hint() {
        let msg = "authentication to mnemos daemon failed — check token (HTTP 401 Unauthorized)";
        let frame = make_error_frame(serde_json::json!(5), -32_603, msg);
        let v: serde_json::Value = serde_json::from_str(&frame).unwrap();
        assert!(v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("check token"));
    }

    #[test]
    fn make_error_frame_transport_contains_unreachable_hint() {
        let msg = "mnemos daemon unreachable — check that mnemosd is running (connection refused)";
        let frame = make_error_frame(serde_json::Value::Null, -32_603, msg);
        let v: serde_json::Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(v["id"], serde_json::Value::Null);
        assert!(v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unreachable"));
    }

    #[test]
    fn make_error_frame_has_no_trailing_newline() {
        let frame = make_error_frame(serde_json::Value::Null, -32_603, "oops");
        assert!(!frame.ends_with('\n'));
    }

    // ── read_frame ────────────────────────────────────────────────────────────

    #[test]
    fn read_frame_reads_single_line() {
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1}\n";
        let frame = read_frame(&mut input.as_ref()).unwrap().unwrap();
        assert_eq!(frame, b"{\"jsonrpc\":\"2.0\",\"id\":1}");
    }

    #[test]
    fn read_frame_skips_blank_lines() {
        let input = b"\n\n{\"jsonrpc\":\"2.0\",\"id\":2}\n";
        let frame = read_frame(&mut input.as_ref()).unwrap().unwrap();
        assert_eq!(frame, b"{\"jsonrpc\":\"2.0\",\"id\":2}");
    }

    #[test]
    fn read_frame_returns_none_on_eof() {
        let input: &[u8] = b"";
        assert!(read_frame(&mut input.as_ref()).unwrap().is_none());
    }
}
