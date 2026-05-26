//! Placeholder — populated in Task 9.
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct EventBus {
    _inner: Arc<()>,
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }
}
