use mnemos_core::rebuild::rebuild_index;
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

/// A `forget`-then-`rebuild` cycle must keep the memory invalidated.
///
/// Before the fix, `Vault::forget` only updated the DB; the markdown file
/// retained `invalid_at: null` in its frontmatter.  A subsequent rebuild
/// would re-read the file and re-insert the memory as fully valid, silently
/// un-doing the soft-deletion.
#[tokio::test]
async fn forget_then_rebuild_keeps_memory_invalidated() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let id = {
        let vault = Vault::open(paths.clone()).await.unwrap();
        let id = vault
            .remember(
                "body",
                RememberOpts {
                    title: Some("doomed".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        vault.forget(&id, Some("test")).await.unwrap();
        id
    };
    // Rebuild from files — the file must carry `invalid_at` in its frontmatter.
    rebuild_index(&paths).await.unwrap();
    // Memory should remain invalidated after the rebuild.
    let vault = Vault::open(paths.clone()).await.unwrap();
    let mem = vault.get(&id).await.unwrap();
    assert!(
        mem.invalid_at.is_some(),
        "memory should still be invalidated after rebuild"
    );
}
