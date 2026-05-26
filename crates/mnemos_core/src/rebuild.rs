use crate::error::Result;
use crate::file_io::{content_hash, read_memory_file};
use crate::paths::Paths;
use crate::providers::Embedder;
use crate::storage::memory_ops::insert_memory;
use crate::storage::vec_ops::insert_memory_vec;
use crate::storage::Storage;
use crate::tier::Tier;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Debug, Default, Serialize)]
pub struct RebuildStats {
    pub memories_indexed: usize,
    pub embeddings_indexed: usize,
    pub errors: usize,
    pub error_paths: Vec<PathBuf>,
}

/// Rebuild the index without re-embedding (embedding skipped).
///
/// Thin wrapper around [`rebuild_index_with_embedder`] with `None` embedder.
pub async fn rebuild_index(paths: &Paths) -> Result<RebuildStats> {
    rebuild_index_with_embedder(paths, None).await
}

/// Rebuild the index, optionally re-embedding every memory when an embedder
/// is provided.
///
/// Truncates all derived index tables (including `memory_vec`) then walks
/// every tier directory, re-inserting each `.md` file.  When `embedder` is
/// `Some`, the body of each memory is embedded and stored in `memory_vec`.
pub async fn rebuild_index_with_embedder(
    paths: &Paths,
    embedder: Option<Arc<dyn Embedder>>,
) -> Result<RebuildStats> {
    paths.ensure_dirs()?;
    // Open Storage which will create a fresh DB + run migrations.
    let storage = Storage::open(&paths.db_path).await?;

    // Truncate derived index tables so that re-indexing an already-populated
    // vault produces the same state as a fresh ingestion.  Order matters:
    // `memory_fts` first (virtual table, no FK), then `memory_links`, then
    // the child tables, and finally `memories` itself.
    //
    // Tables NOT truncated:
    //   - `audit_log`   — append-only; DELETE is forbidden by triggers.
    //   - `entities` / `entity_edges` — not derived from .md files; they would
    //     be managed independently in later plans.
    {
        let (conn, _g) = storage.write_conn().await?;
        let tx = conn.transaction().await?;
        for stmt in [
            "DELETE FROM memory_fts",
            "DELETE FROM memory_links",
            "DELETE FROM memory_chunks",
            "DELETE FROM entity_mentions",
            "DELETE FROM memories",
            "DELETE FROM memory_vec",
        ] {
            tx.execute(stmt, ()).await?;
        }
        tx.commit().await?;
    }

    let mut stats = RebuildStats::default();

    for tier in Tier::all() {
        let dir = paths.tier_dir(*tier);
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            match index_single_file(&storage, &path, embedder.as_ref().map(|a| a.as_ref())).await {
                Ok(embedded) => {
                    stats.memories_indexed += 1;
                    if embedded {
                        stats.embeddings_indexed += 1;
                    }
                }
                Err(e) => {
                    warn!("failed to index {}: {}", path.display(), e);
                    stats.errors += 1;
                    stats.error_paths.push(path);
                }
            }
        }
    }
    info!(
        "rebuild complete: {} indexed, {} embedded, {} errors",
        stats.memories_indexed, stats.embeddings_indexed, stats.errors
    );
    Ok(stats)
}

/// Index a single `.md` file, optionally embedding its body.
///
/// Returns `true` when an embedding was generated and stored, `false`
/// otherwise.
async fn index_single_file(
    storage: &Storage,
    path: &std::path::Path,
    embedder: Option<&dyn Embedder>,
) -> Result<bool> {
    let (mem, body) = read_memory_file(path).await?;
    let hash = content_hash(&body);
    insert_memory(storage, &mem, path.to_string_lossy().as_ref(), &hash).await?;
    if let Some(e) = embedder {
        let v = e.embed(&body).await?;
        insert_memory_vec(storage, &mem.id, &v).await?;
        return Ok(true);
    }
    Ok(false)
}
