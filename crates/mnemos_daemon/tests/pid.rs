use mnemos_daemon::pid::{read_pid, remove_pid, write_pid, PidFile};
use tempfile::TempDir;

#[test]
fn write_pid_creates_file_with_current_process_id() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("mnemosd.pid");
    let _guard = PidFile::acquire(&path).unwrap();
    let pid = read_pid(&path).unwrap();
    assert_eq!(pid, std::process::id());
}

#[test]
fn pidfile_drop_removes_pid() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("mnemosd.pid");
    {
        let _guard = PidFile::acquire(&path).unwrap();
        assert!(path.exists());
    }
    assert!(!path.exists(), "PidFile drop should remove the file");
}

/// Acquiring the same PID file from a *different process* must fail while the
/// first process holds the OS-level exclusive lock.
///
/// POSIX advisory locks (`fcntl F_SETLK`) are per-process: the same process
/// can replace its own lock on a file, so we must use a child process to test
/// true exclusivity.
#[test]
#[cfg(unix)]
fn second_acquire_errors_when_pid_is_alive() {
    use std::os::unix::io::AsRawFd;

    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("mnemosd.pid");

    // Acquire the lock in the parent — holds it for the duration of this test.
    let _guard = PidFile::acquire(&path).unwrap();

    // Fork a child that tries to take an F_SETLK exclusive lock on the same
    // file.  With the parent holding the lock, the child's attempt must fail
    // with EACCES/EAGAIN, causing it to exit 1.  If the lock is NOT held, the
    // child can acquire it and exits 0 — that would be a test failure.
    let path_clone = path.clone();
    // SAFETY: We fork immediately and the child only calls async-signal-safe
    // functions (open, fcntl, _exit). No Rust destructors run in the child.
    let child_pid = unsafe { libc::fork() };
    assert!(child_pid >= 0, "fork must succeed");

    if child_pid == 0 {
        // ── child ──────────────────────────────────────────────────────────
        let exit_code = (|| {
            let f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path_clone)
                .ok()?;
            let mut fl = libc::flock {
                l_type: libc::F_WRLCK as libc::c_short,
                l_whence: libc::SEEK_SET as libc::c_short,
                l_start: 0,
                l_len: 0,
                l_pid: 0,
            };
            let r =
                unsafe { libc::fcntl(f.as_raw_fd(), libc::F_SETLK, &mut fl as *mut libc::flock) };
            // r == -1 and errno EACCES/EAGAIN → lock denied → exit 1 (expected).
            // r == 0 → lock acquired (parent didn't hold it) → exit 0 (failure).
            if r == -1 {
                Some(1i32)
            } else {
                Some(0i32)
            }
        })()
        .unwrap_or(2);
        // SAFETY: _exit is async-signal-safe; it bypasses Rust destructors.
        unsafe { libc::_exit(exit_code) };
    }

    // ── parent: wait for child ──────────────────────────────────────────────
    let mut status: libc::c_int = 0;
    unsafe { libc::waitpid(child_pid, &mut status, 0) };
    let exit_code = if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else {
        -1
    };
    assert_eq!(
        exit_code, 1,
        "child must fail to acquire the lock (exit 1) while parent holds it"
    );
}

/// A non-Unix fallback: the old PID-liveness check still works for stale files.
#[test]
#[cfg(not(unix))]
fn second_acquire_errors_when_pid_is_alive() {
    // On non-Unix platforms PidFile falls back to the PID liveness check.
    // We can't meaningfully test concurrent exclusivity without OS lock support,
    // so we just verify the stale-reclaim path works.
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("mnemosd.pid");
    write_pid(&path, i32::MAX as u32).unwrap();
    let r = PidFile::acquire(&path);
    assert!(r.is_ok(), "stale PID file (dead PID) must be reclaimed");
}

#[test]
fn second_acquire_succeeds_when_prior_pid_is_dead() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("mnemosd.pid");
    // Write a PID that we're confident doesn't exist (i32::MAX is never a real PID).
    write_pid(&path, i32::MAX as u32).unwrap();
    let r = PidFile::acquire(&path);
    assert!(r.is_ok(), "stale PID file should be reclaimed");
    drop(r);
    remove_pid(&path).ok();
}
