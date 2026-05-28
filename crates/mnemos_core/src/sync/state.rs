//! sync_state + sync_conflicts persistence helpers.

use crate::error::Result;
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use libsql::params;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ConflictRow {
    pub id: i64,
    pub ts: String,
    pub path: String,
    pub detected_by: String,
    pub details: Option<String>,
}

pub async fn record_push(storage: &Storage, at: DateTime<Utc>, error: Option<&str>) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "UPDATE sync_state SET last_pushed_at = ?, last_error = ? WHERE id = 1",
        params![at.to_rfc3339(), error.map(|s| s.to_string())],
    )
    .await?;
    Ok(())
}

pub async fn record_pull(storage: &Storage, at: DateTime<Utc>, error: Option<&str>) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "UPDATE sync_state SET last_pulled_at = ?, last_error = ? WHERE id = 1",
        params![at.to_rfc3339(), error.map(|s| s.to_string())],
    )
    .await?;
    Ok(())
}

pub async fn record_conflict(
    storage: &Storage,
    path: &str,
    detected_by: &str,
    details: Option<&str>,
) -> Result<i64> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "INSERT INTO sync_conflicts (ts, path, detected_by, details) VALUES (?, ?, ?, ?)",
        params![
            Utc::now().to_rfc3339(),
            path.to_string(),
            detected_by.to_string(),
            details.map(|s| s.to_string())
        ],
    )
    .await?;
    let mut rows = conn.query("SELECT last_insert_rowid()", ()).await?;
    Ok(rows.next().await?.unwrap().get::<i64>(0)?)
}

pub async fn resolve_conflict(storage: &Storage, id: i64, at: DateTime<Utc>) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "UPDATE sync_conflicts SET resolved_at = ? WHERE id = ?",
        params![at.to_rfc3339(), id],
    )
    .await?;
    Ok(())
}

pub async fn list_unresolved_conflicts(storage: &Storage) -> Result<Vec<ConflictRow>> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT id, ts, path, detected_by, details FROM sync_conflicts
              WHERE resolved_at IS NULL ORDER BY ts DESC",
            (),
        )
        .await?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().await? {
        out.push(ConflictRow {
            id: r.get(0)?,
            ts: r.get(1)?,
            path: r.get(2)?,
            detected_by: r.get(3)?,
            details: r.get(4)?,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncStateRow {
    pub last_pushed_at: Option<String>,
    pub last_pulled_at: Option<String>,
    pub last_error: Option<String>,
}

pub async fn get_sync_state(storage: &Storage) -> Result<SyncStateRow> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT last_pushed_at, last_pulled_at, last_error FROM sync_state WHERE id = 1",
            (),
        )
        .await?;
    let r = rows
        .next()
        .await?
        .ok_or_else(|| crate::error::MnemosError::Internal("sync_state row missing".into()))?;
    Ok(SyncStateRow {
        last_pushed_at: r.get(0)?,
        last_pulled_at: r.get(1)?,
        last_error: r.get(2)?,
    })
}
