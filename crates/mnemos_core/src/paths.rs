use crate::error::{MnemosError, Result};
use crate::tier::Tier;
use directories::ProjectDirs;
use std::path::{Path, PathBuf};

/// Resolved on-disk paths for a Mnemos vault.
#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub files_dir: PathBuf,
    pub db_path: PathBuf,
    pub quarantine_dir: PathBuf,
    pub archived_dir: PathBuf,
    pub entities_dir: PathBuf,
}

impl Paths {
    /// XDG defaults: `~/.local/share/mnemos/`.
    pub fn default_xdg() -> Result<Self> {
        let dirs = ProjectDirs::from("dev", "mnemos", "mnemos")
            .ok_or_else(|| MnemosError::PathError("could not resolve XDG dirs".into()))?;
        Ok(Self::with_root(dirs.data_dir()))
    }

    pub fn with_root(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            files_dir: root.join("files"),
            db_path: root.join("index.db"),
            quarantine_dir: root.join("files").join("quarantine"),
            archived_dir: root.join("files").join("archived"),
            entities_dir: root.join("files").join("entities"),
        }
    }

    pub fn tier_dir(&self, tier: Tier) -> PathBuf {
        self.files_dir.join(tier.dir_name())
    }

    /// Vault root path passed to `SyncBackend::push/pull`. This is the directory
    /// containing `files/` and `index.db`; sync backends decide what subset to
    /// replicate (typically the `files/` subtree).
    pub fn files_root(&self) -> &Path {
        &self.root
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(&self.files_dir)?;
        for tier in Tier::all() {
            std::fs::create_dir_all(self.tier_dir(*tier))?;
        }
        std::fs::create_dir_all(&self.quarantine_dir)?;
        std::fs::create_dir_all(&self.archived_dir)?;
        std::fs::create_dir_all(&self.entities_dir)?;
        Ok(())
    }
}
