//! Tests for P1-2: crash-safe file+DB writes and mnemos doctor --repair.
//!
//! Because we cannot actually crash the process mid-operation, we simulate
//! each crash scenario by directly manipulating the DB/filesystem to leave
//! the state that a mid-operation crash would produce, then verifying that
//! a subsequent retry (or `repair`) brings things back to a consistent state.

use mnemos_core::doctor::{diagnose, repair, DriftKind};
use mnemos_core::storage::memory_ops::{get_memory, insert_memory};
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::{id::new_memory_id, paths::Paths, Storage, Tier};
use tempfile::TempDir;

// ── P1-2: idempotent INSERT OR REPLACE on remember ───────────────────────────

/// Simulates: file written, process crashes before DB INSERT.
/// Recovery: re-running remember with the same id (via insert_memory directly)
/// must succeed via INSERT OR REPLACE without violating the UNIQUE constraint
/// on file_path.
#[tokio::test]
async fn insert_memory_is_idempotent_on_retry() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("crash.db")).await.unwrap();

    let mem = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "Crash test".into(),
        "body text".into(),
    );
    let path = format!("/tmp/{}.md", mem.id);
    let hash = "abc123";

    // First insert (normal path).
    insert_memory(&storage, &mem, &path, hash).await.unwrap();

    // Second insert of the same memory (crash-retry simulation).
    // Must not error (UNIQUE constraint violation) because INSERT OR REPLACE.
    insert_memory(&storage, &mem, &path, hash).await.unwrap();

    // Exactly one row must exist.
    let conn = storage.conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM memories WHERE id = ?",
            libsql::params![mem.id.clone()],
        )
        .await
        .unwrap();
    let count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(count, 1, "INSERT OR REPLACE must leave exactly one DB row");

    // Exactly one FTS row.
    let mut fts_rows = conn
        .query(
            "SELECT COUNT(*) FROM memory_fts WHERE memory_id = ?",
            libsql::params![mem.id.clone()],
        )
        .await
        .unwrap();
    let fts_count: i64 = fts_rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(
        fts_count, 1,
        "INSERT OR REPLACE must leave exactly one FTS row"
    );
}

// ── P1-2 repair: FileNotInDb (file present, DB row absent) ───────────────────

/// Simulates a crash after file write but before DB INSERT.
/// `repair` must re-index the orphaned file.
#[tokio::test]
async fn repair_re_indexes_file_not_in_db() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths.clone()).await.unwrap();

    let id = vault
        .remember(
            "orphan body",
            RememberOpts {
                title: Some("orphan".into()),
                tier: Tier::Semantic,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // Simulate crash state: delete the DB row but leave the file.
    {
        let storage = vault.storage();
        let (conn, _g) = storage.write_conn().await.unwrap();
        conn.execute(
            "DELETE FROM memories WHERE id = ?",
            libsql::params![id.clone()],
        )
        .await
        .unwrap();
        conn.execute(
            "DELETE FROM memory_fts WHERE memory_id = ?",
            libsql::params![id.clone()],
        )
        .await
        .unwrap();
    }

    // Doctor must detect the orphaned file.
    let report = diagnose(&paths).await.unwrap();
    assert!(
        report
            .issues
            .iter()
            .any(|i| matches!(i.kind, DriftKind::FileNotInDb)),
        "must detect FileNotInDb drift after simulated crash"
    );

    // Repair must re-index it.
    let repair_report = repair(&paths).await.unwrap();
    assert!(
        !repair_report.re_indexed.is_empty(),
        "repair must re-index the orphaned file"
    );
    assert!(
        repair_report.re_index_errors.is_empty(),
        "repair must not produce re-index errors: {:?}",
        repair_report.re_index_errors
    );

    // After repair, doctor must be clean (no FileNotInDb issues for that file).
    let after = diagnose(&paths).await.unwrap();
    assert!(
        !after
            .issues
            .iter()
            .any(|i| matches!(i.kind, DriftKind::FileNotInDb)),
        "no FileNotInDb issues after repair"
    );
}

// ── P1-2 repair: DbRowNoFile (DB row present, file absent) ───────────────────

/// Simulates a crash after DB INSERT but before file sync (or a file deleted
/// externally).  `repair` must soft-invalidate the DB row.
#[tokio::test]
async fn repair_soft_invalidates_db_row_no_file() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths.clone()).await.unwrap();

    let id = vault
        .remember(
            "missing file body",
            RememberOpts {
                title: Some("missing".into()),
                tier: Tier::Semantic,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // Delete the file but leave the DB row (crash/external-deletion scenario).
    let file = paths.tier_dir(Tier::Semantic).join(format!("{id}.md"));
    tokio::fs::remove_file(&file).await.unwrap();

    // Doctor must detect the DB row with missing file.
    let report = diagnose(&paths).await.unwrap();
    assert!(
        report
            .issues
            .iter()
            .any(|i| matches!(i.kind, DriftKind::DbRowNoFile)),
        "must detect DbRowNoFile drift"
    );

    // Repair must soft-invalidate the row.
    let repair_report = repair(&paths).await.unwrap();
    assert!(
        !repair_report.soft_invalidated.is_empty(),
        "repair must soft-invalidate the row with missing file"
    );
    assert!(
        repair_report.invalidate_errors.is_empty(),
        "repair must not produce invalidate errors: {:?}",
        repair_report.invalidate_errors
    );

    // The row must now have invalid_at set.
    let storage = Storage::open(&paths.db_path).await.unwrap();
    let mem = get_memory(&storage, &id).await.unwrap();
    assert!(
        mem.invalid_at.is_some(),
        "P1-2 repair: DB row must be soft-invalidated after repair"
    );
}

// ── P1-2: forget retry path ───────────────────────────────────────────────────

/// Simulates the crash-retry path for forget: the DB row is already invalid
/// (step 1 completed) but the file still has no invalid_at (step 2 not done).
/// Calling forget again must succeed (not return MemoryNotFound) and must
/// write the file with the correct invalid_at.
#[tokio::test]
async fn forget_retry_after_partial_crash_succeeds() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths.clone()).await.unwrap();

    let id = vault
        .remember(
            "retry body",
            RememberOpts {
                title: Some("retry test".into()),
                tier: Tier::Semantic,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // Simulate crash after step 1 (DB already invalid) by calling
    // soft_invalidate directly, leaving the file unchanged.
    mnemos_core::storage::memory_ops::soft_invalidate(vault.storage(), &id, chrono::Utc::now())
        .await
        .unwrap();

    // Now call vault.forget() which should detect the already-invalid row and
    // complete the remaining steps (file rewrite + vec delete + audit).
    vault.forget(&id, Some("retry test")).await.unwrap();

    // The memory must still be marked invalid.
    let mem = vault.get(&id).await.unwrap();
    assert!(
        mem.invalid_at.is_some(),
        "memory must be invalid after retry forget"
    );
}
