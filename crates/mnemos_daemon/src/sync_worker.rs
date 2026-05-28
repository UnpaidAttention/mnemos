//! Periodic sync worker. Runs `pull` then `push` on the configured interval.

use crate::state::AppState;
use std::sync::Arc;
use tokio::sync::watch;

/// Handle to the periodic sync worker; `shutdown` stops it and joins.
pub struct SyncHandle {
    pub(crate) join: tokio::task::JoinHandle<()>,
    pub(crate) shutdown: watch::Sender<bool>,
}

impl SyncHandle {
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(true);
        let _ = self.join.await;
    }
}

pub fn spawn(state: AppState) -> Option<SyncHandle> {
    let interval_secs = state.config.sync.interval_secs;
    if interval_secs == 0 {
        return None;
    }
    use crate::config::SyncKind;
    if matches!(state.config.sync.kind, SyncKind::None) {
        return None;
    }

    let (tx, mut rx) = watch::channel(false);
    let join = tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        tick.tick().await; // consume the immediate first tick
        loop {
            tokio::select! {
                _ = rx.changed() => {
                    if *rx.borrow() { break; }
                }
                _ = tick.tick() => {
                    if let Err(e) = run_once(&state).await {
                        tracing::warn!(error = %e, "sync worker pass failed");
                    }
                }
            }
        }
    });
    Some(SyncHandle { join, shutdown: tx })
}

async fn run_once(state: &AppState) -> anyhow::Result<()> {
    use mnemos_core::sync::{filesystem::FilesystemSync, git::GitSync, s3::S3Sync, SyncBackend};
    let backend: Arc<dyn SyncBackend> = match state.config.sync.kind {
        crate::config::SyncKind::None => return Ok(()),
        crate::config::SyncKind::Filesystem => {
            Arc::new(FilesystemSync::new(state.vault.storage().clone()))
        }
        crate::config::SyncKind::Git => Arc::new(GitSync::new(
            state.vault.storage().clone(),
            state.config.sync.git.remote.clone(),
            state.config.sync.git.branch.clone(),
        )),
        crate::config::SyncKind::S3 => Arc::new(S3Sync::new(
            state.vault.storage().clone(),
            state.config.sync.s3.remote.clone(),
        )),
    };
    let files_root = state.vault.paths().files_root().to_path_buf();
    if let Ok(r) = backend.pull(&files_root).await {
        for c in &r.conflicts {
            state.events.publish(crate::events::Event::SyncConflict {
                path: c.clone(),
                detected_by: backend.name().into(),
            });
        }
    }
    let _ = backend.push(&files_root).await;
    Ok(())
}
