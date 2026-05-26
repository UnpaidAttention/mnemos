use mnemos_daemon::auth::{ensure_token, load_token, validate_token};
use tempfile::TempDir;

#[test]
fn ensure_token_writes_32_byte_file_with_mode_0600() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("token");
    let token = ensure_token(&path).unwrap();
    assert_eq!(token.len(), 64); // hex of 32 bytes
    assert!(path.exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }
}

#[test]
fn ensure_token_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("token");
    let a = ensure_token(&path).unwrap();
    let b = ensure_token(&path).unwrap();
    assert_eq!(a, b);
}

#[test]
fn validate_token_uses_constant_time_compare() {
    let token = "abcdef0123456789".repeat(4); // 64 chars
    assert!(validate_token(&token, &token));
    assert!(!validate_token(&token, "wrong"));
    assert!(!validate_token(&token, ""));
}

#[test]
fn load_token_reads_file() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("token");
    let written = ensure_token(&path).unwrap();
    let loaded = load_token(&path).unwrap();
    assert_eq!(written, loaded);
}
