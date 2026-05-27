use mnemos_core::paths::Paths;
use mnemos_core::storage::memory_ops::{link_memory_chunks, recall_as_of};
use mnemos_core::types::Provenance;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::Tier;
use tempfile::TempDir;

#[tokio::test]
async fn remember_persists_provenance_and_chunk_links() {
    let tmp = TempDir::new().unwrap();
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    let id = vault
        .remember(
            "Shaun loves Rust",
            RememberOpts {
                tier: Tier::Semantic,
                provenance: vec![Provenance {
                    session: Some("sess_1".into()),
                    chunks: vec!["chunk_a".into(), "chunk_b".into()],
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let mem = vault.get(&id).await.unwrap();
    assert_eq!(mem.provenance.len(), 1);
    assert_eq!(mem.provenance[0].session.as_deref(), Some("sess_1"));

    link_memory_chunks(vault.storage(), &id, &["chunk_a".into(), "chunk_b".into()])
        .await
        .unwrap();
    let conn = vault.storage().conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM memory_chunks WHERE memory_id = ?",
            libsql::params![id.clone()],
        )
        .await
        .unwrap();
    let n: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(n, 2);
}

#[tokio::test]
async fn patch_updates_tags_and_importance() {
    let tmp = TempDir::new().unwrap();
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let id = vault
        .remember("patch me", RememberOpts::default())
        .await
        .unwrap();

    let updated = vault
        .patch(&id, Some(vec!["x".into(), "y".into()]), Some(0.9))
        .await
        .unwrap();
    assert_eq!(updated.tags, vec!["x".to_string(), "y".to_string()]);
    assert!((updated.importance - 0.9).abs() < 1e-9);

    // Round-trips through the file too.
    let reloaded = vault.get(&id).await.unwrap();
    assert_eq!(reloaded.tags, vec!["x".to_string(), "y".to_string()]);
}

#[tokio::test]
async fn recall_as_of_respects_temporal_window() {
    let tmp = TempDir::new().unwrap();
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let id = vault
        .remember("alpha temporal beacon", RememberOpts::default())
        .await
        .unwrap();
    let mem = vault.get(&id).await.unwrap();

    let future = mem.valid_at + chrono::Duration::days(1);
    let past = mem.valid_at - chrono::Duration::days(1);

    let hits_future = recall_as_of(vault.storage(), "alpha", future, 10)
        .await
        .unwrap();
    assert_eq!(hits_future.len(), 1);
    assert_eq!(hits_future[0].id, id);

    let hits_past = recall_as_of(vault.storage(), "alpha", past, 10)
        .await
        .unwrap();
    assert!(hits_past.is_empty(), "memory not yet valid in the past");
}
