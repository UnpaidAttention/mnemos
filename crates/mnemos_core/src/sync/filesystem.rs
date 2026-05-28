//! Filesystem-sync backend. The vault lives in a Syncthing/Dropbox/iCloud/
//! OneDrive folder; the OS handles bytes. We detect conflict files and surface
//! them through `sync_conflicts` so the UI can present a resolution flow.

use crate::error::Result;
use crate::storage::Storage;
use crate::sync::state::{list_unresolved_conflicts, record_conflict};
use crate::sync::{BackendStatus, SyncBackend, SyncReport};
use async_trait::async_trait;
use std::path::Path;
use walkdir::WalkDir;

pub struct FilesystemSync {
    storage: Storage,
}

impl FilesystemSync {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }
}

/// Heuristics for the four common conflict-file naming conventions.
pub fn is_conflict_file(name: &str) -> bool {
    name.contains(".sync-conflict-")        // Syncthing
        || name.contains("conflicted copy") // Dropbox
        || name.contains(" (Case Conflict") // iCloud rare
        || name.ends_with(".collision.md") // OneDrive style
}

#[async_trait]
impl SyncBackend for FilesystemSync {
    fn name(&self) -> &str {
        "filesystem"
    }

    async fn push(&self, _vault_root: &Path) -> Result<SyncReport> {
        Ok(SyncReport {
            files_changed: 0,
            conflicts: vec![],
            message: "no-op (OS handles sync)".into(),
        })
    }

    async fn pull(&self, vault_root: &Path) -> Result<SyncReport> {
        let mut conflicts: Vec<String> = Vec::new();
        for entry in WalkDir::new(vault_root).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if is_conflict_file(&name) {
                let rel = entry
                    .path()
                    .strip_prefix(vault_root)
                    .unwrap_or(entry.path())
                    .to_string_lossy()
                    .to_string();
                // dedupe against already-known unresolved
                let known = list_unresolved_conflicts(&self.storage).await?;
                if !known.iter().any(|c| c.path == rel) {
                    record_conflict(&self.storage, &rel, "filesystem", Some(&name)).await?;
                }
                conflicts.push(rel);
            }
        }
        Ok(SyncReport {
            files_changed: 0,
            message: format!("detected {} conflict file(s)", conflicts.len()),
            conflicts,
        })
    }

    async fn status(&self) -> Result<BackendStatus> {
        Ok(BackendStatus {
            backend: "filesystem".into(),
            ready: true,
            detail: "OS-managed sync".into(),
        })
    }
}
