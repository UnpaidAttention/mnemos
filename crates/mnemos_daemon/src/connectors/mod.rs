pub mod detect;
pub mod edits;
pub mod descriptors;

use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    /// Detectable and writable (we can auto-connect).
    Detectable,
    /// SDK/wrapper integration shown as a manual tile.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Connected {
    Full,
    Partial,
    None,
}

/// One file edit a connector performs.
pub struct ConfigEdit {
    /// Resolves the target file path (HOME-relative). Fn pointer for testability.
    pub target: fn() -> PathBuf,
    pub strategy: EditStrategy,
}

pub enum EditStrategy {
    /// Insert `value` at `pointer`/`key` in a JSON config.
    JsonMerge { pointer: &'static [&'static str], key: &'static str, value_json: &'static str },
    /// Insert a marked block of `body` into a markdown/text file.
    MarkedBlock { body: &'static str },
    /// Insert the table parsed from `value_toml` at `table_path`/`key` in a TOML config.
    TomlMerge { table_path: &'static [&'static str], key: &'static str, value_toml: &'static str },
}

pub struct ToolConnector {
    pub id: &'static str,
    pub display_name: &'static str,
    pub kind: ToolKind,
    /// Some(reason) if the tool is deprecated.
    pub deprecated: Option<&'static str>,
    /// Detection probe.
    pub detect: fn() -> bool,
    /// Edits (empty for Manual).
    pub edits: Vec<ConfigEdit>,
    /// For Manual tiles (and fallback display): (target_hint, snippet).
    pub manual_snippet: Option<(&'static str, &'static str)>,
}

impl ConfigEdit {
    /// Read the current target file ("" if missing).
    pub fn read(&self) -> String {
        let p = (self.target)();
        std::fs::read_to_string(&p).unwrap_or_default()
    }
    pub fn path(&self) -> PathBuf { (self.target)() }
    pub fn is_present(&self) -> bool {
        let doc = self.read();
        match &self.strategy {
            EditStrategy::JsonMerge { pointer, key, .. } => edits::json_has(&doc, pointer, key),
            EditStrategy::MarkedBlock { .. } => edits::marked_block_present(&doc),
            EditStrategy::TomlMerge { table_path, key, .. } => edits::toml_has(&doc, table_path, key),
        }
    }
    /// Compute the post-apply contents without writing.
    pub fn rendered(&self) -> Result<String, String> {
        let doc = self.read();
        match &self.strategy {
            EditStrategy::JsonMerge { pointer, key, value_json } => {
                let v: serde_json::Value = serde_json::from_str(value_json).map_err(|e| e.to_string())?;
                edits::json_merge(&doc, pointer, key, &v)
            }
            EditStrategy::MarkedBlock { body } => Ok(edits::marked_block_apply(&doc, body)),
            EditStrategy::TomlMerge { table_path, key, value_toml } => edits::toml_merge(&doc, table_path, key, value_toml),
        }
    }
    pub fn removed(&self) -> Result<String, String> {
        let doc = self.read();
        match &self.strategy {
            EditStrategy::JsonMerge { pointer, key, .. } => edits::json_remove(&doc, pointer, key),
            EditStrategy::MarkedBlock { .. } => Ok(edits::marked_block_remove(&doc)),
            EditStrategy::TomlMerge { table_path, key, .. } => edits::toml_remove(&doc, table_path, key),
        }
    }
}

impl ToolConnector {
    pub fn installed(&self) -> bool { (self.detect)() }
    pub fn connected(&self) -> Connected {
        if self.edits.is_empty() { return Connected::None; }
        let present = self.edits.iter().filter(|e| e.is_present()).count();
        if present == 0 { Connected::None }
        else if present == self.edits.len() { Connected::Full }
        else { Connected::Partial }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_target() -> PathBuf {
        PathBuf::from(std::env::var("MNEMOS_TEST_EDIT_FILE").unwrap())
    }

    #[test]
    fn connected_reflects_edit_presence() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("c.json");
        std::env::set_var("MNEMOS_TEST_EDIT_FILE", &f);
        let c = ToolConnector {
            id: "t", display_name: "T", kind: ToolKind::Detectable, deprecated: None,
            detect: || true,
            edits: vec![ConfigEdit {
                target: tmp_target,
                strategy: EditStrategy::JsonMerge { pointer: &["mcpServers"], key: "mnemos", value_json: "{\"command\":\"mnemos-mcp-stdio\"}" },
            }],
            manual_snippet: None,
        };
        assert_eq!(c.connected(), Connected::None);
        std::fs::write(&f, c.edits[0].rendered().unwrap()).unwrap();
        assert_eq!(c.connected(), Connected::Full);
    }
}
