//! Tests for P1-1: atomic per-version migration transactions and cross-session
//! idempotency (write-lock-bypass fix).

use mnemos_core::Storage;
use tempfile::TempDir;

/// A fresh database must reach the latest schema version and be idempotent
/// across multiple opens.
#[tokio::test]
async fn migrations_reach_latest_version_on_fresh_db() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(&tmp.path().join("m.db")).await.unwrap();
    // v11 is the current latest (v11 = strength column migration).
    assert_eq!(s.schema_version().await.unwrap(), 11);
}

/// Opening the same DB file multiple times must not advance the version past
/// the expected maximum (idempotency across sessions).
#[tokio::test]
async fn migrations_are_idempotent_across_sessions() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("idem.db");

    let s1 = Storage::open(&path).await.unwrap();
    let v1 = s1.schema_version().await.unwrap();

    drop(s1);

    let s2 = Storage::open(&path).await.unwrap();
    let v2 = s2.schema_version().await.unwrap();

    drop(s2);

    let s3 = Storage::open(&path).await.unwrap();
    let v3 = s3.schema_version().await.unwrap();

    assert_eq!(v1, v2, "version must not change on second open");
    assert_eq!(v2, v3, "version must not change on third open");
}

/// The schema_migrations table must contain exactly one row per version with
/// no duplicate entries (INSERT OR IGNORE is correct).
#[tokio::test]
async fn schema_migrations_table_has_no_duplicate_rows() {
    let tmp = TempDir::new().unwrap();
    let _s = Storage::open(&tmp.path().join("dup.db")).await.unwrap();
    // Re-open to exercise the idempotency path.
    let s = Storage::open(&tmp.path().join("dup.db")).await.unwrap();

    let conn = s.conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT version, COUNT(*) FROM schema_migrations GROUP BY version HAVING COUNT(*) > 1",
            (),
        )
        .await
        .unwrap();
    let dup = rows.next().await.unwrap();
    assert!(
        dup.is_none(),
        "schema_migrations must have no duplicate version rows"
    );
}

/// Each migration must be recorded in schema_migrations with a monotonically
/// increasing version.
#[tokio::test]
async fn schema_migrations_versions_are_sequential() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(&tmp.path().join("seq.db")).await.unwrap();
    let conn = s.conn().unwrap();

    let mut rows = conn
        .query(
            "SELECT version FROM schema_migrations ORDER BY version ASC",
            (),
        )
        .await
        .unwrap();

    let mut expected = 1i64;
    while let Some(row) = rows.next().await.unwrap() {
        let v: i64 = row.get(0).unwrap();
        assert_eq!(
            v, expected,
            "schema_migrations gap or duplicate: expected version {expected}, got {v}"
        );
        expected += 1;
    }
    // We should have seen at least v1..v11.
    assert!(
        expected > 11,
        "expected at least 11 migration rows, got {}",
        expected - 1
    );
}
