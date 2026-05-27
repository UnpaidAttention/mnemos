//! Background pipeline runner. Loop body lands in Task 13.

use crate::state::AppState;
use tokio::sync::watch;

/// Handle to the background pipeline runner; `shutdown` stops it and joins.
pub struct PipelineHandle {
    pub(crate) join: tokio::task::JoinHandle<()>,
    pub(crate) shutdown: watch::Sender<bool>,
}

impl PipelineHandle {
    /// Signal the runner to stop and await its completion.
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(true);
        let _ = self.join.await;
    }
}

/// Spawn the runner. Filled in Task 13; placeholder idles until shutdown.
pub fn spawn(_state: AppState) -> PipelineHandle {
    let (tx, mut rx) = watch::channel(false);
    let join = tokio::spawn(async move {
        let _ = rx.changed().await;
    });
    PipelineHandle { join, shutdown: tx }
}
