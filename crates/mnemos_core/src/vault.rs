use crate::error::Result;
use crate::file_io::{content_hash, read_memory_file, write_memory_file};
use crate::frontmatter::parse_frontmatter;
use crate::id::new_memory_id;
use crate::paths::Paths;
use crate::storage::audit::write_audit;
use crate::storage::memory_ops::{
    get_memory, insert_memory, list_memories, soft_invalidate, ListFilter,
};
use crate::storage::Storage;
use crate::tier::Tier;
use crate::types::{Memory, MemoryType};
use chrono::Utc;
use serde_json::json;

/// High-level vault: owns `Paths` and `Storage` together.
///
/// All higher-level code (CLI, daemon, MCP server) should work through
/// `Vault` rather than coordinating the two components directly.
#[derive(Clone)]
pub struct Vault {
    paths: Paths,
    storage: Storage,
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
    /// Open a vault: ensure directories exist, open the DB, run migrations.
    pub async fn open(paths: Paths) -> Result<Self> {
        paths.ensure_dirs()?;
        let storage = Storage::open(&paths.db_path).await?;
        Ok(Self { paths, storage })
    }

    /// Borrow the underlying storage handle (e.g. for audit queries in tests).
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// Borrow the resolved path set.
    pub fn paths(&self) -> &Paths {
        &self.paths
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
        Ok(id)
    }

    /// Retrieve a memory by ID (includes soft-invalidated ones).
    pub async fn get(&self, id: &str) -> Result<Memory> {
        get_memory(&self.storage, id).await
    }

    /// Soft-invalidate a memory and write a `forget` audit entry.
    pub async fn forget(&self, id: &str, reason: Option<&str>) -> Result<()> {
        soft_invalidate(&self.storage, id, Utc::now()).await?;
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
