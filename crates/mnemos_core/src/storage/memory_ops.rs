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

    // P1-2: Use INSERT OR REPLACE so a retry after a crash (file written but
    // DB row absent) is idempotent.  The file is always written before this
    // call (by vault.rs), so on a retry the file already exists and the
    // INSERT OR REPLACE simply overwrites the partial/absent row.
    tx.execute(
        "INSERT OR REPLACE INTO memories
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

    // P1-2 / P1-4: FTS5 virtual tables do not honour INSERT OR REPLACE
    // semantics for UNINDEXED columns — a plain INSERT always adds a new row
    // even if one already exists for the same memory_id.  Use DELETE + INSERT
    // so a crash-retry does not accumulate duplicate FTS rows that would
    // inflate BM25 scores.
    tx.execute(
        "DELETE FROM memory_fts WHERE memory_id = ?1",
        params![mem.id.clone()],
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
///
/// P1-4: also removes the FTS row for `old_id` in the same transaction so
/// BM25 search has no ghost rows for superseded memories.
///
/// P2-12: also removes the vector row from `memory_vec` so the vec0 KNN scan
/// never wastes its `k` budget on invalidated/superseded memories.
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

    // P1-4: remove from FTS index to prevent ghost rows corrupting BM25.
    tx.execute(
        "DELETE FROM memory_fts WHERE memory_id = ?",
        params![old_id.to_string()],
    )
    .await?;

    // P2-12: remove vector row so KNN scans only see live memories.
    // The DELETE is a no-op if the memory was never embedded; that is fine.
    tx.execute(
        "DELETE FROM memory_vec WHERE memory_id = ?",
        params![old_id.to_string()],
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Set `invalid_at` on a memory without creating a supersession link.
///
/// Useful for plain retraction / expiry. Returns `MnemosError::MemoryNotFound`
/// if the memory is already invalidated or does not exist.
///
/// P1-4: also removes the FTS row for `id` in the same transaction so BM25
/// search has no ghost rows for forgotten memories.
///
/// P2-12: also removes the vector row from `memory_vec` so the vec0 KNN scan
/// never wastes its `k` budget on invalidated memories.
pub async fn soft_invalidate(storage: &Storage, id: &str, at: DateTime<Utc>) -> Result<()> {
    let (conn, _guard) = storage.write_conn().await?;
    let tx = conn.transaction().await?;

    let affected = tx
        .execute(
            "UPDATE memories SET invalid_at = ? WHERE id = ? AND invalid_at IS NULL",
            params![at.to_rfc3339(), id.to_string()],
        )
        .await?;
    if affected == 0 {
        // Roll back the (no-op) transaction and return the error.
        tx.rollback().await.ok();
        return Err(MnemosError::MemoryNotFound(id.into()));
    }

    // P1-4: remove from FTS index to prevent ghost rows corrupting BM25.
    tx.execute(
        "DELETE FROM memory_fts WHERE memory_id = ?",
        params![id.to_string()],
    )
    .await?;

    // P2-12: remove vector row so KNN scans only see live memories.
    tx.execute(
        "DELETE FROM memory_vec WHERE memory_id = ?",
        params![id.to_string()],
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Filter criteria for [`list_memories`].
#[derive(Debug, Default, Clone)]
pub struct ListFilter {
    /// Restrict results to memories in these tiers. `None` means all tiers.
    pub tiers: Option<Vec<Tier>>,
    /// Restrict results to a specific workspace. `None` matches all workspaces.
    pub workspace: Option<String>,
    /// When `false` (the default), soft-invalidated memories are excluded.
    pub include_invalid: bool,
    /// Maximum number of results to return. `None` means no limit.
    pub limit: Option<usize>,
    /// If non-empty, only return memories whose `tags_json` contains ALL of
    /// these tags (each must appear as a JSON string value in the array).
    ///
    /// P2-11: pushed into SQL via `json_each` to avoid hydrating the entire
    /// tier into Rust memory just to filter by tag.
    pub required_tags: Vec<String>,
}

/// Return memories matching `filter`, ordered newest-first by `created_at`.
pub async fn list_memories(storage: &Storage, filter: ListFilter) -> Result<Vec<Memory>> {
    let conn = storage.conn()?;
    let mut sql = String::from(
        "SELECT id, tier, kind, title, body,
                tags_json, entities_json, links_json, provenance_json,
                created_at, ingested_at, valid_at, invalid_at, superseded_by,
                strength, importance, last_accessed, access_count,
                workspace, source_tool, mnemos_version
         FROM memories WHERE 1=1",
    );
    let mut args: Vec<libsql::Value> = vec![];

    if !filter.include_invalid {
        sql.push_str(" AND invalid_at IS NULL");
    }
    if let Some(ws) = filter.workspace.as_ref() {
        // Workspace filter returns both workspace-tagged AND unscoped (global) memories,
        // per design spec: "workspace='~/code/foo' → returns: workspace-tagged + global".
        // Global memories (e.g. identity facts) surface in every workspace.
        sql.push_str(" AND (workspace IS NULL OR workspace = ?)");
        args.push(ws.clone().into());
    }
    if let Some(tiers) = filter.tiers.as_ref() {
        if !tiers.is_empty() {
            let placeholders = vec!["?"; tiers.len()].join(",");
            sql.push_str(&format!(" AND tier IN ({placeholders})"));
            for t in tiers {
                args.push(t.as_str().to_string().into());
            }
        }
    }
    // P2-11: push tag filter into SQL rather than hydrating the whole tier.
    // Each required tag must appear in the tags_json array. We use a
    // correlated EXISTS + json_each subquery so each tag adds exactly one
    // predicate — no cross-join explosion.
    for tag in &filter.required_tags {
        sql.push_str(" AND EXISTS (SELECT 1 FROM json_each(tags_json) WHERE value = ?)");
        args.push(tag.clone().into());
    }
    sql.push_str(" ORDER BY created_at DESC");
    if let Some(limit) = filter.limit {
        sql.push_str(&format!(" LIMIT {limit}"));
    }

    let mut rows = conn.query(&sql, args).await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(row_to_memory(&row)?);
    }
    Ok(out)
}

/// Record provenance links from a memory to the chunks it was derived from.
/// Idempotent (`INSERT OR IGNORE`). No-op for an empty chunk list.
pub async fn link_memory_chunks(
    storage: &Storage,
    memory_id: &str,
    chunk_ids: &[String],
) -> Result<()> {
    if chunk_ids.is_empty() {
        return Ok(());
    }
    let (conn, _guard) = storage.write_conn().await?;
    for cid in chunk_ids {
        conn.execute(
            "INSERT OR IGNORE INTO memory_chunks (memory_id, chunk_id) VALUES (?, ?)",
            params![memory_id.to_string(), cid.clone()],
        )
        .await?;
    }
    Ok(())
}

/// Insert a typed link between two memories. Idempotent.
pub async fn add_memory_link(
    storage: &Storage,
    source_id: &str,
    target_id: &str,
    kind: &str,
) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "INSERT OR IGNORE INTO memory_links (source_id, target_id, kind) VALUES (?, ?, ?)",
        params![
            source_id.to_string(),
            target_id.to_string(),
            kind.to_string()
        ],
    )
    .await?;
    Ok(())
}

/// Recent valid semantic memories that have not yet been included in a
/// reflection pass, newest first.
pub async fn recent_unreflected(storage: &Storage, limit: usize) -> Result<Vec<Memory>> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT id, tier, kind, title, body,
                    tags_json, entities_json, links_json, provenance_json,
                    created_at, ingested_at, valid_at, invalid_at, superseded_by,
                    strength, importance, last_accessed, access_count,
                    workspace, source_tool, mnemos_version
               FROM memories
              WHERE tier = 'semantic' AND invalid_at IS NULL AND reflected_at IS NULL
              ORDER BY created_at DESC
              LIMIT ?",
            params![limit as i64],
        )
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(row_to_memory(&row)?);
    }
    Ok(out)
}

/// Stamp `reflected_at` on the given memories.
pub async fn mark_reflected(storage: &Storage, ids: &[String], at: DateTime<Utc>) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let (conn, _g) = storage.write_conn().await?;
    let ts = at.to_rfc3339();
    for id in ids {
        conn.execute(
            "UPDATE memories SET reflected_at = ? WHERE id = ?",
            params![ts.clone(), id.clone()],
        )
        .await?;
    }
    Ok(())
}

/// List valid memories of a given kind, newest first.
pub async fn list_by_kind(
    storage: &Storage,
    kind: MemoryType,
    limit: usize,
) -> Result<Vec<Memory>> {
    let kind_str = serde_json::to_string(&kind)?.trim_matches('"').to_string();
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT id, tier, kind, title, body,
                    tags_json, entities_json, links_json, provenance_json,
                    created_at, ingested_at, valid_at, invalid_at, superseded_by,
                    strength, importance, last_accessed, access_count,
                    workspace, source_tool, mnemos_version
               FROM memories
              WHERE kind = ? AND invalid_at IS NULL
              ORDER BY created_at DESC
              LIMIT ?",
            params![kind_str, limit as i64],
        )
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(row_to_memory(&row)?);
    }
    Ok(out)
}

/// Time-travel recall: full-text match `query` restricted to memories that were
/// valid at `as_of` (`valid_at <= as_of < invalid_at`). Ordered by FTS rank.
///
/// The query is treated as a single FTS phrase (quotes stripped) so arbitrary
/// user input cannot produce an FTS syntax error.
pub async fn recall_as_of(
    storage: &Storage,
    query: &str,
    as_of: DateTime<Utc>,
    k: usize,
) -> Result<Vec<Memory>> {
    let phrase = format!("\"{}\"", query.replace('"', " "));
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT m.id, m.tier, m.kind, m.title, m.body,
                    m.tags_json, m.entities_json, m.links_json, m.provenance_json,
                    m.created_at, m.ingested_at, m.valid_at, m.invalid_at, m.superseded_by,
                    m.strength, m.importance, m.last_accessed, m.access_count,
                    m.workspace, m.source_tool, m.mnemos_version
               FROM memory_fts f
               JOIN memories m ON m.id = f.memory_id
              WHERE memory_fts MATCH ?1
                AND m.valid_at <= ?2
                AND (m.invalid_at IS NULL OR m.invalid_at > ?2)
              ORDER BY rank
              LIMIT ?3",
            params![phrase, as_of.to_rfc3339(), k as i64],
        )
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(row_to_memory(&row)?);
    }
    Ok(out)
}
