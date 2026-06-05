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
    /// Embedding maintenance.
    Embed(EmbedArgs),
    /// Daemon process management.
    Daemon(DaemonArgs),
    /// Run a memory decay pass now (Ebbinghaus strength decay).
    Decay,
    /// Sync the vault with the configured backend.
    Sync(SyncArgs),
    /// Export the local vault as a zip.
    Export {
        /// Optional vault root (defaults to the XDG vault).
        #[arg(long)]
        vault: Option<PathBuf>,
        /// Output zip path.
        #[arg(short, long)]
        output: PathBuf,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Import a vault zip into the local vault.
    Import {
        #[arg(long)]
        vault: Option<PathBuf>,
        input: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Re-embed every memory in the vault with a different embedder.
    EmbedRebuild(EmbedRebuildArgs),
    /// systemd user service management.
    Service(ServiceArgs),
    /// Claude Code hook integration (session-start / user-prompt / session-end).
    Hook {
        /// Hook event name: session-start | user-prompt | session-end
        event: String,
    },
}

#[derive(clap::Args, Debug)]
pub struct EmbedRebuildArgs {
    /// Target embedder kind: bundled | ollama | openai | mock
    #[arg(long)]
    pub target: String,
    /// Model identifier (e.g. "all-MiniLM-L6-v2", "nomic-embed-text",
    /// "text-embedding-3-small"). Auto-detected per target if omitted.
    #[arg(long)]
    pub model: Option<String>,
    /// Override the target dim. Auto-detected per known model.
    #[arg(long)]
    pub dim: Option<u32>,
    /// Emit JSON status.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct SyncArgs {
    #[command(subcommand)]
    pub action: SyncAction,
}

#[derive(Subcommand, Debug)]
pub enum SyncAction {
    Push,
    Pull,
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

#[derive(clap::Args, Debug)]
pub struct EmbedArgs {
    #[command(subcommand)]
    pub action: EmbedAction,
}

#[derive(Subcommand, Debug)]
pub enum EmbedAction {
    /// Report how many memories have embeddings vs not.
    Status,
    /// Embed every memory that's missing a vector.
    Backfill {
        #[arg(long, default_value_t = 8)]
        batch_size: usize,
    },
}

#[derive(clap::Args, Debug)]
pub struct DaemonArgs {
    #[command(subcommand)]
    pub action: DaemonAction,
}

#[derive(Subcommand, Debug)]
pub enum DaemonAction {
    /// Spawn `mnemosd` as a background process.
    Start,
    /// Send SIGTERM to the daemon (graceful shutdown).
    Stop,
    /// Stop then start the daemon (stop + start in sequence).
    Restart,
    /// Print whether a daemon is running, its PID, and its address.
    Status,
    /// Tail the daemon log file.
    Logs {
        #[arg(long, default_value_t = 100)]
        lines: usize,
    },
}

#[derive(clap::Args, Debug)]
pub struct ServiceArgs {
    #[command(subcommand)]
    pub action: ServiceAction,
}

#[derive(Subcommand, Debug)]
pub enum ServiceAction {
    /// Install the mnemosd systemd user unit to ~/.config/systemd/user/.
    Install,
    /// Run `systemctl --user enable --now mnemosd` (non-fatal if systemd absent).
    Enable,
    /// Show whether the systemd unit is active (falls back to /health probe).
    Status,
}
