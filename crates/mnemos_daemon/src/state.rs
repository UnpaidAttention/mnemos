//! Shared application state passed to all route handlers.

use mnemos_core::embedder_rebuild::RebuildStatus;
use mnemos_core::providers::{LlmProvider, Reranker};
use mnemos_core::vault::Vault;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::Config;
use crate::events::EventBus;
use crate::pipeline_status::PipelineStatus;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub vault: Vault,
    pub token: String,
    pub events: EventBus,
    pub reranker: Option<Arc<dyn Reranker>>,
    pub llm: Option<Arc<dyn LlmProvider>>,
    pub pipeline_status: PipelineStatus,
    /// Status of the most recent (or in-progress) `embed-rebuild` run.
    /// Defaults to `RebuildStatus::Idle` on startup.
    pub rebuild_status: Arc<Mutex<RebuildStatus>>,
    /// Monotonically increasing counter, incremented each time a rebuild is
    /// started.  The background task captures the generation at start and only
    /// writes its final status if the counter still matches — preventing an
    /// aborted or superseded run from overwriting a later run's status (P1-12).
    pub rebuild_generation: Arc<AtomicU64>,
}
