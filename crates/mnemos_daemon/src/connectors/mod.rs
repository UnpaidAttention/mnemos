pub mod descriptors;
pub mod detect;
pub mod edits;

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

/// Autonomy level for a connected tool.
///
/// - `Autonomous`: MCP entry + all hook edits present (the connector fully
///   auto-injects recall and captures transcripts without user action).
/// - `Connected`: MCP entry present, but one or more hook edits are missing.
/// - `NotInstalled`: no edits are present (tool not configured).
///
/// Service-active check: whether the mnemos daemon service is enabled via
/// systemd is tracked separately in the API response as `requires_service` on
/// the connector descriptor. The daemon cannot shell out to `systemctl` from
/// inside an HTTP handler without a subprocess, so we keep the concepts
/// orthogonal: `AutonomyStatus` answers "are all the file edits in place?";
/// the caller (desktop wizard) handles service enablement when
/// `requires_service == true` and the service is not yet active.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyStatus {
    Autonomous,
    Connected,
    NotInstalled,
}

/// One file edit a connector performs.
pub struct ConfigEdit {
    /// Resolves the target file path (HOME-relative). Fn pointer for testability.
    pub target: fn() -> PathBuf,
    pub strategy: EditStrategy,
}

pub enum EditStrategy {
    /// Insert `value` at `pointer`/`key` in a JSON config.
    JsonMerge {
        pointer: &'static [&'static str],
        key: &'static str,
        value_json: &'static str,
    },
    /// Insert a marked block of `body` into a markdown/text file.
    MarkedBlock { body: &'static str },
    /// Insert the table parsed from `value_toml` at `table_path`/`key` in a TOML config.
    TomlMerge {
        table_path: &'static [&'static str],
        key: &'static str,
        value_toml: &'static str,
    },
    /// Append an object to the JSON array at `pointer` path, idempotently.
    /// Idempotency key: an element is considered "ours" iff it contains a hook
    /// entry whose `command` field equals `match_command`.
    JsonArrayAppend {
        pointer: &'static [&'static str],
        match_command: &'static str,
        value_json: &'static str,
    },
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
    /// True if this connector also needs the mnemos daemon service to be
    /// enabled (e.g. claude-code hooks fire even outside `mnemos` CLI sessions).
    /// The daemon itself does not enable the service — it returns this flag to
    /// the desktop wizard which handles `mnemos service enable` on behalf of
    /// the user.
    pub requires_service: bool,
}

impl ConfigEdit {
    /// Read the current target file ("" if missing).
    pub fn read(&self) -> String {
        let p = (self.target)();
        std::fs::read_to_string(&p).unwrap_or_default()
    }
    pub fn path(&self) -> PathBuf {
        (self.target)()
    }
    pub fn is_present(&self) -> bool {
        let doc = self.read();
        match &self.strategy {
            EditStrategy::JsonMerge { pointer, key, .. } => edits::json_has(&doc, pointer, key),
            EditStrategy::MarkedBlock { .. } => edits::marked_block_present(&doc),
            EditStrategy::TomlMerge {
                table_path, key, ..
            } => edits::toml_has(&doc, table_path, key),
            EditStrategy::JsonArrayAppend {
                pointer,
                match_command,
                ..
            } => edits::json_array_has(&doc, pointer, match_command),
        }
    }
    /// Compute the post-apply contents without writing.
    pub fn rendered(&self) -> Result<String, String> {
        let doc = self.read();
        match &self.strategy {
            EditStrategy::JsonMerge {
                pointer,
                key,
                value_json,
            } => {
                let v: serde_json::Value =
                    serde_json::from_str(value_json).map_err(|e| e.to_string())?;
                edits::json_merge(&doc, pointer, key, &v)
            }
            EditStrategy::MarkedBlock { body } => Ok(edits::marked_block_apply(&doc, body)),
            EditStrategy::TomlMerge {
                table_path,
                key,
                value_toml,
            } => edits::toml_merge(&doc, table_path, key, value_toml),
            EditStrategy::JsonArrayAppend {
                pointer,
                match_command,
                value_json,
            } => {
                let v: serde_json::Value =
                    serde_json::from_str(value_json).map_err(|e| e.to_string())?;
                edits::json_array_append(&doc, pointer, match_command, &v)
            }
        }
    }
    pub fn removed(&self) -> Result<String, String> {
        let doc = self.read();
        match &self.strategy {
            EditStrategy::JsonMerge { pointer, key, .. } => edits::json_remove(&doc, pointer, key),
            EditStrategy::MarkedBlock { .. } => Ok(edits::marked_block_remove(&doc)),
            EditStrategy::TomlMerge {
                table_path, key, ..
            } => edits::toml_remove(&doc, table_path, key),
            EditStrategy::JsonArrayAppend {
                pointer,
                match_command,
                ..
            } => edits::json_array_remove(&doc, pointer, match_command),
        }
    }
}

impl ToolConnector {
    pub fn installed(&self) -> bool {
        (self.detect)()
    }
    pub fn connected(&self) -> Connected {
        if self.edits.is_empty() {
            return Connected::None;
        }
        let present = self.edits.iter().filter(|e| e.is_present()).count();
        if present == 0 {
            Connected::None
        } else if present == self.edits.len() {
            Connected::Full
        } else {
            Connected::Partial
        }
    }

    /// Compute the autonomy status for this connector.
    ///
    /// Contract:
    /// - `Autonomous`: ALL edits present (MCP entry + CLAUDE.md block + all hooks).
    ///   The connector is fully self-operating.
    /// - `Connected`: at least one edit present but not all. Typical case:
    ///   MCP entry wired but hook edits not yet applied.
    /// - `NotInstalled`: no edits present at all.
    ///
    /// This is a pure count of present/total edits — no distinction between
    /// hook edits and other edit types is made here. The desktop wizard uses
    /// this status to decide whether to prompt the user for the extra
    /// `mnemos service enable` step.
    pub fn autonomy_status(&self) -> AutonomyStatus {
        if self.edits.is_empty() {
            return AutonomyStatus::NotInstalled;
        }
        let present_count = self.edits.iter().filter(|e| e.is_present()).count();
        let total = self.edits.len();
        if present_count == 0 {
            AutonomyStatus::NotInstalled
        } else if present_count == total {
            AutonomyStatus::Autonomous
        } else {
            // Some edits present but not all — e.g. MCP entry wired but hooks absent.
            AutonomyStatus::Connected
        }
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
            id: "t",
            display_name: "T",
            kind: ToolKind::Detectable,
            deprecated: None,
            detect: || true,
            edits: vec![ConfigEdit {
                target: tmp_target,
                strategy: EditStrategy::JsonMerge {
                    pointer: &["mcpServers"],
                    key: "mnemos",
                    value_json: "{\"command\":\"mnemos-mcp-stdio\"}",
                },
            }],
            manual_snippet: None,
            requires_service: false,
        };
        assert_eq!(c.connected(), Connected::None);
        std::fs::write(&f, c.edits[0].rendered().unwrap()).unwrap();
        assert_eq!(c.connected(), Connected::Full);
    }
}
