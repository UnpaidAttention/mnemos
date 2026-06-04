//! MCP stdio transport. Reads newline-delimited JSON-RPC from stdin (the MCP
//! stdio framing used by Claude Code and other MCP clients), forwards each
//! request to the daemon's /mcp HTTP endpoint, and writes each response back
//! as one newline-terminated JSON line.
//!
//! Usage: set `MNEMOS_DAEMON_URL` (default `http://127.0.0.1:7423`) and either
//! `MNEMOS_DAEMON_TOKEN` or rely on the token file at `~/.config/mnemos/token`.

use anyhow::{Context, Result};
use std::io::{BufRead, Write};
use std::process::ExitCode;

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
        let frame = match read_frame(&mut stdin.lock()) {
            Ok(Some(f)) => f,
            Ok(None) => break, // EOF — graceful exit
            Err(e) => return Err(e),
        };

        let resp = client
            .post(&mcp_url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(frame)
            .send()
            .await?;
        let body = resp.bytes().await?;
        write_frame(&mut stdout_handle.lock(), &body)?;
    }
    Ok(())
}

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
