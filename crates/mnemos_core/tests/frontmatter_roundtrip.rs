use mnemos_core::frontmatter::{parse_frontmatter, serialize_with_frontmatter};
use mnemos_core::types::MemoryType;
use mnemos_core::Tier;

const SAMPLE: &str = "---
id: mem_01HXEXAMPLE
tier: semantic
type: fact
title: \"User prefers Tauri\"
tags:
- tech-pref
entities:
- tauri
links: []
provenance: []
created_at: 2026-05-22T14:30:00Z
ingested_at: 2026-05-22T14:30:05Z
valid_at: 2026-05-22T14:30:00Z
invalid_at: null
superseded_by: null
strength: 1.0
importance: 0.7
last_accessed: 2026-05-22T14:30:00Z
access_count: 0
mnemos_version: 1
---

Body content goes here.

Second paragraph.
";

#[test]
fn parses_frontmatter_and_body() {
    let (mem, body) = parse_frontmatter(SAMPLE).unwrap();
    assert_eq!(mem.id, "mem_01HXEXAMPLE");
    assert_eq!(mem.tier, Tier::Semantic);
    assert_eq!(mem.kind, MemoryType::Fact);
    assert_eq!(mem.strength, 1.0);
    assert!(body.contains("Body content goes here."));
    assert!(body.contains("Second paragraph."));
    assert!(
        !body.starts_with('\n'),
        "leading blank line should be trimmed"
    );
}

#[test]
fn roundtrip_preserves_data() {
    let (mem_in, body_in) = parse_frontmatter(SAMPLE).unwrap();
    let mut mem = mem_in.clone();
    mem.body = body_in.clone();
    let serialized = serialize_with_frontmatter(&mem).unwrap();
    let (mem_out, body_out) = parse_frontmatter(&serialized).unwrap();
    assert_eq!(mem_in.id, mem_out.id);
    assert_eq!(mem_in.tier, mem_out.tier);
    assert_eq!(mem_in.created_at, mem_out.created_at);
    assert_eq!(mem_in.strength, mem_out.strength);
    assert_eq!(body_in.trim(), body_out.trim());
}

#[test]
fn parse_rejects_missing_delimiter() {
    let result = parse_frontmatter("no frontmatter here");
    assert!(result.is_err());
}

#[test]
fn parse_rejects_truncated_frontmatter() {
    let result = parse_frontmatter("---\nid: mem_X\nno closing");
    assert!(result.is_err());
}
