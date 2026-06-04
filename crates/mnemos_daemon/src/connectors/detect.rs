//! Detection probes: is a tool installed? Either its binary is on PATH or one
//! of its known config paths exists.

use std::path::Path;

/// True if `name` resolves to an executable on `PATH`.
pub fn binary_on_path(name: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(name);
        candidate.is_file()
    })
}

/// True if any of the given paths exists.
pub fn any_path_exists(paths: &[&Path]) -> bool {
    paths.iter().any(|p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn binary_on_path_finds_seeded_binary() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("faketool");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        let prev = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", dir.path());
        assert!(binary_on_path("faketool"));
        assert!(!binary_on_path("definitely-not-a-real-tool-xyz"));
        std::env::set_var("PATH", prev);
    }

    #[test]
    fn any_path_exists_detects_present_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("config");
        std::fs::write(&f, "x").unwrap();
        let missing = PathBuf::from("/no/such/path/xyz");
        assert!(any_path_exists(&[&f, &missing]));
        assert!(!any_path_exists(&[&missing]));
    }
}
