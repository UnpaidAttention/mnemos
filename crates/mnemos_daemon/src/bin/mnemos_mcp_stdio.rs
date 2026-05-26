//! MCP stdio transport. Reads Content-Length-framed JSON-RPC from stdin,
//! forwards each request to the daemon's /mcp HTTP endpoint, writes responses
//! to stdout with the same framing.
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

/// Read one Content-Length-framed message.
///
/// The framing is: `Content-Length: <n>\r\n\r\n<n bytes of body>`.
/// Returns `Ok(None)` on EOF (zero bytes read on the first header line).
fn read_frame<R: BufRead>(r: &mut R) -> Result<Option<Vec<u8>>> {
    let mut header = String::new();
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line)?;
        if n == 0 {
            // EOF before any header line — clean exit
            if header.is_empty() {
                return Ok(None);
            }
            // EOF mid-header — treat as EOF
            return Ok(None);
        }
        // Blank line (CRLF or LF alone) marks end of headers
        if line == "\r\n" || line == "\n" {
            break;
        }
        header.push_str(&line);
    }
    let len: usize = header
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length:").map(str::trim))
        .ok_or_else(|| anyhow::anyhow!("missing Content-Length header in frame"))?
        .parse()
        .context("Content-Length is not a valid integer")?;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload).context("read frame body")?;
    Ok(Some(payload))
}

/// Write one Content-Length-framed response to `w`.
fn write_frame<W: Write>(w: &mut W, body: &[u8]) -> Result<()> {
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(body)?;
    w.flush()?;
    Ok(())
}
