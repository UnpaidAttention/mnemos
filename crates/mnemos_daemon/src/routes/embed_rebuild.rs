//! `GET /v1/embed-rebuild/status`, `POST /v1/embed-rebuild/start`,
//! `POST /v1/embed-rebuild/abort` (Plan 9 Task 10).
//!
//! The rebuild itself lives in [`mnemos_core::embedder_rebuild::rebuild`].
//! This module wraps it with:
//!   * REST surface for kicking off / observing runs
//!   * Single-flight protection via `AppState.rebuild_status`
//!   * WebSocket event emission on start / complete / fail
//!
//! v0.8.0 emits only `EmbedRebuildStarted`, `EmbedRebuildCompleted`, and
//! `EmbedRebuildFailed`. Per-memory `EmbedRebuildProgress` is reserved for
//! v0.9.0 — the desktop UI polls `/status` instead.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use mnemos_core::embedder_rebuild::{rebuild, RebuildOptions, RebuildStatus};
use serde::Deserialize;
use serde_json::Value;
use std::sync::atomic::Ordering;

use crate::error::ApiError;
use crate::events::Event;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/embed-rebuild/status", get(status))
        .route("/v1/embed-rebuild/start", post(start))
        .route("/v1/embed-rebuild/abort", post(abort))
}

#[derive(Deserialize)]
struct StartReq {
    target_kind: String,
    target_model: String,
    target_dim: u32,
}

async fn status(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let status = state.rebuild_status.lock().await.clone();
    Ok(Json(serde_json::to_value(&status).map_err(|e| {
        ApiError::internal(format!("serialize rebuild status: {e}"))
    })?))
}

async fn start(
    State(state): State<AppState>,
    Json(req): Json<StartReq>,
) -> Result<Json<Value>, ApiError> {
    // Single-flight: refuse if a rebuild is already running.
    {
        let current = state.rebuild_status.lock().await;
        if matches!(*current, RebuildStatus::Running { .. }) {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "rebuild already in progress",
            ));
        }
    }

    let opts = RebuildOptions {
        target_kind: req.target_kind.clone(),
        target_model: req.target_model.clone(),
        target_dim: req.target_dim,
        actor: "daemon".into(),
    };

    // Increment the generation counter and capture this run's generation
    // *before* spawning.  The background task will only write its final
    // status when the counter still matches, preventing an aborted or
    // double-started rebuild from overwriting a later run's status (P1-12).
    let this_gen = state
        .rebuild_generation
        .fetch_add(1, Ordering::SeqCst)
        .wrapping_add(1);

    // Flip status → Running before spawning so /status reflects the kickoff
    // even before the worker task starts.
    {
        let mut s = state.rebuild_status.lock().await;
        *s = RebuildStatus::Running {
            processed: 0,
            total: 0,
        };
    }

    state.events.publish(Event::EmbedRebuildStarted {
        target_kind: req.target_kind.clone(),
        target_model: req.target_model.clone(),
        target_dim: req.target_dim,
    });

    // Spawn the rebuild in the background. The handler returns immediately
    // with an acknowledgement; callers poll /status for completion.
    let vault = state.vault.clone();
    let status_handle = state.rebuild_status.clone();
    let gen_handle = state.rebuild_generation.clone();
    let events = state.events.clone();
    tokio::spawn(async move {
        match rebuild(&vault, opts).await {
            Ok(s) => {
                let (processed, skipped, total) = match &s {
                    RebuildStatus::Completed {
                        processed,
                        skipped,
                        total,
                        ..
                    } => (*processed, *skipped, *total),
                    _ => (0, 0, 0),
                };
                // Only write the final status if this task's generation still
                // matches the current generation — if /abort fired or a new
                // rebuild started we must not clobber its status (P1-12).
                let mut guard = status_handle.lock().await;
                if gen_handle.load(Ordering::SeqCst) == this_gen {
                    *guard = s;
                    events.publish(Event::EmbedRebuildCompleted {
                        processed,
                        skipped,
                        total,
                    });
                }
            }
            Err(e) => {
                let err = e.to_string();
                let mut guard = status_handle.lock().await;
                if gen_handle.load(Ordering::SeqCst) == this_gen {
                    *guard = RebuildStatus::Failed {
                        error: err.clone(),
                        processed: 0,
                    };
                    events.publish(Event::EmbedRebuildFailed {
                        error: err,
                        processed: 0,
                    });
                }
            }
        }
    });

    Ok(Json(serde_json::json!({ "started": true })))
}

/// Best-effort abort: flips status to `Failed("aborted")` and advances the
/// generation counter so the in-flight background task will not overwrite the
/// aborted status when it completes (P1-12).  The shadow table preserves
/// partial work for a subsequent resume.
async fn abort(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let mut s = state.rebuild_status.lock().await;
    if matches!(*s, RebuildStatus::Running { .. }) {
        // Advance generation so the running task's generation no longer matches
        // and will not write Completed/Failed over this Aborted status.
        state.rebuild_generation.fetch_add(1, Ordering::SeqCst);
        *s = RebuildStatus::Failed {
            error: "aborted".into(),
            processed: 0,
        };
        Ok(Json(serde_json::json!({ "aborted": true })))
    } else {
        Ok(Json(
            serde_json::json!({ "aborted": false, "reason": "no rebuild running" }),
        ))
    }
}
