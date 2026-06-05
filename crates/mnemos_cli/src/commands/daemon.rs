//! `mnemos daemon` subcommand — process-management for `mnemosd`.

use crate::cli::{DaemonAction, DaemonArgs};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Stdio;

pub async fn run(_vault: Option<PathBuf>, json: bool, args: DaemonArgs) -> Result<()> {
    match args.action {
        DaemonAction::Start => start(json).await,
        DaemonAction::Stop => stop(json).await,
        DaemonAction::Restart => restart(json).await,
        DaemonAction::Status => status(json).await,
        DaemonAction::Logs { lines } => logs(lines).await,
    }
}

async fn start(json: bool) -> Result<()> {
    let pid_path = mnemos_daemon::pid_path()?;
    if pid_path.exists() {
        if let Ok(pid) = mnemos_daemon::pid::read_pid(&pid_path) {
            if process_alive(pid) {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"started": false, "reason": "already running", "pid": pid})
                    );
                } else {
                    println!("mnemosd already running (pid {pid})");
                }
                return Ok(());
            }
        }
    }
    let log = log_path()?;
    if let Some(parent) = log.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .with_context(|| format!("open log {}", log.display()))?;
    let log_err = log_file.try_clone()?;
    // P0-8: the installed binary is "mnemos-daemon" (systemd unit, .deb/rpm
    // package, sidecar manifest, and resolve_daemon_bin all use that name).
    // Fall back to the legacy "mnemosd" name so `cargo install` / dev builds
    // still work if only the old name is on PATH.
    let bin = which::which("mnemos-daemon")
        .or_else(|_| which::which("mnemosd"))
        .unwrap_or_else(|_| PathBuf::from("mnemos-daemon"));
    let bin_name = bin
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("mnemos-daemon")
        .to_string();
    let child = std::process::Command::new(bin)
        .arg("serve")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_err))
        .spawn()
        .with_context(|| format!("spawn {bin_name}"))?;
    // Give the daemon a moment to bind its port and write the PID file.
    std::thread::sleep(std::time::Duration::from_millis(250));
    let pid = child.id();
    if json {
        println!(
            "{}",
            serde_json::json!({"started": true, "pid": pid, "log": log})
        );
    } else {
        println!("mnemosd started (pid {pid}), logs at {}", log.display());
    }
    Ok(())
}

async fn stop(json: bool) -> Result<()> {
    let pid_path = mnemos_daemon::pid_path()?;
    if !pid_path.exists() {
        if json {
            println!(
                "{}",
                serde_json::json!({"stopped": false, "reason": "no PID file"})
            );
        } else {
            println!("no daemon running (no PID file)");
        }
        return Ok(());
    }
    let pid = mnemos_daemon::pid::read_pid(&pid_path)?;
    #[cfg(unix)]
    {
        // SAFETY: kill(pid, SIGTERM) is async-signal-safe and requests graceful shutdown.
        // pid_t is i32 on all supported Unix platforms; the value comes from our own PID
        // file so it is a valid process-space integer.
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    }
    if json {
        println!("{}", serde_json::json!({"stopped": true, "pid": pid}));
    } else {
        println!("sent SIGTERM to pid {pid}");
    }
    Ok(())
}

/// Stop then start the daemon.  This is the implementation of
/// `mnemos daemon restart` (P2-8): no separate DaemonAction::Restart existed
/// before; docs referenced it but the command was absent.
async fn restart(json: bool) -> Result<()> {
    stop(json).await?;
    // Give the daemon a moment to release its port and PID file before
    // we attempt to re-bind them.
    std::thread::sleep(std::time::Duration::from_millis(500));
    start(json).await
}

async fn status(json: bool) -> Result<()> {
    let pid_path = mnemos_daemon::pid_path()?;
    if !pid_path.exists() {
        if json {
            println!("{}", serde_json::json!({"running": false}));
        } else {
            println!("mnemosd not running");
        }
        return Ok(());
    }
    let pid = mnemos_daemon::pid::read_pid(&pid_path)?;
    if !process_alive(pid) {
        if json {
            println!(
                "{}",
                serde_json::json!({"running": false, "stale_pid": pid})
            );
        } else {
            println!("mnemosd not running (stale PID file points at {pid})");
        }
        return Ok(());
    }
    // Process is alive — try to reach the HTTP /health endpoint.
    let cfg = mnemos_daemon::config::Config::load_default().unwrap_or_default();
    let url = format!("http://{}:{}/health", cfg.daemon.host, cfg.daemon.port);
    let healthy = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(500))
        .build()?
        .get(&url)
        .send()
        .await
        .is_ok();
    if json {
        println!(
            "{}",
            serde_json::json!({"running": true, "pid": pid, "url": url, "healthy": healthy})
        );
    } else {
        println!("mnemosd running — pid {pid}, url {url}, healthy={healthy}");
    }
    Ok(())
}

async fn logs(lines: usize) -> Result<()> {
    let path = log_path()?;
    if !path.exists() {
        println!("no log file at {}", path.display());
        return Ok(());
    }
    // P2-9: avoid blocking the async executor with a potentially large
    // read_to_string. We read at most the last ~1 MB of the file in a
    // spawn_blocking task so large log files never stall the runtime.
    const MAX_TAIL_BYTES: u64 = 1_024 * 1_024; // 1 MiB
    let tail = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = std::fs::File::open(&path)?;
        let len = f.metadata()?.len();
        if len > MAX_TAIL_BYTES {
            // Seek near the end so we only buffer the last ~1 MB.
            // We may land mid-line; the first line will be discarded below.
            f.seek(SeekFrom::Start(len - MAX_TAIL_BYTES))?;
        }
        let mut buf = String::new();
        f.read_to_string(&mut buf)?;
        Ok(buf)
    })
    .await??;
    let all: Vec<&str> = tail.lines().collect();
    let start = all.len().saturating_sub(lines);
    for line in &all[start..] {
        println!("{line}");
    }
    Ok(())
}

/// Returns the path to the daemon log file (XDG state dir).
fn log_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .ok_or_else(|| anyhow::anyhow!("could not resolve XDG state dir"))?;
    let state_dir = dirs.state_dir().unwrap_or_else(|| dirs.data_dir());
    Ok(state_dir.join("logs").join("mnemosd.log"))
}

/// Returns `true` if a process with the given PID is currently running.
#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    // SAFETY: kill(pid, 0) is async-signal-safe; signal 0 only checks existence.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// Conservative fallback for non-Unix platforms: assume the process is alive.
#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    true
}
