mod cli;
mod commands;

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
