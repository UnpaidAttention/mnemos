//! Daemon lifecycle for the desktop shell. Delegates to the bundled `mnemos`
//! CLI sidecar's `daemon` subcommands so we reuse its PID/adopt logic.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

#[derive(Debug, Clone, Serialize)]
pub struct DaemonStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub detail: String,
}

/// Run `mnemos daemon <sub> --json` via the sidecar; return (stdout, ok).
async fn run_daemon(app: &AppHandle, sub: &str) -> Result<(String, bool), String> {
    let cmd = app
        .shell()
        .sidecar("mnemos")
        .map_err(|e| format!("resolve sidecar: {e}"))?
        .args(["daemon", sub, "--json"]);
    let out = cmd
        .output()
        .await
        .map_err(|e| format!("run mnemos daemon {sub}: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    Ok((stdout, out.status.success()))
}

pub async fn status(app: &AppHandle) -> DaemonStatus {
    match run_daemon(app, "status").await {
        Ok((stdout, _)) => {
            let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_default();
            let running = v.get("running").and_then(|b| b.as_bool()).unwrap_or(false);
            let pid = v.get("pid").and_then(|p| p.as_u64()).map(|p| p as u32);
            DaemonStatus {
                running,
                pid,
                detail: stdout.trim().to_string(),
            }
        }
        Err(e) => DaemonStatus {
            running: false,
            pid: None,
            detail: e,
        },
    }
}

pub async fn start(app: &AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let mut cmd = app
        .shell()
        .sidecar("mnemos")
        .map_err(|e| format!("resolve sidecar: {e}"))?
        .args(["daemon", "start", "--json"]);
    if let Ok(res) = app.path().resource_dir() {
        // Bundled llama-server + GGUF live under <resource_dir>/_up_/_up_/assets
        // in packaged builds (Tauri maps the ../../assets resource entries to a
        // _up_/_up_ prefix). In dev the daemon falls back to ./assets relative
        // to its CWD, so a missing path here is harmless.
        let assets = res.join("_up_").join("_up_").join("assets");
        // Only override the daemon's asset discovery when the bundled assets
        // are actually present (packaged builds). In a dev/non-packaged run the
        // path doesn't exist; setting the env var anyway would override the
        // daemon's working `./assets` fallback and break the embedder.
        if assets.is_dir() {
            let assets_str = assets.to_string_lossy().to_string();
            cmd = cmd
                .env("MNEMOS_BUNDLED_BIN_DIR", &assets_str)
                .env("MNEMOS_BUNDLED_MODEL_DIR", &assets_str);
        }
    }
    let out = cmd
        .output()
        .await
        .map_err(|e| format!("run mnemos daemon start: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "daemon start failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

pub async fn stop(app: &AppHandle) -> Result<(), String> {
    let (out, ok) = run_daemon(app, "stop").await?;
    if ok {
        Ok(())
    } else {
        Err(format!("daemon stop failed: {out}"))
    }
}

/// Poll the daemon's unauthenticated readiness until healthy or timeout.
/// Returns Ok(()) when /health responds (any status), Err on timeout.
/// `/health` is the daemon's only public (no-auth) route — see
/// `mnemos_daemon::routes::build_router`.
pub async fn wait_healthy(port: u16, timeout_ms: u64) -> Result<(), String> {
    let url = format!("http://127.0.0.1:{port}/health");
    let started = std::time::Instant::now();
    loop {
        if let Ok(resp) = reqwest::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_millis(500))
            .send()
            .await
        {
            let _ = resp;
            return Ok(());
        }
        if started.elapsed().as_millis() as u64 > timeout_ms {
            return Err("daemon did not become healthy in time".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

/// Poll until the daemon's public /health stops responding (process gone), or timeout.
pub async fn wait_stopped(port: u16, timeout_ms: u64) -> Result<(), String> {
    let url = format!("http://127.0.0.1:{port}/health");
    let started = std::time::Instant::now();
    loop {
        let resp = reqwest::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_millis(500))
            .send()
            .await;
        if resp.is_err() {
            return Ok(()); // connection refused → listener down
        }
        if started.elapsed().as_millis() as u64 > timeout_ms {
            return Err("daemon did not stop in time".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}
