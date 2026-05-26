# Mnemos × Claude Code

Plug Mnemos into Claude Code so every session has shared persistent memory.

## One-time setup

1. Install + start the daemon:
   ```bash
   cargo install --path crates/mnemos_daemon       # gets you `mnemosd` + `mnemos-mcp-stdio`
   mnemos daemon start
   mnemos daemon status                           # confirm healthy
   ```

2. Register the MCP server with Claude Code. Edit
   `~/.config/claude-code/mcp_servers.json` (create if absent) and add the
   `mnemos` entry from this directory's `claude_mcp_config.json`.

3. Append the fragment in `CLAUDE.md.fragment` to your `~/.claude/CLAUDE.md`.
   It tells Claude to consult the `mnemos://working` resource at the start of
   every session.

4. Restart Claude Code. In a session, ask `What do you know about me from
   Mnemos?` — Claude should respond with whatever you've remembered (or
   nothing on a fresh vault).

## What this enables

- `claude` can call `remember(body, …)`, `recall(query)`, `forget(id)`,
  `list_memories()`, `get_memory(id)` as MCP tools.
- Claude pulls `mnemos://working` at session start (auto, via the system
  prompt fragment).
- Cross-session continuity — anything one session stores is immediately
  visible to the next.

## Troubleshooting

- `mnemos daemon status` reports `not running`: run `mnemos daemon start`.
- Claude says "tool not found": confirm the MCP entry was added; restart Claude
  Code; check `mnemos daemon logs` for the daemon's view of the connection.
- Tool calls return 401 Unauthorized: the token in `claude_mcp_config.json`
  must match `~/.config/mnemos/token`. Re-run `cat ~/.config/mnemos/token`
  and paste it into the config.
