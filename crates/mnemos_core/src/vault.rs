use crate::error::{MnemosError, Result};
use crate::file_io::{content_hash, read_memory_file, write_memory_file};
use crate::frontmatter::parse_frontmatter;
use crate::id::new_memory_id;
use crate::paths::Paths;
use crate::pipeline::decay::{decay_pass, DecayConfig, DecayStats};
use crate::providers::Embedder;
use crate::storage::audit::write_audit;
use crate::storage::memory_ops::{
    get_memory, insert_memory, list_memories, soft_invalidate, ListFilter,
};
use crate::storage::vec_ops::{delete_memory_vec, insert_memory_vec};
use crate::storage::Storage;
use crate::tier::Tier;
use crate::types::{Memory, MemoryType, Provenance};
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;

/// High-level vault: owns `Paths`, `Storage`, and an optional `Embedder`.
///
/// All higher-level code (CLI, daemon, MCP server) should work through
/// `Vault` rather than coordinating the two components directly.
#[derive(Clone)]
pub struct Vault {
    paths: Paths,
    storage: Storage,
    embedder: Option<Arc<dyn Embedder>>,
}

/// Statistics returned by [`Vault::backfill_embeddings`].
#[derive(Debug, Default, serde::Serialize)]
pub struct BackfillStats {
    /// Number of memories that were newly embedded during this run.
    pub embedded: usize,
    /// Number of active memories that already had an embedding and were left
    /// untouched.
    pub skipped: usize,
    /// Number of memories whose embedding failed (embedder error or storage
    /// error).  These are left without a vector and can be retried.
    pub errors: usize,
}

/// Options for [`Vault::remember`].
#[derive(Debug, Clone, Default)]
pub struct RememberOpts {
    pub title: Option<String>,
    pub tier: Tier,
    pub kind: MemoryType,
    pub tags: Vec<String>,
    pub importance: Option<f64>,
    pub workspace: Option<String>,
    pub source_tool: Option<String>,
    /// Provenance links (session + chunk ids) for memories derived by the
    /// async pipeline. Empty for manually-created memories.
    pub provenance: Vec<Provenance>,
}

impl Vault {
    /// Open a vault without an embedder (embedding is skipped on `remember`).
    pub async fn open(paths: Paths) -> Result<Self> {
        Self::open_with_embedder(paths, None).await
    }

    /// Open a vault with an optional embedder.
    ///
    /// When `embedder` is `Some`, every call to [`remember`][Vault::remember]
    /// will generate and store a vector embedding.  When `None`, the
    /// `memory_vec` table is left untouched.
    ///
    /// If an embedder is provided and this vault has previously been used with
    /// a different dimension, an error is returned to prevent silently mixing
    /// incompatible vectors.  A model-id change at the same dimension produces
    /// a warning and updates the stored model id.
    pub async fn open_with_embedder(
        paths: Paths,
        embedder: Option<Arc<dyn Embedder>>,
    ) -> Result<Self> {
        paths.ensure_dirs()?;
        let storage = Storage::open(&paths.db_path).await?;

        if let Some(e) = embedder.as_ref() {
            let meta = storage.get_vault_meta().await?;
            match (meta.embedder_dim, meta.embedder_model_id.as_deref()) {
                (None, _) | (_, None) => {
                    // First time an embedder is configured — record it.
                    storage.set_vault_meta(e.dim(), e.model_id()).await?;
                }
                (Some(stored_dim), Some(stored_model)) => {
                    if stored_dim != e.dim() {
                        return Err(MnemosError::Validation(format!(
                            "embedder dim mismatch: vault stored {stored_dim}d, \
                             embedder produces {}d (model {} → {})",
                            e.dim(),
                            stored_model,
                            e.model_id()
                        )));
                    }
                    if stored_model != e.model_id() {
                        tracing::warn!(
                            "vault model_id changed: {} → {} (dim {} unchanged)",
                            stored_model,
                            e.model_id(),
                            stored_dim
                        );
                        storage.set_vault_meta(e.dim(), e.model_id()).await?;
                    }
                }
            }
        }

        Ok(Self {
            paths,
            storage,
            embedder,
        })
    }

    /// Borrow the underlying storage handle (e.g. for audit queries in tests).
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// Borrow the resolved path set.
    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    /// Borrow the embedder, if one was supplied at open time.
    pub fn embedder(&self) -> Option<&Arc<dyn Embedder>> {
        self.embedder.as_ref()
    }

    /// Write a new memory to disk and the DB, then emit a `create` audit entry.
    ///
    /// Returns the new memory's ID (e.g. `"mem_01J…"`).
    pub async fn remember(&self, body: &str, opts: RememberOpts) -> Result<String> {
        let id = new_memory_id();
        let now = Utc::now();
        let title = opts.title.unwrap_or_else(|| auto_title(body));
        let mem = Memory {
            id: id.clone(),
            tier: opts.tier,
            kind: opts.kind,
            title,
            body: body.to_string(),
            tags: opts.tags,
            entities: vec![],
            links: vec![],
            provenance: opts.provenance,
            created_at: now,
            ingested_at: now,
            valid_at: now,
            invalid_at: None,
            superseded_by: None,
            strength: 1.0,
            importance: opts.importance.unwrap_or(0.5),
            last_accessed: now,
            access_count: 0,
            workspace: opts.workspace,
            source_tool: opts.source_tool,
            mnemos_version: 1,
        };
        let file_path = write_memory_file(&self.paths, &mem).await?;
        let hash = content_hash(body);
        insert_memory(
            &self.storage,
            &mem,
            file_path.to_string_lossy().as_ref(),
            &hash,
        )
        .await?;
        write_audit(
            &self.storage,
            opts_actor(),
            "create",
            Some(&id),
            Some(json!({"tier": mem.tier.as_str(), "title": mem.title})),
        )
        .await?;
        if let Some(emb) = &self.embedder {
            // Embed the body (not the title; titles are sometimes auto-generated and noisy).
            let vector = emb.embed(body).await?;
            if vector.len() != emb.dim() {
                return Err(MnemosError::Internal(format!(
                    "embedder returned {} dims, expected {}",
                    vector.len(),
                    emb.dim()
                )));
            }
            insert_memory_vec(&self.storage, &id, &vector).await?;
        }
        Ok(id)
    }

    /// Retrieve a memory by ID (includes soft-invalidated ones).
    pub async fn get(&self, id: &str) -> Result<Memory> {
        get_memory(&self.storage, id).await
    }

    /// Soft-invalidate a memory and write a `forget` audit entry.
    ///
    /// After invalidating the DB row the markdown file is rewritten so that
    /// `invalid_at` is present in the frontmatter.  This preserves the
    /// "files are source of truth" invariant: if the DB is wiped and rebuilt
    /// from disk the memory will remain invalidated rather than being
    /// resurrected as fully valid.
    pub async fn forget(&self, id: &str, reason: Option<&str>) -> Result<()> {
        let now = Utc::now();
        soft_invalidate(&self.storage, id, now).await?;

        // Fetch the updated row and rewrite the file with the new invalid_at.
        let mut mem = get_memory(&self.storage, id).await?;
        mem.invalid_at = Some(now); // ensure consistency with the DB value
        let new_path = write_memory_file(&self.paths, &mem).await?;

        // Update the DB row's file_path (in case it changed) and content_hash
        // (the frontmatter changed even though the body did not).
        let new_hash = content_hash(&mem.body);
        {
            let (conn, _g) = self.storage.write_conn().await?;
            conn.execute(
                "UPDATE memories SET file_path = ?, content_hash = ? WHERE id = ?",
                libsql::params![
                    new_path.to_string_lossy().to_string(),
                    new_hash,
                    id.to_string()
                ],
            )
            .await?;
        }

        // Remove the vector from the KNN index.  We delete unconditionally: if
        // no embedding was ever stored the DELETE is a silent no-op.  Chunks are
        // intentionally left alone — they belong to a separate conceptual layer.
        delete_memory_vec(&self.storage, id).await?;

        write_audit(
            &self.storage,
            opts_actor(),
            "forget",
            Some(id),
            Some(json!({"reason": reason})),
        )
        .await?;
        Ok(())
    }

    /// List memories matching the given filter.
    pub async fn list(&self, filter: ListFilter) -> Result<Vec<Memory>> {
        list_memories(&self.storage, filter).await
    }

    /// Patch mutable metadata (tags and/or importance) on a memory.
    ///
    /// Updates the DB row, rewrites the markdown file so disk remains the
    /// source of truth, writes an `update` audit entry, and returns the
    /// refreshed memory. `title` and `body` are not patchable here — those go
    /// through file edits + reindex.
    pub async fn patch(
        &self,
        id: &str,
        tags: Option<Vec<String>>,
        importance: Option<f64>,
    ) -> Result<Memory> {
        let mut mem = get_memory(&self.storage, id).await?;
        if let Some(t) = tags {
            mem.tags = t;
        }
        if let Some(i) = importance {
            mem.importance = i;
        }
        let new_path = write_memory_file(&self.paths, &mem).await?;
        let new_hash = content_hash(&mem.body);
        {
            let (conn, _g) = self.storage.write_conn().await?;
            conn.execute(
                "UPDATE memories SET tags_json = ?, importance = ?, file_path = ?, content_hash = ? WHERE id = ?",
                libsql::params![
                    serde_json::to_string(&mem.tags)?,
                    mem.importance,
                    new_path.to_string_lossy().to_string(),
                    new_hash,
                    id.to_string()
                ],
            )
            .await?;
        }
        write_audit(
            &self.storage,
            opts_actor(),
            "update",
            Some(id),
            Some(json!({ "tags": mem.tags, "importance": mem.importance })),
        )
        .await?;
        get_memory(&self.storage, id).await
    }

    /// Run a decay pass and invalidate any memories that fell below the floor.
    /// Invalidation goes through `forget` so the change is persisted to disk.
    pub async fn run_decay(&self, cfg: &DecayConfig) -> Result<DecayStats> {
        let stats = decay_pass(&self.storage, Utc::now(), cfg).await?;
        for id in &stats.to_invalidate {
            if let Err(e) = self.forget(id, Some("decayed below strength floor")).await {
                tracing::warn!(memory_id = %id, error = %e, "decay invalidation failed");
            }
        }
        Ok(stats)
    }

    /// Read a memory file from disk, bypassing the DB cache.
    ///
    /// Useful when the user has externally edited a file and we want the
    /// on-disk truth rather than the indexed copy.
    pub async fn read_from_disk(&self, path: &std::path::Path) -> Result<(Memory, String)> {
        read_memory_file(path).await
    }

    /// Embed every active memory that does not yet have a vector in
    /// `memory_vec`.
    ///
    /// This is useful when a vault was initially used without an embedder and
    /// you later want to enable semantic search without re-inserting memories.
    /// Memories that already have an embedding are counted in
    /// [`BackfillStats::skipped`] and left untouched.
    ///
    /// `batch_size` controls how many memory bodies are sent to the embedder
    /// in a single call.  Implementations that support real batching (e.g.
    /// HTTP-based embedders) benefit from a larger value; the default
    /// implementation falls back to sequential single-embed calls.
    ///
    /// Returns an error immediately (before processing any memories) when no
    /// embedder is configured.
    pub async fn backfill_embeddings(&self, batch_size: usize) -> Result<BackfillStats> {
        let embedder = self.embedder.as_ref().ok_or_else(|| {
            MnemosError::Validation("backfill requires an embedder to be configured".into())
        })?;

        let mut stats = BackfillStats::default();

        // Find every active memory that has no entry in memory_vec yet.
        let conn = self.storage.conn()?;
        let mut rows = conn
            .query(
                "SELECT m.id, m.body
                   FROM memories m
                  WHERE m.id NOT IN (SELECT memory_id FROM memory_vec)
                    AND m.invalid_at IS NULL",
                (),
            )
            .await?;

        let mut todo: Vec<(String, String)> = Vec::new();
        while let Some(row) = rows.next().await? {
            todo.push((row.get(0)?, row.get(1)?));
        }
        drop(rows);

        // Count how many active memories already have embeddings so we can
        // populate `skipped` accurately.
        let total_active: usize = {
            let mut rs = conn
                .query("SELECT COUNT(*) FROM memories WHERE invalid_at IS NULL", ())
                .await?;
            let r = rs
                .next()
                .await?
                .ok_or_else(|| MnemosError::Internal("COUNT(*) returned no rows".into()))?;
            r.get::<i64>(0)? as usize
        };
        stats.skipped = total_active.saturating_sub(todo.len());

        // Embed and store in batches.
        for chunk in todo.chunks(batch_size.max(1)) {
            let bodies: Vec<String> = chunk.iter().map(|(_, b)| b.clone()).collect();
            match embedder.embed_batch(&bodies).await {
                Ok(vectors) => {
                    for ((id, _), vec) in chunk.iter().zip(vectors.iter()) {
                        match insert_memory_vec(&self.storage, id, vec).await {
                            Ok(()) => stats.embedded += 1,
                            Err(_) => stats.errors += 1,
                        }
                    }
                }
                Err(_) => stats.errors += chunk.len(),
            }
        }

        Ok(stats)
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Derive a short title from the first line of the body.
fn auto_title(body: &str) -> String {
    let line = body.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        "Untitled memory".into()
    } else if line.len() <= 80 {
        line.into()
    } else {
        format!("{}…", &line[..77])
    }
}

/// The actor string stamped on audit entries.
///
/// Plan 3 will propagate the actual MCP client identity here; for now we use
/// a stable sentinel so audit entries are always attributable.
fn opts_actor() -> &'static str {
    "mnemos-cli"
}

/// Parse a markdown file via the vault (convenience re-export).
pub fn parse_file(text: &str) -> Result<(Memory, String)> {
    parse_frontmatter(text)
}
