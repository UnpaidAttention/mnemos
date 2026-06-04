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

/// Embedder selection by env var. Mirrors the daemon's `build_embedder_for_daemon`
/// so the CLI's direct-vault commands (`remember`, `recall`) use the same backend
/// the daemon seeded the vault with. `Vault::open_with_embedder` enforces that the
/// selected kind/dim matches the vault's stored metadata, so a mismatch surfaces a
/// clear error rather than silently corrupting vectors.
///
/// - `MNEMOS_EMBEDDER=bundled` (default) → bundled llama-server HTTP client
///   (`MNEMOS_BUNDLED_URL`, default `http://127.0.0.1:7424`). Requires `mnemosd`
///   to be running, since only the daemon spawns the llama-server child process.
/// - `MNEMOS_EMBEDDER=ollama`  → `OllamaEmbedder` (`MNEMOS_OLLAMA_URL`, `MNEMOS_OLLAMA_MODEL`)
/// - `MNEMOS_EMBEDDER=openai`  → `OpenAiEmbedder` (`OPENAI_API_KEY`, `OPENAI_BASE_URL`, `MNEMOS_EMBEDDER_MODEL`)
/// - `MNEMOS_EMBEDDER=mock`    → deterministic `MockEmbedder` (`MNEMOS_EMBEDDER_DIM`, default 768)
/// - `MNEMOS_EMBEDDER=none`    → no embedder; BM25-only mode
pub fn build_embedder() -> Result<Option<Arc<dyn Embedder>>> {
    let kind = std::env::var("MNEMOS_EMBEDDER").unwrap_or_else(|_| "bundled".into());
    match kind.as_str() {
        "none" => Ok(None),
        "mock" => {
            let dim = std::env::var("MNEMOS_EMBEDDER_DIM")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(768);
            Ok(Some(Arc::new(MockEmbedder::new(dim))))
        }
        "bundled" => {
            let url = std::env::var("MNEMOS_BUNDLED_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:7424".into());
            Ok(Some(Arc::new(BundledEmbedder::new(url))))
        }
        "ollama" => {
            let mut cfg = OllamaConfig::default();
            if let Ok(url) = std::env::var("MNEMOS_OLLAMA_URL") {
                cfg.base_url = url;
            }
            if let Ok(model) = std::env::var("MNEMOS_OLLAMA_MODEL") {
                cfg.model = model;
            }
            Ok(Some(Arc::new(OllamaEmbedder::new(cfg))))
        }
        "openai" => {
            let cfg = openai_embedder::config_from_env()
                .map_err(|e| anyhow!("openai embedder env: {e}"))?;
            let e = OpenAiEmbedder::new(&cfg).map_err(|e| anyhow!("openai embedder init: {e}"))?;
            Ok(Some(Arc::new(e)))
        }
        other => anyhow::bail!("unknown MNEMOS_EMBEDDER value: {other:?}"),
    }
}
