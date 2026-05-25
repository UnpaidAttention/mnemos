use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn recall_returns_matching_memory_by_keyword() {
    let tmp = TempDir::new().unwrap();
    let bin = || {
        let mut c = Command::cargo_bin("mnemos").unwrap();
        c.env("MNEMOS_VAULT", tmp.path());
        c
    };
    bin()
        .args([
            "remember",
            "User uses Tauri for the desktop UI",
            "--title",
            "Tauri choice",
        ])
        .assert()
        .success();
    bin()
        .args([
            "remember",
            "React is a JS UI framework",
            "--title",
            "React notes",
        ])
        .assert()
        .success();

    bin()
        .args(["recall", "tauri"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tauri"));
}

#[test]
fn recall_json_includes_score_and_id() {
    let tmp = TempDir::new().unwrap();
    let mut c = Command::cargo_bin("mnemos").unwrap();
    c.env("MNEMOS_VAULT", tmp.path());
    c.args(["remember", "platypus body", "--title", "Platypus fact"])
        .assert()
        .success();

    let mut c = Command::cargo_bin("mnemos").unwrap();
    let out = c
        .env("MNEMOS_VAULT", tmp.path())
        .args(["--json", "recall", "platypus"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
    let arr = v["hits"].as_array().unwrap();
    assert!(!arr.is_empty(), "no hits");
    assert!(arr[0]["score"].is_number());
    assert!(arr[0]["memory"]["id"].as_str().unwrap().starts_with("mem_"));
}
