use chrono::Utc;
use mnemos_core::paths::Paths;
use mnemos_core::pipeline::community::detect_and_summarize;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::storage::community_ops::list_community_ids;
use mnemos_core::storage::entity_ops::{upsert_edge, upsert_entity};
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::types::MemoryType;
use mnemos_core::vault::Vault;
use tempfile::TempDir;

#[tokio::test]
async fn detects_and_summarizes_communities() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    // Triangle {A,B,C} + edge {D,E}, weak bridge C-D.
    let a = upsert_entity(v.storage(), "Alpha", "concept", None)
        .await
        .unwrap();
    let b = upsert_entity(v.storage(), "Beta", "concept", None)
        .await
        .unwrap();
    let c = upsert_entity(v.storage(), "Gamma", "concept", None)
        .await
        .unwrap();
    let d = upsert_entity(v.storage(), "Delta", "concept", None)
        .await
        .unwrap();
    let f = upsert_entity(v.storage(), "Epsilon", "concept", None)
        .await
        .unwrap();
    let m = "mem_x";
    for (x, y) in [(&a, &b), (&b, &c), (&a, &c)] {
        upsert_edge(v.storage(), x, y, "rel", m, Utc::now())
            .await
            .unwrap();
    }
    upsert_edge(v.storage(), &d, &f, "rel", m, Utc::now())
        .await
        .unwrap();
    upsert_edge(v.storage(), &c, &d, "rel", m, Utc::now())
        .await
        .unwrap();

    let created = detect_and_summarize(&v, &MockLlm::new(), 2).await.unwrap();
    assert!(!created.is_empty(), "at least one community summarized");

    // membership persisted
    assert!(!list_community_ids(v.storage()).await.unwrap().is_empty());

    // summaries are community_summary memories
    let summaries = v
        .list(ListFilter {
            ..Default::default()
        })
        .await
        .unwrap()
        .into_iter()
        .filter(|m| m.kind == MemoryType::CommunitySummary)
        .count();
    assert_eq!(summaries, created.len());
}
