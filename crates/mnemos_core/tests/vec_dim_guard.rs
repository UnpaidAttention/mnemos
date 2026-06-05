//! Tests for P1-5: ensure_vec_tables_dim refuses to silently wipe non-empty
//! vectors at a different dim, and is idempotent when dims match.

use mnemos_core::storage::vec_ops::{
    ensure_vec_tables_dim, insert_memory_vec, memory_vec_declared_dim,
};
use mnemos_core::Storage;
use tempfile::TempDir;

/// When memory_vec is empty, ensure_vec_tables_dim must recreate at the new
/// dim without error.
#[tokio::test]
async fn empty_table_can_be_recreated_at_new_dim() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("vec.db")).await.unwrap();

    // v2 migration creates memory_vec at 768.
    assert_eq!(memory_vec_declared_dim(&storage).await.unwrap(), Some(768));

    // Empty table: safe to recreate at 384.
    ensure_vec_tables_dim(&storage, 384).await.unwrap();
    assert_eq!(memory_vec_declared_dim(&storage).await.unwrap(), Some(384));
}

/// When dims already match, ensure_vec_tables_dim must be a no-op (does not
/// drop/recreate the table, does not error).
#[tokio::test]
async fn same_dim_is_no_op() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("vec_noop.db"))
        .await
        .unwrap();

    ensure_vec_tables_dim(&storage, 768).await.unwrap();
    // Call again at the same dim: must not panic or error.
    ensure_vec_tables_dim(&storage, 768).await.unwrap();
    assert_eq!(memory_vec_declared_dim(&storage).await.unwrap(), Some(768));
}

/// P1-5: if memory_vec is non-empty at a different dim, ensure_vec_tables_dim
/// must return an error instead of silently wiping the vectors.
#[tokio::test]
async fn non_empty_different_dim_returns_error_not_wipe() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("vec_guard.db"))
        .await
        .unwrap();

    // Table starts at 768 (v2 migration default). Insert a vector.
    let vec_768: Vec<f32> = vec![0.1f32; 768];
    insert_memory_vec(&storage, "mem_test_01", &vec_768)
        .await
        .unwrap();

    // Confirm there is now 1 row in memory_vec.
    {
        let conn = storage.conn().unwrap();
        let mut rows = conn
            .query("SELECT COUNT(*) FROM memory_vec", ())
            .await
            .unwrap();
        let count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(count, 1, "precondition: 1 vector in memory_vec");
    }

    // P1-5: attempting to switch to 384 when vectors exist must fail.
    let result = ensure_vec_tables_dim(&storage, 384).await;
    assert!(
        result.is_err(),
        "P1-5: non-empty memory_vec at dim 768 must block switch to dim 384"
    );

    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(
        err_msg.contains("embed-rebuild") || err_msg.contains("768"),
        "error must mention embed-rebuild or the existing dim: {err_msg}"
    );

    // The existing vector must still be present (nothing was wiped).
    {
        let conn = storage.conn().unwrap();
        let mut rows = conn
            .query("SELECT COUNT(*) FROM memory_vec", ())
            .await
            .unwrap();
        let count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(
            count, 1,
            "P1-5: vectors must not be wiped on dim-mismatch error"
        );
    }

    // Dim must still be 768 (table unchanged).
    assert_eq!(
        memory_vec_declared_dim(&storage).await.unwrap(),
        Some(768),
        "declared dim must remain 768 after rejected switch"
    );
}

/// P1-5: switching dim on an empty table with a non-zero declared dim (e.g.
/// after a prior rebuild that left the table empty) must succeed.
#[tokio::test]
async fn empty_non_default_dim_table_can_switch() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("vec_switch.db"))
        .await
        .unwrap();

    // First switch from 768→384 (empty table).
    ensure_vec_tables_dim(&storage, 384).await.unwrap();
    assert_eq!(memory_vec_declared_dim(&storage).await.unwrap(), Some(384));

    // Insert then delete to leave the table empty at 384.
    let vec_384: Vec<f32> = vec![0.0f32; 384];
    insert_memory_vec(&storage, "mem_a", &vec_384)
        .await
        .unwrap();
    mnemos_core::storage::vec_ops::delete_memory_vec(&storage, "mem_a")
        .await
        .unwrap();

    // Now switch from 384→512 (table is empty again): must succeed.
    ensure_vec_tables_dim(&storage, 512).await.unwrap();
    assert_eq!(memory_vec_declared_dim(&storage).await.unwrap(), Some(512));
}
