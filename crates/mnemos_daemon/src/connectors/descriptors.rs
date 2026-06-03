use super::detect::{any_path_exists, binary_on_path};
use super::{ConfigEdit, EditStrategy, ToolConnector, ToolKind};
use std::path::PathBuf;

fn home() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    // Fall back to the platform home dir so an unset/empty HOME doesn't resolve
    // tool configs to relative paths in the daemon's CWD.
    directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_default()
}

// ---- VERIFIED config paths (per each tool's current docs, 2026-06) ----
fn claude_mcp_path() -> PathBuf {
    home().join(".claude.json")
} // JSON, mcpServers
fn claude_md_path() -> PathBuf {
    home().join(".claude").join("CLAUDE.md")
}
fn codex_config_path() -> PathBuf {
    home().join(".codex").join("config.toml")
} // TOML, [mcp_servers.*]
fn codex_agents_path() -> PathBuf {
    home().join(".codex").join("AGENTS.md")
}
fn gemini_settings_path() -> PathBuf {
    home().join(".gemini").join("settings.json")
} // JSON, mcpServers
fn antigravity_dir() -> PathBuf {
    home().join(".gemini").join("antigravity-cli")
}
fn antigravity_mcp_path() -> PathBuf {
    antigravity_dir().join("mcp_config.json")
} // JSON, mcpServers

const HINT: &str = "## Mnemos persistent memory\n\nThis session has a persistent memory server (Mnemos) registered as an MCP provider. At the start of every session, read the `mnemos://working` resource — it contains identity facts and active project context.\n\nWhen the user states a durable preference, project context, or rule not obvious from the codebase, call `remember(...)` so it persists across sessions.";

pub fn registry() -> Vec<ToolConnector> {
    vec![
        ToolConnector {
            id: "claude-code",
            display_name: "Claude Code",
            kind: ToolKind::Detectable,
            deprecated: None,
            detect: || {
                binary_on_path("claude")
                    || any_path_exists(&[&claude_mcp_path(), &home().join(".claude")])
            },
            edits: vec![
                ConfigEdit {
                    target: claude_mcp_path,
                    strategy: EditStrategy::JsonMerge {
                        pointer: &["mcpServers"],
                        key: "mnemos",
                        value_json: r#"{"type":"stdio","command":"mnemos-mcp-stdio","args":[],"env":{"MNEMOS_DAEMON_URL":"http://127.0.0.1:7423"}}"#,
                    },
                },
                ConfigEdit {
                    target: claude_md_path,
                    strategy: EditStrategy::MarkedBlock { body: HINT },
                },
            ],
            manual_snippet: None,
        },
        ToolConnector {
            id: "codex",
            display_name: "Codex",
            kind: ToolKind::Detectable,
            deprecated: None,
            detect: || binary_on_path("codex") || any_path_exists(&[&home().join(".codex")]),
            edits: vec![
                ConfigEdit {
                    target: codex_config_path,
                    strategy: EditStrategy::TomlMerge {
                        table_path: &["mcp_servers"],
                        key: "mnemos",
                        value_toml: "command = \"mnemos-mcp-stdio\"\nargs = []",
                    },
                },
                ConfigEdit {
                    target: codex_agents_path,
                    strategy: EditStrategy::MarkedBlock { body: HINT },
                },
            ],
            manual_snippet: None,
        },
        ToolConnector {
            id: "antigravity-cli",
            display_name: "Antigravity CLI",
            kind: ToolKind::Detectable,
            deprecated: None,
            detect: || binary_on_path("antigravity") || any_path_exists(&[&antigravity_dir()]),
            edits: vec![ConfigEdit {
                target: antigravity_mcp_path,
                strategy: EditStrategy::JsonMerge {
                    pointer: &["mcpServers"],
                    key: "mnemos",
                    value_json: r#"{"command":"mnemos-mcp-stdio"}"#,
                },
            }],
            manual_snippet: None,
        },
        ToolConnector {
            id: "gemini-cli",
            display_name: "Gemini CLI",
            kind: ToolKind::Detectable,
            deprecated: Some("Gemini CLI shuts down 2026-06-18 — migrate to Antigravity CLI"),
            detect: || binary_on_path("gemini") || any_path_exists(&[&gemini_settings_path()]),
            edits: vec![ConfigEdit {
                target: gemini_settings_path,
                strategy: EditStrategy::JsonMerge {
                    pointer: &["mcpServers"],
                    key: "mnemos",
                    value_json: r#"{"command":"mnemos-mcp-stdio"}"#,
                },
            }],
            manual_snippet: None,
        },
        manual(
            "generic-mcp",
            "Generic MCP client",
            "your MCP client config",
            r#"{"mcpServers":{"mnemos":{"command":"mnemos-mcp-stdio"}}}"#,
        ),
        manual(
            "openai-functions",
            "OpenAI function-calling",
            "adapters/openai-functions/schema.json",
            "see adapters/openai-functions/schema.json",
        ),
        manual(
            "hermes",
            "Hermes agent",
            "adapters/hermes-agent/",
            "see adapters/hermes-agent/README.md",
        ),
        manual(
            "openclaw",
            "OpenClaw",
            "adapters/openclaw/",
            "see adapters/openclaw/README.md",
        ),
    ]
}

fn manual(
    id: &'static str,
    name: &'static str,
    target_hint: &'static str,
    snippet: &'static str,
) -> ToolConnector {
    ToolConnector {
        id,
        display_name: name,
        kind: ToolKind::Manual,
        deprecated: None,
        detect: || false,
        edits: vec![],
        manual_snippet: Some((target_hint, snippet)),
    }
}

pub fn by_id(id: &str) -> Option<ToolConnector> {
    registry().into_iter().find(|c| c.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_expected_tools_kinds_and_strategies() {
        let r = registry();
        let claude = r.iter().find(|c| c.id == "claude-code").unwrap();
        assert_eq!(claude.edits.len(), 2, "MCP + CLAUDE.md hint");
        let codex = r.iter().find(|c| c.id == "codex").unwrap();
        assert!(
            matches!(codex.edits[0].strategy, EditStrategy::TomlMerge { .. }),
            "Codex uses TOML"
        );
        assert!(r
            .iter()
            .find(|c| c.id == "gemini-cli")
            .unwrap()
            .deprecated
            .is_some());
        assert!(r.iter().any(|c| c.id == "antigravity-cli"));
        assert!(r
            .iter()
            .any(|c| c.id == "generic-mcp" && c.kind == ToolKind::Manual));
    }
}
