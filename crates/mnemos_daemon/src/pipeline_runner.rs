//! Background pipeline runner: subscribes to `SessionEnded` and turns a
//! session's chunks into durable memories + graph edges.

use crate::events::Event;
use crate::pipeline_status::RecentRun;
use crate::state::AppState;
use chrono::{DateTime, Utc};
use libsql::params;
use mnemos_core::pipeline::co_mention::create_co_mention_edges;
use mnemos_core::pipeline::entities::link_entities;
use mnemos_core::pipeline::extract::{extract_facts, extract_facts_incremental};
use mnemos_core::pipeline::graph::update_graph;
use mnemos_core::pipeline::reflect::{harden_corrections, mine_corrections, reflect};
use mnemos_core::pipeline::resolve::resolve_and_apply;
use mnemos_core::pipeline::ResolveOp;
use mnemos_core::providers::LlmProvider;
use mnemos_core::storage::audit::write_audit;
use mnemos_core::storage::chunk_ops::delete_session_chunks;
use mnemos_core::storage::reflection_ops::{bump_salience, reset_salience};
use mnemos_core::types::{Chunk, Provenance};
use serde_json::json;
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
/// On startup, it first catches up any sessions with `processed_at IS NULL`.
///
/// It also handles `ChunkAdded` events for incremental (mid-session)
/// extraction: when 4+ chunks accumulate for a session or 90 seconds pass
/// since the last chunk with ≥1 pending, it runs the extraction pipeline on
/// just the new chunks (with full session context).
pub fn spawn(state: AppState) -> PipelineHandle {
    let (tx, mut rx) = watch::channel(false);
    // Subscribe BEFORE spawning so no events are missed between spawn and first poll.
    let mut events = state.events.subscribe();
    let join = tokio::spawn(async move {
        // Catch-up: retry any sessions that were never successfully processed.
        // Safe to run because failed sessions are now marked as processed after
        // all retries are exhausted — they won't accumulate across restarts.
        catch_up(&state).await;

        // Track pending chunks per session for batched incremental processing.
        let mut pending: std::collections::HashMap<String, IncrementalState> =
            std::collections::HashMap::new();

        // Tick interval for checking stale pending batches (every 15s).
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(15));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = rx.changed() => {
                    if *rx.borrow() { break; }
                }
                _ = tick.tick() => {
                    // Check for sessions with stale pending chunks (90s+ idle).
                    let now = std::time::Instant::now();
                    let stale_sessions: Vec<String> = pending.iter()
                        .filter(|(_, s)| s.pending_count > 0
                            && now.duration_since(s.last_chunk_at) > std::time::Duration::from_secs(90))
                        .map(|(k, _)| k.clone())
                        .collect();
                    for session_id in stale_sessions {
                        tracing::info!(session_id = %session_id, "incremental pipeline: processing stale batch");
                        run_incremental_pipeline(&state, &session_id).await;
                        pending.remove(&session_id);
                    }
                }
                ev = events.recv() => match ev {
                    Ok(Event::SessionEnded { id }) => {
                        // Final pass: clear any pending incremental state and run full pipeline.
                        pending.remove(&id);
                        process_session(&state, &id).await;
                    }
                    Ok(Event::ChunkAdded { session_id, .. }) => {
                        let entry = pending.entry(session_id.clone()).or_insert_with(|| {
                            IncrementalState {
                                pending_count: 0,
                                last_chunk_at: std::time::Instant::now(),
                            }
                        });
                        entry.pending_count += 1;
                        entry.last_chunk_at = std::time::Instant::now();

                        // Trigger extraction when 4+ chunks have accumulated.
                        if entry.pending_count >= 4 {
                            tracing::info!(
                                session_id = %session_id,
                                pending = entry.pending_count,
                                "incremental pipeline: batch threshold reached"
                            );
                            run_incremental_pipeline(&state, &session_id).await;
                            pending.remove(&session_id);
                        }
                    }
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

/// State for tracking pending chunks for incremental processing.
struct IncrementalState {
    pending_count: usize,
    last_chunk_at: std::time::Instant,
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

    // End any sessions stuck OPEN in the database (hook-created sessions
    // whose session_end was never called). The in-memory SessionManager
    // sweep only handles sessions it created via touch(); hook-originated
    // sessions bypass that entirely.
    end_stale_open_sessions(state).await;

    let ids = match unprocessed_session_ids(state).await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!(error = %e, "catch-up: failed to query unprocessed sessions");
            return;
        }
    };
    if ids.is_empty() {
        tracing::info!("catch-up: no unprocessed sessions found");
        return;
    }
    tracing::info!(count = ids.len(), "catch-up: retrying unprocessed sessions");
    for id in ids {
        process_session(state, &id).await;
    }
}

/// End sessions that are stuck OPEN in the database. These are sessions
/// created by hooks (Claude Code, etc.) where session_end was never called.
/// A session is considered stale if:
/// - `ended_at IS NULL` (never ended)
/// - It has at least one chunk
/// - The most recent chunk was added more than 10 minutes ago
async fn end_stale_open_sessions(state: &AppState) {
    let conn = match state.vault.storage().conn() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "stale-sweep: failed to get DB connection");
            return;
        }
    };
    // Find sessions that are open, have chunks, and whose last chunk
    // is older than 10 minutes.
    let mut rows = match conn
        .query(
            "SELECT s.id FROM sessions s \
             JOIN chunks c ON c.session_id = s.id \
             WHERE s.ended_at IS NULL \
             GROUP BY s.id \
             HAVING max(c.created_at) < datetime('now', '-10 minutes')",
            params![],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "stale-sweep: query failed");
            return;
        }
    };
    let mut stale_ids = Vec::new();
    while let Ok(Some(r)) = rows.next().await {
        if let Ok(id) = r.get::<String>(0) {
            stale_ids.push(id);
        }
    }
    if stale_ids.is_empty() {
        return;
    }
    tracing::info!(count = stale_ids.len(), "stale-sweep: ending abandoned open sessions");
    let now = chrono::Utc::now().to_rfc3339();
    for id in &stale_ids {
        if let Ok((wconn, _g)) = state.vault.storage().write_conn().await {
            let _ = wconn
                .execute(
                    "UPDATE sessions SET ended_at = ? WHERE id = ? AND ended_at IS NULL",
                    params![now.clone(), id.clone()],
                )
                .await;
            tracing::info!(session_id = %id, "stale-sweep: ended abandoned session");
            // Fire SessionEnded so the pipeline picks it up during catch_up.
            state.events.publish(Event::SessionEnded { id: id.clone() });
        }
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
                let _ = write_audit(
                    state.vault.storage(),
                    "mnemos-pipeline",
                    "pipeline_completed",
                    None,
                    Some(json!({
                        "session_id": session_id,
                        "facts_added": n,
                        "attempts": attempt + 1,
                    })),
                )
                .await;
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
    // All retries exhausted — mark as processed so it doesn't linger in the
    // backlog and get retried on every daemon restart. The failure is recorded
    // in pipeline_status and the PipelineFailed event. Users can re-trigger
    // via POST /v1/maintenance/backfill if needed.
    let e = last_err.unwrap();
    tracing::error!(session_id = %session_id, error = %e, "pipeline failed after {MAX_RETRIES} retries; marking processed");
    if let Err(mark_err) = mark_processed(state, session_id).await {
        tracing::warn!(session_id = %session_id, error = %mark_err, "failed to mark session as processed after pipeline failure");
    }
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
    let _ = write_audit(
        state.vault.storage(),
        "mnemos-pipeline",
        "pipeline_failed",
        None,
        Some(json!({
            "session_id": session_id,
            "error": e.to_string(),
            "retries": MAX_RETRIES,
        })),
    )
    .await;
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

    // When extraction_mode is not Local, skip LLM extraction entirely.
    // Chunks are already saved (capture is a separate concern); we just
    // don't run the local model. In mcp-piggyback mode, the conversation
    // LLM handles extraction via proactive MCP tool calls instead.
    use crate::config::ExtractionMode;
    if state.config.autonomy.extraction_mode != ExtractionMode::Local {
        tracing::info!(
            session_id = %session_id,
            mode = ?state.config.autonomy.extraction_mode,
            "skipping local extraction (extraction_mode is not local)"
        );
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
    let added = process_facts(state, &facts, prov, llm).await;
    mark_processed(state, session_id).await?;
    Ok(added)
}

/// Resolve, entity-link, co-mention, and graph-update each candidate fact.
/// Returns the count of facts added or updated. Individual failures are
/// logged and skipped so one bad fact never blocks the rest.
async fn process_facts(
    state: &AppState,
    facts: &[mnemos_core::pipeline::CandidateFact],
    prov: Provenance,
    llm: &dyn LlmProvider,
) -> usize {
    let mut added = 0usize;
    for fact in facts {
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
                    create_co_mention_edges(state.vault.storage(), &mid, mem.valid_at).await
                {
                    tracing::warn!(memory_id = %mid, error = %e, "co-mention edge creation failed");
                }
                if let Err(e) =
                    update_graph(state.vault.storage(), &mid, &mem.body, mem.valid_at, llm).await
                {
                    tracing::warn!(memory_id = %mid, error = %e, "graph update failed");
                }
            }
        }
    }
    added
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

/// Run the incremental extraction pipeline for a session.
///
/// Loads ALL chunks (for conversation context), but only extracts from chunks
/// whose ordinal is greater than the session's `processed_through_ordinal`
/// watermark. Updates the watermark after successful extraction.
async fn run_incremental_pipeline(state: &AppState, session_id: &str) {
    // Skip incremental extraction when not in Local mode.
    use crate::config::ExtractionMode;
    if state.config.autonomy.extraction_mode != ExtractionMode::Local {
        return;
    }

    let Some(llm) = state.llm.clone() else {
        tracing::debug!("incremental pipeline: no LLM configured; skipping");
        return;
    };
    match run_incremental(state, session_id, llm.as_ref()).await {
        Ok(added) => {
            if added > 0 {
                tracing::info!(
                    session_id = %session_id,
                    facts_added = added,
                    "incremental pipeline: extraction complete"
                );
                state.events.publish(Event::PipelineCompleted {
                    session_id: session_id.to_string(),
                    facts_added: added,
                });
                maybe_reflect(state, llm.as_ref(), added).await;
            }
        }
        Err(e) => {
            tracing::warn!(
                session_id = %session_id,
                error = %e,
                "incremental pipeline failed"
            );
        }
    }
}

/// Core logic for incremental extraction.
async fn run_incremental(
    state: &AppState,
    session_id: &str,
    llm: &dyn LlmProvider,
) -> anyhow::Result<usize> {
    let all_chunks = load_chunks(state, session_id).await?;
    if all_chunks.is_empty() {
        return Ok(0);
    }

    // Read the current watermark.
    let watermark = load_watermark(state, session_id).await?.unwrap_or(-1);

    // Split into context (already processed) and new.
    let (context, new): (Vec<&Chunk>, Vec<&Chunk>) = all_chunks
        .iter()
        .partition(|c| (c.ordinal as i64) <= watermark);

    if new.is_empty() {
        return Ok(0);
    }

    let context_owned: Vec<Chunk> = context.into_iter().cloned().collect();
    let new_owned: Vec<Chunk> = new.iter().map(|c| (*c).clone()).collect();
    let new_chunk_ids: Vec<String> = new_owned.iter().map(|c| c.id.clone()).collect();
    let max_ordinal = new_owned.iter().map(|c| c.ordinal).max().unwrap_or(0);

    let custom_schema = state.vault.load_custom_schema();
    let facts =
        extract_facts_incremental(&context_owned, &new_owned, llm, custom_schema.as_deref())
            .await?;

    let prov = Provenance {
        session: Some(session_id.to_string()),
        chunks: new_chunk_ids,
    };
    let added = process_facts(state, &facts, prov, llm).await;

    // Update the watermark.
    update_watermark(state, session_id, max_ordinal as i64).await?;
    Ok(added)
}

/// Load the processed_through_ordinal watermark for a session.
async fn load_watermark(state: &AppState, session_id: &str) -> anyhow::Result<Option<i64>> {
    let conn = state.vault.storage().conn()?;
    let mut rows = conn
        .query(
            "SELECT processed_through_ordinal FROM sessions WHERE id = ?",
            params![session_id.to_string()],
        )
        .await?;
    match rows.next().await? {
        Some(r) => Ok(Some(r.get::<i64>(0)?)),
        None => Ok(None),
    }
}

/// Update the processed_through_ordinal watermark for a session.
async fn update_watermark(state: &AppState, session_id: &str, ordinal: i64) -> anyhow::Result<()> {
    let (conn, _g) = state.vault.storage().write_conn().await?;
    conn.execute(
        "UPDATE sessions SET processed_through_ordinal = ? WHERE id = ?",
        params![ordinal, session_id.to_string()],
    )
    .await?;
    Ok(())
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
