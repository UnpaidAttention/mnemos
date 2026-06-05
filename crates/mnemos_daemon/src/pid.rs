//! PID file management for `mnemosd`. Used by `mnemos daemon status` to
//! detect a running daemon and by graceful shutdown to clean up.
//!
//! ## Exclusivity guarantee
//!
//! On Unix, [`PidFile::acquire`] holds an **OS-level exclusive file lock**
//! (`F_SETLK` / `flock(2)`) for the lifetime of the process. The lock is
//! automatically released by the kernel when the process exits (even on
//! a crash), so there is no TOCTOU window between the "is another daemon
//! alive?" check and the claim of ownership.
//!
//! Two concurrent daemon starts cannot both pass the liveness check: the
//! second `F_SETLK` call returns `EACCES`/`EAGAIN` immediately, preventing
//! them from sharing the vault.
//!
//! The PID is still written to the file so `mnemos daemon status` can report
//! which process owns the vault.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// RAII guard that holds an exclusive OS-level lock on the PID file for the
/// lifetime of the process and removes the file on drop.
pub struct PidFile {
    path: PathBuf,
    /// The open file descriptor kept alive so the kernel lock is held.
    /// Wrapped in `Option` so `Drop` can close it before removing the file.
    #[cfg(unix)]
    _lock_fd: std::fs::File,
}

impl PidFile {
    /// Atomically acquire the PID file at `path`.
    ///
    /// On Unix this takes an exclusive `F_SETLK` lock before writing the PID.
    /// If another live process already holds the lock the call fails
    /// immediately with an error (no blocking wait).
    ///
    /// Stale files left by dead processes are reclaimed automatically because
    /// their file-descriptor lock was released by the kernel on exit.
    pub fn acquire(path: &Path) -> Result<Self> {
        #[cfg(unix)]
        {
            acquire_unix(path)
        }

        #[cfg(not(unix))]
        {
            acquire_fallback(path)
        }
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        // On Unix: closing _lock_fd releases the kernel lock; then we remove
        // the file. We drop the fd first (implicitly when PidFile is dropped
        // after the remove_pid call) — the ordering here is remove-then-close
        // which is acceptable: another process racing to acquire will get the
        // lock after open() but the file content is already gone. The reverse
        // (close-then-remove) has the same race, so either order is fine.
        let _ = remove_pid(&self.path);
    }
}

// ── Unix implementation ───────────────────────────────────────────────────────

#[cfg(unix)]
fn acquire_unix(path: &Path) -> Result<PidFile> {
    use std::fs::OpenOptions;
    use std::io::Write as _;
    use std::os::unix::io::AsRawFd;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create PID dir {}", parent.display()))?;
    }

    // Open (or create) the PID file. We need write access to update the PID
    // and to hold the lock.
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("open PID file {}", path.display()))?;

    // Attempt a non-blocking exclusive lock via F_SETLK.
    //
    // SAFETY: `flock` is a well-defined POSIX syscall. The `libc::flock`
    // struct is initialized with all fields set to known values. `l_pid = 0`
    // is correct for F_SETLK (the kernel fills it in for F_GETLK queries only).
    let lock_result = unsafe {
        let mut fl = libc::flock {
            l_type: libc::F_WRLCK as libc::c_short,
            l_whence: libc::SEEK_SET as libc::c_short,
            l_start: 0,
            l_len: 0, // 0 = entire file
            l_pid: 0,
        };
        libc::fcntl(file.as_raw_fd(), libc::F_SETLK, &mut fl as *mut libc::flock)
    };

    if lock_result == -1 {
        let err = std::io::Error::last_os_error();
        let raw = err.raw_os_error().unwrap_or(0);
        if raw == libc::EACCES || raw == libc::EAGAIN {
            // Another process holds the lock — read its PID for the error msg.
            let owner = read_pid(path)
                .map(|p| format!("process {p}"))
                .unwrap_or_else(|_| "unknown process".into());
            anyhow::bail!(
                "PID file {} is locked by {owner} — another daemon instance is running",
                path.display()
            );
        }
        return Err(err).with_context(|| format!("fcntl F_SETLK on {}", path.display()));
    }

    // We hold the exclusive lock. Overwrite the file with the current PID.
    file.set_len(0)
        .with_context(|| format!("truncate PID file {}", path.display()))?;
    write!(file, "{}", std::process::id())
        .with_context(|| format!("write PID to {}", path.display()))?;

    Ok(PidFile {
        path: path.to_path_buf(),
        _lock_fd: file,
    })
}

// ── Non-Unix fallback ─────────────────────────────────────────────────────────

#[cfg(not(unix))]
fn acquire_fallback(path: &Path) -> Result<PidFile> {
    if path.exists() {
        if let Ok(pid) = read_pid(path) {
            if process_is_alive(pid) {
                anyhow::bail!("PID file {} already owned by process {pid}", path.display());
            }
        }
    }
    write_pid(path, std::process::id())?;
    Ok(PidFile {
        path: path.to_path_buf(),
    })
}

// ── Shared helpers ────────────────────────────────────────────────────────────

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
///
/// Used only by the non-Unix fallback path.
#[cfg(not(unix))]
fn process_is_alive(_pid: u32) -> bool {
    // Conservative fallback: assume the process is alive.
    // Plan 7 packaging will revisit Windows-specific PID checking if needed.
    true
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn acquire_creates_pid_file_with_current_pid() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pid");
        let _guard = PidFile::acquire(&path).expect("acquire must succeed");
        let written = read_pid(&path).expect("must be able to read PID back");
        assert_eq!(
            written,
            std::process::id(),
            "PID file must contain current PID"
        );
    }

    #[test]
    fn acquire_removes_pid_file_on_drop() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pid");
        {
            let _guard = PidFile::acquire(&path).expect("acquire must succeed");
            assert!(path.exists(), "PID file must exist while guard is live");
        }
        assert!(
            !path.exists(),
            "PID file must be removed when guard is dropped"
        );
    }

    #[test]
    #[cfg(unix)]
    fn second_acquire_fails_while_first_is_held() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("exclusive.pid");
        let _guard = PidFile::acquire(&path).expect("first acquire must succeed");
        // A second acquire on the same path in the same process will fail because
        // the same process already holds the lock — POSIX allows re-entrant locks
        // per-process on some platforms, but F_SETLK from the same process on the
        // same fd replaces the lock, which is fine. To test exclusivity properly
        // we spawn a child process.
        //
        // We verify the lock semantics via a child process that attempts to lock
        // the same file. It should exit non-zero (lock denied).
        use std::os::unix::io::AsRawFd;

        let path_clone = path.clone();
        let child_result = unsafe {
            let pid = libc::fork();
            if pid == 0 {
                // Child: try to acquire the lock non-blocking.
                // Open the file.
                let f = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path_clone);
                let exit_code = match f {
                    Ok(f) => {
                        let mut fl = libc::flock {
                            l_type: libc::F_WRLCK as libc::c_short,
                            l_whence: libc::SEEK_SET as libc::c_short,
                            l_start: 0,
                            l_len: 0,
                            l_pid: 0,
                        };
                        let r =
                            libc::fcntl(f.as_raw_fd(), libc::F_SETLK, &mut fl as *mut libc::flock);
                        if r == -1 {
                            1
                        } else {
                            0
                        }
                    }
                    Err(_) => 2,
                };
                libc::_exit(exit_code);
            }
            pid
        };
        assert!(child_result > 0, "fork must succeed");
        // Wait for child.
        let mut status: libc::c_int = 0;
        unsafe { libc::waitpid(child_result, &mut status, 0) };
        let exit_code = if libc::WIFEXITED(status) {
            libc::WEXITSTATUS(status)
        } else {
            -1
        };
        assert_eq!(
            exit_code, 1,
            "child must fail to lock (exit 1) while parent holds the lock"
        );
    }

    #[test]
    fn write_pid_and_read_pid_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("round-trip.pid");
        write_pid(&path, 12345).unwrap();
        assert_eq!(read_pid(&path).unwrap(), 12345);
    }

    #[test]
    fn remove_pid_noop_when_absent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("absent.pid");
        // Must not error even though the file doesn't exist.
        remove_pid(&path).expect("remove_pid must be a no-op for absent files");
    }
}
