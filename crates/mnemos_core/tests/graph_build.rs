use chrono::Utc;
use mnemos_core::graph::MemoryGraph;
use mnemos_core::paths::Paths;
use mnemos_core::storage::entity_ops::{link_entity_mention, upsert_edge, upsert_entity};
use mnemos_core::vault::{RememberOpts, Vault};
use tempfile::TempDir;

#[tokio::test]
async fn load_builds_graph_from_edges_and_mentions() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    let mem = v
        .remember("graph seed", RememberOpts::default())
        .await
        .unwrap();
    let a = upsert_entity(v.storage(), "Rust", "tool", None).await.unwrap();
    let b = upsert_entity(v.storage(), "Tauri", "tool", None).await.unwrap();
    upsert_edge(v.storage(), &a, &b, "uses", &mem, Utc::now())
        .await
        .unwrap();
    link_entity_mention(v.storage(), &mem, &a).await.unwrap();

    let g = MemoryGraph::load(v.storage()).await.unwrap();
    assert_eq!(g.node_count(), 2);
    let ai = g.index_of(&a).unwrap();
    assert_eq!(g.degree(ai), 1.0);
    assert_eq!(g.memories_for_entity(ai), std::slice::from_ref(&mem));
    assert_eq!(g.entities_for_memory(&mem).unwrap(), &[ai]);
}
