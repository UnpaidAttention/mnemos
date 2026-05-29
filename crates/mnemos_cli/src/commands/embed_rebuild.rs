//! `mnemos embed-rebuild` — re-embed every memory with a target embedder
//! (Plan 9 Task 11).
//!
//! Two execution modes:
//!   * **Daemon up** — refuses to run in-process; prints a curl example
//!     pointing at the daemon's `/v1/embed-rebuild/start` endpoint.
//!   * **No daemon** — opens the vault directly and calls
//!     [`mnemos_core::embedder_rebuild::rebuild`] in-process.

use anyhow::{anyhow, Result};
use mnemos_core::embedder_rebuild::{rebuild, RebuildOptions, RebuildStatus};
use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use std::path::PathBuf;

/// Parsed CLI arguments for `mnemos embed-rebuild`.
#[derive(Debug, Clone)]
pub struct EmbedRebuildOpts {
    pub vault: Option<PathBuf>,
    pub target_kind: String,
    pub target_model: String,
    pub target_dim: u32,
    pub json: bool,
    /// Reserved for future use (live progress polling). v0.8.0 always runs to
    /// completion synchronously.
    #[allow(dead_code)]
    pub poll: bool,
}

pub async fn run(opts: EmbedRebuildOpts) -> Result<()> {
    // If a daemon is alive, refuse to run in-process and point at the REST API.
    if daemon_is_running() {
        let cfg = mnemos_daemon::config::Config::load_default().unwrap_or_default();
        let url = format!(
            "http://{}:{}/v1/embed-rebuild/start",
            cfg.daemon.host, cfg.daemon.port
        );
        let body = format!(
            r#"{{"target_kind":"{}","target_model":"{}","target_dim":{}}}"#,
            opts.target_kind, opts.target_model, opts.target_dim
        );
        eprintln!("mnemosd is running — refusing to run rebuild in-process.");
        eprintln!("Use the daemon's REST endpoint instead:");
        eprintln!();
        eprintln!("  curl -X POST {url} \\");
        eprintln!("    -H 'authorization: Bearer $(cat ~/.config/mnemos/token)' \\");
        eprintln!("    -H 'content-type: application/json' \\");
        eprintln!("    -d '{body}'");
        eprintln!();
        return Err(anyhow!("daemon running; rebuild must go via REST"));
    }

    let paths = match opts.vault.as_ref() {
        Some(p) => Paths::with_root(p),
        None => Paths::default_xdg()?,
    };
    // Open the vault WITHOUT a configured embedder. The rebuild builds its own
    // target embedder; the vault doesn't need one for the migration itself.
    // Bypassing crate::commands::open_vault avoids the kind-mismatch check
    // that fires when we're switching embedders (the whole point of the run).
    let vault = Vault::open(paths).await?;

    let rebuild_opts = RebuildOptions {
        target_kind: opts.target_kind.clone(),
        target_model: opts.target_model.clone(),
        target_dim: opts.target_dim,
        actor: "cli".into(),
    };
    let status = rebuild(&vault, rebuild_opts).await?;

    if opts.json {
        println!("{}", serde_json::to_string(&status)?);
    } else {
        match &status {
            RebuildStatus::Completed {
                processed,
                skipped,
                total,
                swapped,
            } => {
                println!(
                    "rebuild complete — processed: {processed}  skipped: {skipped}  total: {total}  swapped: {swapped}"
                );
                println!(
                    "vault is now embedded with {} model {} ({}d)",
                    opts.target_kind, opts.target_model, opts.target_dim
                );
            }
            RebuildStatus::Failed { error, processed } => {
                eprintln!("rebuild failed after {processed} memories: {error}");
                eprintln!("re-run to resume from the shadow table");
                return Err(anyhow!("rebuild failed: {error}"));
            }
            other => {
                println!("rebuild status: {other:?}");
            }
        }
    }
    Ok(())
}

/// Returns `true` if a daemon PID file exists AND the process is alive.
fn daemon_is_running() -> bool {
    let Ok(pid_path) = mnemos_daemon::pid_path() else {
        return false;
    };
    if !pid_path.exists() {
        return false;
    }
    let Ok(pid) = mnemos_daemon::pid::read_pid(&pid_path) else {
        return false;
    };
    process_alive(pid)
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    // SAFETY: kill(pid, 0) is async-signal-safe; signal 0 only checks existence.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn embed_rebuild_in_process_smoke() {
        let tmp = TempDir::new().unwrap();
        let opts = EmbedRebuildOpts {
            vault: Some(tmp.path().to_path_buf()),
            target_kind: "mock".into(),
            target_model: "mock-v2".into(),
            target_dim: 384,
            json: true,
            poll: false,
        };
        // No memories yet → run should succeed with processed=0.
        run(opts).await.unwrap();
    }
}
