//! Ensure the daemon is running before an action that needs it. Probe /health;
//! if down, spawn `mnemos-daemon` detached and wait. Best-effort / fail-open:
//! callers that are fail-open just proceed on false.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

const DAEMON_URL: &str = "http://127.0.0.1:7423";

/// Resolve the daemon binary path: prefer a `mnemos-daemon` sibling of the
/// current executable (installed layout), else fall back to PATH lookup.
pub fn resolve_daemon_bin(current_exe: Option<PathBuf>) -> OsString {
    if let Some(dir) = current_exe.as_ref().and_then(|p| p.parent()) {
        let cand = dir.join("mnemos-daemon");
        if cand.exists() {
            return cand.into_os_string();
        }
    }
    OsString::from("mnemos-daemon")
}

/// Return true when the daemon answers `GET /health` with HTTP 200.
pub async fn is_up() -> bool {
    reqwest::Client::new()
        .get(format!("{DAEMON_URL}/health"))
        .timeout(Duration::from_millis(500))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Ensure the daemon is up; spawn detached + poll if needed. Returns final state.
// Task B will wire callers to these helpers.
pub async fn ensure_daemon(timeout: Duration) -> bool {
    if is_up().await {
        return true;
    }
    let bin = resolve_daemon_bin(std::env::current_exe().ok());
    let _ = std::process::Command::new(&bin)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if is_up().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_prefers_sibling_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("mnemos");
        std::fs::write(&exe, "").unwrap();
        std::fs::write(dir.path().join("mnemos-daemon"), "").unwrap();
        let got = resolve_daemon_bin(Some(exe));
        assert_eq!(got, dir.path().join("mnemos-daemon").into_os_string());
    }

    #[test]
    fn resolve_falls_back_to_path_name() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("mnemos"); // no sibling mnemos-daemon
        std::fs::write(&exe, "").unwrap();
        assert_eq!(
            resolve_daemon_bin(Some(exe)),
            OsString::from("mnemos-daemon")
        );
    }
}
