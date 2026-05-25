use mnemos_core::paths::Paths;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::watcher::{watch_vault, WatchEvent};
use mnemos_core::Tier;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_edit_emits_changed_event() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths.clone()).await.unwrap();

    let id = vault
        .remember(
            "original",
            RememberOpts {
                title: Some("t".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let file = paths.tier_dir(Tier::Semantic).join(format!("{id}.md"));

    let (tx, mut rx) = mpsc::channel(16);
    let _handle = watch_vault(&paths, tx).await.unwrap();

    // Give the watcher time to subscribe before editing
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut content = tokio::fs::read_to_string(&file).await.unwrap();
    content.push_str("\nappended.\n");
    tokio::fs::write(&file, content).await.unwrap();

    let event = tokio::time::timeout(Duration::from_secs(3), rx.recv())
        .await
        .expect("watcher should emit event within 3s")
        .expect("channel should not close");
    match event {
        WatchEvent::Changed(p) => assert_eq!(p, file),
        other => panic!("expected Changed, got {other:?}"),
    }
}
