use super::detect::{any_path_exists, binary_on_path};
use super::{ConfigEdit, EditStrategy, ToolConnector, ToolKind};
use std::path::PathBuf;

fn claude_settings_path() -> PathBuf {
    home().join(".claude").join("settings.json")
}

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

// Hook value JSON for Claude Code settings.json hooks.
// Shape: {"matcher":"","hooks":[{"type":"command","command":"<cmd>"}]}
const HOOK_SESSION_START_JSON: &str =
    r#"{"matcher":"","hooks":[{"type":"command","command":"mnemos hook session-start"}]}"#;
const HOOK_USER_PROMPT_JSON: &str =
    r#"{"matcher":"","hooks":[{"type":"command","command":"mnemos hook user-prompt"}]}"#;
const HOOK_SESSION_END_JSON: &str =
    r#"{"matcher":"","hooks":[{"type":"command","command":"mnemos hook session-end"}]}"#;

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
                // Edit 0: MCP server entry in ~/.claude.json
                ConfigEdit {
                    target: claude_mcp_path,
                    strategy: EditStrategy::JsonMerge {
                        pointer: &["mcpServers"],
                        key: "mnemos",
                        value_json: r#"{"type":"stdio","command":"mnemos-mcp-stdio","args":[],"env":{"MNEMOS_DAEMON_URL":"http://127.0.0.1:7423"}}"#,
                    },
                },
                // Edit 1: session-start hint block in ~/.claude/CLAUDE.md
                ConfigEdit {
                    target: claude_md_path,
                    strategy: EditStrategy::MarkedBlock { body: HINT },
                },
                // Edits 2-4: hooks in ~/.claude/settings.json
                ConfigEdit {
                    target: claude_settings_path,
                    strategy: EditStrategy::JsonArrayAppend {
                        pointer: &["hooks", "SessionStart"],
                        match_command: "mnemos hook session-start",
                        value_json: HOOK_SESSION_START_JSON,
                    },
                },
                ConfigEdit {
                    target: claude_settings_path,
                    strategy: EditStrategy::JsonArrayAppend {
                        pointer: &["hooks", "UserPromptSubmit"],
                        match_command: "mnemos hook user-prompt",
                        value_json: HOOK_USER_PROMPT_JSON,
                    },
                },
                ConfigEdit {
                    target: claude_settings_path,
                    strategy: EditStrategy::JsonArrayAppend {
                        pointer: &["hooks", "SessionEnd"],
                        match_command: "mnemos hook session-end",
                        value_json: HOOK_SESSION_END_JSON,
                    },
                },
            ],
            manual_snippet: None,
            requires_service: true,
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
            requires_service: false,
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
            requires_service: false,
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
            requires_service: false,
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
        requires_service: false,
    }
}

pub fn by_id(id: &str) -> Option<ToolConnector> {
    registry().into_iter().find(|c| c.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::{AutonomyStatus, Connected};
    use std::path::PathBuf;

    #[test]
    fn registry_has_expected_tools_kinds_and_strategies() {
        let r = registry();
        let claude = r.iter().find(|c| c.id == "claude-code").unwrap();
        assert_eq!(claude.edits.len(), 5, "MCP + CLAUDE.md hint + 3 hooks");
        assert!(claude.requires_service, "claude-code needs daemon service");
        // Verify the hook edits are JsonArrayAppend
        assert!(matches!(
            claude.edits[2].strategy,
            EditStrategy::JsonArrayAppend { .. }
        ));
        assert!(matches!(
            claude.edits[3].strategy,
            EditStrategy::JsonArrayAppend { .. }
        ));
        assert!(matches!(
            claude.edits[4].strategy,
            EditStrategy::JsonArrayAppend { .. }
        ));
        let codex = r.iter().find(|c| c.id == "codex").unwrap();
        assert!(
            matches!(codex.edits[0].strategy, EditStrategy::TomlMerge { .. }),
            "Codex uses TOML"
        );
        assert!(!codex.requires_service);
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

    // ---- autonomy_status tests using tempfiles --------------------------------
    //
    // `target` must be a bare `fn() -> PathBuf` (not a closure), so we route
    // through env vars — one uniquely-named var per test to avoid thread races.
    // A process-wide mutex serialises all three tests that touch the env.

    static AUTONOMY_ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Build a connector-under-test whose edits target the files currently named
    /// in the three env vars: MCP_VAR, MD_VAR, SETTINGS_VAR.
    fn make_claude_like_connector(
        mcp_var: &str,
        md_var: &str,
        settings_var: &str,
    ) -> crate::connectors::ToolConnector {
        // We need three distinct fn-pointer families. We use three separate
        // env-var names so that when the mutex ensures tests run one at a time
        // the right files are read by each fn.
        //
        // The env var names are set by each test before calling this function.
        // We copy the var name into a thread_local so the bare fn can read it.
        // Since tests are serialised by AUTONOMY_ENV_MUTEX this is safe.
        std::env::set_var("_MNEMOS_TEST_ACTIVE_MCP", mcp_var);
        std::env::set_var("_MNEMOS_TEST_ACTIVE_MD", md_var);
        std::env::set_var("_MNEMOS_TEST_ACTIVE_SETTINGS", settings_var);

        fn mcp_target() -> PathBuf {
            let var = std::env::var("_MNEMOS_TEST_ACTIVE_MCP").unwrap();
            PathBuf::from(std::env::var(var).unwrap_or_else(|_| "/dev/null".into()))
        }
        fn md_target() -> PathBuf {
            let var = std::env::var("_MNEMOS_TEST_ACTIVE_MD").unwrap();
            PathBuf::from(std::env::var(var).unwrap_or_else(|_| "/dev/null".into()))
        }
        fn settings_target() -> PathBuf {
            let var = std::env::var("_MNEMOS_TEST_ACTIVE_SETTINGS").unwrap();
            PathBuf::from(std::env::var(var).unwrap_or_else(|_| "/dev/null".into()))
        }

        crate::connectors::ToolConnector {
            id: "claude-code-test",
            display_name: "Claude Code Test",
            kind: crate::connectors::ToolKind::Detectable,
            deprecated: None,
            detect: || true,
            edits: vec![
                crate::connectors::ConfigEdit {
                    target: mcp_target,
                    strategy: crate::connectors::EditStrategy::JsonMerge {
                        pointer: &["mcpServers"],
                        key: "mnemos",
                        value_json: r#"{"command":"mnemos-mcp-stdio"}"#,
                    },
                },
                crate::connectors::ConfigEdit {
                    target: md_target,
                    strategy: crate::connectors::EditStrategy::MarkedBlock { body: "hint" },
                },
                crate::connectors::ConfigEdit {
                    target: settings_target,
                    strategy: crate::connectors::EditStrategy::JsonArrayAppend {
                        pointer: &["hooks", "SessionStart"],
                        match_command: "mnemos hook session-start",
                        value_json: HOOK_SESSION_START_JSON,
                    },
                },
                crate::connectors::ConfigEdit {
                    target: settings_target,
                    strategy: crate::connectors::EditStrategy::JsonArrayAppend {
                        pointer: &["hooks", "UserPromptSubmit"],
                        match_command: "mnemos hook user-prompt",
                        value_json: HOOK_USER_PROMPT_JSON,
                    },
                },
                crate::connectors::ConfigEdit {
                    target: settings_target,
                    strategy: crate::connectors::EditStrategy::JsonArrayAppend {
                        pointer: &["hooks", "SessionEnd"],
                        match_command: "mnemos hook session-end",
                        value_json: HOOK_SESSION_END_JSON,
                    },
                },
            ],
            manual_snippet: None,
            requires_service: true,
        }
    }

    #[test]
    fn autonomy_status_not_installed_when_no_edits_present() {
        let _guard = AUTONOMY_ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let mcp_f = dir.path().join("claude.json");
        let md_f = dir.path().join("CLAUDE.md");
        let settings_f = dir.path().join("settings.json");
        std::env::set_var("MNEMOS_T_NI_MCP", &mcp_f);
        std::env::set_var("MNEMOS_T_NI_MD", &md_f);
        std::env::set_var("MNEMOS_T_NI_SETTINGS", &settings_f);

        let c =
            make_claude_like_connector("MNEMOS_T_NI_MCP", "MNEMOS_T_NI_MD", "MNEMOS_T_NI_SETTINGS");
        assert_eq!(c.connected(), Connected::None);
        assert_eq!(c.autonomy_status(), AutonomyStatus::NotInstalled);
    }

    #[test]
    fn autonomy_status_connected_when_only_mcp_present() {
        let _guard = AUTONOMY_ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let mcp_f = dir.path().join("claude.json");
        let md_f = dir.path().join("CLAUDE.md");
        let settings_f = dir.path().join("settings.json");
        std::env::set_var("MNEMOS_T_CONN_MCP", &mcp_f);
        std::env::set_var("MNEMOS_T_CONN_MD", &md_f);
        std::env::set_var("MNEMOS_T_CONN_SETTINGS", &settings_f);

        let c = make_claude_like_connector(
            "MNEMOS_T_CONN_MCP",
            "MNEMOS_T_CONN_MD",
            "MNEMOS_T_CONN_SETTINGS",
        );
        // Apply only the MCP edit (edit 0).
        let mcp_content = c.edits[0].rendered().unwrap();
        std::fs::write(&mcp_f, &mcp_content).unwrap();

        assert_eq!(c.connected(), Connected::Partial);
        assert_eq!(c.autonomy_status(), AutonomyStatus::Connected);
    }

    #[test]
    fn autonomy_status_autonomous_when_all_edits_present() {
        let _guard = AUTONOMY_ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let mcp_f = dir.path().join("claude.json");
        let md_f = dir.path().join("CLAUDE.md");
        let settings_f = dir.path().join("settings.json");
        std::env::set_var("MNEMOS_T_AUTO_MCP", &mcp_f);
        std::env::set_var("MNEMOS_T_AUTO_MD", &md_f);
        std::env::set_var("MNEMOS_T_AUTO_SETTINGS", &settings_f);

        let c = make_claude_like_connector(
            "MNEMOS_T_AUTO_MCP",
            "MNEMOS_T_AUTO_MD",
            "MNEMOS_T_AUTO_SETTINGS",
        );
        // Apply all 5 edits sequentially (hooks share the same settings file,
        // so each rendered() call re-reads the current file state).
        let content0 = c.edits[0].rendered().unwrap();
        std::fs::write(&mcp_f, &content0).unwrap();
        let content1 = c.edits[1].rendered().unwrap();
        std::fs::write(&md_f, &content1).unwrap();
        // For settings.json the three hook edits must be applied in sequence.
        let s2 = c.edits[2].rendered().unwrap();
        std::fs::write(&settings_f, &s2).unwrap();
        let s3 = c.edits[3].rendered().unwrap();
        std::fs::write(&settings_f, &s3).unwrap();
        let s4 = c.edits[4].rendered().unwrap();
        std::fs::write(&settings_f, &s4).unwrap();

        assert_eq!(c.connected(), Connected::Full);
        assert_eq!(c.autonomy_status(), AutonomyStatus::Autonomous);
    }
}
