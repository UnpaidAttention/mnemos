use mnemos_core::MnemosError;

#[test]
fn invalid_frontmatter_error_includes_path() {
    let err = MnemosError::InvalidFrontmatter {
        path: "/tmp/bad.md".into(),
        reason: "missing 'tier'".into(),
    };
    let msg = format!("{err}");
    assert!(msg.contains("/tmp/bad.md"));
    assert!(msg.contains("missing 'tier'"));
}

#[test]
fn not_found_error_includes_id() {
    let err = MnemosError::MemoryNotFound("mem_01HXTEST".into());
    assert!(format!("{err}").contains("mem_01HXTEST"));
}
