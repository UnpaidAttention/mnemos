use chrono::Utc;
use mnemos_core::graph::MemoryGraph;
use mnemos_core::paths::Paths;
use mnemos_core::retrieval::graph_recall::graph_rank;
use mnemos_core::storage::entity_ops::{link_entity_mention, upsert_edge, upsert_entity};
use mnemos_core::vault::{RememberOpts, Vault};
use tempfile::TempDir;

#[tokio::test]
async fn graph_rank_surfaces_multi_hop_memory() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    // mem1 contains the query words; mem2 does NOT — it is only reachable via
    // the entity graph (mem1->Rust -edge- Tauri<-mem2).
    let mem1 = v
        .remember("alpha rust topic", RememberOpts::default())
        .await
        .unwrap();
    let mem2 = v
        .remember("zebra unrelated words", RememberOpts::default())
        .await
        .unwrap();
    let rust = upsert_entity(v.storage(), "Rust", "tool").await.unwrap();
    let tauri = upsert_entity(v.storage(), "Tauri", "tool").await.unwrap();
    upsert_edge(v.storage(), &rust, &tauri, "uses", &mem1, Utc::now())
        .await
        .unwrap();
    link_entity_mention(v.storage(), &mem1, &rust)
        .await
        .unwrap();
    link_entity_mention(v.storage(), &mem2, &tauri)
        .await
        .unwrap();

    let g = MemoryGraph::load(v.storage()).await.unwrap();
    let ranked = graph_rank(v.storage(), &g, "alpha rust", 0.85, 30, 5)
        .await
        .unwrap();

    assert!(ranked.iter().any(|r| r.id == mem1), "seed memory present");
    assert!(
        ranked.iter().any(|r| r.id == mem2),
        "multi-hop memory reachable via the graph should be ranked"
    );
}

#[tokio::test]
async fn graph_rank_empty_without_seeds() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let g = MemoryGraph::load(v.storage()).await.unwrap();
    let ranked = graph_rank(v.storage(), &g, "nothing here", 0.85, 30, 5)
        .await
        .unwrap();
    assert!(ranked.is_empty());
}
