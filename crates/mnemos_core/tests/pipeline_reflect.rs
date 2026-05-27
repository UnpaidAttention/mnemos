use mnemos_core::paths::Paths;
use mnemos_core::pipeline::reflect::reflect;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::Tier;
use tempfile::TempDir;

#[tokio::test]
async fn reflect_creates_typed_reflection_and_marks_sources() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    // The MockLlm reads REFLECT:<kind>|<text> markers out of the memory bodies.
    let src = v
        .remember(
            "We compared editors. REFLECT:preference|Shaun prefers Rust over Go",
            RememberOpts::default(),
        )
        .await
        .unwrap();

    let created = reflect(&v, &MockLlm::new(), 20).await.unwrap();
    assert_eq!(created.len(), 1);

    let refl = v
        .list(ListFilter {
            tiers: Some(vec![Tier::Reflection]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(refl.len(), 1);
    assert_eq!(refl[0].body, "Shaun prefers Rust over Go");
    assert!(refl[0].tags.iter().any(|t| t == "preference"));

    // source is marked reflected → not returned again
    let pending = mnemos_core::storage::memory_ops::recent_unreflected(v.storage(), 10)
        .await
        .unwrap();
    assert!(pending.iter().all(|m| m.id != src));
}
