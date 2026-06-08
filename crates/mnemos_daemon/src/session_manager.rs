//! Implicit session manager: infers sessions from MCP activity.
//!
//! Tools that don't support explicit session start/end hooks (e.g. Codex,
//! Antigravity) still generate MCP calls like `remember` and `recall`. The
//! session manager tracks these calls and creates/ends sessions automatically:
//!
//! - **touch(tool_id, workspace)**: Called on every MCP request. Creates a
//!   session if none exists for this tool+workspace pair, or refreshes the
//!   last-activity timestamp on the existing one.
//!
//! - **sweep()**: Called periodically (every 30s). Ends sessions that have been
//!   idle for longer than the configured timeout (default 5 minutes). Fires
//!   `Event::SessionEnded` for each ended session so the pipeline processes it.

use crate::events::{Event, EventBus};
use chrono::Utc;
use libsql::params;
use mnemos_core::storage::Storage;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Identifies a unique tool + workspace combination.
#[derive(Debug, Hash, Eq, PartialEq, Clone)]
struct SessionKey {
    tool_id: String,
    workspace: Option<String>,
}

/// An active (in-flight) session being tracked.
#[derive(Debug)]
struct ActiveSession {
    id: String,
    last_activity: Instant,
}

/// Manages implicit sessions derived from MCP activity.
pub struct SessionManager {
    active: Arc<RwLock<HashMap<SessionKey, ActiveSession>>>,
    idle_timeout: Duration,
}

// Compile-time assertion: SessionManager is shared via Arc in AppState,
// so it must be Send + Sync.
const _: () = {
    fn _assert_send_sync<T: Send + Sync>() {}
    fn _check() {
        _assert_send_sync::<SessionManager>();
    }
};

impl SessionManager {
    pub fn new(idle_timeout_secs: u64) -> Self {
        Self {
            active: Arc::new(RwLock::new(HashMap::new())),
            idle_timeout: Duration::from_secs(idle_timeout_secs),
        }
    }

    /// Called on every MCP request. Returns the session ID for the given
    /// tool+workspace, creating a new session if needed.
    pub async fn touch(
        &self,
        storage: &Storage,
        tool_id: &str,
        workspace: Option<&str>,
    ) -> anyhow::Result<String> {
        let key = SessionKey {
            tool_id: tool_id.to_string(),
            workspace: workspace.map(String::from),
        };

        // Fast path: session already exists.
        {
            let mut active = self.active.write().await;
            if let Some(session) = active.get_mut(&key) {
                session.last_activity = Instant::now();
                return Ok(session.id.clone());
            }
        }

        // Slow path: create a new session.
        let id = mnemos_core::id::new_session_id();
        let (conn, _g) = storage.write_conn().await?;
        conn.execute(
            "INSERT INTO sessions (id, source_tool, workspace, started_at) VALUES (?, ?, ?, ?)",
            params![
                id.clone(),
                tool_id.to_string(),
                workspace.map(String::from),
                Utc::now().to_rfc3339()
            ],
        )
        .await?;
        drop(_g);

        tracing::info!(
            tool = %tool_id,
            workspace = ?workspace,
            session = %id,
            "implicit session created"
        );

        let mut active = self.active.write().await;
        active.insert(
            key,
            ActiveSession {
                id: id.clone(),
                last_activity: Instant::now(),
            },
        );

        Ok(id)
    }

    /// Sweep for idle sessions and end them. Returns the IDs of ended sessions.
    pub async fn sweep(
        &self,
        storage: &Storage,
    ) -> Vec<String> {
        let mut ended = Vec::new();
        let now = Instant::now();

        let mut active = self.active.write().await;
        let keys_to_remove: Vec<SessionKey> = active
            .iter()
            .filter(|(_, s)| now.duration_since(s.last_activity) > self.idle_timeout)
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            if let Some(session) = active.remove(&key) {
                // Mark session as ended in the database.
                if let Ok((conn, _g)) = storage.write_conn().await {
                    let _ = conn
                        .execute(
                            "UPDATE sessions SET ended_at = ? WHERE id = ? AND ended_at IS NULL",
                            params![Utc::now().to_rfc3339(), session.id.clone()],
                        )
                        .await;
                }
                tracing::info!(
                    session = %session.id,
                    tool = %key.tool_id,
                    idle_secs = now.duration_since(session.last_activity).as_secs(),
                    "implicit session ended (idle timeout)"
                );
                ended.push(session.id);
            }
        }

        ended
    }

    /// Spawn the background sweep loop. Runs every 30s and publishes
    /// `SessionEnded` events for any sessions that have been idle.
    pub fn spawn_sweep_loop(
        self: Arc<Self>,
        vault: mnemos_core::vault::Vault,
        events: EventBus,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let ended = self.sweep(vault.storage()).await;
                for id in ended {
                    events.publish(Event::SessionEnded { id });
                }
            }
        })
    }
}
