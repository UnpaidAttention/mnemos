mod cli;
mod commands;
pub mod daemon_ctl;
pub mod transcript;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Cmd};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let args = Cli::parse();
    match args.command {
        Cmd::Remember(a) => commands::remember::run(args.vault, args.json, a).await,
        Cmd::Recall(a) => commands::recall::run(args.vault, args.json, a).await,
        Cmd::Get { id } => commands::get::run(args.vault, args.json, id).await,
        Cmd::List(a) => commands::list::run(args.vault, args.json, a).await,
        Cmd::Forget { id, reason } => {
            commands::forget::run(args.vault, args.json, id, reason).await
        }
        Cmd::Rebuild => commands::rebuild::run(args.vault, args.json).await,
        Cmd::Doctor => commands::doctor::run(args.vault, args.json).await,
        Cmd::Status => commands::status::run(args.vault, args.json).await,
        Cmd::Embed(a) => commands::embed::run(args.vault, args.json, a).await,
        Cmd::Daemon(a) => commands::daemon::run(args.vault, args.json, a).await,
        Cmd::Decay => commands::decay::run(args.vault, args.json).await,
        Cmd::Sync(a) => commands::sync::run(args.vault, args.json, a.action).await,
        Cmd::Export {
            vault,
            output,
            json,
        } => commands::export::run(vault, output, json).await,
        Cmd::Import { vault, input, json } => commands::import::run(vault, input, json).await,
        Cmd::Service(a) => commands::service::run(args.vault, args.json, a),
        Cmd::EmbedRebuild(a) => {
            // Auto-detect default model + dim per target if not supplied.
            let (default_model, default_dim) = match a.target.as_str() {
                "bundled" => ("all-MiniLM-L6-v2", 384u32),
                "ollama" => ("nomic-embed-text", 768),
                "openai" => ("text-embedding-3-small", 1536),
                "mock" => ("mock", 768),
                _ => ("", 0),
            };
            let model = a.model.unwrap_or_else(|| default_model.to_string());
            let dim = a.dim.unwrap_or(default_dim);
            let opts = commands::embed_rebuild::EmbedRebuildOpts {
                vault: args.vault,
                target_kind: a.target,
                target_model: model,
                target_dim: dim,
                json: a.json || args.json,
                poll: false,
            };
            commands::embed_rebuild::run(opts).await
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_env("MNEMOS_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
