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
fn rebuild_reports_indexed_count() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp)
        .args(["remember", "body", "--title", "t"])
        .assert()
        .success();
    cmd(&tmp)
        .args(["remember", "body2", "--title", "t2"])
        .assert()
        .success();
    std::fs::remove_file(tmp.path().join("index.db")).unwrap();

    cmd(&tmp)
        .args(["rebuild"])
        .assert()
        .success()
        .stdout(predicate::str::contains("indexed: 2"));
}

#[test]
fn doctor_reports_clean_vault() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp)
        .args(["remember", "x", "--title", "y"])
        .assert()
        .success();
    cmd(&tmp)
        .args(["doctor"])
        .assert()
        .success()
        // New doctor output: "no drift issues" when no file/DB divergence found.
        .stdout(predicate::str::contains("no drift issues"));
}

#[test]
fn status_shows_memory_count() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp)
        .args(["remember", "body", "--title", "t"])
        .assert()
        .success();
    cmd(&tmp)
        .args(["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("memories: 1"));
}
