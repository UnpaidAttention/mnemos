// Connect/disconnect a tool against a fixture HOME and assert files change correctly.
use mnemos_daemon::connectors::{descriptors, edits};

#[test]
fn claude_code_connect_then_disconnect_roundtrip() {
    let home = tempfile::tempdir().unwrap();
    std::env::set_var("HOME", home.path());
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    std::fs::write(home.path().join(".claude/CLAUDE.md"), "# mine\n\nkeep me\n").unwrap();
    std::fs::write(home.path().join(".claude.json"), r#"{"mcpServers":{"other":{"command":"x"}}}"#).unwrap();

    let c = descriptors::by_id("claude-code").unwrap();

    for e in &c.edits {
        let rendered = e.rendered().unwrap();
        edits::backup(&e.path()).unwrap();
        edits::atomic_write(&e.path(), &rendered).unwrap();
    }
    assert_eq!(format!("{:?}", c.connected()), "Full");
    let mcp = std::fs::read_to_string(home.path().join(".claude.json")).unwrap();
    assert!(mcp.contains("mnemos") && mcp.contains("other"), "added mnemos, kept other");
    let md = std::fs::read_to_string(home.path().join(".claude/CLAUDE.md")).unwrap();
    assert!(md.contains("keep me") && md.contains("mnemos:start"));

    for e in &c.edits {
        let removed = e.removed().unwrap();
        edits::atomic_write(&e.path(), &removed).unwrap();
    }
    assert_eq!(format!("{:?}", c.connected()), "None");
    let md2 = std::fs::read_to_string(home.path().join(".claude/CLAUDE.md")).unwrap();
    assert!(md2.contains("keep me") && !md2.contains("mnemos:start"), "user content kept, block gone");
}
