pub mod daemon;
pub mod decay;
pub mod doctor;
pub mod embed;
pub mod embed_rebuild;
pub mod export;
pub mod forget;
pub mod get;
pub mod hook;
pub mod import;
pub mod list;
pub mod rebuild;
pub mod recall;
pub mod remember;
pub mod service;
pub mod status;
pub mod sync;

use anyhow::{anyhow, Result};
use mnemos_core::providers::{
    bundled::BundledEmbedder,
    mock::MockEmbedder,
    ollama::{OllamaConfig, OllamaEmbedder},
    openai_embedder::{self, OpenAiEmbedder},
    Embedder,
};
use mnemos_core::{paths::Paths, vault::Vault};
use mnemos_daemon::config::{Config, EmbedderKind};
use std::path::PathBuf;
use std::sync::Arc;

pub async fn open_vault(vault_override: Option<PathBuf>) -> Result<Vault> {
    let paths = match vault_override {
        Some(p) => Paths::with_root(&p),
        None => Paths::default_xdg()?,
    };
    let embedder = build_embedder()?;
    Ok(Vault::open_with_embedder(paths, embedder).await?)
}

/// Build the embedder from `config.toml`, with environment-variable overrides.
///
/// This uses the same `Config::load_default()` as the daemon, ensuring the CLI
/// and daemon always agree on the embedder kind, model, URL, and dimension.
/// Previously the CLI only read `MNEMOS_EMBEDDER` env vars and defaulted to
/// `bundled` (384d), causing a dimension mismatch when the daemon was
/// configured for `ollama` (768d).
pub fn build_embedder() -> Result<Option<Arc<dyn Embedder>>> {
    let cfg = Config::load_default().unwrap_or_default();
    let ecfg = &cfg.embedder;

    match ecfg.kind {
        EmbedderKind::None => Ok(None),
        EmbedderKind::Mock => {
            let dim = if ecfg.dim > 0 { ecfg.dim } else { 768 };
            Ok(Some(Arc::new(MockEmbedder::new(dim))))
        }
        EmbedderKind::Bundled => {
            let url = if ecfg.url.is_empty() {
                "http://127.0.0.1:7424".to_string()
            } else {
                ecfg.url.clone()
            };
            let embedder =
                BundledEmbedder::new(url).map_err(|e| anyhow!("bundled embedder init: {e}"))?;
            Ok(Some(Arc::new(embedder)))
        }
        EmbedderKind::Ollama => {
            let mut oc = OllamaConfig::default();
            if !ecfg.url.is_empty() {
                oc.base_url = ecfg.url.clone();
            }
            if !ecfg.model.is_empty() {
                oc.model = ecfg.model.clone();
            }
            Ok(Some(Arc::new(OllamaEmbedder::new(oc))))
        }
        EmbedderKind::OpenAi => {
            let oc = openai_embedder::config_from_env()
                .map_err(|e| anyhow!("openai embedder env: {e}"))?;
            let e = OpenAiEmbedder::new(&oc).map_err(|e| anyhow!("openai embedder init: {e}"))?;
            Ok(Some(Arc::new(e)))
        }
    }
}

