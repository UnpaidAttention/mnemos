//! Integration test: drives the built binary and asserts the resulting
//! `latest.json` matches the expected manifest shape.

use std::process::Command;
use tempfile::TempDir;

#[test]
fn round_trip_three_platforms() {
    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("latest.json");
    // Cargo sets CARGO_BIN_EXE_<binary-name> for integration tests.
    let bin = env!("CARGO_BIN_EXE_mnemos-release-manifest");

    let status = Command::new(bin)
        .args([
            "--version", "0.7.0",
            "--notes", "test",
            "--pub-date", "2026-05-28T13:00:00Z",
            "--platform", "darwin-x86_64",
            "--platform", "linux-x86_64",
            "--platform", "windows-x86_64",
            "--url", "https://example/mac.tar.gz",
            "--url", "https://example/linux.tar.gz",
            "--url", "https://example/win.tar.gz",
            "--signature", "sig-mac",
            "--signature", "sig-linux",
            "--signature", "sig-win",
            "--output",
        ])
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success());

    let text = std::fs::read_to_string(&out).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["version"], "0.7.0");
    assert_eq!(v["platforms"]["darwin-x86_64"]["signature"], "sig-mac");
    assert_eq!(v["platforms"]["linux-x86_64"]["url"], "https://example/linux.tar.gz");
    assert_eq!(v["platforms"]["windows-x86_64"]["signature"], "sig-win");
}
