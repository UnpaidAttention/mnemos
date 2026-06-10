//! Entity + edge storage primitives backing the knowledge graph.

use crate::error::Result;
use crate::id::{new_edge_id, new_entity_id};
use crate::storage::Storage;
use crate::types::Entity;
use chrono::{DateTime, Utc};
use libsql::params;

/// Insert an entity by unique `name`, or return the id of the existing one.
/// When `description` is `Some` and the entity already exists, the description
/// is updated only if the new one is longer (enrichment, not overwrite).
pub async fn upsert_entity(
    storage: &Storage,
    name: &str,
    kind: &str,
    description: Option<&str>,
) -> Result<String> {
    let (conn, _guard) = storage.write_conn().await?;
    let mut rows = conn
        .query(
            "SELECT id, description FROM entities WHERE name = ?",
            params![name.to_string()],
        )
        .await?;
    if let Some(r) = rows.next().await? {
        let id: String = r.get::<String>(0)?;
        let existing_desc: Option<String> = r.get::<Option<String>>(1)?;
        drop(rows);
        // Enrich: update description if new one is provided and longer
        if let Some(new_desc) = description {
            let new_desc = new_desc.trim();
            let should_update = match &existing_desc {
                None => !new_desc.is_empty(),
                Some(old) => new_desc.len() > old.len(),
            };
            if should_update {
                conn.execute(
                    "UPDATE entities SET description = ? WHERE id = ?",
                    params![new_desc.to_string(), id.clone()],
                )
                .await?;
            }
        }
        return Ok(id);
    }
    drop(rows);
    let id = new_entity_id();
    let desc_val = description
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());
    conn.execute(
        "INSERT INTO entities (id, name, kind, aliases, description, file_path, created_at)
             VALUES (?, ?, ?, '[]', ?, NULL, ?)",
        params![
            id.clone(),
            name.to_string(),
            kind.to_string(),
            desc_val,
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

/// Resolve entity names for the given ids (skips ids that no longer exist).
pub async fn entity_names(storage: &Storage, ids: &[String]) -> Result<Vec<String>> {
    let conn = storage.conn()?;
    let mut out = Vec::new();
    for id in ids {
        let mut rows = conn
            .query(
                "SELECT name FROM entities WHERE id = ?",
                params![id.clone()],
            )
            .await?;
        if let Some(r) = rows.next().await? {
            out.push(r.get::<String>(0)?);
        }
    }
    Ok(out)
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

/// Reassign all mentions and edges from `source` to `target`, then delete the
/// source entity row. Self-loops created by the merge are removed. Transaction-
/// wrapped; idempotent if `source` is already gone.
pub async fn merge_entities(storage: &Storage, source: &str, target: &str) -> Result<()> {
    if source == target {
        return Ok(());
    }
    let (conn, _g) = storage.write_conn().await?;
    let tx = conn.transaction().await?;
    let mut t = tx
        .query(
            "SELECT 1 FROM entities WHERE id = ?",
            params![target.to_string()],
        )
        .await?;
    if t.next().await?.is_none() {
        return Err(crate::error::MnemosError::EntityNotFound(target.into()));
    }
    drop(t);
    let mut s = tx
        .query(
            "SELECT name FROM entities WHERE id = ?",
            params![source.to_string()],
        )
        .await?;
    let source_row = s.next().await?;
    let source_name: Option<String> = match source_row {
        Some(r) => Some(r.get::<String>(0)?),
        None => None,
    };
    drop(s);
    if source_name.is_none() {
        return Ok(());
    }

    // Move mentions: idempotent insert into target, then delete source rows.
    tx.execute(
        "INSERT OR IGNORE INTO entity_mentions (memory_id, entity_id)
            SELECT memory_id, ? FROM entity_mentions WHERE entity_id = ?",
        params![target.to_string(), source.to_string()],
    )
    .await?;
    tx.execute(
        "DELETE FROM entity_mentions WHERE entity_id = ?",
        params![source.to_string()],
    )
    .await?;

    // Reassign edges (both endpoints).
    tx.execute(
        "UPDATE entity_edges SET source_entity_id = ? WHERE source_entity_id = ?",
        params![target.to_string(), source.to_string()],
    )
    .await?;
    tx.execute(
        "UPDATE entity_edges SET target_entity_id = ? WHERE target_entity_id = ?",
        params![target.to_string(), source.to_string()],
    )
    .await?;
    tx.execute(
        "DELETE FROM entity_edges WHERE source_entity_id = target_entity_id",
        (),
    )
    .await?;

    // Reassign community membership (entity_id is PRIMARY KEY → INSERT OR IGNORE).
    tx.execute(
        "INSERT OR IGNORE INTO entity_communities (entity_id, community_id, detected_at)
            SELECT ?, community_id, detected_at FROM entity_communities WHERE entity_id = ?",
        params![target.to_string(), source.to_string()],
    )
    .await?;
    tx.execute(
        "DELETE FROM entity_communities WHERE entity_id = ?",
        params![source.to_string()],
    )
    .await?;

    // Append the source's name as an alias on the target so look-ups still resolve.
    if let Some(src_name) = source_name {
        let mut arows = tx
            .query(
                "SELECT aliases FROM entities WHERE id = ?",
                params![target.to_string()],
            )
            .await?;
        let aliases_json: String = arows
            .next()
            .await?
            .ok_or_else(|| crate::error::MnemosError::EntityNotFound(target.into()))?
            .get(0)?;
        drop(arows);
        let mut aliases: Vec<String> = serde_json::from_str(&aliases_json).unwrap_or_default();
        if !aliases.iter().any(|a| a == &src_name) {
            aliases.push(src_name);
        }
        tx.execute(
            "UPDATE entities SET aliases = ? WHERE id = ?",
            params![serde_json::to_string(&aliases)?, target.to_string()],
        )
        .await?;
    }

    tx.execute(
        "DELETE FROM entities WHERE id = ?",
        params![source.to_string()],
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod merge_tests {
    use super::*;
    use crate::paths::Paths;
    use crate::vault::{RememberOpts, Vault};
    use tempfile::TempDir;

    #[tokio::test]
    async fn merge_moves_mentions_and_edges() {
        let tmp = TempDir::new().unwrap();
        let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
        let a = upsert_entity(v.storage(), "A", "x", None).await.unwrap();
        let b = upsert_entity(v.storage(), "B", "x", None).await.unwrap();
        let c = upsert_entity(v.storage(), "C", "x", None).await.unwrap();
        // entity_mentions has an ON DELETE CASCADE FK to memories — create a
        // real memory so the link insert satisfies the constraint.
        let mem = v.remember("source", RememberOpts::default()).await.unwrap();
        upsert_edge(v.storage(), &a, &c, "rel", &mem, chrono::Utc::now())
            .await
            .unwrap();
        link_entity_mention(v.storage(), &mem, &a).await.unwrap();

        merge_entities(v.storage(), &a, &b).await.unwrap();

        let conn = v.storage().conn().unwrap();
        let mut r1 = conn
            .query(
                "SELECT COUNT(*) FROM entities WHERE id = ?",
                params![a.clone()],
            )
            .await
            .unwrap();
        let n1: i64 = r1.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(n1, 0);
        let mut r2 = conn
            .query(
                "SELECT COUNT(*) FROM entity_mentions WHERE entity_id = ?",
                params![b.clone()],
            )
            .await
            .unwrap();
        let n2: i64 = r2.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(n2, 1);
        let mut r3 = conn
            .query(
                "SELECT COUNT(*) FROM entity_edges WHERE source_entity_id = ?1 OR target_entity_id = ?1",
                params![b.clone()],
            )
            .await
            .unwrap();
        let n3: i64 = r3.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(n3, 1);
    }

    #[tokio::test]
    async fn merge_missing_target_returns_error() {
        let tmp = TempDir::new().unwrap();
        let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
        let a = upsert_entity(v.storage(), "A", "x", None).await.unwrap();
        let err = merge_entities(v.storage(), &a, "ent_nope")
            .await
            .unwrap_err();
        assert!(matches!(err, crate::error::MnemosError::EntityNotFound(_)));
    }

    #[tokio::test]
    async fn merge_idempotent_when_source_missing() {
        let tmp = TempDir::new().unwrap();
        let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
        let b = upsert_entity(v.storage(), "B", "x", None).await.unwrap();
        // Source doesn't exist; target does. Should be a no-op Ok.
        merge_entities(v.storage(), "ent_gone", &b).await.unwrap();
    }
}
