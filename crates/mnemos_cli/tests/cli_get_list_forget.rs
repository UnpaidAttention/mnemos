use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn cmd(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("mnemos").unwrap();
    c.env("MNEMOS_VAULT", tmp.path());
    c
}

fn seed(tmp: &TempDir, title: &str, body: &str) -> String {
    let out = cmd(tmp)
        .args(["remember", body, "--title", title])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(out).unwrap().trim().to_string()
}

#[test]
fn get_prints_memory_contents() {
    let tmp = TempDir::new().unwrap();
    let id = seed(&tmp, "Greeting", "hello world");
    cmd(&tmp)
        .args(["get", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Greeting"))
        .stdout(predicate::str::contains("hello world"));
}

#[test]
fn list_filters_by_tier() {
    let tmp = TempDir::new().unwrap();
    let _ = seed(&tmp, "A", "a");
    cmd(&tmp)
        .args([
            "remember",
            "rule body",
            "--title",
            "Rule",
            "--tier",
            "procedural",
        ])
        .assert()
        .success();

    let out = cmd(&tmp)
        .args(["--json", "list", "--tier", "procedural"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let memories = v["memories"].as_array().unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0]["title"], "Rule");
}

#[test]
fn forget_then_list_omits_invalidated() {
    let tmp = TempDir::new().unwrap();
    let id = seed(&tmp, "Doomed", "to be forgotten");
    cmd(&tmp)
        .args(["forget", &id, "--reason", "test"])
        .assert()
        .success();

    let out = cmd(&tmp)
        .args(["--json", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["memories"].as_array().unwrap().len(), 0);

    let out = cmd(&tmp)
        .args(["--json", "list", "--include-invalid"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["memories"].as_array().unwrap().len(), 1);
}
