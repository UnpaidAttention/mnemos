use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::{paths::Paths, tier::Tier, types::MemoryType};
use tempfile::TempDir;

#[tokio::test]
async fn remember_writes_file_and_db_row() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();

    let id = vault
        .remember(
            "body text",
            RememberOpts {
                title: Some("hello".into()),
                tier: Tier::Semantic,
                kind: MemoryType::Fact,
                tags: vec!["t1".into()],
                importance: Some(0.6),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert!(id.starts_with("mem_"));
    let file = tmp.path().join("files/semantic").join(format!("{id}.md"));
    assert!(file.exists(), "memory file should exist");

    let loaded = vault.get(&id).await.unwrap();
    assert_eq!(loaded.title, "hello");
    assert_eq!(loaded.body.trim(), "body text");
    assert_eq!(loaded.tags, vec!["t1"]);
}

#[tokio::test]
async fn forget_invalidates_memory_and_audits() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();

    let id = vault
        .remember(
            "delete me",
            RememberOpts {
                title: Some("trash".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    vault.forget(&id, Some("test reason")).await.unwrap();
    let mem = vault.get(&id).await.unwrap();
    assert!(mem.invalid_at.is_some());

    let entries = mnemos_core::storage::audit::list_audit(vault.storage(), Some(&id))
        .await
        .unwrap();
    assert!(entries.iter().any(|e| e.action == "create"));
    assert!(entries.iter().any(|e| e.action == "forget"));
}
