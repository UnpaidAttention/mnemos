use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn cmd(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("mnemos").unwrap();
    c.env("MNEMOS_VAULT", tmp.path())
        .env("MNEMOS_EMBEDDER", "mock");
    c
}

#[test]
fn embed_status_reports_counts() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp)
        .args(["remember", "a", "--title", "t1"])
        .assert()
        .success();
    cmd(&tmp)
        .args(["remember", "b", "--title", "t2"])
        .assert()
        .success();
    let out = cmd(&tmp)
        .args(["--json", "embed", "status"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["memories_active"].as_u64().unwrap(), 2);
    assert_eq!(v["memories_embedded"].as_u64().unwrap(), 2);
    assert_eq!(v["memories_unembedded"].as_u64().unwrap(), 0);
}

#[test]
fn embed_backfill_processes_unembedded_memories() {
    let tmp = TempDir::new().unwrap();

    // Phase 1: insert WITHOUT embedder
    {
        let mut c = Command::cargo_bin("mnemos").unwrap();
        c.env("MNEMOS_VAULT", tmp.path())
            .env("MNEMOS_EMBEDDER", "none")
            .args(["remember", "no-embed body", "--title", "x"])
            .assert()
            .success();
    }

    // Phase 2: backfill WITH embedder
    cmd(&tmp)
        .args(["embed", "backfill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("embedded: 1"));

    // Verify
    let out = cmd(&tmp)
        .args(["--json", "embed", "status"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["memories_unembedded"].as_u64().unwrap(), 0);
}
