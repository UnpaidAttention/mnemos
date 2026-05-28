use mnemos_core::storage::Storage;
use mnemos_core::sync::filesystem::FilesystemSync;
use mnemos_core::sync::state::list_unresolved_conflicts;
use mnemos_core::sync::SyncBackend;
use tempfile::TempDir;
use tokio::fs;

#[tokio::test]
async fn filesystem_pull_detects_syncthing_and_dropbox_conflicts() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("memories")).await.unwrap();
    fs::write(root.join("memories/mem_a.md"), "---\n---\nok")
        .await
        .unwrap();
    // Syncthing pattern
    fs::write(
        root.join("memories/mem_a.sync-conflict-20260101-000000-LAPTOP.md"),
        "x",
    )
    .await
    .unwrap();
    // Dropbox pattern
    fs::write(
        root.join("memories/mem_a (Shaun's conflicted copy 2026-05-01).md"),
        "x",
    )
    .await
    .unwrap();

    let storage = Storage::open(&root.join(".mnemos.db")).await.unwrap();
    let backend = FilesystemSync::new(storage.clone());
    let report = backend.pull(root).await.unwrap();
    assert_eq!(report.conflicts.len(), 2);

    let open = list_unresolved_conflicts(&storage).await.unwrap();
    assert_eq!(open.len(), 2);

    // push is a no-op but returns Ok
    let r2 = backend.push(root).await.unwrap();
    assert!(r2.message.to_lowercase().contains("no-op") || r2.files_changed == 0);
}
