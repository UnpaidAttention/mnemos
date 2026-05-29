//! Vector-index helpers for `memory_vec` and `chunk_vec` (sqlite-vec `vec0` tables).
//!
//! # sqlite-vec KNN query syntax
//!
//! `vec0` uses a special `MATCH` / `k` syntax for KNN queries:
//!
//! ```sql
//! SELECT rowid, distance
//!   FROM <table>
//!  WHERE embedding MATCH <blob>
//!    AND k = <limit>
//!  ORDER BY distance
//! ```
//!
//! The `MATCH` operand must be either a JSON array (`'[0.1, 0.2, …]'`) **or**
//! a raw little-endian `FLOAT[N]` BLOB. We pass a BLOB directly to avoid the
//! JSON serialisation cost.  `k` must come after `MATCH` in the WHERE clause —
//! the two predicates are non-commutative in vec0's query planner.

use crate::error::Result;
use crate::storage::Storage;
use libsql::params;
use serde::Serialize;

/// One KNN hit from a `vec0` virtual table.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VecHit {
    pub memory_id: String,
    /// L2 distance (lower = more similar) returned by sqlite-vec.
    pub distance: f32,
}

/// Serialize a `&[f32]` as the byte representation sqlite-vec expects
/// (a BLOB of little-endian f32s).
pub(crate) fn f32s_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Read the declared dimension of the `memory_vec` virtual table from
/// `sqlite_master` (sqlite-vec stores the column as `FLOAT[N]`). Returns `None`
/// if the table does not exist or its dim cannot be parsed.
pub async fn memory_vec_declared_dim(storage: &Storage) -> Result<Option<usize>> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='memory_vec'",
            (),
        )
        .await?;
    let Some(row) = rows.next().await? else {
        return Ok(None);
    };
    let Ok(sql) = row.get::<String>(0) else {
        return Ok(None);
    };
    if let Some(open) = sql.find("FLOAT[") {
        let after = &sql[open + "FLOAT[".len()..];
        if let Some(close) = after.find(']') {
            if let Ok(n) = after[..close].trim().parse::<usize>() {
                return Ok(Some(n));
            }
        }
    }
    Ok(None)
}

/// Ensure the `memory_vec` and `chunk_vec` virtual tables are declared at the
/// given embedding dimension, recreating them only if the current declared dim
/// differs.
///
/// The static v2 migration creates both tables at a fixed `FLOAT[768]`
/// (nomic-embed-text's dim). A fresh vault whose embedder produces a different
/// dimension (e.g. the 384-dim bundled embedder) needs the tables rebuilt at
/// that dim before the first vector insert, otherwise sqlite-vec rejects the
/// insert with a dimension mismatch.
///
/// The dim-difference guard makes this idempotent and avoids destroying vectors
/// already inserted at the correct dim (e.g. by `rebuild_index_with_embedder`,
/// which populates `memory_vec` without seeding `vault_meta`). When the dim
/// already matches, this is a no-op.
pub async fn ensure_vec_tables_dim(storage: &Storage, dim: usize) -> Result<()> {
    if memory_vec_declared_dim(storage).await? == Some(dim) {
        return Ok(());
    }
    let (conn, _g) = storage.write_conn().await?;
    let tx = conn.transaction().await?;
    tx.execute("DROP TABLE IF EXISTS memory_vec", ()).await?;
    tx.execute("DROP TABLE IF EXISTS chunk_vec", ()).await?;
    tx.execute(
        &format!(
            "CREATE VIRTUAL TABLE memory_vec USING vec0(
                memory_id TEXT PRIMARY KEY,
                embedding FLOAT[{dim}]
            )"
        ),
        (),
    )
    .await?;
    tx.execute(
        &format!(
            "CREATE VIRTUAL TABLE chunk_vec USING vec0(
                chunk_id TEXT PRIMARY KEY,
                embedding FLOAT[{dim}]
            )"
        ),
        (),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Insert (or replace) a memory embedding in `memory_vec`.
pub async fn insert_memory_vec(
    storage: &Storage,
    memory_id: &str,
    embedding: &[f32],
) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    let bytes = f32s_to_bytes(embedding);
    conn.execute(
        "INSERT OR REPLACE INTO memory_vec (memory_id, embedding) VALUES (?, ?)",
        params![memory_id.to_string(), bytes],
    )
    .await?;
    Ok(())
}

/// Remove a memory embedding from `memory_vec`.
pub async fn delete_memory_vec(storage: &Storage, memory_id: &str) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "DELETE FROM memory_vec WHERE memory_id = ?",
        params![memory_id.to_string()],
    )
    .await?;
    Ok(())
}

/// Insert (or replace) a chunk embedding in `chunk_vec`.
pub async fn insert_chunk_vec(storage: &Storage, chunk_id: &str, embedding: &[f32]) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    let bytes = f32s_to_bytes(embedding);
    conn.execute(
        "INSERT OR REPLACE INTO chunk_vec (chunk_id, embedding) VALUES (?, ?)",
        params![chunk_id.to_string(), bytes],
    )
    .await?;
    Ok(())
}

/// Remove a chunk embedding from `chunk_vec`.
pub async fn delete_chunk_vec(storage: &Storage, chunk_id: &str) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "DELETE FROM chunk_vec WHERE chunk_id = ?",
        params![chunk_id.to_string()],
    )
    .await?;
    Ok(())
}

/// K nearest neighbours from `memory_vec`.
///
/// Returns up to `k` [`VecHit`]s ordered ascending by L2 distance (nearest
/// first).  Returns an empty `Vec` when the table is empty.
pub async fn knn_memory(storage: &Storage, query: &[f32], k: usize) -> Result<Vec<VecHit>> {
    let conn = storage.conn()?;
    let bytes = f32s_to_bytes(query);
    // sqlite-vec requires: MATCH first, then k = ?  (order matters in vec0's query planner).
    let mut rows = conn
        .query(
            "SELECT memory_id, distance FROM memory_vec
             WHERE embedding MATCH ?
               AND k = ?
             ORDER BY distance",
            params![bytes, k as i64],
        )
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(VecHit {
            memory_id: row.get(0)?,
            distance: row.get::<f64>(1)? as f32,
        });
    }
    Ok(out)
}

/// K nearest neighbours from `chunk_vec`.
///
/// Returns up to `k` `(chunk_id, distance)` pairs ordered ascending by L2
/// distance (nearest first).
pub async fn knn_chunks(storage: &Storage, query: &[f32], k: usize) -> Result<Vec<(String, f32)>> {
    let conn = storage.conn()?;
    let bytes = f32s_to_bytes(query);
    // Same vec0 ordering constraint as knn_memory.
    let mut rows = conn
        .query(
            "SELECT chunk_id, distance FROM chunk_vec
             WHERE embedding MATCH ?
               AND k = ?
             ORDER BY distance",
            params![bytes, k as i64],
        )
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push((row.get(0)?, row.get::<f64>(1)? as f32));
    }
    Ok(out)
}
