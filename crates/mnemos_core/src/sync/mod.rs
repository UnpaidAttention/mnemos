//! Pluggable cloud-sync backends. Files are the durable record; each backend
//! syncs the on-disk vault. The DB is rebuilt from files on pull when needed.

pub mod filesystem;
pub mod git;
pub mod s3;
pub mod state;

use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Outcome of a push or pull operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncReport {
    pub files_changed: usize,
    pub conflicts: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStatus {
    pub backend: String,
    pub ready: bool,
    pub detail: String,
}

#[async_trait]
pub trait SyncBackend: Send + Sync {
    fn name(&self) -> &str;
    async fn push(&self, vault_root: &Path) -> Result<SyncReport>;
    async fn pull(&self, vault_root: &Path) -> Result<SyncReport>;
    async fn status(&self) -> Result<BackendStatus>;
}
