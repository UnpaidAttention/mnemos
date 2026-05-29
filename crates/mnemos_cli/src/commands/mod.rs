pub mod daemon;
pub mod decay;
pub mod doctor;
pub mod embed;
pub mod embed_rebuild;
pub mod export;
pub mod forget;
pub mod get;
pub mod import;
pub mod list;
pub mod rebuild;
pub mod recall;
pub mod remember;
pub mod status;
pub mod sync;

use anyhow::Result;
use mnemos_core::providers::{
    mock::MockEmbedder,
    ollama::{OllamaConfig, OllamaEmbedder},
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

/// Embedder selection by env var:
/// - `MNEMOS_EMBEDDER=none`   → no embedder; BM25-only mode
/// - `MNEMOS_EMBEDDER=mock`   → deterministic MockEmbedder (tests / CI)
/// - `MNEMOS_EMBEDDER=ollama` (default) → OllamaEmbedder using env or defaults
pub fn build_embedder() -> Result<Option<Arc<dyn Embedder>>> {
    let kind = std::env::var("MNEMOS_EMBEDDER").unwrap_or_else(|_| "ollama".into());
    match kind.as_str() {
        "none" => Ok(None),
        "mock" => {
            let dim = std::env::var("MNEMOS_EMBEDDER_DIM")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(768);
            Ok(Some(Arc::new(MockEmbedder::new(dim))))
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
        other => anyhow::bail!("unknown MNEMOS_EMBEDDER value: {other:?}"),
    }
}
