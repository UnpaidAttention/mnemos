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
    assert!(matches!(cfg.embedder.kind, EmbedderKind::Ollama));
    assert_eq!(cfg.embedder.dim, 768);
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
