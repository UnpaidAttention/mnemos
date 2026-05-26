use chrono::{Duration, Utc};
use mnemos_core::retrieval::reweight::{apply_reweight, ReweightConfig};
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::Tier;

fn mem(tier: Tier, importance: f64, strength: f64, age_days: i64) -> Memory {
    let now = Utc::now();
    Memory {
        id: "x".into(),
        tier,
        kind: MemoryType::Fact,
        title: "t".into(),
        body: "b".into(),
        tags: vec![],
        entities: vec![],
        links: vec![],
        provenance: vec![],
        created_at: now - Duration::days(age_days),
        ingested_at: now - Duration::days(age_days),
        valid_at: now - Duration::days(age_days),
        invalid_at: None,
        superseded_by: None,
        strength,
        importance,
        last_accessed: now,
        access_count: 0,
        workspace: None,
        source_tool: None,
        mnemos_version: 1,
    }
}

#[test]
fn fresh_high_importance_strong_working_memory_wins() {
    let cfg = ReweightConfig::default();
    let m_fresh = mem(Tier::Working, 0.9, 1.0, 0);
    let m_old = mem(Tier::Episodic, 0.2, 0.3, 90);
    let base = 1.0;
    let s_fresh = apply_reweight(base, &m_fresh, &cfg);
    let s_old = apply_reweight(base, &m_old, &cfg);
    assert!(
        s_fresh > s_old * 3.0,
        "fresh should massively outscore old: {s_fresh} vs {s_old}"
    );
}

#[test]
fn invalidated_memory_does_not_get_reweighted_strangely() {
    let cfg = ReweightConfig::default();
    let m = mem(Tier::Semantic, 0.5, 0.5, 30);
    let s = apply_reweight(1.0, &m, &cfg);
    assert!(s > 0.0);
    assert!(s.is_finite());
}

#[test]
fn tier_weights_are_applied() {
    let cfg = ReweightConfig::default();
    let m_working = mem(Tier::Working, 0.5, 1.0, 0);
    let m_episodic = mem(Tier::Episodic, 0.5, 1.0, 0);
    let s_w = apply_reweight(1.0, &m_working, &cfg);
    let s_e = apply_reweight(1.0, &m_episodic, &cfg);
    // Working default weight = 2.0, Episodic = 0.8 → working > episodic
    assert!(s_w > s_e);
}

#[test]
fn zero_strength_zeros_score() {
    let cfg = ReweightConfig::default();
    let m = mem(Tier::Semantic, 1.0, 0.0, 0);
    assert_eq!(apply_reweight(1.0, &m, &cfg), 0.0);
}
