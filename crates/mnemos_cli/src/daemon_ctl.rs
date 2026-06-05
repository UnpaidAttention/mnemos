//! Ensure the daemon is running before an action that needs it. Probe /health;
//! if down, spawn `mnemos-daemon` detached and wait. Best-effort / fail-open:
//! callers that are fail-open just proceed on false.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

/// Default daemon base URL used when the config cannot be read.
const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:7423";

/// Return the daemon base URL (no trailing slash), resolving it from
/// `MNEMOS_DAEMON_PORT` / `config.daemon.*` at runtime.
///
/// Resolution order (first match wins):
/// 1. `MNEMOS_DAEMON_PORT` env var — port only; host is always 127.0.0.1.
/// 2. `config.daemon.host` + `config.daemon.port` from the default config file.
/// 3. Hard-coded fallback `http://127.0.0.1:7423`.
///
/// This function is fail-open: any error reading the config file falls back to
/// the default URL so hooks never fail due to a missing or malformed config.
pub fn daemon_base_url() -> String {
    // Fast path: honour the env var used by apply_env_overrides.
    if let Ok(port_str) = std::env::var("MNEMOS_DAEMON_PORT") {
        if let Ok(port) = port_str.trim().parse::<u16>() {
            return format!("http://127.0.0.1:{port}");
        }
    }
    // Slow path: load the full config (reads MNEMOS_CONFIG_PATH / XDG path).
    match mnemos_daemon::config::Config::load_default() {
        Ok(cfg) => format!("http://{}:{}", cfg.daemon.host, cfg.daemon.port),
        Err(_) => DEFAULT_DAEMON_URL.to_string(),
    }
}

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
    let url = format!("{}/health", daemon_base_url());
    reqwest::Client::new()
        .get(url)
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

    /// When MNEMOS_DAEMON_PORT is set, daemon_base_url() must honour it.
    #[test]
    fn daemon_base_url_honours_mnemos_daemon_port_env_var() {
        // Isolate env mutation to avoid polluting parallel tests.
        // We use a port that is unlikely to be in use so the test is
        // self-contained (no real socket needed — we only test URL formation).
        let key = "MNEMOS_DAEMON_PORT";
        // Save previous value.
        let prev = std::env::var(key).ok();
        std::env::set_var(key, "19999");
        let url = daemon_base_url();
        // Restore previous value.
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
        assert_eq!(
            url, "http://127.0.0.1:19999",
            "daemon_base_url() must use the port from MNEMOS_DAEMON_PORT"
        );
    }

    /// When MNEMOS_DAEMON_PORT is absent, the default port (7423) is used.
    #[test]
    fn daemon_base_url_falls_back_to_default_port_when_env_absent() {
        // Remove the env var for this test (best-effort; may already be absent).
        let key = "MNEMOS_DAEMON_PORT";
        let prev = std::env::var(key).ok();
        std::env::remove_var(key);
        // Also point config away from any real user config so the test is hermetic.
        let prev_cfg = std::env::var("MNEMOS_CONFIG_PATH").ok();
        std::env::set_var("MNEMOS_CONFIG_PATH", "/nonexistent/config.toml");

        let url = daemon_base_url();

        // Restore.
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
        match prev_cfg {
            Some(v) => std::env::set_var("MNEMOS_CONFIG_PATH", v),
            None => std::env::remove_var("MNEMOS_CONFIG_PATH"),
        }
        // When the config file doesn't exist, Config::load_default() returns
        // Config::default() which has port 7423.
        assert_eq!(
            url, "http://127.0.0.1:7423",
            "daemon_base_url() must fall back to default port 7423"
        );
    }
}
