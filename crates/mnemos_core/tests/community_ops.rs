use chrono::Utc;
use mnemos_core::storage::community_ops::{
    community_members, list_community_ids, store_communities,
};
use mnemos_core::storage::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn store_and_read_membership() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("c.db")).await.unwrap();
    assert!(storage.schema_version().await.unwrap() >= 6);

    let assignments = vec![
        ("ent_a".to_string(), 0usize),
        ("ent_b".to_string(), 0usize),
        ("ent_c".to_string(), 1usize),
    ];
    store_communities(&storage, &assignments, Utc::now())
        .await
        .unwrap();

    assert_eq!(list_community_ids(&storage).await.unwrap(), vec![0, 1]);
    let mut m0 = community_members(&storage, 0).await.unwrap();
    m0.sort();
    assert_eq!(m0, vec!["ent_a".to_string(), "ent_b".to_string()]);

    // A re-run fully replaces membership.
    store_communities(&storage, &[("ent_a".to_string(), 5usize)], Utc::now())
        .await
        .unwrap();
    assert_eq!(list_community_ids(&storage).await.unwrap(), vec![5]);
}
