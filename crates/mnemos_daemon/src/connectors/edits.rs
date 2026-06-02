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

pub const BLOCK_START: &str = "<!-- mnemos:start -->";
pub const BLOCK_END: &str = "<!-- mnemos:end -->";

/// Insert or replace the marked block containing `body` in `doc`. The block is
/// delimited by BLOCK_START/BLOCK_END so it can be detected and removed cleanly.
pub fn marked_block_apply(doc: &str, body: &str) -> String {
    let block = format!("{BLOCK_START}\n{}\n{BLOCK_END}", body.trim_end());
    if let (Some(s), Some(e)) = (doc.find(BLOCK_START), doc.find(BLOCK_END)) {
        let end = e + BLOCK_END.len();
        let mut out = String::with_capacity(doc.len());
        out.push_str(&doc[..s]);
        out.push_str(&block);
        out.push_str(&doc[end..]);
        out
    } else {
        let sep = if doc.is_empty() || doc.ends_with('\n') { "" } else { "\n" };
        format!("{doc}{sep}\n{block}\n")
    }
}

/// True if the marked block is present.
pub fn marked_block_present(doc: &str) -> bool {
    doc.contains(BLOCK_START) && doc.contains(BLOCK_END)
}

/// Remove the marked block. No-op if absent. Preserves surrounding content.
pub fn marked_block_remove(doc: &str) -> String {
    match (doc.find(BLOCK_START), doc.find(BLOCK_END)) {
        (Some(s), Some(e)) => {
            let end = (e + BLOCK_END.len()).min(doc.len());
            let mut out = String::new();
            out.push_str(doc[..s].trim_end_matches('\n'));
            out.push_str(&doc[end..]);
            out
        }
        _ => doc.to_string(),
    }
}

/// Insert the table parsed from `value_toml` at `table_path`/`key` in a TOML
/// document. Creates intermediate tables. Idempotent (replaces the key).
pub fn toml_merge(doc: &str, table_path: &[&str], key: &str, value_toml: &str) -> Result<String, String> {
    let mut root: toml::Value = if doc.trim().is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        doc.parse().map_err(|e: toml::de::Error| e.to_string())?
    };
    if !root.is_table() {
        return Err("config root is not a TOML table".into());
    }
    let mut cur = &mut root;
    for seg in table_path {
        cur = cur
            .as_table_mut()
            .ok_or_else(|| format!("`{seg}` parent is not a table"))?
            .entry(seg.to_string())
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    }
    let entry: toml::Value = value_toml.parse().map_err(|e: toml::de::Error| e.to_string())?;
    cur.as_table_mut()
        .ok_or_else(|| "target is not a table".to_string())?
        .insert(key.to_string(), entry);
    toml::to_string_pretty(&root).map_err(|e| e.to_string())
}

/// True if `table_path`/`key` exists in the TOML document.
pub fn toml_has(doc: &str, table_path: &[&str], key: &str) -> bool {
    let root: toml::Value = match doc.parse() { Ok(v) => v, Err(_) => return false };
    let mut cur = &root;
    for seg in table_path {
        match cur.get(seg) { Some(v) => cur = v, None => return false }
    }
    cur.get(key).is_some()
}

/// Remove `table_path`/`key` from the TOML document. No-op if absent.
pub fn toml_remove(doc: &str, table_path: &[&str], key: &str) -> Result<String, String> {
    if doc.trim().is_empty() { return Ok(doc.to_string()); }
    let mut root: toml::Value = doc.parse().map_err(|e: toml::de::Error| e.to_string())?;
    let mut cur = &mut root;
    for seg in table_path {
        match cur.as_table_mut().and_then(|t| t.get_mut(*seg)) {
            Some(v) => cur = v,
            None => return toml::to_string_pretty(&root).map_err(|e| e.to_string()),
        }
    }
    if let Some(t) = cur.as_table_mut() { t.remove(key); }
    toml::to_string_pretty(&root).map_err(|e| e.to_string())
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

    #[test]
    fn marked_block_apply_is_idempotent_and_removable() {
        let original = "# My CLAUDE.md\n\nmy own notes\n";
        let once = marked_block_apply(original, "hint body");
        assert!(marked_block_present(&once));
        assert!(once.contains("my own notes"), "preserves user content");
        let twice = marked_block_apply(&once, "hint body");
        assert_eq!(once.matches(BLOCK_START).count(), 1);
        assert_eq!(twice.matches(BLOCK_START).count(), 1, "no duplicate block");
        let removed = marked_block_remove(&twice);
        assert!(!marked_block_present(&removed));
        assert!(removed.contains("my own notes"), "user content survives removal");
    }

    #[test]
    fn toml_merge_inserts_nested_table_idempotently_and_preserves_keys() {
        let start = "model = \"gpt\"\n\n[mcp_servers.other]\ncommand = \"x\"\n";
        let once = toml_merge(start, &["mcp_servers"], "mnemos", "command = \"mnemos-mcp-stdio\"\nargs = []").unwrap();
        assert!(toml_has(&once, &["mcp_servers"], "mnemos"));
        assert!(toml_has(&once, &["mcp_servers"], "other"), "keeps other server");
        assert!(once.contains("model = \"gpt\""), "keeps top-level key");
        // idempotent
        let twice = toml_merge(&once, &["mcp_servers"], "mnemos", "command = \"mnemos-mcp-stdio\"\nargs = []").unwrap();
        let parsed: toml::Value = twice.parse().unwrap();
        assert_eq!(parsed["mcp_servers"].as_table().unwrap().len(), 2);
        // removable
        let removed = toml_remove(&twice, &["mcp_servers"], "mnemos").unwrap();
        assert!(!toml_has(&removed, &["mcp_servers"], "mnemos"));
        assert!(toml_has(&removed, &["mcp_servers"], "other"));
    }
}
