use mnemos_core::rebuild::rebuild_index;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::{paths::Paths, Tier};
use tempfile::TempDir;

/// Calling `rebuild_index` on an already-populated vault must succeed cleanly.
///
/// Before the fix, every INSERT hit `UNIQUE constraint failed: memories.file_path`
/// because the function re-indexed without first truncating the existing rows.
#[tokio::test]
async fn rebuild_handles_populated_db_idempotently() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    {
        let vault = Vault::open(paths.clone()).await.unwrap();
        for i in 0..3 {
            vault
                .remember(
                    &format!("body {i}"),
                    RememberOpts {
                        title: Some(format!("Title {i}")),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
        }
    }
    // DB is still populated; rebuild must not fail with constraint errors.
    let stats = rebuild_index(&paths).await.unwrap();
    assert_eq!(stats.memories_indexed, 3);
    assert_eq!(stats.errors, 0);

    // Rows should still be queryable after the rebuild.
    let vault = Vault::open(paths.clone()).await.unwrap();
    let memories = vault
        .list(mnemos_core::storage::memory_ops::ListFilter::default())
        .await
        .unwrap();
    assert_eq!(memories.len(), 3);
}

#[tokio::test]
async fn rebuild_recreates_index_from_files() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());

    // Create three memories
    let ids = {
        let vault = Vault::open(paths.clone()).await.unwrap();
        let mut ids = vec![];
        for i in 0..3 {
            let id = vault
                .remember(
                    &format!("body {i}"),
                    RememberOpts {
                        title: Some(format!("Title {i}")),
                        tier: Tier::Semantic,
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            ids.push(id);
        }
        ids
    };

    // Wipe the DB; files remain
    tokio::fs::remove_file(&paths.db_path).await.unwrap();

    // Rebuild
    let stats = rebuild_index(&paths).await.unwrap();
    assert_eq!(stats.memories_indexed, 3);
    assert_eq!(stats.errors, 0);

    // Verify
    let vault = Vault::open(paths.clone()).await.unwrap();
    for id in &ids {
        let mem = vault.get(id).await.unwrap();
        assert!(mem.title.starts_with("Title "));
    }
}
