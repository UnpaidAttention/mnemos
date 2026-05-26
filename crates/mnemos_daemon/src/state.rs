//! Shared application state passed to all route handlers.

use mnemos_core::providers::Reranker;
use mnemos_core::vault::Vault;
use std::sync::Arc;

use crate::config::Config;
use crate::events::EventBus;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub vault: Vault,
    pub token: String,
    pub events: EventBus,
    pub reranker: Option<Arc<dyn Reranker>>,
}
