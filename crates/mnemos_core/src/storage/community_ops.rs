//! Persistence for entity→community membership.

use crate::error::Result;
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use libsql::params;

/// Fully replace community membership with the given `(entity_id, community_id)`
/// assignments (stale entities are removed).
pub async fn store_communities(
    storage: &Storage,
    assignments: &[(String, usize)],
    now: DateTime<Utc>,
) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    let tx = conn.transaction().await?;
    tx.execute("DELETE FROM entity_communities", ()).await?;
    let ts = now.to_rfc3339();
    for (entity_id, community_id) in assignments {
        tx.execute(
            "INSERT OR REPLACE INTO entity_communities (entity_id, community_id, detected_at)
                 VALUES (?, ?, ?)",
            params![entity_id.clone(), *community_id as i64, ts.clone()],
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Entity ids belonging to a community.
pub async fn community_members(storage: &Storage, community_id: usize) -> Result<Vec<String>> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT entity_id FROM entity_communities WHERE community_id = ?",
            params![community_id as i64],
        )
        .await?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().await? {
        out.push(r.get::<String>(0)?);
    }
    Ok(out)
}

/// Distinct community ids, ascending.
pub async fn list_community_ids(storage: &Storage) -> Result<Vec<usize>> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT DISTINCT community_id FROM entity_communities ORDER BY community_id",
            (),
        )
        .await?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().await? {
        out.push(r.get::<i64>(0)? as usize);
    }
    Ok(out)
}
