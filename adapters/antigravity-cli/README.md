# Mnemos × Antigravity CLI

Google's Antigravity CLI is the successor to Gemini CLI (which shuts down
2026-06-18). It supports MCP servers via a dedicated `mcp_config.json`.

## Config location (verified 2026-06)

- **Antigravity CLI:** `~/.gemini/antigravity-cli/mcp_config.json`
  (or `.agents/mcp_config.json` inside an active workspace)
- **Antigravity IDE / general:** `~/.gemini/antigravity/mcp_config.json`

The file holds a single top-level `mcpServers` object (same shape as Gemini
CLI's `settings.json`). Remote HTTP servers use `serverUrl` (not `url`).

## Setup (automatic — recommended)

In the Mnemos desktop app: **Settings → Connections → Antigravity CLI →
Connect**. Mnemos previews the change, backs up the file, and writes the
`mnemos` entry into `mcp_config.json` for you.

## Setup (manual)

Add the `mnemos` server from `mcp_config.json` in this directory to Antigravity's
`mcp_config.json`. The `mnemos-mcp-stdio` command reads the daemon token from
`~/.config/mnemos/token` automatically — no secret goes in the config.

> Antigravity is new and closed-source; if a release moves the config path,
> update the `antigravity-cli` connector descriptor in
> `crates/mnemos_daemon/src/connectors/descriptors.rs` to match.
