//! Manages a second `llama-server` child process for chat completions (LLM).
//!
//! Similar to `bundled_embedder.rs` but runs the model in chat-completion mode
//! (no `--embedding` flag) on a separate port (default 7425).
//!
//! The bundled LLM model (Qwen3-0.6B Q4_K_M, ~462 MB) is small enough to run
//! on CPU without bogging down the system, while still being capable of
//! structured JSON output for entity extraction, reflections, and community
//! summaries.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::{watch, Mutex};

/// Configuration for the bundled LLM llama-server child process.
#[derive(Debug, Clone)]
pub struct BundledLlmConfig {
    /// Path to the `llama-server` executable (shared with the embedder).
    pub binary: PathBuf,
    /// Path to the GGUF chat model file.
    pub model: PathBuf,
    /// Port to bind on (default 7425 — one above the embedder's 7424).
    pub port: u16,
    /// Host to bind on (default 127.0.0.1).
    pub host: String,
}

impl Default for BundledLlmConfig {
    fn default() -> Self {
        Self {
            binary: super::bundled_embedder::default_binary_path(),
            model: default_llm_model_path(),
            port: 7425,
            host: "127.0.0.1".into(),
        }
    }
}

/// Resolve the bundled LLM GGUF model path.
///
/// Order of precedence:
/// 1. `MNEMOS_BUNDLED_LLM_MODEL` env var (full path to .gguf file)
/// 2. `MNEMOS_BUNDLED_MODEL_DIR` env var + model filename
/// 3. Packaged install at `/usr/lib/mnemos/Qwen3-0.6B-Q4_K_M.gguf`
/// 4. Dev layout `assets/Qwen3-0.6B-Q4_K_M.gguf`
const LLM_MODEL_FILENAME: &str = "Qwen3-0.6B-Q4_K_M.gguf";

pub fn default_llm_model_path() -> PathBuf {
    if let Ok(env) = std::env::var("MNEMOS_BUNDLED_LLM_MODEL") {
        return PathBuf::from(env);
    }
    if let Ok(env) = std::env::var("MNEMOS_BUNDLED_MODEL_DIR") {
        return PathBuf::from(env).join(LLM_MODEL_FILENAME);
    }
    // XDG data home: ~/.local/share/mnemos/assets/<model>
    if let Some(xdg) = super::bundled_embedder::xdg_assets_dir() {
        let p = xdg.join(LLM_MODEL_FILENAME);
        if p.exists() {
            return p;
        }
    }
    let install = PathBuf::from("/usr/lib/mnemos").join(LLM_MODEL_FILENAME);
    if install.exists() {
        return install;
    }
    PathBuf::from("assets").join(LLM_MODEL_FILENAME)
}

/// Handle returned by [`spawn`].
pub struct BundledLlmHandle {
    child: Arc<Mutex<Option<Child>>>,
    shutdown_tx: watch::Sender<bool>,
}

impl BundledLlmHandle {
    /// Gracefully stop the child process.
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let mut guard = self.child.lock().await;
        if let Some(mut c) = guard.take() {
            let _ = c.start_kill();
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), c.wait()).await;
        }
    }
}

/// Spawn a single `llama-server` child for chat completions.
fn spawn_child(cfg: &BundledLlmConfig, log_path: &std::path::Path) -> Result<Child> {
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
        // Chat completion mode — no --embedding flag.
        // 8K context allows reflection prompts (which include multiple source
        // memories) to fit comfortably. Qwen3-0.6B supports up to 32K.
        .arg("--ctx-size")
        .arg("8192")
        // Single-slot: pipeline tasks are sequential, no need for parallelism.
        .arg("--parallel")
        .arg("1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawn {}", cfg.binary.display()))
}

/// Spawn `llama-server` for chat completions and return a handle immediately.
///
/// Like `bundled_embedder::spawn`, this is non-blocking: the server warms up
/// in the background while the daemon serves requests. Pipeline tasks that
/// need the LLM will get errors until it's ready, which is acceptable since
/// they're best-effort background tasks.
pub async fn spawn(cfg: BundledLlmConfig, llm_ready: Arc<tokio::sync::watch::Sender<bool>>) -> Result<BundledLlmHandle> {
    if !cfg.binary.exists() {
        anyhow::bail!(
            "bundled llama-server binary not found at {}. Run scripts/fetch-bundled-assets.sh.",
            cfg.binary.display()
        );
    }
    if !cfg.model.exists() {
        anyhow::bail!(
            "bundled LLM model not found at {}. Run scripts/fetch-bundled-assets.sh.",
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

    let probe_url = format!("http://{}:{}/health", cfg.host, cfg.port);
    let probe_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;

    let child_for_health = child.clone();
    let cfg_for_health = cfg.clone();
    let probe_client_h = probe_client;
    let probe_url_h = probe_url;

    tokio::spawn(async move {
        // Phase 1: initial readiness wait (up to 180s — model load may be slow on CPU)
        let mut ready = false;
        for _ in 0..1800u16 {
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
            tracing::info!("bundled LLM server is healthy and ready (pipeline enabled)");
            let _ = llm_ready.send(true);
        } else {
            tracing::warn!(
                "bundled LLM server did not become healthy within 180s; \
                 learning pipeline will be unavailable"
            );
        }

        // Phase 2: steady-state 30s watchdog
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
                            tracing::warn!("bundled LLM server unhealthy; restarting (backoff {:?})", backoff);
                            tokio::time::sleep(backoff).await;
                            backoff = std::cmp::min(backoff * 2, std::time::Duration::from_secs(60));
                            consecutive_fails = 0;
                            let mut guard = child_for_health.lock().await;
                            if let Some(mut c) = guard.take() {
                                let _ = c.start_kill();
                                let _ = c.wait().await;
                            }
                            match log_path() {
                                Ok(ref restart_log) => match spawn_child(&cfg_for_health, restart_log) {
                                    Ok(c) => *guard = Some(c),
                                    Err(e) => tracing::error!("bundled LLM server restart failed: {e}"),
                                },
                                Err(e) => tracing::error!(
                                    "bundled LLM server restart: cannot resolve log path: {e}"
                                ),
                            }
                        }
                    }
                }
            }
        }
    });

    Ok(BundledLlmHandle { child, shutdown_tx })
}

fn log_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos").context("ProjectDirs")?;
    Ok(dirs
        .data_local_dir()
        .join("logs")
        .join("llama-server-llm.log"))
}
