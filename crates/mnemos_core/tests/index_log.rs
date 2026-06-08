use mnemos_core::paths::Paths;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::Tier;
use tempfile::TempDir;

#[tokio::test]
async fn index_log_files_created_and_populated() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths.clone()).await.unwrap();

    // Create a memory
    let id = vault
        .remember(
            "Shaun prefers Rust over Go",
            RememberOpts {
                title: Some("Shaun's Preference".to_string()),
                tier: Tier::Semantic,
                kind: mnemos_core::types::MemoryType::Fact,
                tags: vec!["rust".to_string(), "go".to_string()],
                workspace: Some("my_project".to_string()),
                source_tool: Some("mcp_client".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // Verify index.md was created in the vault root
    let index_path = paths.root.join("index.md");
    assert!(index_path.exists(), "index.md must exist in vault root");
    let index_content = std::fs::read_to_string(&index_path).unwrap();
    assert!(
        index_content.contains("Shaun's Preference"),
        "index must contain memory title"
    );
    assert!(index_content.contains(&id), "index must contain memory id");
    assert!(
        index_content.contains("tool: `mcp_client`"),
        "index must contain tool info"
    );

    // Verify log.md was created in the vault root
    let log_path = paths.root.join("log.md");
    assert!(log_path.exists(), "log.md must exist in vault root");
    let log_content = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        log_content.contains("create"),
        "log must mention create action"
    );
    assert!(log_content.contains(&id), "log must contain memory id");
}
