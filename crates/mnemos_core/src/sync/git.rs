//! Git-remote sync backend. Shells out to `git` from the vault root.

use crate::error::{MnemosError, Result};
use crate::storage::Storage;
use crate::sync::{BackendStatus, SyncBackend, SyncReport};
use async_trait::async_trait;
use chrono::Utc;
use std::path::Path;
use tokio::fs;
use tokio::process::Command;

/// Maximum time to wait for a single `git` subprocess before treating it as
/// hung.  Five minutes is generous for a first push/pull; hung remotes (SSH
/// with no timeout configured, dead network, stalled server) typically never
/// return at all, so we must bound the wait ourselves (P1-10).
const GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

pub struct GitSync {
    #[allow(dead_code)]
    storage: Storage,
    remote: String,
    branch: String,
}

impl GitSync {
    pub fn new(storage: Storage, remote: String, branch: String) -> Self {
        Self {
            storage,
            remote,
            branch,
        }
    }
}

/// Run a `git` subcommand with a 5-minute timeout.
///
/// The child is spawned with `.kill_on_drop(true)` so if the timeout fires
/// (or the future is cancelled because the sync worker is shutting down) the
/// underlying process is killed rather than left as an orphan (P1-10).
async fn run(cwd: &Path, args: &[&str]) -> Result<String> {
    use std::process::Stdio as StdioBlocking;

    let child = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .stdin(StdioBlocking::null())
        .stdout(StdioBlocking::piped())
        .stderr(StdioBlocking::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| MnemosError::Internal(format!("git invocation failed: {e}")))?;

    let wait_fut = child.wait_with_output();
    let out = tokio::time::timeout(GIT_TIMEOUT, wait_fut)
        .await
        .map_err(|_| {
            MnemosError::Internal(format!(
                "git {} timed out after {}s",
                args.join(" "),
                GIT_TIMEOUT.as_secs()
            ))
        })?
        .map_err(|e| MnemosError::Internal(format!("git wait failed: {e}")))?;

    if !out.status.success() {
        return Err(MnemosError::Internal(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

async fn ensure_gitattributes(root: &Path) -> Result<()> {
    let path = root.join(".gitattributes");
    let line = "*.md merge=mnemos-frontmatter\n";
    if !path.exists() {
        fs::write(&path, line).await?;
        return Ok(());
    }
    let cur = fs::read_to_string(&path).await?;
    if !cur.contains("mnemos-frontmatter") {
        fs::write(&path, format!("{cur}\n{line}")).await?;
    }
    Ok(())
}

async fn ensure_merge_driver_config(root: &Path) -> Result<()> {
    let _ = run(
        root,
        &[
            "config",
            "merge.mnemos-frontmatter.name",
            "mnemos memory frontmatter merge",
        ],
    )
    .await;
    let _ = run(
        root,
        &[
            "config",
            "merge.mnemos-frontmatter.driver",
            "mnemos-merge-driver %A %O %B",
        ],
    )
    .await;
    Ok(())
}

#[async_trait]
impl SyncBackend for GitSync {
    fn name(&self) -> &str {
        "git"
    }

    async fn push(&self, vault_root: &Path) -> Result<SyncReport> {
        ensure_gitattributes(vault_root).await?;
        ensure_merge_driver_config(vault_root).await?;
        run(vault_root, &["add", "."]).await?;
        let status = run(vault_root, &["status", "--porcelain"]).await?;
        if status.trim().is_empty() {
            return Ok(SyncReport {
                files_changed: 0,
                conflicts: vec![],
                message: "nothing to push".into(),
            });
        }
        let msg = format!("mnemos sync {}", Utc::now().to_rfc3339());
        run(vault_root, &["commit", "-m", &msg]).await?;
        let _ = run(vault_root, &["remote", "add", "origin", &self.remote]).await;
        run(vault_root, &["push", "origin", &self.branch]).await?;
        Ok(SyncReport {
            files_changed: status.lines().count(),
            conflicts: vec![],
            message: format!("pushed to {}", self.remote),
        })
    }

    async fn pull(&self, vault_root: &Path) -> Result<SyncReport> {
        ensure_merge_driver_config(vault_root).await?;
        let _ = run(vault_root, &["remote", "add", "origin", &self.remote]).await;
        match run(vault_root, &["pull", "--rebase", "origin", &self.branch]).await {
            Ok(out) => Ok(SyncReport {
                files_changed: 0,
                conflicts: vec![],
                message: out.lines().last().unwrap_or("pulled").to_string(),
            }),
            Err(e) => Err(e),
        }
    }

    async fn status(&self) -> Result<BackendStatus> {
        let ready = which::which("git").is_ok();
        Ok(BackendStatus {
            backend: "git".into(),
            ready,
            detail: if ready {
                format!("remote {}", self.remote)
            } else {
                "git not on PATH".into()
            },
        })
    }
}
