//! Tests for P1-7: WAL journal mode and connection PRAGMAs applied at open.

use mnemos_core::Storage;
use tempfile::TempDir;

/// P1-7: Storage::open must set journal_mode=WAL so concurrent reads during
/// background writes are possible and SQLITE_BUSY is avoided.
#[tokio::test]
async fn journal_mode_is_wal() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("wal.db")).await.unwrap();
    let conn = storage.conn().unwrap();

    let mut rows = conn.query("PRAGMA journal_mode", ()).await.unwrap();
    let row = rows
        .next()
        .await
        .unwrap()
        .expect("PRAGMA journal_mode returned no rows");
    let mode: String = row.get(0).unwrap();
    assert_eq!(mode, "wal", "P1-7: journal_mode must be WAL, got: {mode}");
}

/// P1-7: Storage::open must complete without error when the PRAGMA batch
/// (including busy_timeout=5000) is applied.  busy_timeout is a connection-
/// scoped setting in SQLite that is not observable via `PRAGMA busy_timeout`
/// on a *different* connection — the fact that `open()` succeeds is the
/// observable proof that the batch was executed successfully.
#[tokio::test]
async fn pragma_batch_applied_without_error() {
    let tmp = TempDir::new().unwrap();
    // If the PRAGMA batch in Storage::open fails, this unwrap panics the test.
    let _storage = Storage::open(&tmp.path().join("busy.db")).await.unwrap();
    // Additionally verify WAL (the one persistent, cross-connection observable).
    let conn = _storage.conn().unwrap();
    let mut rows = conn.query("PRAGMA journal_mode", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let mode: String = row.get(0).unwrap();
    assert_eq!(
        mode, "wal",
        "P1-7: journal_mode=WAL must be set as part of the PRAGMA batch"
    );
}

/// P1-7: synchronous must be NORMAL (1) or FULL (2) — never OFF (0).
/// NORMAL is safe under WAL; OFF would risk corruption.
#[tokio::test]
async fn synchronous_is_at_least_normal() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("sync.db")).await.unwrap();
    let conn = storage.conn().unwrap();

    let mut rows = conn.query("PRAGMA synchronous", ()).await.unwrap();
    let row = rows
        .next()
        .await
        .unwrap()
        .expect("PRAGMA synchronous returned no rows");
    let level: i64 = row.get(0).unwrap();
    // 0=OFF, 1=NORMAL, 2=FULL, 3=EXTRA — require at least NORMAL.
    assert!(
        level >= 1,
        "P1-7: synchronous must be NORMAL (1) or higher, got: {level}"
    );
}

/// P1-7: WAL mode must survive a close-and-reopen cycle (libsql persists WAL
/// mode in the database header).
#[tokio::test]
async fn wal_mode_persists_after_reopen() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("wal_reopen.db");

    {
        let _s = Storage::open(&path).await.unwrap();
    }
    // Re-open with a fresh Storage handle.
    let s2 = Storage::open(&path).await.unwrap();
    let conn = s2.conn().unwrap();
    let mut rows = conn.query("PRAGMA journal_mode", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let mode: String = row.get(0).unwrap();
    assert_eq!(mode, "wal", "WAL mode must persist after close-and-reopen");
}
