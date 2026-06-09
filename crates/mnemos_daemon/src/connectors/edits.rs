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

/// Write `contents` to `path` atomically using a unique temp file in the same
/// directory, then rename into place.
///
/// Using `tempfile::NamedTempFile` avoids two races that the old fixed-name
/// `.mnemos.tmp` approach suffered from:
///   1. Two concurrent writers targeting the same destination would clobber
///      each other's temp file.
///   2. If the rename failed (e.g. cross-device) the orphaned `.mnemos.tmp`
///      was left on disk; `NamedTempFile` deletes it on drop instead.
pub fn atomic_write(path: &Path, contents: &str) -> Result<(), String> {
    // Determine the directory in which to create the temp file.  The temp file
    // MUST live on the same filesystem as the destination so that the rename is
    // atomic.  `path.parent()` covers the overwhelming majority of cases; if
    // `path` has no parent component (bare filename, extremely unlikely in
    // practice) we fall back to the current directory.
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    use std::io::Write as _;
    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    tmp.write_all(contents.as_bytes())
        .map_err(|e| e.to_string())?;
    // `persist` does the atomic rename; on failure it returns the NamedTempFile
    // back so the caller can inspect the error — and `Drop` cleans up the temp
    // file automatically if this function returns an Err.
    tmp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Insert `value` at `pointer`/`key` in a JSON document string, creating
/// intermediate objects as needed. Returns the new document string. Idempotent
/// (replaces an existing key, never duplicates).
pub fn json_merge(
    doc: &str,
    pointer: &[&str],
    key: &str,
    value: &serde_json::Value,
) -> Result<String, String> {
    let mut root: serde_json::Value = if doc.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(doc).map_err(|e| e.to_string())?
    };
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
    let mut root: serde_json::Value = if doc.trim().is_empty() {
        return Ok(doc.to_string());
    } else {
        serde_json::from_str(doc).map_err(|e| e.to_string())?
    };
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

// ---- JsonArrayAppend helpers ------------------------------------------------

/// Append `value` to the array at `pointer` in a JSON document, creating
/// intermediate objects as needed. Idempotent: skips if an existing element
/// has `.hooks[].command == match_command`.
pub fn json_array_append(
    doc: &str,
    pointer: &[&str],
    match_command: &str,
    value: &serde_json::Value,
) -> Result<String, String> {
    let mut root: serde_json::Value = if doc.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(doc).map_err(|e| e.to_string())?
    };
    if !root.is_object() {
        return Err("config root is not a JSON object".into());
    }
    // Navigate / create the pointer path, ensuring intermediate nodes are objects.
    let mut cur = &mut root;
    let last = pointer.last().copied().unwrap_or("");
    let parents = if pointer.is_empty() {
        &[] as &[&str]
    } else {
        &pointer[..pointer.len() - 1]
    };
    for seg in parents {
        cur = cur
            .as_object_mut()
            .ok_or_else(|| format!("`{seg}` parent is not an object"))?
            .entry(seg.to_string())
            .or_insert_with(|| serde_json::json!({}));
    }
    // The leaf must be an array (create it if absent).
    let arr = cur
        .as_object_mut()
        .ok_or_else(|| "target parent is not an object".to_string())?
        .entry(last.to_string())
        .or_insert_with(|| serde_json::json!([]));
    if !arr.is_array() {
        return Err(format!(
            "`{last}` exists but is not a JSON array; refusing to overwrite"
        ));
    }
    // invariant: the entry was either just created as `json!([])` by
    // `or_insert_with` above, or it passed the `is_array()` check — so
    // `as_array_mut()` is infallible here.
    let arr = arr
        .as_array_mut()
        .expect("invariant: arr passed is_array() check above");
    // Idempotency: skip if any element already has our command.
    if arr.iter().any(|el| element_has_command(el, match_command)) {
        return serde_json::to_string_pretty(&root).map_err(|e| e.to_string());
    }
    arr.push(value.clone());
    serde_json::to_string_pretty(&root).map_err(|e| e.to_string())
}

/// True iff `pointer` path contains an array that has an element whose
/// `.hooks[].command == match_command`.
pub fn json_array_has(doc: &str, pointer: &[&str], match_command: &str) -> bool {
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
    cur.as_array()
        .map(|arr| arr.iter().any(|el| element_has_command(el, match_command)))
        .unwrap_or(false)
}

/// Remove the element(s) whose `.hooks[].command == match_command` from the
/// array at `pointer`. Prunes the array (and parent key) if it becomes empty.
pub fn json_array_remove(
    doc: &str,
    pointer: &[&str],
    match_command: &str,
) -> Result<String, String> {
    let mut root: serde_json::Value = if doc.trim().is_empty() {
        return Ok(doc.to_string());
    } else {
        serde_json::from_str(doc).map_err(|e| e.to_string())?
    };
    // Navigate to the parent of the leaf.
    let (parents, leaf) = if pointer.is_empty() {
        return serde_json::to_string_pretty(&root).map_err(|e| e.to_string());
    } else {
        (&pointer[..pointer.len() - 1], pointer[pointer.len() - 1])
    };
    let mut cur = &mut root;
    for seg in parents {
        match cur.as_object_mut().and_then(|o| o.get_mut(*seg)) {
            Some(v) => cur = v,
            None => return serde_json::to_string_pretty(&root).map_err(|e| e.to_string()),
        }
    }
    if let Some(arr_val) = cur.as_object_mut().and_then(|o| o.get_mut(leaf)) {
        if let Some(arr) = arr_val.as_array_mut() {
            arr.retain(|el| !element_has_command(el, match_command));
        }
        // Prune empty array key.
        if arr_val.as_array().map(|a| a.is_empty()).unwrap_or(false) {
            // invariant: `cur` was reached via `as_object_mut()` navigation
            // above, so it is always an object here.
            cur.as_object_mut()
                .expect("invariant: cur reached via as_object_mut() navigation")
                .remove(leaf);
        }
    }
    serde_json::to_string_pretty(&root).map_err(|e| e.to_string())
}

/// Helper: does element `el` have a hook entry with `command == cmd`?
/// Matches the Claude Code hooks shape: `{"matcher":"","hooks":[{"type":"command","command":"..."}]}`
fn element_has_command(el: &serde_json::Value, cmd: &str) -> bool {
    if let Some(hooks) = el.get("hooks").and_then(|h| h.as_array()) {
        hooks
            .iter()
            .any(|h| h.get("command").and_then(|c| c.as_str()) == Some(cmd))
    } else {
        false
    }
}

pub const BLOCK_START: &str = "<!-- mnemos:start -->";
pub const BLOCK_END: &str = "<!-- mnemos:end -->";

/// Insert or replace the marked block containing `body` in `doc`. The block is
/// delimited by BLOCK_START/BLOCK_END so it can be detected and removed cleanly.
///
/// Malformed-document policy (single marker present, or markers in reversed order):
///   - A well-formed pair (`start.is_some() && end.is_some() && start_idx < end_idx`)
///     triggers an in-place replace.
///   - NEITHER marker present → append a fresh block (normal first-write path).
///   - Any other case (exactly one marker, or end before start) is malformed.  To
///     avoid creating a second `BLOCK_START` or producing corrupted output we first
///     strip any lone `BLOCK_START` or `BLOCK_END` lines from the document, then
///     append a fresh, well-formed block.
pub fn marked_block_apply(doc: &str, body: &str) -> String {
    let block = format!("{BLOCK_START}\n{}\n{BLOCK_END}", body.trim_end());
    let start_idx = doc.find(BLOCK_START);
    let end_idx = doc.find(BLOCK_END);

    match (start_idx, end_idx) {
        // Well-formed pair in correct order — replace in-place.
        (Some(s), Some(e)) if s < e => {
            let end = e + BLOCK_END.len();
            let mut out = String::with_capacity(doc.len());
            out.push_str(&doc[..s]);
            out.push_str(&block);
            out.push_str(&doc[end..]);
            out
        }
        // Neither marker present — first-write append.
        (None, None) => {
            let sep = if doc.is_empty() || doc.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            format!("{doc}{sep}\n{block}\n")
        }
        // Malformed: exactly one marker, or end before start.  Strip any lone
        // marker lines to avoid duplicates, then append a clean block.
        _ => {
            let cleaned: String = doc
                .lines()
                .filter(|l| *l != BLOCK_START && *l != BLOCK_END)
                .collect::<Vec<_>>()
                .join("\n");
            // Re-add the trailing newline that `lines()` strips.
            let cleaned = if doc.ends_with('\n') {
                format!("{cleaned}\n")
            } else {
                cleaned
            };
            let sep = if cleaned.is_empty() || cleaned.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            format!("{cleaned}{sep}\n{block}\n")
        }
    }
}

/// True if the marked block is present AND well-formed (start before end).
pub fn marked_block_present(doc: &str) -> bool {
    match (doc.find(BLOCK_START), doc.find(BLOCK_END)) {
        (Some(s), Some(e)) => s < e,
        _ => false,
    }
}

/// Remove the marked block. No-op if absent or malformed. Preserves surrounding content.
///
/// Only removes when both markers are found AND start appears before end; a
/// malformed document (single marker, or reversed order) is returned unchanged
/// to avoid emitting corrupted slices.
pub fn marked_block_remove(doc: &str) -> String {
    match (doc.find(BLOCK_START), doc.find(BLOCK_END)) {
        (Some(s), Some(e)) if s < e => {
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
pub fn toml_merge(
    doc: &str,
    table_path: &[&str],
    key: &str,
    value_toml: &str,
) -> Result<String, String> {
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
    let entry: toml::Value = value_toml
        .parse()
        .map_err(|e: toml::de::Error| e.to_string())?;
    cur.as_table_mut()
        .ok_or_else(|| "target is not a table".to_string())?
        .insert(key.to_string(), entry);
    toml::to_string_pretty(&root).map_err(|e| e.to_string())
}

/// True if `table_path`/`key` exists in the TOML document.
pub fn toml_has(doc: &str, table_path: &[&str], key: &str) -> bool {
    let root: toml::Value = match doc.parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mut cur = &root;
    for seg in table_path {
        match cur.get(seg) {
            Some(v) => cur = v,
            None => return false,
        }
    }
    cur.get(key).is_some()
}

/// Remove `table_path`/`key` from the TOML document. No-op if absent.
pub fn toml_remove(doc: &str, table_path: &[&str], key: &str) -> Result<String, String> {
    if doc.trim().is_empty() {
        return Ok(doc.to_string());
    }
    let mut root: toml::Value = doc.parse().map_err(|e: toml::de::Error| e.to_string())?;
    let mut cur = &mut root;
    for seg in table_path {
        match cur.as_table_mut().and_then(|t| t.get_mut(*seg)) {
            Some(v) => cur = v,
            None => return toml::to_string_pretty(&root).map_err(|e| e.to_string()),
        }
    }
    if let Some(t) = cur.as_table_mut() {
        t.remove(key);
    }
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
        let out = json_merge(
            start,
            &["mcpServers"],
            "mnemos",
            &json!({"command":"mnemos-mcp-stdio"}),
        )
        .unwrap();
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
        assert!(
            removed.contains("my own notes"),
            "user content survives removal"
        );
    }

    #[test]
    fn toml_merge_inserts_nested_table_idempotently_and_preserves_keys() {
        let start = "model = \"gpt\"\n\n[mcp_servers.other]\ncommand = \"x\"\n";
        let once = toml_merge(
            start,
            &["mcp_servers"],
            "mnemos",
            "command = \"mnemos-mcp-stdio\"\nargs = []",
        )
        .unwrap();
        assert!(toml_has(&once, &["mcp_servers"], "mnemos"));
        assert!(
            toml_has(&once, &["mcp_servers"], "other"),
            "keeps other server"
        );
        assert!(once.contains("model = \"gpt\""), "keeps top-level key");
        // idempotent
        let twice = toml_merge(
            &once,
            &["mcp_servers"],
            "mnemos",
            "command = \"mnemos-mcp-stdio\"\nargs = []",
        )
        .unwrap();
        let parsed: toml::Value = twice.parse().unwrap();
        assert_eq!(parsed["mcp_servers"].as_table().unwrap().len(), 2);
        // removable
        let removed = toml_remove(&twice, &["mcp_servers"], "mnemos").unwrap();
        assert!(!toml_has(&removed, &["mcp_servers"], "mnemos"));
        assert!(toml_has(&removed, &["mcp_servers"], "other"));
    }

    #[test]
    fn marked_block_handles_malformed_single_marker() {
        // --- only BLOCK_START present (no end) ---
        let only_start = format!("# header\n{BLOCK_START}\nsome content\n");

        // marked_block_present must be false — a lone start is not a valid block.
        assert!(
            !marked_block_present(&only_start),
            "lone start marker must not count as present"
        );

        // marked_block_remove must return the document unchanged.
        assert_eq!(
            marked_block_remove(&only_start),
            only_start,
            "remove on lone-start doc must be a no-op"
        );

        // marked_block_apply must not produce two BLOCK_START occurrences.
        let applied = marked_block_apply(&only_start, "new body");
        assert_eq!(
            applied.matches(BLOCK_START).count(),
            1,
            "apply on lone-start doc must not duplicate BLOCK_START"
        );
        assert!(
            applied.contains(BLOCK_END),
            "apply on lone-start doc must produce a closing BLOCK_END"
        );
        assert!(marked_block_present(&applied), "result must be well-formed");

        // --- BLOCK_END before BLOCK_START (reversed order) ---
        let reversed = format!("# header\n{BLOCK_END}\nmiddle\n{BLOCK_START}\nfooter\n");

        // marked_block_present must be false — end before start is not valid.
        assert!(
            !marked_block_present(&reversed),
            "reversed markers must not count as present"
        );

        // marked_block_remove must return the document unchanged.
        assert_eq!(
            marked_block_remove(&reversed),
            reversed,
            "remove on reversed-marker doc must be a no-op"
        );
    }

    // ---- JsonArrayAppend tests -----------------------------------------------

    fn session_start_hook() -> serde_json::Value {
        json!({
            "matcher": "",
            "hooks": [{"type": "command", "command": "mnemos hook session-start"}]
        })
    }

    #[test]
    fn json_array_append_creates_entry_in_empty_doc() {
        let out = json_array_append(
            "{}",
            &["hooks", "SessionStart"],
            "mnemos hook session-start",
            &session_start_hook(),
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(
            arr[0]["hooks"][0]["command"].as_str().unwrap(),
            "mnemos hook session-start"
        );
    }

    #[test]
    fn json_array_append_is_idempotent() {
        let once = json_array_append(
            "{}",
            &["hooks", "SessionStart"],
            "mnemos hook session-start",
            &session_start_hook(),
        )
        .unwrap();
        let twice = json_array_append(
            &once,
            &["hooks", "SessionStart"],
            "mnemos hook session-start",
            &session_start_hook(),
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&twice).unwrap();
        assert_eq!(
            parsed["hooks"]["SessionStart"].as_array().unwrap().len(),
            1,
            "apply twice must not duplicate entry"
        );
    }

    #[test]
    fn json_array_has_detects_presence() {
        let doc = json_array_append(
            "{}",
            &["hooks", "SessionStart"],
            "mnemos hook session-start",
            &session_start_hook(),
        )
        .unwrap();
        assert!(json_array_has(
            &doc,
            &["hooks", "SessionStart"],
            "mnemos hook session-start"
        ));
        assert!(!json_array_has(
            &doc,
            &["hooks", "SessionStart"],
            "mnemos hook user-prompt"
        ));
        assert!(!json_array_has(
            "{}",
            &["hooks", "SessionStart"],
            "mnemos hook session-start"
        ));
    }

    #[test]
    fn json_array_remove_deletes_only_our_entry_and_leaves_user_hooks() {
        // Start with one user hook + one mnemos hook in the same array.
        let user_hook = json!({
            "matcher": "",
            "hooks": [{"type": "command", "command": "echo hello"}]
        });
        let doc_with_user =
            json_array_append("{}", &["hooks", "SessionStart"], "echo hello", &user_hook).unwrap();
        let doc_with_both = json_array_append(
            &doc_with_user,
            &["hooks", "SessionStart"],
            "mnemos hook session-start",
            &session_start_hook(),
        )
        .unwrap();
        // Verify both are present.
        assert!(json_array_has(
            &doc_with_both,
            &["hooks", "SessionStart"],
            "echo hello"
        ));
        assert!(json_array_has(
            &doc_with_both,
            &["hooks", "SessionStart"],
            "mnemos hook session-start"
        ));
        // Remove only the mnemos hook.
        let removed = json_array_remove(
            &doc_with_both,
            &["hooks", "SessionStart"],
            "mnemos hook session-start",
        )
        .unwrap();
        assert!(
            !json_array_has(
                &removed,
                &["hooks", "SessionStart"],
                "mnemos hook session-start"
            ),
            "mnemos hook must be gone"
        );
        assert!(
            json_array_has(&removed, &["hooks", "SessionStart"], "echo hello"),
            "user hook must survive"
        );
    }

    #[test]
    fn json_array_remove_prunes_empty_array_key() {
        let doc = json_array_append(
            "{}",
            &["hooks", "SessionStart"],
            "mnemos hook session-start",
            &session_start_hook(),
        )
        .unwrap();
        let removed = json_array_remove(
            &doc,
            &["hooks", "SessionStart"],
            "mnemos hook session-start",
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&removed).unwrap();
        // Array key should be pruned when empty.
        assert!(
            parsed["hooks"].get("SessionStart").is_none(),
            "empty array key must be pruned"
        );
    }

    // Fix 1: leaf is a non-array scalar — must Err, must not mutate.
    #[test]
    fn json_array_append_returns_err_when_leaf_is_non_array_scalar() {
        // `hooks.SessionStart` is a string, not an array.
        let original = r#"{"hooks":{"SessionStart":"oops"}}"#;
        let result = json_array_append(
            original,
            &["hooks", "SessionStart"],
            "mnemos hook session-start",
            &session_start_hook(),
        );
        assert!(
            result.is_err(),
            "must return Err when leaf exists as a non-array value"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("SessionStart"),
            "error message should name the offending key; got: {err}"
        );
        assert!(
            err.contains("not a JSON array"),
            "error message should say 'not a JSON array'; got: {err}"
        );
        // Crucially: the original content must be untouched (the apply path
        // must not have written anything before we returned the Err).
        // We verify this by re-parsing: the key must still be the string "oops".
        let reparsed: serde_json::Value = serde_json::from_str(original).unwrap();
        assert_eq!(
            reparsed["hooks"]["SessionStart"].as_str(),
            Some("oops"),
            "original value must be untouched after Err return"
        );
    }

    // Fix 4: malformed JSON input — all three array helpers must return Err cleanly.
    #[test]
    fn json_array_helpers_return_err_on_malformed_json() {
        let bad = "{not json";
        let hook = session_start_hook();
        let cmd = "mnemos hook session-start";
        let ptr: &[&str] = &["hooks", "SessionStart"];

        let append_result = json_array_append(bad, ptr, cmd, &hook);
        assert!(
            append_result.is_err(),
            "json_array_append must Err on malformed input"
        );

        // json_array_has returns bool — false on parse error, no panic.
        let has_result = std::panic::catch_unwind(|| json_array_has(bad, ptr, cmd));
        assert!(
            has_result.is_ok(),
            "json_array_has must not panic on malformed input"
        );
        assert!(
            !has_result.unwrap(),
            "json_array_has must return false on malformed input"
        );

        let remove_result = json_array_remove(bad, ptr, cmd);
        assert!(
            remove_result.is_err(),
            "json_array_remove must Err on malformed input"
        );
    }
}
