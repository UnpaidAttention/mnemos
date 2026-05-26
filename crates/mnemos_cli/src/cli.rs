use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "mnemos", version, about = "Local-first AI memory provider")]
pub struct Cli {
    /// Override vault root. Defaults to ~/.local/share/mnemos/
    #[arg(long, global = true, env = "MNEMOS_VAULT")]
    pub vault: Option<PathBuf>,

    /// Emit machine-readable JSON output where supported.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Store something new.
    Remember(RememberArgs),
    /// Search memories (BM25 only in Plan 1).
    Recall(RecallArgs),
    /// Print a single memory by ID.
    Get { id: String },
    /// List memories with filters.
    List(ListArgs),
    /// Soft-invalidate a memory.
    Forget {
        id: String,
        #[arg(long)]
        reason: Option<String>,
    },
    /// Rebuild the DB index from files on disk.
    Rebuild,
    /// Diagnose file/DB drift and quarantine entries.
    Doctor,
    /// Quick vault health summary.
    Status,
}

#[derive(clap::Args, Debug)]
pub struct RememberArgs {
    /// Body text. If absent, read from stdin.
    pub body: Option<String>,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long, default_value = "semantic")]
    pub tier: String,
    #[arg(long, value_delimiter = ',')]
    pub tags: Vec<String>,
    #[arg(long)]
    pub importance: Option<f64>,
    #[arg(long)]
    pub workspace: Option<String>,
    #[arg(long)]
    pub source_tool: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct RecallArgs {
    pub query: String,
    #[arg(short, long, default_value_t = 10)]
    pub k: usize,
    #[arg(long, value_delimiter = ',')]
    pub tier: Vec<String>,
    #[arg(long)]
    pub workspace: Option<String>,
    #[arg(long)]
    pub include_invalid: bool,
    /// Cross-encoder rerank the top-k results.
    #[arg(long)]
    pub rerank: bool,
    /// Emit per-hit explainability trace.
    #[arg(long)]
    pub explain: bool,
}

#[derive(clap::Args, Debug)]
pub struct ListArgs {
    #[arg(long, value_delimiter = ',')]
    pub tier: Vec<String>,
    #[arg(long)]
    pub workspace: Option<String>,
    #[arg(long)]
    pub include_invalid: bool,
    #[arg(short, long, default_value_t = 50)]
    pub limit: usize,
}
