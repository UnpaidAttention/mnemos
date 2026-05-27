use mnemos_core::paths::Paths;
use mnemos_core::pipeline::entities::link_entities;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::vault::Vault;
use tempfile::TempDir;

#[tokio::test]
async fn links_entities_and_records_mentions() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    // Create a real memory so the FK on entity_mentions is satisfied.
    let id = v
        .remember("anything", mnemos_core::vault::RememberOpts::default())
        .await
        .unwrap();

    let ids = link_entities(v.storage(), &id, "@Shaun ships @Rust code", &MockLlm::new())
        .await
        .unwrap();
    assert_eq!(ids.len(), 2);

    let conn = v.storage().conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM entity_mentions WHERE memory_id = ?",
            libsql::params![id.clone()],
        )
        .await
        .unwrap();
    let n: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(n, 2);
}
