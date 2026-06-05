//! Mnemos daemon: long-running HTTP + WebSocket + MCP server.

#![deny(rust_2018_idioms)]
#![warn(clippy::all)]

pub mod auth;
pub mod bundled_embedder;
pub mod config;
pub mod connectors;
pub mod error;
pub mod events;
pub mod llm;
pub mod mcp;
pub mod pid;
pub mod pipeline_runner;
pub mod pipeline_status;
pub mod routes;
pub mod state;
pub mod sync_worker;

use anyhow::Result;
use mnemos_core::vault::Vault;

use crate::config::Config;
use crate::state::AppState;
use std::sync::Arc;

/// Block on the axum service until the listener errors or the future is dropped.
pub async fn serve(listener: tokio::net::TcpListener, app: axum::Router) -> anyhow::Result<()> {
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

pub async fn build_app(config: Config, vault: Vault) -> Result<(axum::Router, AppState)> {
    build_app_with_reranker(config, vault, None).await
}

pub async fn build_app_with_reranker(
    config: Config,
    vault: Vault,
    reranker: Option<Arc<dyn mnemos_core::providers::Reranker>>,
) -> Result<(axum::Router, AppState)> {
    let (app, state, _pipeline, _sync, _bundled) =
        build_app_full(config, vault, reranker, None).await?;
    Ok((app, state))
}

/// Full constructor: also wires the LLM and spawns the pipeline runner when an
/// LLM is configured, plus the periodic sync worker when sync is enabled, and
/// the bundled llama-server child process when the embedder kind is
/// `EmbedderKind::Bundled`.
///
/// Returns the handles (for graceful shutdown) when each was spawned.
pub async fn build_app_full(
    config: Config,
    vault: Vault,
    reranker: Option<Arc<dyn mnemos_core::providers::Reranker>>,
    llm: Option<Arc<dyn mnemos_core::providers::LlmProvider>>,
) -> Result<(
    axum::Router,
    AppState,
    Option<crate::pipeline_runner::PipelineHandle>,
    Option<crate::sync_worker::SyncHandle>,
    Option<crate::bundled_embedder::BundledHandle>,
)> {
    let token_path = config_token_path()?;
    let token = auth::ensure_token(&token_path)?;
    let bundled = if matches!(config.embedder.kind, config::EmbedderKind::Bundled) {
        let bcfg = bundled_embedder::BundledEmbedderConfig::default();
        // In dev / test environments where the bundled binary may not be
        // installed, skip the spawn with a warning rather than aborting startup.
        // Packaged installs always have the binary in /usr/lib/mnemos/.
        if !bcfg.binary.exists() || !bcfg.model.exists() {
            tracing::warn!(
                binary = %bcfg.binary.display(),
                model = %bcfg.model.display(),
                "bundled embedder configured but assets missing; skipping spawn (run scripts/fetch-bundled-assets.sh or reinstall the Mnemos package)"
            );
            None
        } else {
            Some(
                bundled_embedder::spawn(bcfg)
                    .await
                    .map_err(|e| anyhow::anyhow!("spawn bundled embedder: {e}"))?,
            )
        }
    } else {
        None
    };
    let state = AppState {
        config: Arc::new(config),
        vault,
        token,
        events: events::EventBus::new(),
        reranker,
        llm,
        pipeline_status: pipeline_status::PipelineStatus::new(),
        rebuild_status: Arc::new(tokio::sync::Mutex::new(
            mnemos_core::embedder_rebuild::RebuildStatus::Idle,
        )),
        rebuild_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };
    let app = routes::build_router(state.clone());
    let pipeline = if state.llm.is_some() {
        Some(pipeline_runner::spawn(state.clone()))
    } else {
        None
    };
    let sync = sync_worker::spawn(state.clone());
    Ok((app, state, pipeline, sync, bundled))
}

/// Resolve the canonical path to the daemon's auth token file.
///
/// The file lives at `~/.config/mnemos/token` (XDG config dir).
pub fn token_path() -> Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .ok_or_else(|| anyhow::anyhow!("could not resolve XDG config dir"))?;
    Ok(dirs.config_dir().join("token"))
}

fn config_token_path() -> Result<std::path::PathBuf> {
    token_path()
}

/// Resolve the canonical path to the daemon's PID file.
///
/// Uses the XDG state directory when available (e.g. `~/.local/state/mnemos/mnemosd.pid`);
/// falls back to the data directory on platforms where state dir is absent.
pub fn pid_path() -> Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .ok_or_else(|| anyhow::anyhow!("could not resolve XDG state dir"))?;
    let state_dir = dirs.state_dir().unwrap_or_else(|| dirs.data_dir());
    Ok(state_dir.join("mnemosd.pid"))
}
