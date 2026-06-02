//! File-edit primitives for tool connectors: safe backup + atomic write, and
//! the JSON-merge / marked-block strategies used to add or remove the mnemos
//! entry from a tool's config files.

use std::path::Path;

/// Back up `path` to `<path>.mnemos.bak` if it exists and no backup exists yet.
pub fn backup(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let bak = path.with_extension(format!(
        "{}.mnemos.bak",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    if !bak.exists() {
        std::fs::copy(path, &bak).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Write `contents` to `path` atomically (temp file in same dir + rename).
pub fn atomic_write(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("mnemos.tmp");
    std::fs::write(&tmp, contents).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Insert `value` at `pointer`/`key` in a JSON document string, creating
/// intermediate objects as needed. Returns the new document string. Idempotent
/// (replaces an existing key, never duplicates).
pub fn json_merge(doc: &str, pointer: &[&str], key: &str, value: &serde_json::Value) -> Result<String, String> {
    let mut root: serde_json::Value =
        if doc.trim().is_empty() { serde_json::json!({}) } else { serde_json::from_str(doc).map_err(|e| e.to_string())? };
    if !root.is_object() {
        return Err("config root is not a JSON object".into());
    }
    let mut cur = &mut root;
    for seg in pointer {
        cur = cur
            .as_object_mut()
            .ok_or_else(|| format!("`{seg}` parent is not an object"))?
            .entry(seg.to_string())
            .or_insert_with(|| serde_json::json!({}));
    }
    cur.as_object_mut()
        .ok_or_else(|| "target is not an object".to_string())?
        .insert(key.to_string(), value.clone());
    serde_json::to_string_pretty(&root).map_err(|e| e.to_string())
}

/// True if `pointer`/`key` exists in the JSON document.
pub fn json_has(doc: &str, pointer: &[&str], key: &str) -> bool {
    let root: serde_json::Value = match serde_json::from_str(doc) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mut cur = &root;
    for seg in pointer {
        match cur.get(seg) {
            Some(v) => cur = v,
            None => return false,
        }
    }
    cur.get(key).is_some()
}

/// Remove `pointer`/`key` from the JSON document; returns new string. No-op if absent.
pub fn json_remove(doc: &str, pointer: &[&str], key: &str) -> Result<String, String> {
    let mut root: serde_json::Value =
        if doc.trim().is_empty() { return Ok(doc.to_string()); } else { serde_json::from_str(doc).map_err(|e| e.to_string())? };
    let mut cur = &mut root;
    for seg in pointer {
        match cur.as_object_mut().and_then(|o| o.get_mut(*seg)) {
            Some(v) => cur = v,
            None => return serde_json::to_string_pretty(&root).map_err(|e| e.to_string()),
        }
    }
    if let Some(o) = cur.as_object_mut() {
        o.remove(key);
    }
    serde_json::to_string_pretty(&root).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_merge_inserts_nested_and_is_idempotent() {
        let v = json!({"command":"mnemos-mcp-stdio"});
        let once = json_merge("{}", &["mcp", "servers"], "mnemos", &v).unwrap();
        assert!(json_has(&once, &["mcp", "servers"], "mnemos"));
        let twice = json_merge(&once, &["mcp", "servers"], "mnemos", &v).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&twice).unwrap();
        assert_eq!(parsed["mcp"]["servers"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn json_merge_preserves_existing_keys() {
        let start = r#"{"mcpServers":{"other":{"command":"x"}}}"#;
        let out = json_merge(start, &["mcpServers"], "mnemos", &json!({"command":"mnemos-mcp-stdio"})).unwrap();
        assert!(json_has(&out, &["mcpServers"], "other"));
        assert!(json_has(&out, &["mcpServers"], "mnemos"));
    }

    #[test]
    fn json_remove_strips_only_mnemos() {
        let start = r#"{"mcpServers":{"other":{},"mnemos":{}}}"#;
        let out = json_remove(start, &["mcpServers"], "mnemos").unwrap();
        assert!(!json_has(&out, &["mcpServers"], "mnemos"));
        assert!(json_has(&out, &["mcpServers"], "other"));
    }

    #[test]
    fn backup_and_atomic_write_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("c.json");
        std::fs::write(&f, "{}").unwrap();
        backup(&f).unwrap();
        assert!(f.with_extension("json.mnemos.bak").exists());
        atomic_write(&f, "{\"a\":1}").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "{\"a\":1}");
    }
}
