use chrono::Utc;
use mnemos_core::graph::MemoryGraph;
use mnemos_core::paths::Paths;
use mnemos_core::retrieval::hybrid::hybrid_recall_full;
use mnemos_core::retrieval::RecallOpts;
use mnemos_core::storage::entity_ops::{link_entity_mention, upsert_edge, upsert_entity};
use mnemos_core::vault::{RememberOpts, Vault};
use tempfile::TempDir;

#[tokio::test]
async fn graph_fusion_pulls_in_multi_hop_memory() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    let mem1 = v
        .remember("alpha rust topic", RememberOpts::default())
        .await
        .unwrap();
    let mem2 = v
        .remember("zebra unrelated words", RememberOpts::default())
        .await
        .unwrap();
    let rust = upsert_entity(v.storage(), "Rust", "tool", None).await.unwrap();
    let tauri = upsert_entity(v.storage(), "Tauri", "tool", None).await.unwrap();
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
    let opts = RecallOpts {
        k: 10,
        explain: true,
        ..Default::default()
    };
    // No embedder (BM25 + PPR only); BM25 alone would never return mem2.
    let hits = hybrid_recall_full(v.storage(), None, None, Some(&g), "alpha rust", opts)
        .await
        .unwrap();

    let m2 = hits.iter().find(|h| h.memory.id == mem2);
    assert!(
        m2.is_some(),
        "graph fusion should surface the multi-hop memory"
    );
    assert!(
        m2.unwrap().ppr_rank.is_some(),
        "mem2 came from the PPR retriever"
    );
    // sanity: mem1 still present
    assert!(hits.iter().any(|h| h.memory.id == mem1));
}
