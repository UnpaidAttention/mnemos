use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn cmd(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("mnemos").unwrap();
    c.env("MNEMOS_VAULT", tmp.path())
        .env("MNEMOS_EMBEDDER", "mock")
        .env("XDG_CONFIG_HOME", tmp.path().join("config"))
        .env("XDG_STATE_HOME", tmp.path().join("state"));
    c
}

#[test]
fn daemon_status_when_no_daemon_running_says_so() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp)
        .args(["daemon", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("not running"));
}

#[test]
fn daemon_status_json_when_no_daemon_returns_running_false() {
    let tmp = TempDir::new().unwrap();
    let out = cmd(&tmp)
        .args(["--json", "daemon", "status"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["running"], false);
}
