//! Builds the configured `LlmProvider` for the daemon.

use crate::config::{Config, LlmKind};
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::providers::ollama_llm::{OllamaLlm, OllamaLlmConfig};
use mnemos_core::providers::openai_llm::{self, OpenAiLlm, OpenAiLlmConfig};
use mnemos_core::providers::LlmProvider;
use std::sync::Arc;

/// Construct the LLM provider from config, or `None` when `kind = none`.
pub fn build_llm_for_daemon(cfg: &Config) -> Option<Arc<dyn LlmProvider>> {
    match cfg.llm.kind {
        LlmKind::None => None,
        LlmKind::Mock => Some(Arc::new(MockLlm::new())),
        LlmKind::Bundled => {
            // The bundled llama-server exposes an OpenAI-compatible
            // /v1/chat/completions endpoint on the configured port.
            let base_url = cfg.llm.url.clone();
            let oc = OpenAiLlmConfig {
                base_url: if base_url.is_empty() {
                    "http://127.0.0.1:7425".into()
                } else {
                    base_url
                },
                // llama-server does not require an API key, but the
                // OpenAiLlm client rejects an empty key. Use a dummy.
                api_key: "bundled".into(),
                model: cfg.llm.model.clone(),
                timeout_secs: cfg.llm.timeout_secs,
            };
            match OpenAiLlm::new(&oc) {
                Ok(llm) => Some(Arc::new(llm)),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to init bundled LLM client; learning pipeline disabled");
                    None
                }
            }
        }
        LlmKind::Ollama => {
            // Default to gemma4:12b when user hasn't explicitly set a model,
            // or is still using the bundled model name from the default config.
            let model = if cfg.llm.model.is_empty()
                || cfg.llm.model == "Qwen3-0.6B"
                || cfg.llm.model == "llama3.2"
            {
                "gemma4:12b".to_string()
            } else {
                cfg.llm.model.clone()
            };
            let url = if cfg.llm.url.is_empty()
                || cfg.llm.url == "http://127.0.0.1:7425"
            {
                "http://localhost:11434".to_string()
            } else {
                cfg.llm.url.clone()
            };
            let oc = OllamaLlmConfig {
                base_url: url,
                model,
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
        LlmKind::OpenAi => match openai_llm::config_from_env() {
            Ok(mut oc) => {
                if !cfg.llm.model.is_empty() && cfg.llm.model != "llama3.2" {
                    oc.model = cfg.llm.model.clone();
                }
                oc.timeout_secs = cfg.llm.timeout_secs;
                match OpenAiLlm::new(&oc) {
                    Ok(llm) => Some(Arc::new(llm)),
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to init OpenAI LLM; learning pipeline disabled");
                        None
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "OpenAI LLM env not configured; learning pipeline disabled");
                None
            }
        },
    }
}
