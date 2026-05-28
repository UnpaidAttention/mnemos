use mnemos_core::storage::Storage;
use mnemos_core::sync::git::GitSync;
use mnemos_core::sync::SyncBackend;
use std::process::Command;
use tempfile::TempDir;
use tokio::fs;

fn have_git() -> bool {
    which::which("git").is_ok()
}

#[tokio::test]
async fn git_push_pull_round_trip_via_local_bare_remote() {
    if !have_git() {
        eprintln!("git not on PATH; skipping");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let remote = tmp.path().join("remote.git");
    Command::new("git")
        .args(["init", "--bare", "-b", "main"])
        .arg(&remote)
        .status()
        .unwrap();
    let local = tmp.path().join("local");
    fs::create_dir_all(local.join("memories")).await.unwrap();
    fs::write(
        local.join("memories/mem_a.md"),
        "---\nid: mem_a\n---\nhello",
    )
    .await
    .unwrap();
    for args in [
        vec!["-C", local.to_str().unwrap(), "init", "-b", "main"],
        vec![
            "-C",
            local.to_str().unwrap(),
            "remote",
            "add",
            "origin",
            remote.to_str().unwrap(),
        ],
        vec![
            "-C",
            local.to_str().unwrap(),
            "config",
            "user.email",
            "t@t.test",
        ],
        vec!["-C", local.to_str().unwrap(), "config", "user.name", "Test"],
    ] {
        Command::new("git").args(&args).status().unwrap();
    }

    let storage = Storage::open(&local.join(".mnemos.db")).await.unwrap();
    let backend = GitSync::new(storage, remote.to_string_lossy().to_string(), "main".into());
    let r = backend.push(&local).await.unwrap();
    assert!(r.message.to_lowercase().contains("pushed") || r.files_changed >= 1);

    let other = tmp.path().join("other");
    Command::new("git")
        .args(["clone", remote.to_str().unwrap(), other.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(other.join("memories/mem_a.md").exists());
}
