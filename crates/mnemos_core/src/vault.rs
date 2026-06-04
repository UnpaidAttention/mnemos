use crate::error::{MnemosError, Result};
use crate::file_io::{content_hash, read_memory_file, write_memory_file};
use crate::frontmatter::parse_frontmatter;
use crate::id::new_memory_id;
use crate::paths::Paths;
use crate::pipeline::decay::{decay_pass, DecayConfig, DecayStats};
use crate::providers::Embedder;
use crate::storage::audit::write_audit;
use crate::storage::memory_ops::{
    add_memory_link, get_memory, insert_memory, list_memories, soft_invalidate, supersede_memory,
    ListFilter,
};
use crate::storage::vault_meta::{get_embedder_meta, set_embedder_meta, EmbedderMeta};
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
    /// **Vault meta is authoritative.** When this vault has previously been
    /// seeded with an embedder, three fields are checked against the configured
    /// embedder:
    ///   - `embedder_dim` — hard mismatch fails to prevent mixing incompatible vectors
    ///   - `embedder_kind` — hard mismatch fails to prevent silently swapping backends
    ///   - `embedder_model_id` — change at the same kind+dim is logged as a warning
    ///
    /// On a fresh vault (no previous embedder), all three fields are recorded
    /// atomically so they always move together.
    pub async fn open_with_embedder(
        paths: Paths,
        embedder: Option<Arc<dyn Embedder>>,
    ) -> Result<Self> {
        paths.ensure_dirs()?;
        let storage = Storage::open(&paths.db_path).await?;

        if let Some(e) = embedder.as_ref() {
            let meta = get_embedder_meta(&storage).await?;
            let configured = EmbedderMeta {
                kind: e.kind().to_string(),
                model: e.model_id().to_string(),
                dim: e.dim() as u32,
            };
            // A vault is considered "seeded" once both dim and model are set.
            // (The migration backfills `kind = "bundled"` for v8→v9 upgrades,
            // but leaves dim/model untouched — those land on first remember
            // or first open with an embedder.)
            let is_seeded = meta.dim != 0 && !meta.model.is_empty();
            if !is_seeded {
                // First time an embedder is configured. The static v2 migration
                // created `memory_vec`/`chunk_vec` at the legacy 768-dim default;
                // align them with this embedder's dim so the first remember
                // succeeds (the bundled embedder is 384-dim). The dim-difference
                // guard makes this a no-op when they already match, so it never
                // wipes vectors a same-dim index rebuild already inserted.
                crate::storage::vec_ops::ensure_vec_tables_dim(&storage, configured.dim as usize)
                    .await?;
                // Record all three fields atomically.
                set_embedder_meta(&storage, &configured).await?;
            } else {
                if meta.dim != configured.dim {
                    return Err(MnemosError::Validation(format!(
                        "embedder dim mismatch: vault stored {}d, \
                         embedder produces {}d (model {} → {})",
                        meta.dim, configured.dim, meta.model, configured.model,
                    )));
                }
                if !meta.kind.is_empty() && meta.kind != configured.kind {
                    return Err(MnemosError::Validation(format!(
                        "embedder kind mismatch: vault seeded with {:?}, \
                         configured embedder is {:?}. To switch backends safely, \
                         run an embed-rebuild.",
                        meta.kind, configured.kind,
                    )));
                }
                if meta.model != configured.model {
                    tracing::warn!(
                        "vault model_id changed: {} → {} (kind {} unchanged, dim {} unchanged)",
                        meta.model,
                        configured.model,
                        configured.kind,
                        configured.dim
                    );
                    set_embedder_meta(&storage, &configured).await?;
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

    /// Re-tier a memory: update DB row, rewrite/move the file to the new
    /// tier directory, write a `promote` audit entry, and return the refreshed
    /// memory. Idempotent when `new_tier` matches the current tier.
    pub async fn promote(&self, id: &str, new_tier: Tier) -> Result<Memory> {
        let mut mem = get_memory(&self.storage, id).await?;
        if mem.tier == new_tier {
            return Ok(mem);
        }
        // Capture the old on-disk location before we rewrite the row.
        let old_file_path: Option<String> = {
            let conn = self.storage.conn()?;
            let mut r = conn
                .query(
                    "SELECT file_path FROM memories WHERE id = ?",
                    libsql::params![id.to_string()],
                )
                .await?;
            r.next().await?.and_then(|row| row.get::<String>(0).ok())
        };
        mem.tier = new_tier;
        let new_path = write_memory_file(&self.paths, &mem).await?;
        if let Some(old) = old_file_path.as_deref() {
            if old != new_path.to_string_lossy() {
                let _ = tokio::fs::remove_file(old).await;
            }
        }
        let new_hash = content_hash(&mem.body);
        {
            let (conn, _g) = self.storage.write_conn().await?;
            conn.execute(
                "UPDATE memories SET tier = ?, file_path = ?, content_hash = ? WHERE id = ?",
                libsql::params![
                    mem.tier.as_str().to_string(),
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
            "promote",
            Some(id),
            Some(json!({ "tier": mem.tier.as_str() })),
        )
        .await?;
        get_memory(&self.storage, id).await
    }

    /// Write a reflection-tier memory and link it back to its source memories
    /// with `reflects_on` edges.
    pub async fn remember_reflection(
        &self,
        body: &str,
        title: Option<String>,
        kind: MemoryType,
        tags: Vec<String>,
        reflects_on: &[String],
        provenance: Vec<Provenance>,
    ) -> Result<String> {
        let id = self
            .remember(
                body,
                RememberOpts {
                    title,
                    tier: Tier::Reflection,
                    kind,
                    tags,
                    provenance,
                    source_tool: Some("mnemos-reflection".into()),
                    ..Default::default()
                },
            )
            .await?;
        for src in reflects_on {
            add_memory_link(&self.storage, &id, src, "reflects_on").await?;
        }
        Ok(id)
    }

    /// Store a [`Correction`][crate::correction::Correction] as a Procedural-tier memory.
    ///
    /// Steps:
    /// 1. Validate the correction (`why` must be substantive; `right` must not
    ///    weaponize a safeguard).
    /// 2. Search for a near-duplicate correction and, if found, reinforce its
    ///    access count rather than inserting a second entry.  When no embedder
    ///    is configured the dedup search is skipped (returns `Ok(None)`).
    /// 3. Create a new `Procedural / Correction` memory via [`remember`][Vault::remember].
    /// 4. If `supersedes` is given, mark that memory invalid and link it.
    ///
    /// Returns the ID of the memory that represents this correction (either the
    /// existing reinforced one or the newly created one).
    pub async fn remember_correction(
        &self,
        correction: crate::correction::Correction,
        supersedes: Option<String>,
    ) -> Result<String> {
        correction
            .validate()
            .map_err(|e| MnemosError::Validation(e.to_string()))?;

        if let Some(existing_id) = self.find_duplicate_correction(&correction).await? {
            self.reinforce_correction(&existing_id).await?;
            return Ok(existing_id);
        }

        let mut tags = correction.trigger_tags();
        tags.push("correction".to_string());
        let id = self
            .remember(
                &correction.to_body(),
                RememberOpts {
                    title: Some(truncate_title(&correction.right)),
                    tier: Tier::Procedural,
                    kind: MemoryType::Correction,
                    tags,
                    importance: Some(0.8),
                    workspace: None,
                    source_tool: None,
                    provenance: vec![],
                },
            )
            .await?;

        if let Some(old_id) = supersedes {
            // Best-effort: if the old memory is already invalid or gone, ignore the error.
            let _ = supersede_memory(&self.storage, &old_id, &id, Utc::now()).await;
        }

        Ok(id)
    }

    /// Search for an existing `Correction` memory whose semantic content is
    /// close enough to `c` that inserting a duplicate would be wasteful.
    ///
    /// Requires an embedder.  When none is configured this returns `Ok(None)`
    /// immediately so the caller proceeds to create a fresh entry.
    async fn find_duplicate_correction(
        &self,
        c: &crate::correction::Correction,
    ) -> Result<Option<String>> {
        // No embedder → no vector search → skip dedup.
        if self.embedder.is_none() {
            return Ok(None);
        }

        let query = format!("{} {}", c.trigger.as_deref().unwrap_or(""), c.right);

        // Dense recall via retrieval layer.  A recall error (e.g. no vectors
        // yet) is treated as "no duplicate found" rather than a hard failure.
        let opts = crate::retrieval::RecallOpts {
            k: 5,
            ..Default::default()
        };
        let hits = match crate::retrieval::dense::dense_recall(
            &self.storage,
            self.embedder.as_ref().unwrap().as_ref(),
            &query,
            opts,
        )
        .await
        {
            Ok(h) => h,
            Err(_) => return Ok(None),
        };

        const DUP_THRESHOLD: f64 = 0.9;
        for hit in hits {
            if hit.memory.kind == MemoryType::Correction && hit.score >= DUP_THRESHOLD {
                return Ok(Some(hit.memory.id));
            }
        }
        Ok(None)
    }

    /// Bump `access_count` and `last_accessed` on an existing correction memory
    /// to signal reinforcement (the correction has been seen again).
    async fn reinforce_correction(&self, id: &str) -> Result<()> {
        let now = Utc::now();
        let (conn, _guard) = self.storage.write_conn().await?;
        conn.execute(
            "UPDATE memories
                SET access_count = access_count + 1,
                    last_accessed = ?
              WHERE id = ?",
            libsql::params![now.to_rfc3339(), id.to_string()],
        )
        .await?;
        Ok(())
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
        // Cut at a char boundary at or before byte 77.  A raw byte slice
        // would panic mid-codepoint for non-ASCII input (P0-4).
        let cut = line
            .char_indices()
            .map(|(i, _)| i)
            .take_while(|&i| i <= 77)
            .last()
            .unwrap_or(0);
        format!("{}…", &line[..cut])
    }
}

/// Produce a short title from `s` (trimmed).  If longer than 72 chars, the
/// result is truncated to 71 chars followed by `…`.
fn truncate_title(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.len() <= 72 {
        trimmed.into()
    } else {
        // Cut at a char boundary at or before byte 71 (right may contain
        // non-ASCII text; a raw byte slice would panic mid-codepoint).
        let cut = trimmed
            .char_indices()
            .map(|(i, _)| i)
            .take_while(|&i| i <= 71)
            .last()
            .unwrap_or(0);
        format!("{}…", &trimmed[..cut])
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── auto_title ──────────────────────────────────────────────────────────

    #[test]
    fn auto_title_empty_body() {
        assert_eq!(auto_title(""), "Untitled memory");
    }

    #[test]
    fn auto_title_short_ascii() {
        assert_eq!(auto_title("hello world"), "hello world");
    }

    #[test]
    fn auto_title_exactly_80_ascii_chars() {
        let body = "a".repeat(80);
        assert_eq!(auto_title(&body), body);
    }

    /// Non-ASCII body whose first line exceeds 80 bytes must not panic and
    /// must produce a title that is valid UTF-8 ending on a char boundary.
    /// (Regression test for P0-4: raw `&line[..77]` panics on multibyte chars.)
    #[test]
    fn auto_title_non_ascii_long_first_line_does_not_panic() {
        // Each Japanese character is 3 UTF-8 bytes.
        // 30 chars × 3 bytes = 90 bytes — well over the 80-byte threshold.
        let body = "日本語のテキストサンプル日本語のテキストサンプル日本語のテキスト";
        let title = auto_title(body);
        // Must be valid UTF-8 (would panic on invalid slice otherwise).
        assert!(std::str::from_utf8(title.as_bytes()).is_ok());
        // Must end with the ellipsis marker.
        assert!(
            title.ends_with('…'),
            "expected title to end with '…', got: {title:?}"
        );
    }

    #[test]
    fn auto_title_emoji_long_first_line_does_not_panic() {
        // Each emoji is 4 UTF-8 bytes.  25 emoji = 100 bytes > 80 threshold.
        let body = "🦀".repeat(25);
        let title = auto_title(&body);
        assert!(std::str::from_utf8(title.as_bytes()).is_ok());
        assert!(title.ends_with('…'));
    }

    #[test]
    fn auto_title_mixed_ascii_and_non_ascii_long() {
        // Accented characters are 2 bytes each.  Build a string > 80 bytes
        // that has a multibyte char straddling byte 77.
        // 'é' = 0xC3 0xA9 (2 bytes).  Place one at byte position 76-77
        // so the old raw slice would cut inside it.
        //   76 × 'a' + 'é' + 'a'…  = 76 + 2 + remainder bytes
        let body = format!("{}é{}", "a".repeat(76), "a".repeat(10));
        assert!(body.len() > 80); // confirm precondition
        let title = auto_title(&body);
        assert!(std::str::from_utf8(title.as_bytes()).is_ok());
        assert!(title.ends_with('…'));
    }
}
