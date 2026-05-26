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
