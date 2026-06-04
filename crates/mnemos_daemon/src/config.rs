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
    pub openai: OpenAiConfig,
    pub reranker: RerankerConfig,
    pub retrieval: RetrievalConfig,
    pub mcp: McpConfig,
    pub logging: LoggingConfig,
    pub reflection: ReflectionConfig,
    pub community: CommunityConfig,
    pub sync: SyncConfig,
    pub autonomy: AutonomyConfig,
}

/// Cross-cutting OpenAI credentials shared by the OpenAI embedder and LLM
/// backends. `api_key` is read from this struct first; if empty, the
/// backends fall back to the `OPENAI_API_KEY` environment variable. Empty
/// values are normal on fresh installs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub api_key: String,
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
    Bundled,
    Ollama,
    OpenAi,
    Mock,
    None,
}

impl EmbedderKind {
    /// Stable lowercase tag matching the `serde(rename_all = "lowercase")`
    /// repr. Mirrors the strings written into `vault_meta.embedder_kind` so
    /// the doctor can compare vault-vs-config without re-deriving them.
    pub fn as_str(&self) -> &'static str {
        match self {
            EmbedderKind::Bundled => "bundled",
            EmbedderKind::Ollama => "ollama",
            EmbedderKind::OpenAi => "openai",
            EmbedderKind::Mock => "mock",
            EmbedderKind::None => "none",
        }
    }
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
        // Fresh installs default to the bundled llama-server backend. Ollama
        // remains opt-in via `MNEMOS_EMBEDDER=ollama` or config.toml.
        Self {
            kind: EmbedderKind::Bundled,
            url: "http://127.0.0.1:7424".into(),
            model: "all-MiniLM-L6-v2".into(),
            dim: 384,
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
    OpenAi,
    Mock,
    None,
}

impl Default for LlmConfig {
    fn default() -> Self {
        // Fresh installs default to no LLM: reflections and community
        // summaries silently skip until the user opts into Ollama or OpenAI
        // via `MNEMOS_LLM` or config.toml.
        Self {
            kind: LlmKind::None,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    pub kind: SyncKind,
    /// Periodic push/pull interval in seconds (0 = disabled, manual only).
    pub interval_secs: u64,
    pub git: GitSyncConfig,
    pub s3: S3SyncConfig,
    /// Reserved for Turso embedded replicas. Not wired in v0.6.0.
    pub turso: TursoSyncConfig,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SyncKind {
    #[default]
    None,
    Filesystem,
    Git,
    S3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GitSyncConfig {
    pub remote: String,
    pub branch: String,
}

impl Default for GitSyncConfig {
    fn default() -> Self {
        Self {
            remote: String::new(),
            branch: "main".into(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct S3SyncConfig {
    pub remote: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TursoSyncConfig {
    pub enabled: bool,
    pub url: String,
    pub auth_token: String,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            kind: SyncKind::None,
            interval_secs: 0,
            git: GitSyncConfig::default(),
            s3: S3SyncConfig::default(),
            turso: TursoSyncConfig::default(),
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

/// Resolve the path to the daemon's `config.toml`.
///
/// Honors `MNEMOS_CONFIG_PATH` when set (used by tests and operators who want
/// to point the daemon at a non-default location). Otherwise falls back to the
/// XDG-managed `~/.config/mnemos/config.toml`.
pub fn default_config_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("MNEMOS_CONFIG_PATH") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .context("could not resolve XDG config dir")?;
    Ok(dirs.config_dir().join("config.toml"))
}

fn apply_env_overrides(cfg: &mut Config) {
    if let Ok(v) = std::env::var("MNEMOS_EMBEDDER") {
        cfg.embedder.kind = match v.as_str() {
            "bundled" => EmbedderKind::Bundled,
            "ollama" => EmbedderKind::Ollama,
            "openai" => EmbedderKind::OpenAi,
            "mock" => EmbedderKind::Mock,
            "none" => EmbedderKind::None,
            _ => EmbedderKind::Bundled,
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
            "ollama" => LlmKind::Ollama,
            "openai" => LlmKind::OpenAi,
            "mock" => LlmKind::Mock,
            "none" => LlmKind::None,
            _ => LlmKind::None,
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

// ── Autonomy ──────────────────────────────────────────────────────────────────

/// Configuration for the autonomy layer (capture, retention, recall budget).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutonomyConfig {
    /// Whether the session-end hook captures transcripts at all.
    /// Defaults to `true`.
    pub capture: bool,
    /// Raw-chunk retention policy after the pipeline has distilled a session.
    ///
    /// - `"distill-and-prune"` (default): delete raw chunks after distillation.
    /// - `"keep-raw"`: retain raw chunks indefinitely.
    pub retention: String,
    /// Maximum characters of recall context injected by the `user-prompt` hook.
    /// Defaults to 1200 (~300 tokens at 4 chars/token).
    pub recall_budget_chars: usize,
}

impl Default for AutonomyConfig {
    fn default() -> Self {
        Self {
            capture: true,
            retention: "distill-and-prune".to_string(),
            recall_budget_chars: 1200,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A config TOML that does not contain an `[autonomy]` section must
    /// deserialize with the `AutonomyConfig` defaults.
    #[test]
    fn autonomy_defaults_when_section_absent() {
        let toml = r#"
[daemon]
port = 7423
"#;
        let cfg: Config = toml::from_str(toml).expect("must parse");
        assert!(cfg.autonomy.capture, "capture defaults to true");
        assert_eq!(
            cfg.autonomy.retention, "distill-and-prune",
            "retention defaults to distill-and-prune"
        );
        assert_eq!(
            cfg.autonomy.recall_budget_chars, 1200,
            "recall_budget_chars defaults to 1200"
        );
    }

    /// Values present in `[autonomy]` override the defaults.
    #[test]
    fn autonomy_section_overrides_defaults() {
        let toml = r#"
[autonomy]
retention = "keep-raw"
capture = false
recall_budget_chars = 600
"#;
        let cfg: Config = toml::from_str(toml).expect("must parse");
        assert!(!cfg.autonomy.capture, "capture should be false");
        assert_eq!(cfg.autonomy.retention, "keep-raw");
        assert_eq!(cfg.autonomy.recall_budget_chars, 600);
    }

    /// A partial `[autonomy]` section only overrides the keys that are present;
    /// unspecified keys still use their defaults.
    #[test]
    fn autonomy_partial_section_merges_with_defaults() {
        let toml = r#"
[autonomy]
retention = "keep-raw"
"#;
        let cfg: Config = toml::from_str(toml).expect("must parse");
        assert!(
            cfg.autonomy.capture,
            "unspecified capture should default to true"
        );
        assert_eq!(cfg.autonomy.retention, "keep-raw");
        assert_eq!(
            cfg.autonomy.recall_budget_chars, 1200,
            "unspecified recall_budget_chars should default to 1200"
        );
    }

    /// The default Config (via Default::default()) also uses AutonomyConfig defaults.
    #[test]
    fn config_default_has_autonomy_defaults() {
        let cfg = Config::default();
        assert!(cfg.autonomy.capture);
        assert_eq!(cfg.autonomy.retention, "distill-and-prune");
        assert_eq!(cfg.autonomy.recall_budget_chars, 1200);
    }
}
