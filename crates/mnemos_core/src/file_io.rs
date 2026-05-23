use crate::error::Result;
use crate::frontmatter::{parse_frontmatter_at, serialize_with_frontmatter};
use crate::paths::Paths;
use crate::types::Memory;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt as _;

/// Read a memory file from disk. Returns the [`Memory`] (with body populated)
/// and the body string (also held by the Memory, for ergonomic access).
pub async fn read_memory_file(path: &Path) -> Result<(Memory, String)> {
    let text = tokio::fs::read_to_string(path).await?;
    parse_frontmatter_at(&text, path)
}

/// Write a memory to disk atomically (write to `.tmp`, then rename).
///
/// Returns the final path on success. The temporary file is removed by the
/// rename, so no stray `.tmp` files remain after a successful write.
///
/// Path layout: `<files>/<tier_dir>/<id>.md`
pub async fn write_memory_file(paths: &Paths, mem: &Memory) -> Result<PathBuf> {
    let dir = paths.tier_dir(mem.tier);
    tokio::fs::create_dir_all(&dir).await?;

    let final_path = dir.join(format!("{}.md", mem.id));
    let tmp_path = dir.join(format!("{}.md.tmp", mem.id));

    let serialized = serialize_with_frontmatter(mem)?;
    {
        let mut f = tokio::fs::File::create(&tmp_path).await?;
        f.write_all(serialized.as_bytes()).await?;
        f.sync_data().await?;
    }
    tokio::fs::rename(&tmp_path, &final_path).await?;
    Ok(final_path)
}

/// Stable SHA-256 hex hash of any text (used to detect file ↔ DB drift).
pub fn content_hash(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    format!("{digest:x}")
}
