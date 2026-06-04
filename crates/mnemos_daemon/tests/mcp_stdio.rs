use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, config::Config, serve};
use std::process::Stdio;
use tempfile::TempDir;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::Command;

#[tokio::test]
async fn stdio_subprocess_forwards_initialize_to_daemon() {
    // Start a real daemon on a random port.
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    let token = state.token.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        serve(listener, app).await.unwrap();
    });

    // Spawn the stdio binary pointed at our daemon.
    let bin = env!("CARGO_BIN_EXE_mnemos-mcp-stdio");
    let mut child = Command::new(bin)
        .env("MNEMOS_DAEMON_URL", format!("http://{addr}"))
        .env("MNEMOS_DAEMON_TOKEN", &token)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // Send an initialize request as newline-delimited JSON (MCP stdio framing).
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#;
    stdin
        .write_all(format!("{body}\n").as_bytes())
        .await
        .unwrap();
    stdin.flush().await.unwrap();

    // Read one newline-delimited response line.
    use tokio::io::AsyncBufReadExt;
    let mut line = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        reader.read_line(&mut line),
    )
    .await
    .expect("response within 5s")
    .unwrap();
    let resp: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(resp["id"], 1);
    assert!(resp["result"]["serverInfo"]["name"].is_string());

    child.kill().await.ok();
}
