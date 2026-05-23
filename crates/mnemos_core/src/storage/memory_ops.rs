use crate::error::{MnemosError, Result};
use crate::storage::Storage;
use crate::tier::Tier;
use crate::types::{Memory, MemoryType};
use chrono::{DateTime, Utc};
use libsql::{params, Row};
use std::str::FromStr;

pub async fn insert_memory(
    storage: &Storage,
    mem: &Memory,
    file_path: &str,
    content_hash: &str,
) -> Result<()> {
    let (conn, _guard) = storage.write_conn().await?;
    let tx = conn.transaction().await?;

    tx.execute(
        "INSERT INTO memories
            (id, tier, kind, title, body, file_path, content_hash,
             tags_json, entities_json, links_json, provenance_json,
             created_at, ingested_at, valid_at, invalid_at, superseded_by,
             strength, importance, last_accessed, access_count,
             workspace, source_tool, mnemos_version, version)
         VALUES
            (?1, ?2, ?3, ?4, ?5, ?6, ?7,
             ?8, ?9, ?10, ?11,
             ?12, ?13, ?14, ?15, ?16,
             ?17, ?18, ?19, ?20,
             ?21, ?22, ?23, ?24)",
        params![
            mem.id.clone(),
            mem.tier.as_str().to_string(),
            serde_json::to_string(&mem.kind)?
                .trim_matches('"')
                .to_string(),
            mem.title.clone(),
            mem.body.clone(),
            file_path.to_string(),
            content_hash.to_string(),
            serde_json::to_string(&mem.tags)?,
            serde_json::to_string(&mem.entities)?,
            serde_json::to_string(&mem.links)?,
            serde_json::to_string(&mem.provenance)?,
            mem.created_at.to_rfc3339(),
            mem.ingested_at.to_rfc3339(),
            mem.valid_at.to_rfc3339(),
            mem.invalid_at.map(|d| d.to_rfc3339()),
            mem.superseded_by.clone(),
            mem.strength,
            mem.importance,
            mem.last_accessed.to_rfc3339(),
            mem.access_count as i64,
            mem.workspace.clone(),
            mem.source_tool.clone(),
            mem.mnemos_version as i64,
            1_i64,
        ],
    )
    .await?;

    tx.execute(
        "INSERT INTO memory_fts (memory_id, title, body) VALUES (?1, ?2, ?3)",
        params![mem.id.clone(), mem.title.clone(), mem.body.clone()],
    )
    .await?;

    for link in &mem.links {
        tx.execute(
            "INSERT OR IGNORE INTO memory_links (source_id, target_id, kind) VALUES (?1, ?2, 'link')",
            params![mem.id.clone(), link.clone()],
        )
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn get_memory(storage: &Storage, id: &str) -> Result<Memory> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT id, tier, kind, title, body,
                    tags_json, entities_json, links_json, provenance_json,
                    created_at, ingested_at, valid_at, invalid_at, superseded_by,
                    strength, importance, last_accessed, access_count,
                    workspace, source_tool, mnemos_version
             FROM memories WHERE id = ?1",
            params![id.to_string()],
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| MnemosError::MemoryNotFound(id.into()))?;
    row_to_memory(&row)
}

pub(crate) fn row_to_memory(row: &Row) -> Result<Memory> {
    let tier_str: String = row.get(1)?;
    let kind_str: String = row.get(2)?;
    let kind: MemoryType = serde_json::from_str(&format!("\"{kind_str}\""))?;
    Ok(Memory {
        id: row.get(0)?,
        tier: Tier::from_str(&tier_str)?,
        kind,
        title: row.get(3)?,
        body: row.get(4)?,
        tags: serde_json::from_str(&row.get::<String>(5)?)?,
        entities: serde_json::from_str(&row.get::<String>(6)?)?,
        links: serde_json::from_str(&row.get::<String>(7)?)?,
        provenance: serde_json::from_str(&row.get::<String>(8)?)?,
        created_at: parse_ts(&row.get::<String>(9)?)?,
        ingested_at: parse_ts(&row.get::<String>(10)?)?,
        valid_at: parse_ts(&row.get::<String>(11)?)?,
        invalid_at: row
            .get::<Option<String>>(12)?
            .map(|s| parse_ts(&s))
            .transpose()?,
        superseded_by: row.get(13)?,
        strength: row.get(14)?,
        importance: row.get(15)?,
        last_accessed: parse_ts(&row.get::<String>(16)?)?,
        access_count: row.get::<i64>(17)? as u64,
        workspace: row.get(18)?,
        source_tool: row.get(19)?,
        mnemos_version: row.get::<i64>(20)? as u32,
    })
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| MnemosError::Validation(format!("bad timestamp '{s}': {e}")))
}

/// Mark `old_id` as invalid at `invalid_at`, set its `superseded_by` to
/// `new_id`, and insert a `supersedes` link from `new_id` → `old_id`.
///
/// Returns `MnemosError::MemoryNotFound` if `old_id` is already invalidated
/// or does not exist.
pub async fn supersede_memory(
    storage: &Storage,
    old_id: &str,
    new_id: &str,
    invalid_at: DateTime<Utc>,
) -> Result<()> {
    let (conn, _guard) = storage.write_conn().await?;
    let tx = conn.transaction().await?;

    let affected = tx
        .execute(
            "UPDATE memories
                SET invalid_at = ?, superseded_by = ?
              WHERE id = ? AND invalid_at IS NULL",
            params![
                invalid_at.to_rfc3339(),
                new_id.to_string(),
                old_id.to_string()
            ],
        )
        .await?;
    if affected == 0 {
        return Err(MnemosError::MemoryNotFound(old_id.into()));
    }

    tx.execute(
        "INSERT OR IGNORE INTO memory_links (source_id, target_id, kind)
             VALUES (?, ?, 'supersedes')",
        params![new_id.to_string(), old_id.to_string()],
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Set `invalid_at` on a memory without creating a supersession link.
///
/// Useful for plain retraction / expiry. Returns `MnemosError::MemoryNotFound`
/// if the memory is already invalidated or does not exist.
pub async fn soft_invalidate(storage: &Storage, id: &str, at: DateTime<Utc>) -> Result<()> {
    let (conn, _guard) = storage.write_conn().await?;
    let affected = conn
        .execute(
            "UPDATE memories SET invalid_at = ? WHERE id = ? AND invalid_at IS NULL",
            params![at.to_rfc3339(), id.to_string()],
        )
        .await?;
    if affected == 0 {
        return Err(MnemosError::MemoryNotFound(id.into()));
    }
    Ok(())
}
