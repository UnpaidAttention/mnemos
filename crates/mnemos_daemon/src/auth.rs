//! Bearer token issuance + validation. Token lives at
//! `~/.config/mnemos/token` (mode 0600 on Unix).
//!
//! 32 random bytes, hex-encoded → 64-char ASCII string.

use anyhow::{Context, Result};
use rand::RngCore;
use std::path::Path;

const TOKEN_BYTES: usize = 32;

/// Returns the token at `path`, creating it if absent. On Unix, sets mode 0600.
pub fn ensure_token(path: &Path) -> Result<String> {
    if path.exists() {
        return load_token(path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create token dir {}", parent.display()))?;
    }
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    std::fs::write(path, &hex).with_context(|| format!("write token {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(hex)
}

/// Reads and returns the token stored at `path`.
pub fn load_token(path: &Path) -> Result<String> {
    let s =
        std::fs::read_to_string(path).with_context(|| format!("read token {}", path.display()))?;
    Ok(s.trim().to_string())
}

/// Constant-time string equality. Returns false for length-mismatched inputs.
pub fn validate_token(expected: &str, presented: &str) -> bool {
    let a = expected.as_bytes();
    let b = presented.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}
