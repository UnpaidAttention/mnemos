//! `config.toml` schema and loader.
//!
//! Resolution order: file values → environment-variable overrides → defaults.

use anyhow::{Context, Result};
use mnemos_core::retrieval::reweight::ReweightConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub daemon: DaemonConfig,
    pub vault: VaultConfig,
    pub embedder: EmbedderConfig,
    pub llm: LlmConfig,
    pub reranker: RerankerConfig,
    pub retrieval: RetrievalConfig,
    pub mcp: McpConfig,
    pub logging: LoggingConfig,
    pub reflection: ReflectionConfig,
    pub community: CommunityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub host: String,
    pub port: u16,
    pub auto_start: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VaultConfig {
    pub root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbedderConfig {
    pub kind: EmbedderKind,
    pub url: String,
    pub model: String,
    pub dim: usize,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EmbedderKind {
    Ollama,
    Mock,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RerankerConfig {
    pub enabled: bool,
    pub kind: RerankerKind,
    pub model_path: Option<PathBuf>,
    pub tokenizer_path: Option<PathBuf>,
    pub max_seq_len: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RerankerKind {
    None,
    Onnx,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RetrievalConfig {
    pub default_k: usize,
    pub rrf_k: usize,
    pub reweight: ReweightConfig,
    pub ppr_alpha: f64,
    pub ppr_iterations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    pub enabled: bool,
    pub sampling_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 7423,
            auto_start: true,
        }
    }
}

impl Default for VaultConfig {
    fn default() -> Self {
        let root = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
            .map(|p| p.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("./mnemos-data"));
        Self { root }
    }
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            kind: EmbedderKind::Ollama,
            url: "http://localhost:11434".into(),
            model: "nomic-embed-text".into(),
            dim: 768,
            timeout_secs: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    pub kind: LlmKind,
    pub url: String,
    pub model: String,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LlmKind {
    Ollama,
    Mock,
    None,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            kind: LlmKind::Ollama,
            url: "http://localhost:11434".into(),
            model: "llama3.2".into(),
            timeout_secs: 120,
        }
    }
}

impl Default for RerankerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            kind: RerankerKind::None,
            model_path: None,
            tokenizer_path: None,
            max_seq_len: 512,
        }
    }
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            default_k: 10,
            rrf_k: 60,
            reweight: ReweightConfig::default(),
            ppr_alpha: 0.85,
            ppr_iterations: 30,
        }
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sampling_enabled: true,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
            format: "compact".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReflectionConfig {
    /// Salience accumulator threshold that triggers a reflection pass.
    pub salience_threshold: f64,
    /// Max recent un-reflected memories considered per reflection pass.
    pub max_sources: usize,
}

impl Default for ReflectionConfig {
    fn default() -> Self {
        Self {
            salience_threshold: 5.0,
            max_sources: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommunityConfig {
    /// Minimum entities for a community to be summarized.
    pub min_community_size: usize,
}

impl Default for CommunityConfig {
    fn default() -> Self {
        Self {
            min_community_size: 3,
        }
    }
}

impl Config {
    /// Load config from `path`. Falls back to `Config::default()` when the file
    /// does not exist. Env-var overrides are applied after deserialization.
    pub fn load_from(path: &Path) -> Result<Self> {
        let mut cfg: Config = if path.exists() {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("read {}", path.display()))?;
            toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?
        } else {
            Config::default()
        };
        apply_env_overrides(&mut cfg);
        expand_paths(&mut cfg)?;
        Ok(cfg)
    }

    /// Load from the platform-default XDG config path.
    pub fn load_default() -> Result<Self> {
        let path = default_config_path()?;
        Self::load_from(&path)
    }
}

fn default_config_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .context("could not resolve XDG config dir")?;
    Ok(dirs.config_dir().join("config.toml"))
}

fn apply_env_overrides(cfg: &mut Config) {
    if let Ok(v) = std::env::var("MNEMOS_EMBEDDER") {
        cfg.embedder.kind = match v.as_str() {
            "mock" => EmbedderKind::Mock,
            "none" => EmbedderKind::None,
            _ => EmbedderKind::Ollama,
        };
    }
    if let Ok(v) = std::env::var("MNEMOS_OLLAMA_URL") {
        cfg.embedder.url = v;
    }
    if let Ok(v) = std::env::var("MNEMOS_OLLAMA_MODEL") {
        cfg.embedder.model = v;
    }
    if let Ok(v) = std::env::var("MNEMOS_EMBEDDER_DIM") {
        if let Ok(n) = v.parse::<usize>() {
            cfg.embedder.dim = n;
        }
    }
    if let Ok(v) = std::env::var("MNEMOS_VAULT") {
        cfg.vault.root = PathBuf::from(v);
    }
    if let Ok(v) = std::env::var("MNEMOS_DAEMON_PORT") {
        if let Ok(p) = v.parse::<u16>() {
            cfg.daemon.port = p;
        }
    }
    if let Ok(v) = std::env::var("MNEMOS_LOG") {
        cfg.logging.level = v;
    }
    if let Ok(v) = std::env::var("MNEMOS_LLM") {
        cfg.llm.kind = match v.as_str() {
            "mock" => LlmKind::Mock,
            "none" => LlmKind::None,
            _ => LlmKind::Ollama,
        };
    }
    if let Ok(v) = std::env::var("MNEMOS_LLM_URL") {
        cfg.llm.url = v;
    }
    if let Ok(v) = std::env::var("MNEMOS_LLM_MODEL") {
        cfg.llm.model = v;
    }
}

fn expand_paths(cfg: &mut Config) -> Result<()> {
    cfg.vault.root = expand_tilde(&cfg.vault.root)?;
    if let Some(p) = cfg.reranker.model_path.as_mut() {
        *p = expand_tilde(p)?;
    }
    if let Some(p) = cfg.reranker.tokenizer_path.as_mut() {
        *p = expand_tilde(p)?;
    }
    Ok(())
}

fn expand_tilde(p: &Path) -> Result<PathBuf> {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/") {
        let home = directories::UserDirs::new()
            .map(|u| u.home_dir().to_path_buf())
            .context("could not resolve home dir for ~/ expansion")?;
        Ok(home.join(rest))
    } else {
        Ok(p.to_path_buf())
    }
}
