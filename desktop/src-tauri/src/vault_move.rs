//! Validation + execution of a vault directory move. Safety-first: the source
//! is preserved until the destination is confirmed; only the caller
//! (commands::move_vault) removes the old dir after a healthy restart.

use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
pub enum MoveError {
    SamePath,
    TargetNotEmpty(PathBuf),
    SourceMissing(PathBuf),
    Io(String),
}

impl std::fmt::Display for MoveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MoveError::SamePath => write!(f, "new location is the same as the current one"),
            MoveError::TargetNotEmpty(p) => {
                write!(f, "target directory is not empty: {}", p.display())
            }
            MoveError::SourceMissing(p) => write!(f, "current vault not found: {}", p.display()),
            MoveError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// Validate a proposed move. `target` may or may not exist; if it exists it
/// must be an empty directory.
pub fn validate(source: &Path, target: &Path) -> Result<(), MoveError> {
    let src = source.canonicalize().map_err(|_| MoveError::SourceMissing(source.into()))?;
    let tgt_abs = if target.is_absolute() { target.to_path_buf() } else {
        std::env::current_dir().map_err(|e| MoveError::Io(e.to_string()))?.join(target)
    };
    if tgt_abs == src {
        return Err(MoveError::SamePath);
    }
    if tgt_abs.exists() {
        let mut entries = std::fs::read_dir(&tgt_abs).map_err(|e| MoveError::Io(e.to_string()))?;
        if entries.next().is_some() {
            return Err(MoveError::TargetNotEmpty(tgt_abs));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_same_path() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(validate(dir.path(), dir.path()), Err(MoveError::SamePath));
    }

    #[test]
    fn rejects_nonempty_target() {
        let src = tempfile::tempdir().unwrap();
        let tgt = tempfile::tempdir().unwrap();
        std::fs::write(tgt.path().join("x"), b"data").unwrap();
        assert_eq!(
            validate(src.path(), tgt.path()),
            Err(MoveError::TargetNotEmpty(tgt.path().canonicalize().unwrap()))
        );
    }

    #[test]
    fn rejects_missing_source() {
        let tgt = tempfile::tempdir().unwrap();
        let missing = tgt.path().join("does-not-exist");
        assert_eq!(validate(&missing, tgt.path()), Err(MoveError::SourceMissing(missing)));
    }

    #[test]
    fn accepts_new_empty_target() {
        let src = tempfile::tempdir().unwrap();
        let tgt = src.path().join("new-loc");
        assert!(validate(src.path(), &tgt).is_ok());
    }
}
