//! Event bus: broadcasts typed events to all connected WebSocket subscribers.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

/// All events the daemon can emit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    MemoryCreated {
        id: String,
        title: String,
        tier: String,
    },
    MemoryUpdated {
        id: String,
    },
    MemoryInvalidated {
        id: String,
        reason: Option<String>,
    },
    SessionStarted {
        id: String,
    },
    SessionEnded {
        id: String,
    },
    ChunkAdded {
        session_id: String,
        chunk_id: String,
    },
    PipelineCompleted {
        session_id: String,
        facts_added: usize,
    },
    PipelineFailed {
        session_id: String,
        error: String,
    },
    ReflectionCompleted {
        reflections_created: usize,
    },
    CommunityDetected {
        communities: usize,
    },
    SyncStarted {
        backend: String,
        direction: String,
    },
    SyncCompleted {
        backend: String,
        direction: String,
        files_changed: usize,
    },
    SyncFailed {
        backend: String,
        direction: String,
        error: String,
    },
    SyncConflict {
        path: String,
        detected_by: String,
    },
    EmbedRebuildStarted {
        target_kind: String,
        target_model: String,
        target_dim: u32,
    },
    EmbedRebuildProgress {
        processed: usize,
        total: usize,
    },
    EmbedRebuildCompleted {
        processed: usize,
        skipped: usize,
        total: usize,
    },
    EmbedRebuildFailed {
        error: String,
        processed: usize,
    },
}

/// Cloneable handle to the broadcast channel that backs all WebSocket subscriptions.
#[derive(Clone)]
pub struct EventBus {
    tx: Arc<broadcast::Sender<Event>>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx: Arc::new(tx) }
    }

    /// Returns a new receiver that will see all future events.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// Fire-and-forget publish. Silently drops if there are no subscribers.
    pub fn publish(&self, e: Event) {
        let _ = self.tx.send(e);
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
