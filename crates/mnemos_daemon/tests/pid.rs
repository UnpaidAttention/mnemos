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

#[test]
fn second_acquire_errors_when_pid_is_alive() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("mnemosd.pid");
    let _g = PidFile::acquire(&path).unwrap();
    let r = PidFile::acquire(&path);
    assert!(r.is_err(), "second acquire must fail while first is alive");
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
