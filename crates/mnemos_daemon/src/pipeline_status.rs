//! Observable, in-memory pipeline status surfaced by `GET /v1/pipelines`.

use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

const RECENT_CAP: usize = 20;

#[derive(Debug, Default, Clone, Serialize)]
pub struct PipelineCounters {
    pub completed: u64,
    pub failed: u64,
    pub facts_added: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecentRun {
    pub session_id: String,
    pub facts_added: usize,
    pub ok: bool,
    pub at: String,
}

#[derive(Debug, Default)]
struct Inner {
    counters: PipelineCounters,
    recent: VecDeque<RecentRun>,
}

/// Cloneable handle to pipeline run statistics.
#[derive(Clone, Default)]
pub struct PipelineStatus {
    inner: Arc<Mutex<Inner>>,
}

impl PipelineStatus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the outcome of a pipeline run.
    pub async fn record(&self, run: RecentRun) {
        let mut g = self.inner.lock().await;
        if run.ok {
            g.counters.completed += 1;
            g.counters.facts_added += run.facts_added as u64;
        } else {
            g.counters.failed += 1;
        }
        g.recent.push_front(run);
        while g.recent.len() > RECENT_CAP {
            g.recent.pop_back();
        }
    }

    /// Snapshot the counters and the recent-runs list (newest first).
    pub async fn snapshot(&self) -> (PipelineCounters, Vec<RecentRun>) {
        let g = self.inner.lock().await;
        (g.counters.clone(), g.recent.iter().cloned().collect())
    }
}
