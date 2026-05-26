use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_daemon::config::{Config, EmbedderKind};
use mnemos_daemon::{build_app_with_reranker, serve};
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "mnemosd", version, about = "Mnemos daemon")]
struct Cli {
    /// Path to config.toml (default: XDG)
    #[arg(long)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Start the daemon (default if no subcommand given).
    Serve,
    /// Print the resolved config and exit.
    PrintConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    let cfg = match args.config.as_ref() {
        Some(p) => Config::load_from(p),
        None => Config::load_default(),
    }?;

    init_tracing(&cfg.logging);

    match args.command.unwrap_or(Cmd::Serve) {
        Cmd::Serve => serve_cmd(cfg).await,
        Cmd::PrintConfig => {
            println!(
                "{}",
                toml::to_string_pretty(&cfg).unwrap_or_else(|e| e.to_string())
            );
            Ok(())
        }
    }
}

fn init_tracing(cfg: &mnemos_daemon::config::LoggingConfig) {
    let filter = EnvFilter::try_new(&cfg.level).unwrap_or_else(|_| EnvFilter::new("info"));
    if cfg.format == "json" {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .compact()
            .init();
    }
}

async fn serve_cmd(cfg: Config) -> Result<()> {
    let paths = Paths::with_root(&cfg.vault.root);
    let embedder = build_embedder_for_daemon(&cfg)?;
    let vault = Vault::open_with_embedder(paths, embedder)
        .await
        .context("opening vault")?;
    let bind = format!("{}:{}", cfg.daemon.host, cfg.daemon.port);
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("bind {bind}"))?;
    tracing::info!(addr = %listener.local_addr()?, "mnemosd listening");
    let reranker = build_reranker_for_daemon(&cfg)?;
    let (app, _state) = build_app_with_reranker(cfg, vault, reranker).await?;
    serve(listener, app).await
}

fn build_reranker_for_daemon(
    cfg: &Config,
) -> Result<Option<Arc<dyn mnemos_core::providers::Reranker>>> {
    use mnemos_daemon::config::RerankerKind;
    if !cfg.reranker.enabled || matches!(cfg.reranker.kind, RerankerKind::None) {
        return Ok(None);
    }
    #[cfg(feature = "rerank-onnx")]
    {
        use mnemos_core::providers::onnx_reranker::{OnnxReranker, OnnxRerankerConfig};
        let oc = OnnxRerankerConfig {
            model_path: cfg
                .reranker
                .model_path
                .clone()
                .ok_or_else(|| anyhow::anyhow!("reranker.model_path required"))?,
            tokenizer_path: cfg
                .reranker
                .tokenizer_path
                .clone()
                .ok_or_else(|| anyhow::anyhow!("reranker.tokenizer_path required"))?,
            max_seq_len: cfg.reranker.max_seq_len,
        };
        return Ok(Some(Arc::new(OnnxReranker::load(oc)?)));
    }
    #[cfg(not(feature = "rerank-onnx"))]
    {
        anyhow::bail!(
            "reranker.enabled = true but binary was built without --features rerank-onnx"
        );
    }
}

fn build_embedder_for_daemon(
    cfg: &Config,
) -> Result<Option<Arc<dyn mnemos_core::providers::Embedder>>> {
    use mnemos_core::providers::{
        mock::MockEmbedder,
        ollama::{OllamaConfig, OllamaEmbedder},
    };
    Ok(match cfg.embedder.kind {
        EmbedderKind::None => None,
        EmbedderKind::Mock => Some(Arc::new(MockEmbedder::new(cfg.embedder.dim))),
        EmbedderKind::Ollama => {
            let oc = OllamaConfig {
                base_url: cfg.embedder.url.clone(),
                model: cfg.embedder.model.clone(),
                dim: cfg.embedder.dim,
                timeout_secs: cfg.embedder.timeout_secs,
            };
            Some(Arc::new(OllamaEmbedder::new(oc)))
        }
    })
}
