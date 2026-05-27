//! Entity + edge storage primitives backing the knowledge graph.

use crate::error::Result;
use crate::id::{new_edge_id, new_entity_id};
use crate::storage::Storage;
use crate::types::Entity;
use chrono::{DateTime, Utc};
use libsql::params;

/// Insert an entity by unique `name`, or return the id of the existing one.
pub async fn upsert_entity(storage: &Storage, name: &str, kind: &str) -> Result<String> {
    let (conn, _guard) = storage.write_conn().await?;
    let mut rows = conn
        .query(
            "SELECT id FROM entities WHERE name = ?",
            params![name.to_string()],
        )
        .await?;
    if let Some(r) = rows.next().await? {
        return Ok(r.get::<String>(0)?);
    }
    drop(rows);
    let id = new_entity_id();
    conn.execute(
        "INSERT INTO entities (id, name, kind, aliases, description, file_path, created_at)
             VALUES (?, ?, ?, '[]', NULL, NULL, ?)",
        params![
            id.clone(),
            name.to_string(),
            kind.to_string(),
            Utc::now().to_rfc3339()
        ],
    )
    .await?;
    Ok(id)
}

/// Look up an entity by exact name.
pub async fn find_entity_by_name(storage: &Storage, name: &str) -> Result<Option<Entity>> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT id, name, kind, aliases, description, file_path, created_at
                 FROM entities WHERE name = ?",
            params![name.to_string()],
        )
        .await?;
    match rows.next().await? {
        None => Ok(None),
        Some(r) => Ok(Some(Entity {
            id: r.get(0)?,
            name: r.get(1)?,
            kind: r.get(2)?,
            aliases: serde_json::from_str(&r.get::<String>(3)?)?,
            description: r.get(4)?,
            file_path: r.get(5)?,
            created_at: DateTime::parse_from_rfc3339(&r.get::<String>(6)?)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| crate::error::MnemosError::Validation(format!("bad ts: {e}")))?,
        })),
    }
}

/// Record that `memory_id` mentions `entity_id`. Idempotent.
pub async fn link_entity_mention(
    storage: &Storage,
    memory_id: &str,
    entity_id: &str,
) -> Result<()> {
    let (conn, _guard) = storage.write_conn().await?;
    conn.execute(
        "INSERT OR IGNORE INTO entity_mentions (memory_id, entity_id) VALUES (?, ?)",
        params![memory_id.to_string(), entity_id.to_string()],
    )
    .await?;
    Ok(())
}

/// Insert a relationship edge, or reinforce the existing active one.
///
/// "Active" = same `(source, target, relation)` with `invalid_at IS NULL`. When
/// found, the edge's `weight` is bumped and `source_memory_id` is appended to
/// its provenance list. Returns the edge id either way.
pub async fn upsert_edge(
    storage: &Storage,
    source_id: &str,
    target_id: &str,
    relation: &str,
    source_memory_id: &str,
    valid_at: DateTime<Utc>,
) -> Result<String> {
    let (conn, _guard) = storage.write_conn().await?;
    let mut rows = conn
        .query(
            "SELECT id, source_memory_ids FROM entity_edges
              WHERE source_entity_id = ? AND target_entity_id = ?
                AND relation = ? AND invalid_at IS NULL",
            params![
                source_id.to_string(),
                target_id.to_string(),
                relation.to_string()
            ],
        )
        .await?;
    if let Some(r) = rows.next().await? {
        let id: String = r.get(0)?;
        let mids_json: String = r.get(1)?;
        drop(rows);
        let mut mids: Vec<String> = serde_json::from_str(&mids_json).unwrap_or_default();
        if !mids.iter().any(|m| m == source_memory_id) {
            mids.push(source_memory_id.to_string());
        }
        conn.execute(
            "UPDATE entity_edges SET weight = weight + 1.0, source_memory_ids = ? WHERE id = ?",
            params![serde_json::to_string(&mids)?, id.clone()],
        )
        .await?;
        return Ok(id);
    }
    drop(rows);
    let id = new_edge_id();
    let mids = serde_json::to_string(&vec![source_memory_id.to_string()])?;
    conn.execute(
        "INSERT INTO entity_edges
            (id, source_entity_id, target_entity_id, relation, created_at, valid_at, invalid_at, weight, source_memory_ids)
         VALUES (?, ?, ?, ?, ?, ?, NULL, 1.0, ?)",
        params![
            id.clone(),
            source_id.to_string(),
            target_id.to_string(),
            relation.to_string(),
            Utc::now().to_rfc3339(),
            valid_at.to_rfc3339(),
            mids
        ],
    )
    .await?;
    Ok(id)
}
