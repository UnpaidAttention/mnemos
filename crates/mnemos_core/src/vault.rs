use crate::error::{MnemosError, Result};
use crate::file_io::{content_hash, read_memory_file, write_memory_file};
use crate::frontmatter::parse_frontmatter;
use crate::id::new_memory_id;
use crate::paths::Paths;
use crate::providers::Embedder;
use crate::storage::audit::write_audit;
use crate::storage::memory_ops::{
    get_memory, insert_memory, list_memories, soft_invalidate, ListFilter,
};
use crate::storage::vec_ops::{delete_memory_vec, insert_memory_vec};
use crate::storage::Storage;
use crate::tier::Tier;
use crate::types::{Memory, MemoryType};
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
    pub async fn open_with_embedder(
        paths: Paths,
        embedder: Option<Arc<dyn Embedder>>,
    ) -> Result<Self> {
        paths.ensure_dirs()?;
        let storage = Storage::open(&paths.db_path).await?;
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
            provenance: vec![],
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

    /// Read a memory file from disk, bypassing the DB cache.
    ///
    /// Useful when the user has externally edited a file and we want the
    /// on-disk truth rather than the indexed copy.
    pub async fn read_from_disk(&self, path: &std::path::Path) -> Result<(Memory, String)> {
        read_memory_file(path).await
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
