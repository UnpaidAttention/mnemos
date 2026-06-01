//! Minimal `config.toml` editing for the desktop shell. Only touches
//! `vault.root`. Mirrors the read→merge→write the daemon's PUT /v1/config
//! route performs, but standalone (the shell is a separate cargo workspace).

use std::path::{Path, PathBuf};

/// Resolve `config.toml`. Honors `MNEMOS_CONFIG_PATH` (used by the daemon and
/// tests); otherwise `~/.config/mnemos/config.toml`.
pub fn config_path() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("MNEMOS_CONFIG_PATH") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .ok_or_else(|| "could not resolve XDG config dir".to_string())?;
    Ok(dirs.config_dir().join("config.toml"))
}

/// Read the current `vault.root` from `config.toml`, or `None` if unset/missing.
pub fn read_vault_root(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let value: toml::Value = toml::from_str(&text).map_err(|e| e.to_string())?;
    Ok(value
        .get("vault")
        .and_then(|v| v.get("root"))
        .and_then(|r| r.as_str())
        .map(PathBuf::from))
}

/// Set `vault.root` in `config.toml`, preserving all other keys. Creates the
/// file and parent dir if absent.
pub fn write_vault_root(path: &Path, root: &Path) -> Result<(), String> {
    let mut doc: toml::Value = if path.exists() {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        toml::from_str(&text).map_err(|e| e.to_string())?
    } else {
        toml::Value::Table(Default::default())
    };
    let table = doc
        .as_table_mut()
        .ok_or_else(|| "config root is not a table".to_string())?;
    let vault = table
        .entry("vault".to_string())
        .or_insert_with(|| toml::Value::Table(Default::default()));
    vault
        .as_table_mut()
        .ok_or_else(|| "[vault] is not a table".to_string())?
        .insert(
            "root".to_string(),
            toml::Value::String(root.to_string_lossy().into_owned()),
        );
    let text = toml::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_vault_root_and_preserves_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config.toml");
        std::fs::write(&cfg, "[daemon]\nport = 7423\n\n[vault]\nroot = \"/old\"\n").unwrap();

        write_vault_root(&cfg, Path::new("/new/place")).unwrap();

        assert_eq!(read_vault_root(&cfg).unwrap(), Some(PathBuf::from("/new/place")));
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.contains("port = 7423"), "other keys preserved: {text}");
    }

    #[test]
    fn writes_into_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("nested").join("config.toml");
        write_vault_root(&cfg, Path::new("/data")).unwrap();
        assert_eq!(read_vault_root(&cfg).unwrap(), Some(PathBuf::from("/data")));
    }

    #[test]
    fn read_missing_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_vault_root(&dir.path().join("nope.toml")).unwrap(), None);
    }
}
