//! REST endpoints for session lifecycle and chunk ingestion.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use libsql::params;
use mnemos_core::id::{new_chunk_id, new_session_id};
use mnemos_core::types::{Chunk, Session};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/sessions", post(start_session))
        .route("/v1/sessions/{id}", get(get_session))
        .route("/v1/sessions/{id}/chunks", post(add_chunk))
        .route("/v1/sessions/{id}/end", post(end_session))
}

#[derive(Debug, Deserialize)]
struct StartSessionReq {
    #[serde(default)]
    source_tool: Option<String>,
    #[serde(default)]
    workspace: Option<String>,
}

#[derive(Debug, Serialize)]
struct StartSessionResp {
    id: String,
}

async fn start_session(
    State(state): State<AppState>,
    Json(req): Json<StartSessionReq>,
) -> Result<(StatusCode, Json<StartSessionResp>), ApiError> {
    let id = new_session_id();
    let (conn, _g) = state.vault.storage().write_conn().await?;
    conn.execute(
        "INSERT INTO sessions (id, source_tool, workspace, started_at) VALUES (?, ?, ?, ?)",
        params![
            id.clone(),
            req.source_tool,
            req.workspace,
            Utc::now().to_rfc3339()
        ],
    )
    .await
    .map_err(mnemos_core::error::MnemosError::from)?;
    state
        .events
        .publish(crate::events::Event::SessionStarted { id: id.clone() });
    Ok((StatusCode::CREATED, Json(StartSessionResp { id })))
}

#[derive(Debug, Deserialize)]
struct AddChunkReq {
    #[serde(default)]
    speaker: Option<String>,
    #[serde(default)]
    ordinal: Option<u32>,
    body: String,
    #[serde(default)]
    source_meta: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct AddChunkResp {
    chunk_id: String,
}

async fn add_chunk(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<AddChunkReq>,
) -> Result<(StatusCode, Json<AddChunkResp>), ApiError> {
    let chunk_id = new_chunk_id();
    let ordinal = req.ordinal.unwrap_or(0);
    let source_meta_str = req.source_meta.as_ref().map(|v| v.to_string());
    let (conn, _g) = state.vault.storage().write_conn().await?;
    conn.execute(
        "INSERT INTO chunks (id, session_id, speaker, ordinal, body, created_at, source_meta)
            VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![
            chunk_id.clone(),
            session_id,
            req.speaker,
            ordinal as i64,
            req.body,
            Utc::now().to_rfc3339(),
            source_meta_str,
        ],
    )
    .await
    .map_err(mnemos_core::error::MnemosError::from)?;
    Ok((StatusCode::CREATED, Json(AddChunkResp { chunk_id })))
}

#[derive(Debug, Deserialize)]
struct EndSessionReq {
    #[serde(default)]
    summary: Option<String>,
}

async fn end_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<EndSessionReq>,
) -> Result<StatusCode, ApiError> {
    let (conn, _g) = state.vault.storage().write_conn().await?;
    let n = conn
        .execute(
            "UPDATE sessions SET ended_at = ?, summary = ? WHERE id = ?",
            params![Utc::now().to_rfc3339(), req.summary, id.clone()],
        )
        .await
        .map_err(mnemos_core::error::MnemosError::from)?;
    if n == 0 {
        return Err(ApiError::not_found(format!("session {id}")));
    }
    state
        .events
        .publish(crate::events::Event::SessionEnded { id: id.clone() });
    Ok(StatusCode::OK)
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state.vault.storage().conn()?;

    let mut rs = conn
        .query(
            "SELECT id, source_tool, workspace, started_at, ended_at, summary
             FROM sessions WHERE id = ?",
            params![id.clone()],
        )
        .await
        .map_err(mnemos_core::error::MnemosError::from)?;
    let row = rs
        .next()
        .await
        .map_err(mnemos_core::error::MnemosError::from)?
        .ok_or_else(|| ApiError::not_found(format!("session {id}")))?;

    let ended_at_raw: Option<String> = row.get(4).map_err(mnemos_core::error::MnemosError::from)?;
    let session = Session {
        id: row
            .get::<String>(0)
            .map_err(mnemos_core::error::MnemosError::from)?,
        source_tool: row
            .get::<Option<String>>(1)
            .map_err(mnemos_core::error::MnemosError::from)?,
        workspace: row
            .get::<Option<String>>(2)
            .map_err(mnemos_core::error::MnemosError::from)?,
        started_at: parse_ts(
            &row.get::<String>(3)
                .map_err(mnemos_core::error::MnemosError::from)?,
        )?,
        ended_at: ended_at_raw.map(|s| parse_ts(&s)).transpose()?,
        summary: row
            .get::<Option<String>>(5)
            .map_err(mnemos_core::error::MnemosError::from)?,
    };

    let mut cs = conn
        .query(
            "SELECT id, session_id, speaker, ordinal, body, created_at, source_tool, source_meta
             FROM chunks WHERE session_id = ? ORDER BY ordinal ASC",
            params![id.clone()],
        )
        .await
        .map_err(mnemos_core::error::MnemosError::from)?;
    let mut chunks: Vec<Chunk> = Vec::new();
    while let Some(r) = cs
        .next()
        .await
        .map_err(mnemos_core::error::MnemosError::from)?
    {
        let source_meta_raw: Option<String> =
            r.get(7).map_err(mnemos_core::error::MnemosError::from)?;
        chunks.push(Chunk {
            id: r
                .get::<String>(0)
                .map_err(mnemos_core::error::MnemosError::from)?,
            session_id: r
                .get::<String>(1)
                .map_err(mnemos_core::error::MnemosError::from)?,
            speaker: r
                .get::<Option<String>>(2)
                .map_err(mnemos_core::error::MnemosError::from)?,
            ordinal: r
                .get::<i64>(3)
                .map_err(mnemos_core::error::MnemosError::from)? as u32,
            body: r
                .get::<String>(4)
                .map_err(mnemos_core::error::MnemosError::from)?,
            created_at: parse_ts(
                &r.get::<String>(5)
                    .map_err(mnemos_core::error::MnemosError::from)?,
            )?,
            source_tool: r
                .get::<Option<String>>(6)
                .map_err(mnemos_core::error::MnemosError::from)?,
            source_meta: source_meta_raw
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(|e: serde_json::Error| ApiError::internal(e.to_string()))?,
        });
    }

    Ok(Json(
        serde_json::json!({ "session": session, "chunks": chunks }),
    ))
}

fn parse_ts(s: &str) -> Result<chrono::DateTime<chrono::Utc>, ApiError> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&chrono::Utc))
        .map_err(|e| ApiError::internal(format!("bad ts '{s}': {e}")))
}
