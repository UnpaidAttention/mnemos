use chrono::{TimeZone, Utc};
use mnemos_core::types::{Memory, MemoryType, Provenance};
use mnemos_core::Tier;

#[test]
fn memory_serializes_to_frontmatter_yaml() {
    let mem = Memory {
        id: "mem_01HXTEST".into(),
        tier: Tier::Semantic,
        kind: MemoryType::Fact,
        title: "User prefers Tauri".into(),
        body: "Because of small bundle size.".into(),
        tags: vec!["tech-pref".into()],
        entities: vec!["tauri".into()],
        links: vec![],
        provenance: vec![Provenance {
            session: Some("sess_01HX".into()),
            chunks: vec!["chunk_01HA".into()],
        }],
        created_at: Utc.with_ymd_and_hms(2026, 5, 22, 14, 30, 0).unwrap(),
        ingested_at: Utc.with_ymd_and_hms(2026, 5, 22, 14, 30, 5).unwrap(),
        valid_at: Utc.with_ymd_and_hms(2026, 5, 22, 14, 30, 0).unwrap(),
        invalid_at: None,
        superseded_by: None,
        strength: 1.0,
        importance: 0.7,
        last_accessed: Utc.with_ymd_and_hms(2026, 5, 22, 14, 30, 0).unwrap(),
        access_count: 0,
        workspace: None,
        source_tool: None,
        mnemos_version: 1,
    };
    let yaml = serde_yaml::to_string(&mem).unwrap();
    assert!(yaml.contains("id: mem_01HXTEST"));
    assert!(yaml.contains("tier: semantic"));
    assert!(yaml.contains("type: fact"));
    assert!(yaml.contains("strength: 1.0"));
    let back: Memory = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(back.id, mem.id);
    assert_eq!(back.tier, mem.tier);
    assert_eq!(back.strength, mem.strength);
}

#[test]
fn memory_type_serializes_kebab_case() {
    let json = serde_json::to_string(&MemoryType::CommunitySummary).unwrap();
    assert_eq!(json, "\"community-summary\"");
}

#[test]
fn memory_json_includes_body() {
    let mut mem = Memory::new_now(
        "mem_X".into(),
        mnemos_core::Tier::Semantic,
        MemoryType::Fact,
        "t".into(),
        "the body text".into(),
    );
    mem.body = "the body text".into();
    let json = serde_json::to_value(&mem).unwrap();
    assert_eq!(
        json["body"], "the body text",
        "body must be present in JSON serialization"
    );
}
