//! Manages the `llama-server` child process that serves the bundled embedder.
//!
//! Lifecycle:
//!   - spawn(): fork llama-server on $port, wait for /health, return a handle
//!   - health task: every 30s poll /health, restart with exponential backoff on
//!     3 consecutive failures
//!   - shutdown(): SIGTERM, wait 2s, SIGKILL (via `kill_on_drop`)
//!
//! Logs route to the platform-appropriate data-local dir
//! (e.g. `~/.local/share/mnemos/logs/llama-server.log` on Linux).

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::{watch, Mutex};

/// Configuration for the bundled llama-server child process.
#[derive(Debug, Clone)]
pub struct BundledEmbedderConfig {
    /// Path to the `llama-server` executable.
    pub binary: PathBuf,
    /// Path to the GGUF model file.
    pub model: PathBuf,
    /// Port to bind on (default 7424).
    pub port: u16,
    /// Host to bind on (default 127.0.0.1).
    pub host: String,
}

impl Default for BundledEmbedderConfig {
    fn default() -> Self {
        Self {
            binary: default_binary_path(),
            model: default_model_path(),
            port: 7424,
            host: "127.0.0.1".into(),
        }
    }
}

/// Resolve the bundled `llama-server` binary path.
///
/// Order of precedence:
/// 1. `MNEMOS_BUNDLED_BIN_DIR` env var (`<dir>/llama-server`)
/// 2. Packaged install wrapper at `/usr/bin/mnemos-llama-server` (sets
///    `LD_LIBRARY_PATH` then execs the real binary in `/usr/lib/mnemos/`)
/// 3. Raw packaged install at `/usr/lib/mnemos/llama-server` (relies on
///    `LD_LIBRARY_PATH` being set externally)
/// 4. Dev layout `assets/llama-server-linux-x86_64`
pub fn default_binary_path() -> PathBuf {
    if let Ok(env) = std::env::var("MNEMOS_BUNDLED_BIN_DIR") {
        return PathBuf::from(env).join("llama-server");
    }
    // Packaged install: wrapper at /usr/bin/mnemos-llama-server sets
    // LD_LIBRARY_PATH=/usr/lib/mnemos so the dynamically-linked binary can
    // find its bundled .so neighbors.
    let wrapper = PathBuf::from("/usr/bin/mnemos-llama-server");
    if wrapper.exists() {
        return wrapper;
    }
    // Fallback: raw binary directly. Callers must arrange LD_LIBRARY_PATH.
    let install = PathBuf::from("/usr/lib/mnemos/llama-server");
    if install.exists() {
        return install;
    }
    PathBuf::from("assets/llama-server-linux-x86_64")
}

/// Resolve the bundled GGUF model path.
///
/// Order of precedence:
/// 1. `MNEMOS_BUNDLED_MODEL_DIR` env var
/// 2. Packaged install at `/usr/lib/mnemos/all-MiniLM-L6-v2.Q8_0.gguf`
/// 3. Dev layout `assets/all-MiniLM-L6-v2.Q8_0.gguf`
pub fn default_model_path() -> PathBuf {
    if let Ok(env) = std::env::var("MNEMOS_BUNDLED_MODEL_DIR") {
        return PathBuf::from(env).join("all-MiniLM-L6-v2.Q8_0.gguf");
    }
    let install = PathBuf::from("/usr/lib/mnemos/all-MiniLM-L6-v2.Q8_0.gguf");
    if install.exists() {
        return install;
    }
    PathBuf::from("assets/all-MiniLM-L6-v2.Q8_0.gguf")
}

/// Handle returned by [`spawn`]. Drop or call [`BundledHandle::shutdown`] to
/// terminate the child process; the underlying `tokio::process::Child` is
/// configured with `kill_on_drop(true)` so even an unclean drop will reap it.
pub struct BundledHandle {
    child: Arc<Mutex<Option<Child>>>,
    shutdown_tx: watch::Sender<bool>,
}

impl BundledHandle {
    /// Gracefully stop the health-task and SIGTERM the child. Waits up to 2s
    /// for the child to exit; after that, `kill_on_drop` will SIGKILL.
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let mut guard = self.child.lock().await;
        if let Some(mut c) = guard.take() {
            let _ = c.start_kill();
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), c.wait()).await;
        }
    }
}

/// Spawn a single `llama-server` child, routing stdout+stderr to `log_path`
/// opened in append mode.
///
/// Extracted so both the initial spawn and the watchdog-restart branch use
/// identical arguments and log routing — preventing post-restart output from
/// being silently dropped to `/dev/null` (P1-11).
fn spawn_child(cfg: &BundledEmbedderConfig, log_path: &std::path::Path) -> Result<Child> {
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .with_context(|| format!("open log {}", log_path.display()))?;
    let log_err = log.try_clone().context("clone log fd")?;
    Command::new(&cfg.binary)
        .arg("--model")
        .arg(&cfg.model)
        .arg("--host")
        .arg(&cfg.host)
        .arg("--port")
        .arg(cfg.port.to_string())
        .arg("--embedding")
        .arg("--pooling")
        .arg("mean")
        .arg("--ctx-size")
        .arg("8192")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawn {}", cfg.binary.display()))
}

/// Spawn `llama-server` and wait for its `/health` endpoint to come up.
///
/// On success, returns a [`BundledHandle`] and a detached background task
/// that polls health every 30s and restarts the child with exponential
/// backoff on 3 consecutive failures.
pub async fn spawn(cfg: BundledEmbedderConfig) -> Result<BundledHandle> {
    if !cfg.binary.exists() {
        anyhow::bail!(
            "bundled llama-server binary not found at {}. Run scripts/fetch-bundled-assets.sh or reinstall the Mnemos package.",
            cfg.binary.display()
        );
    }
    if !cfg.model.exists() {
        anyhow::bail!(
            "bundled GGUF model not found at {}. Run scripts/fetch-bundled-assets.sh or reinstall the Mnemos package.",
            cfg.model.display()
        );
    }

    let initial_log_path = log_path()?;
    if let Some(parent) = initial_log_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let child = spawn_child(&cfg, &initial_log_path)?;

    let child = Arc::new(Mutex::new(Some(child)));
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

    // Wait up to 5s for the health endpoint to come up.
    let base = format!("http://{}:{}", cfg.host, cfg.port);
    let probe_url = format!("{base}/health");
    let probe_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;
    let mut ready = false;
    for _ in 0..50 {
        if shutdown_rx.has_changed().unwrap_or(false) && *shutdown_rx.borrow() {
            break;
        }
        if let Ok(r) = probe_client.get(&probe_url).send().await {
            if r.status().is_success() {
                ready = true;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if !ready {
        // Bring down the child we spawned before bailing.
        let mut guard = child.lock().await;
        if let Some(mut c) = guard.take() {
            let _ = c.start_kill();
        }
        anyhow::bail!(
            "llama-server did not become healthy within 5s; check {}",
            initial_log_path.display()
        );
    }

    // Background health task: poll every 30s, restart on 3 consecutive
    // failures. Stops when shutdown_tx fires.
    let child_for_health = child.clone();
    let cfg_for_health = cfg.clone();
    let probe_client_h = probe_client.clone();
    let probe_url_h = probe_url.clone();
    tokio::spawn(async move {
        let mut consecutive_fails = 0u32;
        let mut backoff = std::time::Duration::from_secs(1);
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
        tick.tick().await;
        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() { break; }
                }
                _ = tick.tick() => {
                    let ok = probe_client_h
                        .get(&probe_url_h)
                        .send()
                        .await
                        .map(|r| r.status().is_success())
                        .unwrap_or(false);
                    if ok {
                        consecutive_fails = 0;
                        backoff = std::time::Duration::from_secs(1);
                    } else {
                        consecutive_fails += 1;
                        if consecutive_fails >= 3 {
                            tracing::warn!("llama-server unhealthy; restarting (backoff {:?})", backoff);
                            tokio::time::sleep(backoff).await;
                            backoff = std::cmp::min(backoff * 2, std::time::Duration::from_secs(60));
                            consecutive_fails = 0;
                            // Restart: kill old child, spawn new one.
                            // Re-open the log file in append mode so
                            // post-restart output is captured rather than
                            // silently dropped to /dev/null (P1-11).
                            let mut guard = child_for_health.lock().await;
                            if let Some(mut c) = guard.take() {
                                let _ = c.start_kill();
                                let _ = c.wait().await;
                            }
                            match log_path() {
                                Ok(restart_log) => match spawn_child(&cfg_for_health, &restart_log) {
                                    Ok(c) => *guard = Some(c),
                                    Err(e) => tracing::error!("llama-server restart failed: {e}"),
                                },
                                Err(e) => tracing::error!("llama-server restart: cannot resolve log path: {e}"),
                            }
                        }
                    }
                }
            }
        }
    });

    Ok(BundledHandle { child, shutdown_tx })
}

fn log_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos").context("ProjectDirs")?;
    Ok(dirs.data_local_dir().join("logs").join("llama-server.log"))
}
