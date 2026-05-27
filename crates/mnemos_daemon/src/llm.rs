//! Builds the configured `LlmProvider` for the daemon.

use crate::config::{Config, LlmKind};
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::providers::ollama_llm::{OllamaLlm, OllamaLlmConfig};
use mnemos_core::providers::LlmProvider;
use std::sync::Arc;

/// Construct the LLM provider from config, or `None` when `kind = none`.
pub fn build_llm_for_daemon(cfg: &Config) -> Option<Arc<dyn LlmProvider>> {
    match cfg.llm.kind {
        LlmKind::None => None,
        LlmKind::Mock => Some(Arc::new(MockLlm::new())),
        LlmKind::Ollama => {
            let oc = OllamaLlmConfig {
                base_url: cfg.llm.url.clone(),
                model: cfg.llm.model.clone(),
                timeout_secs: cfg.llm.timeout_secs,
            };
            match OllamaLlm::new(oc) {
                Ok(llm) => Some(Arc::new(llm)),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to init Ollama LLM; learning pipeline disabled");
                    None
                }
            }
        }
    }
}
