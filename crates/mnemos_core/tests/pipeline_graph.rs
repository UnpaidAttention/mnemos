use chrono::Utc;
use mnemos_core::paths::Paths;
use mnemos_core::pipeline::graph::update_graph;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::vault::Vault;
use tempfile::TempDir;

#[tokio::test]
async fn builds_edges_and_entities_from_triples() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    let edges = update_graph(
        v.storage(),
        "mem_1",
        "Shaun~uses~Rust and Shaun~works_at~Armellini",
        Utc::now(),
        &MockLlm::new(),
    )
    .await
    .unwrap();
    assert_eq!(edges.len(), 2);

    let conn = v.storage().conn().unwrap();
    let mut er = conn
        .query("SELECT COUNT(*) FROM entity_edges", ())
        .await
        .unwrap();
    let edge_count: i64 = er.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(edge_count, 2);

    let mut nr = conn
        .query("SELECT COUNT(*) FROM entities", ())
        .await
        .unwrap();
    let node_count: i64 = nr.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(node_count, 3, "Shaun, Rust, Armellini");
}
