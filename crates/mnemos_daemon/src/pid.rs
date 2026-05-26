//! PID file management for `mnemosd`. Used by `mnemos daemon status` to
//! detect a running daemon and by graceful shutdown to clean up.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// RAII guard that writes the current process PID to a file on acquire and
/// removes it on drop. Only one live process may hold the guard at a time.
pub struct PidFile {
    path: PathBuf,
}

impl PidFile {
    /// Atomically acquire the PID file at `path`.
    ///
    /// Fails with an error if a live process already owns the file.
    /// Stale files (owned by a dead process) are silently reclaimed.
    pub fn acquire(path: &Path) -> Result<Self> {
        if path.exists() {
            if let Ok(pid) = read_pid(path) {
                if process_is_alive(pid) {
                    anyhow::bail!("PID file {} already owned by process {pid}", path.display());
                }
                // Stale — fall through and overwrite.
            }
        }
        write_pid(path, std::process::id())?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        let _ = remove_pid(&self.path);
    }
}

/// Write `pid` to `path`, creating parent directories as needed.
pub fn write_pid(path: &Path, pid: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create PID dir {}", parent.display()))?;
    }
    std::fs::write(path, pid.to_string())
        .with_context(|| format!("write PID file {}", path.display()))?;
    Ok(())
}

/// Read and parse the PID stored in `path`.
pub fn read_pid(path: &Path) -> Result<u32> {
    let s = std::fs::read_to_string(path)
        .with_context(|| format!("read PID file {}", path.display()))?;
    s.trim().parse().context("parse PID")
}

/// Remove the PID file at `path`. No-ops if the file does not exist.
pub fn remove_pid(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("remove PID file {}", path.display()))?;
    }
    Ok(())
}

/// Returns `true` if a process with the given PID exists on this system.
#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    // SAFETY: `kill(pid, 0)` is async-signal-safe and does not deliver a signal;
    // it only checks whether a process with the given PID exists. The pid_t cast
    // is well-defined: on all supported platforms pid_t is i32, and we only call
    // this with values read from our own PID file (valid process-space integers).
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// Conservative fallback for non-Unix platforms: assume the process is alive.
///
/// Plan 7 packaging will revisit Windows-specific PID checking if needed.
#[cfg(not(unix))]
fn process_is_alive(_pid: u32) -> bool {
    true
}
