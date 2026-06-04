//! Chunk-level storage helpers.
//!
//! Provides CRUD operations on the `chunks` table and the associated
//! `chunk_vec` virtual table.  Used by the distil-and-prune retention
//! policy to remove raw chunks after the pipeline has finished processing
//! a session.

use crate::error::Result;
use crate::storage::vec_ops::delete_chunk_vec;
use crate::storage::Storage;
use libsql::params;

/// Delete all chunks that belong to `session_id`, including their vector
/// embeddings from `chunk_vec`.
///
/// Returns the number of chunks deleted.  If the session has no chunks, or
/// `session_id` is unknown, the function returns `Ok(0)`.
///
/// # Retention contract
///
/// This function is called by the distil-and-prune retention policy AFTER
/// the pipeline has extracted memories and mined corrections from the
/// chunks.  The session row itself, distilled memories, correction records,
/// and `memory_chunks` provenance links are left completely untouched.
pub async fn delete_session_chunks(storage: &Storage, session_id: &str) -> Result<usize> {
    // 1. Collect the chunk ids for this session.
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT id FROM chunks WHERE session_id = ?1",
            params![session_id.to_string()],
        )
        .await?;
    let mut chunk_ids: Vec<String> = Vec::new();
    while let Some(row) = rows.next().await? {
        chunk_ids.push(row.get(0)?);
    }
    drop(rows);

    if chunk_ids.is_empty() {
        return Ok(0);
    }

    // 2. Delete each chunk's vector embedding from the vec0 table.
    //    These are best-effort: a missing vec row (e.g. chunk was never
    //    embedded) is not an error — delete_chunk_vec is idempotent.
    for cid in &chunk_ids {
        delete_chunk_vec(storage, cid).await?;
    }

    // 3. Bulk-delete the chunk rows.
    let (conn, _guard) = storage.write_conn().await?;
    conn.execute(
        "DELETE FROM chunks WHERE session_id = ?1",
        params![session_id.to_string()],
    )
    .await?;

    Ok(chunk_ids.len())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use crate::vault::Vault;
    use chrono::Utc;
    use libsql::params as lp;
    use tempfile::TempDir;

    /// Insert a session row and return its id.
    async fn insert_session(storage: &Storage, session_id: &str) -> Result<()> {
        let (conn, _g) = storage.write_conn().await?;
        conn.execute(
            "INSERT INTO sessions (id, source_tool, started_at) VALUES (?1, ?2, ?3)",
            lp![
                session_id.to_string(),
                "test".to_string(),
                Utc::now().to_rfc3339()
            ],
        )
        .await?;
        Ok(())
    }

    /// Insert a bare chunk row (no vector) for the given session.
    async fn insert_chunk(storage: &Storage, chunk_id: &str, session_id: &str) -> Result<()> {
        let (conn, _g) = storage.write_conn().await?;
        conn.execute(
            "INSERT INTO chunks (id, session_id, speaker, ordinal, body, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            lp![
                chunk_id.to_string(),
                session_id.to_string(),
                "user".to_string(),
                0_i64,
                "test body".to_string(),
                Utc::now().to_rfc3339()
            ],
        )
        .await?;
        Ok(())
    }

    /// Count rows in `chunks` for a given session.
    async fn count_chunks(storage: &Storage, session_id: &str) -> i64 {
        let conn = storage.conn().unwrap();
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM chunks WHERE session_id = ?1",
                lp![session_id.to_string()],
            )
            .await
            .unwrap();
        rows.next().await.unwrap().unwrap().get(0).unwrap()
    }

    /// Count rows in `chunk_vec` for a given chunk id.
    async fn count_chunk_vec(storage: &Storage, chunk_id: &str) -> i64 {
        let conn = storage.conn().unwrap();
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM chunk_vec WHERE chunk_id = ?1",
                lp![chunk_id.to_string()],
            )
            .await
            .unwrap();
        rows.next().await.unwrap().unwrap().get(0).unwrap()
    }

    // ── delete_session_chunks: happy path ─────────────────────────────────────

    /// Inserting 2 chunks then calling delete_session_chunks returns 2,
    /// leaves 0 chunks for that session, and does not touch any other session.
    #[tokio::test]
    async fn deletes_two_chunks_returns_count_two() {
        let tmp = TempDir::new().unwrap();
        let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
        let storage = v.storage();

        insert_session(storage, "sess-a").await.unwrap();
        insert_chunk(storage, "chunk-1", "sess-a").await.unwrap();
        insert_chunk(storage, "chunk-2", "sess-a").await.unwrap();

        let deleted = delete_session_chunks(storage, "sess-a").await.unwrap();
        assert_eq!(deleted, 2, "should report 2 chunks deleted");
        assert_eq!(
            count_chunks(storage, "sess-a").await,
            0,
            "no chunks should remain for sess-a"
        );
    }

    /// Chunks from a different session are not touched.
    #[tokio::test]
    async fn does_not_touch_other_session_chunks() {
        let tmp = TempDir::new().unwrap();
        let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
        let storage = v.storage();

        insert_session(storage, "sess-target").await.unwrap();
        insert_session(storage, "sess-other").await.unwrap();
        insert_chunk(storage, "chunk-t", "sess-target")
            .await
            .unwrap();
        insert_chunk(storage, "chunk-o", "sess-other")
            .await
            .unwrap();

        let deleted = delete_session_chunks(storage, "sess-target").await.unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(count_chunks(storage, "sess-target").await, 0);
        assert_eq!(
            count_chunks(storage, "sess-other").await,
            1,
            "sess-other chunk must be untouched"
        );
    }

    /// Session with no chunks returns Ok(0).
    #[tokio::test]
    async fn returns_zero_when_no_chunks_for_session() {
        let tmp = TempDir::new().unwrap();
        let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
        let storage = v.storage();

        insert_session(storage, "sess-empty").await.unwrap();

        let deleted = delete_session_chunks(storage, "sess-empty").await.unwrap();
        assert_eq!(deleted, 0);
    }

    /// Unknown session id returns Ok(0) without error.
    #[tokio::test]
    async fn returns_zero_for_unknown_session() {
        let tmp = TempDir::new().unwrap();
        let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
        let storage = v.storage();

        let deleted = delete_session_chunks(storage, "session-does-not-exist")
            .await
            .unwrap();
        assert_eq!(deleted, 0);
    }

    /// Chunk vectors in chunk_vec are deleted alongside the chunk rows.
    #[tokio::test]
    async fn vectors_are_deleted_with_chunks() {
        let tmp = TempDir::new().unwrap();
        let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
        let storage = v.storage();

        insert_session(storage, "sess-vec").await.unwrap();
        insert_chunk(storage, "chunk-v1", "sess-vec").await.unwrap();

        // Insert a vector for this chunk (768-dim — matches the v2 migration schema).
        let embedding: Vec<f32> = vec![0.1_f32; 768];
        crate::storage::vec_ops::insert_chunk_vec(storage, "chunk-v1", &embedding)
            .await
            .unwrap();
        assert_eq!(count_chunk_vec(storage, "chunk-v1").await, 1);

        delete_session_chunks(storage, "sess-vec").await.unwrap();

        assert_eq!(
            count_chunk_vec(storage, "chunk-v1").await,
            0,
            "chunk vector should be removed"
        );
    }

    /// A chunk without a vector (never embedded) can still be deleted cleanly.
    #[tokio::test]
    async fn chunk_without_vector_deleted_cleanly() {
        let tmp = TempDir::new().unwrap();
        let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
        let storage = v.storage();

        insert_session(storage, "sess-novec").await.unwrap();
        insert_chunk(storage, "chunk-nv", "sess-novec")
            .await
            .unwrap();
        // No vector inserted.

        let deleted = delete_session_chunks(storage, "sess-novec").await.unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(count_chunks(storage, "sess-novec").await, 0);
    }

    /// The session row itself is untouched after deleting its chunks.
    #[tokio::test]
    async fn session_row_untouched_after_chunk_deletion() {
        let tmp = TempDir::new().unwrap();
        let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
        let storage = v.storage();

        insert_session(storage, "sess-keep").await.unwrap();
        insert_chunk(storage, "chunk-k", "sess-keep").await.unwrap();

        delete_session_chunks(storage, "sess-keep").await.unwrap();

        let conn = storage.conn().unwrap();
        let mut rows = conn
            .query("SELECT COUNT(*) FROM sessions WHERE id = 'sess-keep'", ())
            .await
            .unwrap();
        let n: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(n, 1, "session row must survive chunk deletion");
    }
}
