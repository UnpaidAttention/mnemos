use crate::error::Result;
use crate::retrieval::{RecallHit, RecallOpts};
use crate::storage::memory_ops::row_to_memory;
use crate::storage::Storage;
use libsql::Value;

/// FTS5 BM25 recall. Returns up to `opts.k` hits sorted by bm25 score (best first).
pub async fn bm25_recall(
    storage: &Storage,
    query: &str,
    opts: RecallOpts,
) -> Result<Vec<RecallHit>> {
    let conn = storage.conn()?;

    let fts_query = escape_fts5_query(query);
    let mut sql = String::from(
        "SELECT m.id, m.tier, m.kind, m.title, m.body,
                m.tags_json, m.entities_json, m.links_json, m.provenance_json,
                m.created_at, m.ingested_at, m.valid_at, m.invalid_at, m.superseded_by,
                m.strength, m.importance, m.last_accessed, m.access_count,
                m.workspace, m.source_tool, m.mnemos_version,
                bm25(memory_fts) AS s
           FROM memory_fts
           JOIN memories m ON m.id = memory_fts.memory_id
          WHERE memory_fts MATCH ?",
    );
    let mut args: Vec<Value> = vec![fts_query.into()];

    if !opts.include_invalid {
        sql.push_str(" AND m.invalid_at IS NULL");
    }
    if let Some(ws) = opts.workspace.as_ref() {
        // Workspace filter returns both workspace-tagged AND unscoped (global) memories,
        // per design spec: "workspace='~/code/foo' → returns: workspace-tagged + global".
        // Global memories (e.g. identity facts) surface in every workspace.
        sql.push_str(" AND (m.workspace IS NULL OR m.workspace = ?)");
        args.push(ws.clone().into());
    }
    if let Some(tiers) = opts.tiers.as_ref() {
        if !tiers.is_empty() {
            let placeholders = vec!["?"; tiers.len()].join(",");
            sql.push_str(&format!(" AND m.tier IN ({placeholders})"));
            for t in tiers {
                args.push(t.as_str().to_string().into());
            }
        }
    }
    // BM25 returns lower-is-better; sort ascending then invert.
    sql.push_str(" ORDER BY s ASC LIMIT ?");
    args.push((opts.k as i64).into());

    let mut rows = conn.query(&sql, args).await?;
    let mut hits = Vec::new();
    let mut rank = 0usize;
    while let Some(row) = rows.next().await? {
        rank += 1;
        let memory = row_to_memory(&row)?;
        let raw: f64 = row.get(21)?;
        hits.push(RecallHit {
            memory,
            score: -raw, // higher = better
            bm25_rank: Some(rank),
        });
    }
    Ok(hits)
}

/// Escape FTS5 special characters in a free-form user query. Quotes the
/// query as a phrase if it contains punctuation that FTS5 would reject.
fn escape_fts5_query(q: &str) -> String {
    let trimmed = q.trim();
    if trimmed.is_empty() {
        return "\"\"".into();
    }
    // FTS5 allows alphanumeric tokens, AND/OR/NOT, parens, quotes.
    // For simplicity in Plan 1: wrap the whole query in quotes (phrase mode)
    // and escape inner double-quotes by doubling.
    let escaped = trimmed.replace('"', "\"\"");
    format!("\"{escaped}\"")
}
