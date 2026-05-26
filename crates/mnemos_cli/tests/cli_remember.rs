use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn remember_writes_file_and_prints_id() {
    let tmp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("mnemos").unwrap();
    cmd.env("MNEMOS_VAULT", tmp.path())
        .env("MNEMOS_EMBEDDER", "mock")
        .args([
            "remember", "the body", "--title", "my title", "--tier", "semantic",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mem_"));

    // verify file exists
    let semantic_dir = tmp.path().join("files/semantic");
    let count = std::fs::read_dir(&semantic_dir).unwrap().count();
    assert_eq!(count, 1);
}

#[test]
fn remember_emits_json_when_flagged() {
    let tmp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("mnemos").unwrap();
    let out = cmd
        .env("MNEMOS_VAULT", tmp.path())
        .env("MNEMOS_EMBEDDER", "mock")
        .args(["--json", "remember", "body", "--title", "j"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
    assert!(v["id"].as_str().unwrap().starts_with("mem_"));
}
