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
    // XDG data home: ~/.local/share/mnemos/assets/llama-server
    if let Some(xdg) = xdg_assets_dir() {
        let p = xdg.join("llama-server");
        if p.exists() {
            return p;
        }
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
/// 2. User models dir: `~/.local/share/mnemos/models/` (download-on-demand)
/// 3. XDG assets dir: `~/.local/share/mnemos/assets/`
/// 4. Packaged install at `/usr/lib/mnemos/all-MiniLM-L6-v2.Q8_0.gguf`
/// 5. Dev layout `assets/all-MiniLM-L6-v2.Q8_0.gguf`
pub fn default_model_path() -> PathBuf {
    if let Ok(env) = std::env::var("MNEMOS_BUNDLED_MODEL_DIR") {
        return PathBuf::from(env).join("all-MiniLM-L6-v2.Q8_0.gguf");
    }
    // User models dir (populated by desktop app download-on-demand)
    if let Some(models) = models_dir() {
        let p = models.join("all-MiniLM-L6-v2.Q8_0.gguf");
        if p.exists() {
            return p;
        }
    }
    // XDG data home: ~/.local/share/mnemos/assets/all-MiniLM-L6-v2.Q8_0.gguf
    if let Some(xdg) = xdg_assets_dir() {
        let p = xdg.join("all-MiniLM-L6-v2.Q8_0.gguf");
        if p.exists() {
            return p;
        }
    }
    let install = PathBuf::from("/usr/lib/mnemos/all-MiniLM-L6-v2.Q8_0.gguf");
    if install.exists() {
        return install;
    }
    PathBuf::from("assets/all-MiniLM-L6-v2.Q8_0.gguf")
}

/// Resolve `$XDG_DATA_HOME/mnemos/assets/` (defaults to `~/.local/share/mnemos/assets/`).
pub fn xdg_assets_dir() -> Option<PathBuf> {
    let base = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|b| b.data_dir().to_path_buf()))?;
    Some(base.join("mnemos").join("assets"))
}

/// Resolve `~/.local/share/mnemos/models/` — the user models directory where
/// the desktop app places models downloaded on-demand during setup.
pub fn models_dir() -> Option<PathBuf> {
    let base = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|b| b.data_dir().to_path_buf()))?;
    Some(base.join("mnemos").join("models"))
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

/// Spawn `llama-server` and return a [`BundledHandle`] **immediately** without
/// waiting for the health endpoint to come up.
///
/// # P2-13: non-blocking startup
///
/// Cold GGUF model loads routinely take 10-60 s on low-end hardware. The
/// previous implementation blocked daemon startup for up to 5 s waiting for
/// llama-server to become healthy; if it didn't come up in time the daemon
/// refused to start at all — even though BM25-only recall works immediately.
///
/// Now `spawn` fires a background readiness task that:
/// 1. Polls `<host:port>/health` every 100 ms for up to 120 s, logging an
///    info message once ready and a warning if it times out.
/// 2. After readiness (or timeout), transitions to the normal 30-second
///    watchdog loop that restarts the child on 3 consecutive failures.
///
/// The HTTP listener is returned and can serve requests while the embedder
/// warms up. The `/health` endpoint reports `embedder.status = "degraded"`
/// during this window.
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

    let base = format!("http://{}:{}", cfg.host, cfg.port);
    let probe_url = format!("{base}/health");
    let probe_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;

    // Background task: wait for initial readiness, then run the 30 s watchdog.
    let child_for_health = child.clone();
    let cfg_for_health = cfg.clone();
    let probe_client_h = probe_client;
    let probe_url_h = probe_url;
    tokio::spawn(async move {
        // --- Phase 1: initial readiness wait (up to 120 s) ---
        // Poll every 100 ms; do NOT block the caller.
        let mut ready = false;
        // 1200 iterations × 100 ms = 120 s maximum wait.
        for _ in 0..1200u16 {
            if shutdown_rx.has_changed().unwrap_or(false) && *shutdown_rx.borrow() {
                return;
            }
            if let Ok(r) = probe_client_h.get(&probe_url_h).send().await {
                if r.status().is_success() {
                    ready = true;
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        if ready {
            tracing::info!("bundled llama-server is healthy and ready");
        } else {
            tracing::warn!(
                log = %{
                    log_path().map(|p| p.display().to_string())
                        .unwrap_or_else(|_| "<unknown>".into())
                },
                "llama-server did not become healthy within 120 s; \
                 semantic recall will be unavailable until it does. \
                 Check the llama-server log for details."
            );
        }

        // --- Phase 2: steady-state 30 s watchdog ---
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
                                Err(e) => tracing::error!(
                                    "llama-server restart: cannot resolve log path: {e}"
                                ),
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
