use crate::error::Result;
use crate::paths::Paths;
use crate::storage::Storage;
use crate::tier::Tier;
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;

/// The kind of drift detected between the file system and the database.
#[derive(Debug, Serialize)]
pub enum DriftKind {
    /// A `.md` file exists on disk but has no corresponding DB row.
    FileNotInDb,
    /// A DB row references a file path that no longer exists on disk.
    DbRowNoFile,
    /// Both the file and DB row exist, but the stored `content_hash` does not
    /// match a freshly computed hash of the file body.
    HashMismatch,
    /// A file is parked in the quarantine directory awaiting review.
    QuarantineFile,
}

/// A single detected drift issue.
#[derive(Debug, Serialize)]
pub struct DriftIssue {
    pub kind: DriftKind,
    pub path: Option<PathBuf>,
    pub memory_id: Option<String>,
    pub detail: String,
}

/// Summary report produced by [`diagnose`].
#[derive(Debug, Serialize, Default)]
pub struct DoctorReport {
    /// Total number of `.md` files scanned across all tier directories.
    pub files_scanned: usize,
    /// Total number of rows in the `memories` table.
    pub db_rows: usize,
    /// All detected drift issues.
    pub issues: Vec<DriftIssue>,
}

/// Walk the vault's tier directories and database, detect any drift, and
/// return a [`DoctorReport`].
///
/// Detects four categories of drift:
/// 1. `FileNotInDb` — `.md` file exists but the DB has no matching row.
/// 2. `DbRowNoFile` — DB row's `file_path` points to a missing file.
/// 3. `HashMismatch` — both exist but the stored `content_hash` differs from
///    a freshly computed hash of the file body (frontmatter excluded).
/// 4. `QuarantineFile` — any file present in the quarantine directory.
pub async fn diagnose(paths: &Paths) -> Result<DoctorReport> {
    paths.ensure_dirs()?;
    let storage = Storage::open(&paths.db_path).await?;
    let mut report = DoctorReport::default();

    // ── 1. Load all (id, file_path, content_hash) rows from the DB ───────────
    let mut db_files: std::collections::HashMap<String, (String, String)> = Default::default();
    {
        let conn = storage.conn()?;
        let mut rows = conn
            .query("SELECT id, file_path, content_hash FROM memories", ())
            .await?;
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            let file_path: String = row.get(1)?;
            let content_hash: String = row.get(2)?;
            db_files.insert(id, (file_path, content_hash));
        }
    }
    report.db_rows = db_files.len();

    // Build a set of file_paths known to the DB for O(1) lookup.
    let db_paths: HashSet<&str> = db_files.values().map(|(p, _)| p.as_str()).collect();

    // ── 2. Walk all tier directories ─────────────────────────────────────────
    let mut seen_paths: HashSet<String> = HashSet::new();
    for tier in Tier::all() {
        let dir = paths.tier_dir(*tier);
        if !dir.exists() {
            continue;
        }
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            report.files_scanned += 1;
            let p_str = path.to_string_lossy().to_string();
            seen_paths.insert(p_str.clone());

            if !db_paths.contains(p_str.as_str()) {
                report.issues.push(DriftIssue {
                    kind: DriftKind::FileNotInDb,
                    path: Some(path),
                    memory_id: None,
                    detail: "file present but not indexed".into(),
                });
            }
        }
    }

    // ── 3. Quarantine directory ───────────────────────────────────────────────
    if paths.quarantine_dir.exists() {
        let mut entries = tokio::fs::read_dir(&paths.quarantine_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            report.issues.push(DriftIssue {
                kind: DriftKind::QuarantineFile,
                path: Some(entry.path()),
                memory_id: None,
                detail: "quarantined file awaiting review".into(),
            });
        }
    }

    // ── 4. DB rows pointing at missing files ──────────────────────────────────
    for (id, (p_str, _)) in &db_files {
        if !seen_paths.contains(p_str.as_str()) {
            report.issues.push(DriftIssue {
                kind: DriftKind::DbRowNoFile,
                path: Some(PathBuf::from(p_str)),
                memory_id: Some(id.clone()),
                detail: "DB row references missing file".into(),
            });
        }
    }

    // ── 5. Hash-mismatch check ────────────────────────────────────────────────
    // Hash only the *body* (excluding frontmatter) to match how vault.rs
    // computes the stored hash via `content_hash(body)`.
    for (id, (p_str, stored_hash)) in &db_files {
        if let Ok(text) = tokio::fs::read_to_string(p_str).await {
            if let Ok((_, body)) = crate::frontmatter::parse_frontmatter(&text) {
                let live = crate::file_io::content_hash(&body);
                if &live != stored_hash {
                    report.issues.push(DriftIssue {
                        kind: DriftKind::HashMismatch,
                        path: Some(PathBuf::from(p_str)),
                        memory_id: Some(id.clone()),
                        detail: "file content differs from indexed hash".into(),
                    });
                }
            }
        }
    }

    Ok(report)
}
