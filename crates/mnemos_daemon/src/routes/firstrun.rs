//! `GET /v1/first-run` and `POST /v1/first-run/complete` — track whether the
//! first-run wizard has been completed for this vault. The timestamp lives in
//! `vault_meta.first_run_completed_at` (added by schema migration v8).

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use mnemos_core::error::MnemosError;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/first-run", get(get_state))
        .route("/v1/first-run/complete", post(complete))
}

async fn get_state(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let conn = state.vault.storage().conn()?;
    let mut rows = conn
        .query(
            "SELECT first_run_completed_at FROM vault_meta WHERE id = 1",
            (),
        )
        .await
        .map_err(MnemosError::from)?;
    let completed_at: Option<String> = match rows.next().await.map_err(MnemosError::from)? {
        Some(r) => r.get::<Option<String>>(0).map_err(MnemosError::from)?,
        None => None,
    };
    Ok(Json(json!({ "completed_at": completed_at })))
}

async fn complete(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let (conn, _g) = state.vault.storage().write_conn().await?;
    conn.execute(
        "UPDATE vault_meta SET first_run_completed_at = ? WHERE id = 1",
        libsql::params![chrono::Utc::now().to_rfc3339()],
    )
    .await
    .map_err(MnemosError::from)?;
    Ok(Json(json!({ "completed": true })))
}
