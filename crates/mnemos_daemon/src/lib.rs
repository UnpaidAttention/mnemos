//! Mnemos daemon: long-running HTTP + WebSocket + MCP server.

#![deny(rust_2018_idioms)]
#![warn(clippy::all)]

pub mod auth;
pub mod config;
pub mod error;
pub mod events;
pub mod routes;
pub mod state;

use anyhow::Result;
use mnemos_core::vault::Vault;

use crate::config::Config;
use crate::state::AppState;
use std::sync::Arc;

pub async fn build_app(config: Config, vault: Vault) -> Result<(axum::Router, AppState)> {
    let token_path = config_token_path()?;
    let token = auth::ensure_token(&token_path)?;
    let state = AppState {
        config: Arc::new(config),
        vault,
        token,
        events: events::EventBus::new(),
    };
    let app = routes::build_router(state.clone());
    Ok((app, state))
}

fn config_token_path() -> Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .ok_or_else(|| anyhow::anyhow!("could not resolve XDG config dir"))?;
    Ok(dirs.config_dir().join("token"))
}
