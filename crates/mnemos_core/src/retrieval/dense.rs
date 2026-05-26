use crate::error::Result;
use crate::providers::Embedder;
use crate::retrieval::{RecallHit, RecallOpts};
use crate::storage::memory_ops::row_to_memory;
use crate::storage::Storage;
use libsql::Value;

/// Dense KNN over `memory_vec`, joined with `memories` for tier/workspace
/// filtering and full hydration. Returns `RecallHit`s sorted by ascending
/// L2 distance (best first).
pub async fn dense_recall(
    storage: &Storage,
    embedder: &dyn Embedder,
    query: &str,
    opts: RecallOpts,
) -> Result<Vec<RecallHit>> {
    let q_vec = embedder.embed(query).await?;
    if q_vec.len() != embedder.dim() {
        return Err(crate::error::MnemosError::Internal(format!(
            "embedder dim mismatch: got {}, declared {}",
            q_vec.len(),
            embedder.dim()
        )));
    }
    let bytes = f32s_to_bytes(&q_vec);

    // Over-fetch from vec0 so post-join tier/workspace filtering still
    // produces ~k results.
    let fetch_k = (opts.k * 5).max(opts.k);

    let conn = storage.conn()?;
    let mut sql = String::from(
        "SELECT m.id, m.tier, m.kind, m.title, m.body,
                m.tags_json, m.entities_json, m.links_json, m.provenance_json,
                m.created_at, m.ingested_at, m.valid_at, m.invalid_at, m.superseded_by,
                m.strength, m.importance, m.last_accessed, m.access_count,
                m.workspace, m.source_tool, m.mnemos_version,
                v.distance
         FROM memory_vec v
         JOIN memories m ON m.id = v.memory_id
         WHERE v.embedding MATCH ? AND v.k = ?",
    );

    let mut args: Vec<Value> = vec![bytes.into(), (fetch_k as i64).into()];

    if !opts.include_invalid {
        sql.push_str(" AND m.invalid_at IS NULL");
    }
    // See workspace filter rationale comment in memory_ops.rs::list_memories.
    if let Some(ws) = opts.workspace.as_ref() {
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
    sql.push_str(" ORDER BY v.distance ASC");

    let mut rows = conn.query(&sql, args).await?;
    let mut hits = Vec::new();
    let mut rank = 0usize;
    while let Some(row) = rows.next().await? {
        if hits.len() >= opts.k {
            break;
        }
        rank += 1;
        let memory = row_to_memory(&row)?;
        let distance: f64 = row.get(21)?;
        hits.push(RecallHit {
            memory,
            // Convert distance → similarity for "higher=better" consistency
            // with BM25's `score` field. Mapping: similarity = 1 / (1 + distance).
            score: 1.0 / (1.0 + distance),
            bm25_rank: None,
            dense_rank: Some(rank),
            dense_distance: Some(distance as f32),
            explain: None,
        });
    }
    Ok(hits)
}

fn f32s_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}
