//! Salience accumulator backing reflection triggers.

use crate::error::{MnemosError, Result};
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use libsql::params;

/// Current accumulated salience.
pub async fn get_salience(storage: &Storage) -> Result<f64> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query("SELECT salience FROM reflection_state WHERE id = 1", ())
        .await?;
    match rows.next().await? {
        Some(r) => Ok(r.get::<f64>(0)?),
        None => Ok(0.0),
    }
}

/// Add `delta` to the accumulator; returns the new value.
pub async fn bump_salience(storage: &Storage, delta: f64) -> Result<f64> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "UPDATE reflection_state SET salience = salience + ? WHERE id = 1",
        params![delta],
    )
    .await?;
    drop(_g);
    get_salience(storage).await
}

/// Reset the accumulator to zero and record the reflection time.
pub async fn reset_salience(storage: &Storage, now: DateTime<Utc>) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    let n = conn
        .execute(
            "UPDATE reflection_state SET salience = 0, last_reflected_at = ? WHERE id = 1",
            params![now.to_rfc3339()],
        )
        .await?;
    if n == 0 {
        return Err(MnemosError::Internal("reflection_state row missing".into()));
    }
    Ok(())
}
