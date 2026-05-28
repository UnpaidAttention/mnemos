//! `GET /v1/sync/status`, `POST /v1/sync/push|pull`, `GET /v1/sync/conflicts`.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use mnemos_core::sync::{state, SyncBackend};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::config::SyncKind;
use crate::error::ApiError;
use crate::events::Event;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/sync/status", get(status))
        .route("/v1/sync/push", post(push))
        .route("/v1/sync/pull", post(pull))
        .route("/v1/sync/conflicts", get(conflicts))
}

fn make_backend(state: &AppState) -> Option<Arc<dyn SyncBackend>> {
    use mnemos_core::sync::{filesystem::FilesystemSync, git::GitSync, s3::S3Sync};
    let s = state.vault.storage().clone();
    match state.config.sync.kind {
        SyncKind::None => None,
        SyncKind::Filesystem => Some(Arc::new(FilesystemSync::new(s))),
        SyncKind::Git => Some(Arc::new(GitSync::new(
            s,
            state.config.sync.git.remote.clone(),
            state.config.sync.git.branch.clone(),
        ))),
        SyncKind::S3 => Some(Arc::new(S3Sync::new(
            s,
            state.config.sync.s3.remote.clone(),
        ))),
    }
}

async fn status(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    match make_backend(&state) {
        None => Ok(Json(
            json!({ "backend": "none", "ready": false, "detail": "sync disabled" }),
        )),
        Some(b) => {
            let st = b.status().await?;
            let row = state::get_sync_state(state.vault.storage()).await.ok();
            Ok(Json(json!({
                "backend": st.backend,
                "ready": st.ready,
                "detail": st.detail,
                "last_pushed_at": row.as_ref().and_then(|r| r.last_pushed_at.clone()),
                "last_pulled_at": row.as_ref().and_then(|r| r.last_pulled_at.clone()),
                "last_error":     row.as_ref().and_then(|r| r.last_error.clone()),
            })))
        }
    }
}

async fn push(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let b =
        make_backend(&state).ok_or_else(|| ApiError::new(StatusCode::CONFLICT, "sync disabled"))?;
    state.events.publish(Event::SyncStarted {
        backend: b.name().into(),
        direction: "push".into(),
    });
    match b.push(state.vault.paths().files_root()).await {
        Ok(r) => {
            state::record_push(state.vault.storage(), Utc::now(), None).await?;
            state.events.publish(Event::SyncCompleted {
                backend: b.name().into(),
                direction: "push".into(),
                files_changed: r.files_changed,
            });
            Ok(Json(json!({
                "files_changed": r.files_changed,
                "message": r.message,
                "conflicts": r.conflicts,
            })))
        }
        Err(e) => {
            let msg = e.to_string();
            let _ = state::record_push(state.vault.storage(), Utc::now(), Some(&msg)).await;
            state.events.publish(Event::SyncFailed {
                backend: b.name().into(),
                direction: "push".into(),
                error: msg,
            });
            Err(e.into())
        }
    }
}

async fn pull(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let b =
        make_backend(&state).ok_or_else(|| ApiError::new(StatusCode::CONFLICT, "sync disabled"))?;
    state.events.publish(Event::SyncStarted {
        backend: b.name().into(),
        direction: "pull".into(),
    });
    match b.pull(state.vault.paths().files_root()).await {
        Ok(r) => {
            state::record_pull(state.vault.storage(), Utc::now(), None).await?;
            for c in &r.conflicts {
                state.events.publish(Event::SyncConflict {
                    path: c.clone(),
                    detected_by: b.name().into(),
                });
            }
            state.events.publish(Event::SyncCompleted {
                backend: b.name().into(),
                direction: "pull".into(),
                files_changed: r.files_changed,
            });
            Ok(Json(json!({
                "files_changed": r.files_changed,
                "message": r.message,
                "conflicts": r.conflicts,
            })))
        }
        Err(e) => {
            let msg = e.to_string();
            let _ = state::record_pull(state.vault.storage(), Utc::now(), Some(&msg)).await;
            state.events.publish(Event::SyncFailed {
                backend: b.name().into(),
                direction: "pull".into(),
                error: msg,
            });
            Err(e.into())
        }
    }
}

async fn conflicts(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let rows = state::list_unresolved_conflicts(state.vault.storage()).await?;
    Ok(Json(json!({ "conflicts": rows })))
}
