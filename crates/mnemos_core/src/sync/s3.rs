//! S3-compatible sync backend via `rclone`.

use crate::error::{MnemosError, Result};
use crate::storage::Storage;
use crate::sync::{BackendStatus, SyncBackend, SyncReport};
use async_trait::async_trait;
use std::path::Path;
use tokio::process::Command;

pub struct S3Sync {
    #[allow(dead_code)]
    storage: Storage,
    remote: String,
}

impl S3Sync {
    pub fn new(storage: Storage, remote: String) -> Self {
        Self { storage, remote }
    }
}

async fn rclone(args: &[&str]) -> Result<String> {
    let out = Command::new("rclone")
        .args(args)
        .output()
        .await
        .map_err(|e| MnemosError::Internal(format!("rclone invocation failed: {e}")))?;
    if !out.status.success() {
        return Err(MnemosError::Internal(format!(
            "rclone {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[async_trait]
impl SyncBackend for S3Sync {
    fn name(&self) -> &str {
        "s3"
    }

    async fn push(&self, vault_root: &Path) -> Result<SyncReport> {
        let local = vault_root.to_string_lossy().to_string();
        rclone(&["sync", "--fast-list", &local, &self.remote]).await?;
        Ok(SyncReport {
            files_changed: 0,
            conflicts: vec![],
            message: format!("rclone sync → {}", self.remote),
        })
    }

    async fn pull(&self, vault_root: &Path) -> Result<SyncReport> {
        let local = vault_root.to_string_lossy().to_string();
        rclone(&["sync", "--fast-list", &self.remote, &local]).await?;
        Ok(SyncReport {
            files_changed: 0,
            conflicts: vec![],
            message: format!("rclone sync ← {}", self.remote),
        })
    }

    async fn status(&self) -> Result<BackendStatus> {
        let ready = which::which("rclone").is_ok();
        Ok(BackendStatus {
            backend: "s3".into(),
            ready,
            detail: if ready {
                format!("rclone target {}", self.remote)
            } else {
                "rclone not on PATH — install rclone and run `rclone config`".into()
            },
        })
    }
}
