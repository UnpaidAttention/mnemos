use mnemos_daemon::config::{Config, EmbedderKind, RerankerKind};
use tempfile::TempDir;

#[test]
fn config_loads_from_toml_file() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
[daemon]
host = "127.0.0.1"
port = 9999

[vault]
root = "/tmp/test-vault"

[embedder]
kind = "mock"
dim = 384

[reranker]
enabled = false
"#,
    )
    .unwrap();
    let cfg = Config::load_from(&path).unwrap();
    assert_eq!(cfg.daemon.port, 9999);
    assert!(matches!(cfg.embedder.kind, EmbedderKind::Mock));
    assert_eq!(cfg.embedder.dim, 384);
    assert!(!cfg.reranker.enabled);
    assert!(matches!(cfg.reranker.kind, RerankerKind::None));
}

#[test]
fn config_defaults_when_file_absent() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("does-not-exist.toml");
    let cfg = Config::load_from(&path).unwrap(); // fallback to defaults
    assert_eq!(cfg.daemon.port, 7423);
    // Fresh installs default to the bundled llama-server backend (Plan 9).
    assert!(matches!(cfg.embedder.kind, EmbedderKind::Bundled));
    assert_eq!(cfg.embedder.dim, 384);
    assert!(!cfg.reranker.enabled);
}

#[test]
fn env_overrides_take_precedence() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
[embedder]
kind = "ollama"
url = "http://localhost:11434"
"#,
    )
    .unwrap();
    std::env::set_var("MNEMOS_OLLAMA_URL", "http://override:11434");
    let cfg = Config::load_from(&path).unwrap();
    assert_eq!(cfg.embedder.url, "http://override:11434");
    std::env::remove_var("MNEMOS_OLLAMA_URL");
}

#[test]
fn reweight_defaults_match_recall_opts() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    std::fs::write(&path, "").unwrap();
    let cfg = Config::load_from(&path).unwrap();
    assert_eq!(cfg.retrieval.reweight.recency_decay, 0.02);
    assert_eq!(cfg.retrieval.reweight.tier_weight_working, 2.0);
}

#[test]
fn llm_defaults_to_none() {
    // Fresh installs default to `LlmKind::None` (Plan 9 Task 8). Reflections
    // and community summaries silently skip until the user opts in via
    // `MNEMOS_LLM=ollama` or `MNEMOS_LLM=openai`.
    use mnemos_daemon::config::{Config, LlmKind};
    let cfg = Config::default();
    assert_eq!(cfg.llm.kind, LlmKind::None);
    // The url/model defaults remain set so opting in via env keeps working.
    assert_eq!(cfg.llm.model, "llama3.2");
    assert!(cfg.llm.url.contains("11434"));
}

#[test]
fn retrieval_ppr_defaults() {
    use mnemos_daemon::config::Config;
    let cfg = Config::default();
    assert_eq!(cfg.retrieval.ppr_alpha, 0.85);
    assert_eq!(cfg.retrieval.ppr_iterations, 30);
}

/// P2-21: MNEMOS_LOG_FORMAT env var must override the logging.format field.
/// This test is sequential (env mutation) — run with `-- --test-threads=1`
/// if flakiness arises, but env vars are isolated per process in CI.
#[test]
fn mnemos_log_format_env_override() {
    use mnemos_daemon::config::Config;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("lf.toml");
    std::fs::write(&path, "[logging]\nformat = \"compact\"\n").unwrap();

    // Baseline: toml value.
    let cfg = Config::load_from(&path).unwrap();
    assert_eq!(cfg.logging.format, "compact");

    // With env override to "json".
    std::env::set_var("MNEMOS_LOG_FORMAT", "json");
    let cfg_json = Config::load_from(&path).unwrap();
    assert_eq!(
        cfg_json.logging.format, "json",
        "MNEMOS_LOG_FORMAT=json must override toml logging.format"
    );
    std::env::remove_var("MNEMOS_LOG_FORMAT");

    // With env override back to "compact".
    std::env::set_var("MNEMOS_LOG_FORMAT", "compact");
    let cfg_compact = Config::load_from(&path).unwrap();
    assert_eq!(cfg_compact.logging.format, "compact");
    std::env::remove_var("MNEMOS_LOG_FORMAT");
}

/// P2-21: BundledEmbedder::new() must return Result, not panic.
#[test]
fn bundled_embedder_new_returns_result() {
    use mnemos_core::providers::bundled::BundledEmbedder;
    // Building the reqwest client should succeed in normal environments.
    let result = BundledEmbedder::new("http://127.0.0.1:7424");
    assert!(
        result.is_ok(),
        "BundledEmbedder::new must return Ok in a normal environment"
    );
}
