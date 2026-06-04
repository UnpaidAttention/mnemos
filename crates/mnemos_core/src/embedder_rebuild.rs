//! Atomic, resumable embedder migration.
//!
//! Re-embeds every active memory in the vault with a target embedder, writing
//! new vectors to a shadow table (`memory_embeddings_v2`), then atomically
//! swapping the live `memory_vec` virtual table.
//!
//! # Resumability
//!
//! The shadow table persists across invocations. A `rebuild` call that crashes
//! mid-way leaves the shadow table populated up to the last committed row; the
//! next call skips those memories and resumes from the next un-embedded one.
//! The vault remains usable throughout — readers see the OLD vectors in
//! `memory_vec` until the swap.
//!
//! # Swap strategy
//!
//! `memory_vec` is a sqlite-vec `vec0` virtual table with a fixed `FLOAT[D]`
//! dimension declared at create time. ALTER TABLE RENAME does NOT work on
//! virtual tables. Two paths:
//!
//! 1. **Same dim**: DELETE FROM memory_vec (truncate) + bulk INSERT from shadow.
//! 2. **Different dim**: DROP memory_vec, CREATE VIRTUAL TABLE memory_vec with
//!    the new dim, bulk INSERT from shadow.
//!
//! Both paths run inside a single transaction; readers either see the old
//! state or the new state, never an intermediate.
//!
//! TODO(v0.9.0): Background cleanup of the shadow table after swap. v0.8.0
//! leaves it in place — rows are small (embedding metadata only after swap).

use crate::error::{MnemosError, Result};
use crate::providers::Embedder;
use crate::storage::audit::write_audit;
use crate::storage::vault_meta::{set_embedder_meta, EmbedderMeta};
use crate::storage::Storage;
use crate::vault::Vault;
use chrono::Utc;
use libsql::params;
use serde::{Deserialize, Serialize};

/// Options controlling a rebuild run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebuildOptions {
    /// Backend tag for the target embedder: `"bundled"`, `"ollama"`, `"openai"`,
    /// `"mock"`. `"none"` is rejected — rebuild requires a real backend.
    pub target_kind: String,
    /// Backend-specific model id (e.g. `"all-MiniLM-L6-v2"`).
    pub target_model: String,
    /// Target embedding dimension. If different from the current `memory_vec`
    /// dim, the table is recreated during the swap.
    pub target_dim: u32,
    /// Actor string written to the audit-log entry.
    pub actor: String,
}

/// Lifecycle status of a rebuild run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RebuildStatus {
    /// No rebuild has been started in this process.
    Idle,
    /// A rebuild is in progress.
    Running { processed: usize, total: usize },
    /// A rebuild completed successfully.
    Completed {
        processed: usize,
        skipped: usize,
        total: usize,
        swapped: bool,
    },
    /// A rebuild failed mid-run. Partial work is preserved in the shadow table.
    Failed { error: String, processed: usize },
}

/// Run an atomic, resumable rebuild against `vault`.
///
/// On success returns `RebuildStatus::Completed` with counts and a `swapped`
/// flag indicating the live `memory_vec` was replaced.
pub async fn rebuild(vault: &Vault, opts: RebuildOptions) -> Result<RebuildStatus> {
    let storage = vault.storage();

    // 1. Ensure the shadow table exists. Idempotent — resumes from prior runs.
    ensure_shadow_table(storage).await?;

    // 1a. Purge any rows that were produced by a DIFFERENT model/kind than the
    //     current target.  Without this, a repeated rebuild (A → B → A or A → B
    //     with a crash mid-B) would treat stale model-A vectors as valid and
    //     silently install wrong-model embeddings into the live index (P0-5).
    purge_stale_shadow_rows(storage, &opts).await?;

    // 2. List active memories in deterministic order.
    let memory_ids = list_active_memory_ids(storage).await?;
    let total = memory_ids.len();

    // 3. Build the target embedder.
    let target_embedder = build_target_embedder(&opts).await?;

    // 4. Embed any memory not yet in the shadow table.
    let mut processed = 0;
    let mut skipped = 0;
    for id in &memory_ids {
        if shadow_has(storage, id).await? {
            skipped += 1;
            continue;
        }

        let body = load_memory_body(storage, id).await?;
        let vector = target_embedder.embed(&body).await?;
        if vector.len() != opts.target_dim as usize {
            return Err(MnemosError::Internal(format!(
                "target embedder produced {} dims, expected {}",
                vector.len(),
                opts.target_dim
            )));
        }
        insert_shadow_row(storage, id, &vector, &opts).await?;
        processed += 1;
    }

    // 5. Atomic swap: replace memory_vec contents with shadow vectors.
    swap_memory_vec(storage, opts.target_dim).await?;

    // 6. Update vault_meta atomically.
    set_embedder_meta(
        storage,
        &EmbedderMeta {
            kind: opts.target_kind.clone(),
            model: opts.target_model.clone(),
            dim: opts.target_dim,
        },
    )
    .await?;

    // 7. Audit log.
    write_audit(
        storage,
        &opts.actor,
        "embedder_migrated",
        None,
        Some(serde_json::json!({
            "target_kind": opts.target_kind,
            "target_model": opts.target_model,
            "target_dim": opts.target_dim,
            "processed": processed,
            "skipped": skipped,
            "total": total,
        })),
    )
    .await?;

    Ok(RebuildStatus::Completed {
        processed,
        skipped,
        total,
        swapped: true,
    })
}

/// Create the shadow table if it does not exist.
async fn ensure_shadow_table(storage: &Storage) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS memory_embeddings_v2 (
            memory_id TEXT PRIMARY KEY,
            embedding BLOB NOT NULL,
            embedder_kind TEXT NOT NULL,
            embedder_model TEXT NOT NULL,
            embedder_dim INTEGER NOT NULL,
            created_at TEXT NOT NULL
        )",
        (),
    )
    .await?;
    Ok(())
}

/// Delete shadow rows whose embedder kind or model does not match the rebuild
/// target.  This prevents a repeated rebuild (e.g. model A → model B, then
/// B → A again, or a crash mid-B followed by a re-run for B) from reusing
/// vectors produced by the wrong model (P0-5).
async fn purge_stale_shadow_rows(storage: &Storage, opts: &RebuildOptions) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "DELETE FROM memory_embeddings_v2 \
         WHERE embedder_kind != ? OR embedder_model != ?",
        params![opts.target_kind.clone(), opts.target_model.clone()],
    )
    .await?;
    Ok(())
}

async fn list_active_memory_ids(storage: &Storage) -> Result<Vec<String>> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT id FROM memories WHERE invalid_at IS NULL ORDER BY id",
            (),
        )
        .await?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().await? {
        out.push(r.get::<String>(0)?);
    }
    Ok(out)
}

async fn shadow_has(storage: &Storage, memory_id: &str) -> Result<bool> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT 1 FROM memory_embeddings_v2 WHERE memory_id = ?",
            params![memory_id.to_string()],
        )
        .await?;
    Ok(rows.next().await?.is_some())
}

async fn load_memory_body(storage: &Storage, memory_id: &str) -> Result<String> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT body FROM memories WHERE id = ?",
            params![memory_id.to_string()],
        )
        .await?;
    let row = rows.next().await?.ok_or_else(|| {
        MnemosError::Internal(format!("memory {memory_id} disappeared mid-rebuild"))
    })?;
    Ok(row.get::<String>(0)?)
}

async fn insert_shadow_row(
    storage: &Storage,
    memory_id: &str,
    vector: &[f32],
    opts: &RebuildOptions,
) -> Result<()> {
    let bytes = f32_vec_to_bytes(vector);
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "INSERT INTO memory_embeddings_v2 \
         (memory_id, embedding, embedder_kind, embedder_model, embedder_dim, created_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
        params![
            memory_id.to_string(),
            bytes,
            opts.target_kind.clone(),
            opts.target_model.clone(),
            opts.target_dim as i64,
            Utc::now().to_rfc3339(),
        ],
    )
    .await?;
    Ok(())
}

/// Atomically replace `memory_vec` contents with vectors from the shadow table.
///
/// `memory_vec` is a sqlite-vec virtual table with fixed dimension. If the
/// current dim already matches `target_dim` we just truncate and refill;
/// otherwise we DROP and CREATE the virtual table with the new dim.
async fn swap_memory_vec(storage: &Storage, target_dim: u32) -> Result<()> {
    let current_dim = current_memory_vec_dim(storage).await?;

    // Read the shadow rows up-front (read-only).
    let shadow_rows = read_shadow_rows(storage).await?;

    let (conn, _g) = storage.write_conn().await?;
    let tx = conn.transaction().await?;

    if Some(target_dim) == current_dim {
        // Same dim — truncate and refill.
        tx.execute("DELETE FROM memory_vec", ()).await?;
    } else {
        // Dimension change — recreate the virtual table.
        tx.execute("DROP TABLE IF EXISTS memory_vec", ()).await?;
        let create = format!(
            "CREATE VIRTUAL TABLE memory_vec USING vec0(
                memory_id TEXT PRIMARY KEY,
                embedding FLOAT[{}]
            )",
            target_dim
        );
        tx.execute(&create, ()).await?;
    }

    for (id, bytes) in &shadow_rows {
        tx.execute(
            "INSERT INTO memory_vec (memory_id, embedding) VALUES (?, ?)",
            params![id.clone(), bytes.clone()],
        )
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Read every (memory_id, embedding) pair from the shadow table.
async fn read_shadow_rows(storage: &Storage) -> Result<Vec<(String, Vec<u8>)>> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query("SELECT memory_id, embedding FROM memory_embeddings_v2", ())
        .await?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().await? {
        let id: String = r.get(0)?;
        let bytes: Vec<u8> = r.get(1)?;
        out.push((id, bytes));
    }
    Ok(out)
}

/// Try to infer the current `memory_vec` declared dimension. Returns `None` if
/// the table does not exist or its schema cannot be parsed.
///
/// sqlite-vec exposes the column as `FLOAT[N]` in `sqlite_master.sql`.
async fn current_memory_vec_dim(storage: &Storage) -> Result<Option<u32>> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'memory_vec'",
            (),
        )
        .await?;
    let Some(row) = rows.next().await? else {
        return Ok(None);
    };
    let sql: Option<String> = row.get(0).ok();
    let Some(sql) = sql else {
        return Ok(None);
    };
    // Look for the substring "FLOAT[<N>]".
    if let Some(open) = sql.find("FLOAT[") {
        let after = &sql[open + "FLOAT[".len()..];
        if let Some(close) = after.find(']') {
            if let Ok(n) = after[..close].trim().parse::<u32>() {
                return Ok(Some(n));
            }
        }
    }
    Ok(None)
}

/// Construct the target embedder for the rebuild.
///
/// Duplicates the daemon's factory because `mnemos_core` can't depend on
/// `mnemos_daemon`. Reads env (`OLLAMA_URL`, `MNEMOS_OLLAMA_URL`, OpenAI vars)
/// the same way the daemon's factory does.
async fn build_target_embedder(opts: &RebuildOptions) -> Result<Box<dyn Embedder>> {
    use crate::providers::{
        bundled::BundledEmbedder,
        mock::MockEmbedder,
        ollama::{OllamaConfig, OllamaEmbedder},
        openai_embedder::{self, OpenAiEmbedder},
    };
    match opts.target_kind.as_str() {
        "bundled" => {
            // Default port is 7424 per Plan 9. Operators can override via env.
            let url = std::env::var("MNEMOS_BUNDLED_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:7424".into());
            Ok(Box::new(BundledEmbedder::new(url)))
        }
        "ollama" => {
            let base_url = std::env::var("MNEMOS_OLLAMA_URL")
                .or_else(|_| std::env::var("OLLAMA_URL"))
                .unwrap_or_else(|_| "http://localhost:11434".into());
            let cfg = OllamaConfig {
                base_url,
                model: opts.target_model.clone(),
                dim: opts.target_dim as usize,
                timeout_secs: 30,
            };
            Ok(Box::new(OllamaEmbedder::new(cfg)))
        }
        "openai" => {
            let mut cfg = openai_embedder::config_from_env()?;
            cfg.model = opts.target_model.clone();
            cfg.dim = opts.target_dim;
            let e = OpenAiEmbedder::new(&cfg)?;
            Ok(Box::new(e))
        }
        "mock" => Ok(Box::new(MockEmbedder::new(opts.target_dim as usize))),
        "none" => Err(MnemosError::Internal(
            "cannot rebuild into 'none' — disables semantic recall".into(),
        )),
        other => Err(MnemosError::Internal(format!(
            "unknown target embedder: {other}"
        ))),
    }
}

fn f32_vec_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for &x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}
