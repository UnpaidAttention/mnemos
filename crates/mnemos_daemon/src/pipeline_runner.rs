//! Background pipeline runner: subscribes to `SessionEnded` and turns a
//! session's chunks into durable memories + graph edges.

use crate::events::Event;
use crate::pipeline_status::RecentRun;
use crate::state::AppState;
use chrono::{DateTime, Utc};
use libsql::params;
use mnemos_core::pipeline::entities::link_entities;
use mnemos_core::pipeline::extract::extract_facts;
use mnemos_core::pipeline::graph::update_graph;
use mnemos_core::pipeline::reflect::{harden_corrections, mine_corrections, reflect};
use mnemos_core::pipeline::resolve::resolve_and_apply;
use mnemos_core::pipeline::ResolveOp;
use mnemos_core::providers::LlmProvider;
use mnemos_core::storage::chunk_ops::delete_session_chunks;
use mnemos_core::storage::reflection_ops::{bump_salience, reset_salience};
use mnemos_core::types::{Chunk, Provenance};
use tokio::sync::broadcast::error::RecvError;
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

/// Spawn the runner. It processes `SessionEnded` events until told to stop.
/// On startup, it first catches up any sessions with `processed_at IS NULL`
/// (e.g. from previous failed runs or daemon restarts).
pub fn spawn(state: AppState) -> PipelineHandle {
    let (tx, mut rx) = watch::channel(false);
    // Subscribe BEFORE spawning so no events are missed between spawn and first poll.
    let mut events = state.events.subscribe();
    let join = tokio::spawn(async move {
        // Catch-up: retry any sessions that were never successfully processed.
        catch_up(&state).await;

        loop {
            tokio::select! {
                _ = rx.changed() => {
                    if *rx.borrow() { break; }
                }
                ev = events.recv() => match ev {
                    Ok(Event::SessionEnded { id }) => process_session(&state, &id).await,
                    Ok(_) => {}
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "pipeline runner lagged; some events dropped");
                    }
                    Err(RecvError::Closed) => break,
                },
            }
        }
    });
    PipelineHandle { join, shutdown: tx }
}

/// Find and process any sessions whose `processed_at` is still NULL.
/// Waits for the LLM to become available before processing.
async fn catch_up(state: &AppState) {
    // Wait for the LLM to become available (it starts asynchronously).
    if state.llm.is_none() {
        tracing::info!("catch-up: no LLM configured, skipping");
        return;
    }
    // Await the readiness signal from the bundled LLM health check (or
    // immediate for non-bundled providers).  Uses watch::Receiver::wait_for
    // which returns immediately if the value is already true.
    let mut rx = state.llm_ready_rx.clone();
    if tokio::time::timeout(
        std::time::Duration::from_secs(200),
        rx.wait_for(|ready| *ready),
    )
    .await
    .is_err()
    {
        tracing::warn!("catch-up: LLM not ready after 200s, skipping");
        return;
    }

    let ids = match unprocessed_session_ids(state).await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!(error = %e, "catch-up: failed to query unprocessed sessions");
            return;
        }
    };
    if ids.is_empty() {
        return;
    }
    tracing::info!(count = ids.len(), "catch-up: retrying unprocessed sessions");
    for id in ids {
        process_session(state, &id).await;
    }
}

/// Query the DB for session IDs that have `processed_at IS NULL` and an
/// `ended_at` timestamp (i.e. the session finished but wasn't processed).
async fn unprocessed_session_ids(state: &AppState) -> anyhow::Result<Vec<String>> {
    let conn = state.vault.storage().conn()?;
    let mut rows = conn
        .query(
            "SELECT id FROM sessions WHERE processed_at IS NULL AND ended_at IS NOT NULL ORDER BY started_at ASC",
            params![],
        )
        .await?;
    let mut ids = Vec::new();
    while let Some(r) = rows.next().await? {
        ids.push(r.get::<String>(0)?);
    }
    Ok(ids)
}

async fn process_session(state: &AppState, session_id: &str) {
    let Some(llm) = state.llm.clone() else {
        return;
    };
    // Retry with exponential backoff on transient failures.
    const MAX_RETRIES: u32 = 3;
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..=MAX_RETRIES {
        match run_pipeline(state, session_id, llm.as_ref()).await {
            Ok(n) => {
                state
                    .pipeline_status
                    .record(RecentRun {
                        session_id: session_id.to_string(),
                        facts_added: n,
                        ok: true,
                        at: Utc::now().to_rfc3339(),
                    })
                    .await;
                state.events.publish(Event::PipelineCompleted {
                    session_id: session_id.to_string(),
                    facts_added: n,
                });
                maybe_reflect(state, llm.as_ref(), n).await;
                maybe_mine_and_harden(state, llm.as_ref(), session_id).await;
                maybe_prune_chunks(state, session_id).await;
                return;
            }
            Err(e) => {
                if attempt < MAX_RETRIES {
                    let delay = std::time::Duration::from_secs(1 << (attempt + 1));
                    tracing::warn!(
                        session_id = %session_id,
                        attempt = attempt + 1,
                        retry_in = ?delay,
                        error = %e,
                        "pipeline failed, retrying"
                    );
                    tokio::time::sleep(delay).await;
                }
                last_err = Some(e);
            }
        }
    }
    // All retries exhausted.
    let e = last_err.unwrap();
    tracing::error!(session_id = %session_id, error = %e, "pipeline failed after {MAX_RETRIES} retries");
    state
        .pipeline_status
        .record(RecentRun {
            session_id: session_id.to_string(),
            facts_added: 0,
            ok: false,
            at: Utc::now().to_rfc3339(),
        })
        .await;
    state.events.publish(Event::PipelineFailed {
        session_id: session_id.to_string(),
        error: e.to_string(),
    });
}

async fn run_pipeline(
    state: &AppState,
    session_id: &str,
    llm: &dyn LlmProvider,
) -> anyhow::Result<usize> {
    if is_processed(state, session_id).await? {
        return Ok(0);
    }
    let chunks = load_chunks(state, session_id).await?;
    if chunks.is_empty() {
        mark_processed(state, session_id).await?;
        return Ok(0);
    }
    let chunk_ids: Vec<String> = chunks.iter().map(|c| c.id.clone()).collect();
    let custom_schema = state.vault.load_custom_schema();
    let facts = extract_facts(&chunks, llm, custom_schema.as_deref()).await?;
    let prov = Provenance {
        session: Some(session_id.to_string()),
        chunks: chunk_ids,
    };
    let mut added = 0usize;
    for fact in &facts {
        // A single fact's failure (transient LLM/parse error) must not discard
        // the remaining facts or leave the session unprocessed. Log and continue,
        // mirroring the entity/graph stages below.
        let (op, new_id) = match resolve_and_apply(&state.vault, fact, prov.clone(), llm).await {
            Ok(result) => result,
            Err(e) => {
                tracing::warn!(error = %e, "resolve_and_apply failed for a fact; skipping");
                continue;
            }
        };
        if let Some(mid) = new_id {
            if matches!(op, ResolveOp::Add | ResolveOp::Update { .. }) {
                added += 1;
            }
            if let Ok(mem) = state.vault.get(&mid).await {
                state.events.publish(Event::MemoryCreated {
                    id: mid.clone(),
                    title: mem.title.clone(),
                    tier: mem.tier.as_str().to_string(),
                });
                if let Err(e) = link_entities(state.vault.storage(), &mid, &mem.body, llm).await {
                    tracing::warn!(memory_id = %mid, error = %e, "entity linking failed");
                }
                if let Err(e) =
                    update_graph(state.vault.storage(), &mid, &mem.body, mem.valid_at, llm).await
                {
                    tracing::warn!(memory_id = %mid, error = %e, "graph update failed");
                }
            }
        }
    }
    mark_processed(state, session_id).await?;
    Ok(added)
}

async fn is_processed(state: &AppState, session_id: &str) -> anyhow::Result<bool> {
    let conn = state.vault.storage().conn()?;
    let mut rows = conn
        .query(
            "SELECT processed_at FROM sessions WHERE id = ?",
            params![session_id.to_string()],
        )
        .await?;
    match rows.next().await? {
        Some(r) => Ok(r.get::<Option<String>>(0)?.is_some()),
        None => Ok(true), // unknown session — nothing to do
    }
}

async fn mark_processed(state: &AppState, session_id: &str) -> anyhow::Result<()> {
    let (conn, _g) = state.vault.storage().write_conn().await?;
    conn.execute(
        "UPDATE sessions SET processed_at = ? WHERE id = ?",
        params![Utc::now().to_rfc3339(), session_id.to_string()],
    )
    .await?;
    Ok(())
}

/// Bump salience by the number of facts added; if it crosses the configured
/// threshold, run a reflection pass, reset the accumulator, and emit an event.
async fn maybe_reflect(state: &AppState, llm: &dyn LlmProvider, added: usize) {
    if added == 0 {
        return;
    }
    let salience = match bump_salience(state.vault.storage(), added as f64).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "salience bump failed");
            return;
        }
    };
    if salience < state.config.reflection.salience_threshold {
        return;
    }
    match reflect(&state.vault, llm, state.config.reflection.max_sources).await {
        Ok(created) => {
            let _ = reset_salience(state.vault.storage(), chrono::Utc::now()).await;
            state.events.publish(Event::ReflectionCompleted {
                reflections_created: created.len(),
            });
        }
        Err(e) => tracing::warn!(error = %e, "reflection pass failed"),
    }
}

/// Run the correction-mining pass followed by the hardening pass for a
/// session that just completed. Both are fire-and-forget: errors are logged
/// and the caller is never blocked.
async fn maybe_mine_and_harden(state: &AppState, llm: &dyn LlmProvider, session_id: &str) {
    if let Err(e) = mine_corrections(&state.vault, llm, session_id).await {
        tracing::warn!(session_id = %session_id, error = %e, "correction mining failed");
    }
    if let Err(e) = harden_corrections(&state.vault, llm, 3).await {
        tracing::warn!(error = %e, "correction hardening failed");
    }
}

/// If `config.autonomy.retention == DistillAndPrune`, delete the raw chunks
/// for `session_id` now that the pipeline + correction passes are done.
///
/// Errors are logged and swallowed — distillation already succeeded and a
/// prune failure must never surface as a pipeline failure.
async fn maybe_prune_chunks(state: &AppState, session_id: &str) {
    use crate::config::RetentionPolicy;
    if state.config.autonomy.retention != RetentionPolicy::DistillAndPrune {
        return;
    }
    match delete_session_chunks(state.vault.storage(), session_id).await {
        Ok(n) => {
            tracing::info!(
                session_id = %session_id,
                chunks_pruned = n,
                "distill-and-prune: raw chunks deleted"
            );
        }
        Err(e) => {
            tracing::warn!(
                session_id = %session_id,
                error = %e,
                "distill-and-prune: failed to delete chunks (non-fatal)"
            );
        }
    }
}

async fn load_chunks(state: &AppState, session_id: &str) -> anyhow::Result<Vec<Chunk>> {
    let conn = state.vault.storage().conn()?;
    let mut rows = conn
        .query(
            "SELECT id, session_id, speaker, ordinal, body, created_at, source_tool, source_meta
               FROM chunks WHERE session_id = ? ORDER BY ordinal ASC",
            params![session_id.to_string()],
        )
        .await?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().await? {
        let source_meta_raw: Option<String> = r.get(7)?;
        let created: String = r.get(5)?;
        out.push(Chunk {
            id: r.get(0)?,
            session_id: r.get(1)?,
            speaker: r.get(2)?,
            ordinal: r.get::<i64>(3)? as u32,
            body: r.get(4)?,
            created_at: DateTime::parse_from_rfc3339(&created)?.with_timezone(&Utc),
            source_tool: r.get(6)?,
            source_meta: source_meta_raw
                .map(|s| serde_json::from_str(&s))
                .transpose()?,
        });
    }
    Ok(out)
}
