//! Mnemos daemon: long-running HTTP + WebSocket + MCP server.

#![deny(rust_2018_idioms)]
#![warn(clippy::all)]

pub mod auth;
pub mod config;
pub mod error;
pub mod events;
pub mod mcp;
pub mod routes;
pub mod state;

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
    let token_path = config_token_path()?;
    let token = auth::ensure_token(&token_path)?;
    let state = AppState {
        config: Arc::new(config),
        vault,
        token,
        events: events::EventBus::new(),
        reranker,
    };
    let app = routes::build_router(state.clone());
    Ok((app, state))
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
