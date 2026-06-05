//! Typed accessors for the embedder-related `vault_meta` columns.
//!
//! `EmbedderMeta` groups the three fields the daemon needs to decide whether a
//! configured embedder is compatible with the vault: backend kind, model id,
//! and embedding dimension. The atomic `set_embedder_meta` writer guarantees
//! all three move together — useful on first remember and after a deliberate
//! re-embed.
//!
//! The pre-existing single-field accessors on `Storage` (`get_vault_meta` /
//! `set_vault_meta`) remain for backwards compatibility with code paths that
//! only touch `embedder_dim` + `embedder_model_id`.

use crate::error::{MnemosError, Result};
use crate::storage::Storage;
use libsql::params;
use serde::{Deserialize, Serialize};

/// Identity of the embedder that seeded this vault's vectors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbedderMeta {
    /// Backend tag: `"bundled"`, `"ollama"`, `"openai"`, `"mock"`, …
    pub kind: String,
    /// Backend-specific model identifier (e.g. `"nomic-embed-text"`).
    pub model: String,
    /// Embedding dimension. Must match the configured embedder at startup.
    pub dim: u32,
}

/// Read the embedder identity for this vault. Returns the row's current
/// values; on a fresh vault `kind` is the migration default (`"bundled"`)
/// and `model` / `dim` are empty until first remember.
pub async fn get_embedder_meta(storage: &Storage) -> Result<EmbedderMeta> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT COALESCE(embedder_kind, ''), \
                    COALESCE(embedder_model_id, ''), \
                    COALESCE(embedder_dim, 0) \
               FROM vault_meta WHERE id = 1",
            (),
        )
        .await?;
    let r = rows
        .next()
        .await?
        .ok_or_else(|| MnemosError::Internal("vault_meta row missing".into()))?;
    let kind: String = r.get(0)?;
    let model: String = r.get(1)?;
    let dim: i64 = r.get(2)?;
    Ok(EmbedderMeta {
        kind,
        model,
        dim: dim as u32,
    })
}

/// Atomically set all three embedder fields. Used by migration backfill,
/// first remember, and any deliberate re-embed. `updated_at` is bumped so
/// downstream consumers can see the change.
///
/// # P2-16
///
/// Returns `MnemosError::Internal` if no `vault_meta` row with `id = 1`
/// exists (which indicates the database was not properly initialized via
/// `Storage::open`). Silent success on a zero-affected-row UPDATE would
/// mask metadata corruption that later triggers a silent vector-table drop.
pub async fn set_embedder_meta(storage: &Storage, meta: &EmbedderMeta) -> Result<()> {
    // P2-16: check whether the row exists BEFORE acquiring the write lock to
    // avoid holding the lock while doing a read.  The row is created by
    // Storage::open (migration v1) and is permanent, so a missing row is a
    // hard invariant violation, not a race condition.
    {
        let conn = storage.conn()?;
        let mut rows = conn
            .query("SELECT COUNT(*) FROM vault_meta WHERE id = 1", ())
            .await?;
        let count: i64 = rows
            .next()
            .await?
            .ok_or_else(|| MnemosError::Internal("vault_meta COUNT query returned no rows".into()))?
            .get(0)?;
        if count == 0 {
            return Err(MnemosError::Internal(
                "vault_meta row missing; run Storage::open to initialize".into(),
            ));
        }
    }

    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "UPDATE vault_meta \
            SET embedder_kind = ?, \
                embedder_model_id = ?, \
                embedder_dim = ?, \
                updated_at = ? \
            WHERE id = 1",
        params![
            meta.kind.clone(),
            meta.model.clone(),
            meta.dim as i64,
            chrono::Utc::now().to_rfc3339(),
        ],
    )
    .await?;
    Ok(())
}
