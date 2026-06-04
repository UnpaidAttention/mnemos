use mnemos_core::correction::Correction;
use mnemos_core::paths::Paths;
use mnemos_core::pipeline::reflect::harden_corrections;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::vault::Vault;
use mnemos_core::Tier;
use tempfile::TempDir;

#[tokio::test]
async fn three_same_trigger_corrections_harden_into_one_rule() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    for i in 0..3 {
        v.remember_correction(
            Correction {
                wrong: format!("variant {i}"),
                // The RULE: marker drives MockLlm's TASK=harden branch.
                right: "RULE:always run cargo fmt before commit".into(),
                why: "CI rejects unformatted code".into(),
                trigger: Some("git commit formatting".into()),
            },
            None,
        )
        .await
        .unwrap();
    }
    let created = harden_corrections(&v, &MockLlm::new(), 3).await.unwrap();
    assert_eq!(created.len(), 1, "one hardened rule from the cluster");
    let rule = v.get(&created[0]).await.unwrap();
    assert_eq!(rule.tier, Tier::Reflection);
    assert!(
        rule.tags.contains(&"mnemos:hardened".to_string()),
        "rule must carry mnemos:hardened tag; got: {:?}",
        rule.tags
    );
}
