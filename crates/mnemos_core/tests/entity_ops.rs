use chrono::Utc;
use mnemos_core::paths::Paths;
use mnemos_core::storage::entity_ops::{
    find_entity_by_name, link_entity_mention, upsert_edge, upsert_entity,
};
use mnemos_core::storage::memory_ops::insert_memory;
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::vault::Vault;
use mnemos_core::Tier;
use tempfile::TempDir;

async fn vault() -> Vault {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    Vault::open(Paths::with_root(tmp.path())).await.unwrap()
}

#[tokio::test]
async fn upsert_entity_is_idempotent_by_name() {
    let v = vault().await;
    let a = upsert_entity(v.storage(), "Rust", "tool").await.unwrap();
    let b = upsert_entity(v.storage(), "Rust", "tool").await.unwrap();
    assert_eq!(a, b);
    let found = find_entity_by_name(v.storage(), "Rust").await.unwrap();
    assert_eq!(found.unwrap().id, a);
}

#[tokio::test]
async fn link_mention_is_idempotent() {
    let v = vault().await;
    let e = upsert_entity(v.storage(), "Shaun", "person").await.unwrap();

    // Insert a real memory so the FK on entity_mentions.memory_id is satisfied.
    let mem_id = "mem_1".to_string();
    let mem = Memory::new_now(
        mem_id.clone(),
        Tier::Semantic,
        MemoryType::Fact,
        "test title".into(),
        "test body".into(),
    );
    insert_memory(v.storage(), &mem, "/tmp/mem_1.md", "hash1")
        .await
        .unwrap();

    link_entity_mention(v.storage(), &mem_id, &e).await.unwrap();
    link_entity_mention(v.storage(), &mem_id, &e).await.unwrap();
    let conn = v.storage().conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM entity_mentions WHERE entity_id = ?",
            libsql::params![e.clone()],
        )
        .await
        .unwrap();
    let n: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn upsert_edge_merges_and_bumps_weight() {
    let v = vault().await;
    let s = upsert_entity(v.storage(), "Shaun", "person").await.unwrap();
    let t = upsert_entity(v.storage(), "Rust", "tool").await.unwrap();
    let e1 = upsert_edge(v.storage(), &s, &t, "uses", "mem_1", Utc::now())
        .await
        .unwrap();
    let e2 = upsert_edge(v.storage(), &s, &t, "uses", "mem_2", Utc::now())
        .await
        .unwrap();
    assert_eq!(e1, e2, "same (source,target,relation) edge is reused");
    let conn = v.storage().conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT weight, source_memory_ids FROM entity_edges WHERE id = ?",
            libsql::params![e1.clone()],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let weight: f64 = row.get(0).unwrap();
    let mids_json: String = row.get(1).unwrap();
    assert!((weight - 2.0).abs() < 1e-9);
    let mids: Vec<String> = serde_json::from_str(&mids_json).unwrap();
    assert_eq!(mids.len(), 2);
}
