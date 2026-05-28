//! `mnemos sync push|pull|status` — local CLI that runs the configured backend
//! in-process against the vault on disk. Backend selection is via env vars:
//! `MNEMOS_SYNC_KIND={filesystem|git|s3}` plus backend-specific config vars.

use crate::cli::SyncAction;
use crate::commands::open_vault;
use anyhow::Result;
use mnemos_core::sync::{filesystem::FilesystemSync, git::GitSync, s3::S3Sync, SyncBackend};
use std::path::PathBuf;

fn backend_from_env(storage: mnemos_core::storage::Storage) -> Option<Box<dyn SyncBackend>> {
    let kind = std::env::var("MNEMOS_SYNC_KIND").unwrap_or_else(|_| "none".into());
    match kind.as_str() {
        "filesystem" => Some(Box::new(FilesystemSync::new(storage))),
        "git" => {
            let remote = std::env::var("MNEMOS_SYNC_GIT_REMOTE").ok()?;
            let branch = std::env::var("MNEMOS_SYNC_GIT_BRANCH").unwrap_or_else(|_| "main".into());
            Some(Box::new(GitSync::new(storage, remote, branch)))
        }
        "s3" => {
            let remote = std::env::var("MNEMOS_SYNC_S3_REMOTE").ok()?;
            Some(Box::new(S3Sync::new(storage, remote)))
        }
        _ => None,
    }
}

pub async fn run(vault: Option<PathBuf>, json: bool, action: SyncAction) -> Result<()> {
    let v = open_vault(vault).await?;
    let backend = backend_from_env(v.storage().clone());
    let report = match (backend, action) {
        (None, _) => {
            println!("sync disabled (set MNEMOS_SYNC_KIND or use the daemon's [sync] config)");
            return Ok(());
        }
        (Some(b), SyncAction::Status) => {
            let s = b.status().await?;
            if json {
                println!("{}", serde_json::to_string(&s)?);
            } else {
                println!(
                    "backend: {}  ready: {}  detail: {}",
                    s.backend, s.ready, s.detail
                );
            }
            return Ok(());
        }
        (Some(b), SyncAction::Push) => b.push(v.paths().files_root()).await?,
        (Some(b), SyncAction::Pull) => b.pull(v.paths().files_root()).await?,
    };
    if json {
        println!("{}", serde_json::to_string(&report)?);
    } else {
        println!(
            "changed {}  conflicts {}  {}",
            report.files_changed,
            report.conflicts.len(),
            report.message
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn sync_status_on_empty_vault_is_disabled() {
        std::env::set_var("MNEMOS_EMBEDDER", "none");
        let tmp = TempDir::new().unwrap();
        run(Some(tmp.path().to_path_buf()), true, SyncAction::Status)
            .await
            .unwrap();
    }
}
