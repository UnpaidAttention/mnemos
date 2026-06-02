# AI-Tool Auto-Connect Wizard — Design

- **Date:** 2026-06-02
- **Status:** Approved (brainstorming) — pending spec review → implementation plan
- **Components:** `crates/mnemos_daemon` (connectors module + REST), `desktop/src` (Connections UI), `adapters/` (new `antigravity-cli/`, descriptor data)
- **Goal:** Make getting started frictionless: the desktop app auto-detects AI tools installed on the machine, shows their connection status, and — with one confirmation — writes the MCP config (and session-start memory hint) into each tool for the user, instead of copy-pasting JSON.

## Problem

Today connecting an AI tool to Mnemos is manual: the first-run wizard shows a JSON snippet (`{"mcpServers":{"mnemos":{"command":"mnemos-mcp-stdio"}}}`) and points at `adapters/*/` READMEs. The user must find the right config file per tool, edit it correctly, and (for Claude Code) also append a `CLAUDE.md` fragment. This is the biggest remaining setup friction.

## Decisions (locked in brainstorming)

1. **Scope = all existing adapters represented**, split by capability (see Descriptor Kinds). Auto-connect for the detectable CLIs; manual tiles for the SDK/wrapper integrations.
2. **Antigravity CLI is first-class.** Gemini CLI is being shut down **2026-06-18** (auth returns `410 Gone` after); Google's replacement is Antigravity CLI, which still supports MCP but via a dedicated `mcp_config.json`. v1 adds an `antigravity-cli` connector and marks `gemini-cli` **deprecated**.
3. **Connect = preview-diff → confirm → apply** (backup + atomic write + idempotent merge). Never write blind.
4. **Include the session-start memory hint** (e.g. Claude Code `CLAUDE.md` fragment) as part of connect — a tool's descriptor may have **multiple edits**, and is "Connected" only when all are present.
5. **Logic lives in the daemon** (Rust), exposed via REST, so the UI and a future `mnemos connect` CLI share it.

## Descriptor Kinds

A tool is described by a `ToolConnector` in a registry. Two kinds:

- **Detectable + writable** — `claude-code`, `codex`, `antigravity-cli`, `gemini-cli` (legacy/deprecated). Has a detection probe and one or more **config edits** that can be auto-applied.
- **Manual** — `generic-mcp`, `openai-functions`, `hermes`, `openclaw`. No standard installed-config to write; rendered as info tiles with the copy-snippet (today's behavior). The wizard is honest that these are manual.

## Connector model

```
ToolConnector {
  id: &str,                 // "claude-code"
  display_name: &str,
  kind: Detectable | Manual,
  deprecated: Option<&str>, // e.g. "Shutting down 2026-06-18 — migrate to Antigravity CLI"
  detect: fn() -> Installed,        // binary on PATH and/or config dir present
  edits: Vec<ConfigEdit>,           // empty for Manual
  manual_snippet: Option<Snippet>,  // for Manual tiles / fallback display
}

ConfigEdit {
  target: PathResolver,             // resolves the file path (honoring XDG/home)
  strategy: JsonMerge | MarkedBlock,
  // JsonMerge: merge a JSON object (the mnemos MCP entry) into a JSON config, idempotent.
  // MarkedBlock: insert/replace a block delimited by
  //   <!-- mnemos:start --> ... <!-- mnemos:end --> in a markdown/text file.
  payload: ...,                     // the JSON value or the text block
  is_present: fn(&str) -> bool,     // already applied?
}
```

**Claude Code** descriptor edits:
1. `JsonMerge` into the Claude Code MCP config (`~/.claude.json` or the MCP servers file — exact path verified at implementation) — adds `mnemos` → `{ "command": "mnemos-mcp-stdio" }`.
2. `MarkedBlock` into `~/.claude/CLAUDE.md` — the `adapters/claude-code/CLAUDE.md.fragment` content, so Claude consults `mnemos://working` each session.

Other detectable tools: the `JsonMerge` MCP edit, plus a `MarkedBlock` hint into their instruction file where one exists (Codex `AGENTS.md`, Gemini `GEMINI.md`, Antigravity's equivalent — each verified at implementation; if a tool has no instruction-file convention, it simply has only the MCP edit).

**Token stays out of written files:** edits reference the `mnemos-mcp-stdio` command, which auto-reads `~/.config/mnemos/token`. No secret is written into any tool config.

## Daemon REST API

| Method/Path | Purpose | Response |
|-------------|---------|----------|
| `GET /v1/connectors` | List all connectors with live status | `[{ id, display_name, kind, deprecated, installed, connected: full\|partial\|none, edits: [{target, present}] }]` |
| `POST /v1/connectors/{id}/preview` | Compute the diff to apply | `{ edits: [{ target, before_excerpt, after_excerpt, diff }] }` |
| `POST /v1/connectors/{id}/connect` | Back up + apply all edits | `{ connected: "full", applied: [...] }` |
| `POST /v1/connectors/{id}/disconnect` | Remove only mnemos-added content | `{ connected: "none" }` |

All under the existing bearer-auth middleware (these read/write the user's home files; same trust boundary as the daemon).

## Data flow (connect)

UI `GET /v1/connectors` → render tiles with status badges → user clicks **Connect** → `POST .../preview` returns per-edit before/after + diff → UI shows the confirm dialog (all edits for that tool) → user confirms → `POST .../connect`:
1. For each edit: read target (or treat missing as empty), back up to `<file>.mnemos.bak`, compute merged content (JSON merge or marked-block insert), write atomically (temp + rename).
2. If any edit fails: roll back already-applied edits from their backups, return an error naming what failed.
3. Re-evaluate `is_present` for all edits → return `connected`.

**Disconnect** reverses: JSON edits remove the `mnemos` key; MarkedBlock edits strip the `mnemos:start..end` block; leaves all other content intact.

## UI

A single **`Connections`** component, used in two places:
- **First-run wizard** — replaces the static "Connect your AI tools" snippet step with the live detector (detected tools listed with one-click Connect).
- **Settings → Connections** — a dedicated page to connect/disconnect anytime.

Each tile: tool name, status badge (**Installed** / **Connected ✓** / **Partially connected** / **Deprecated ⚠** / **Not installed**), and Connect/Disconnect actions. Connect opens the preview-diff confirm. Manual tiles show the copy-snippet with target path. A top-level note flags `mnemos-mcp-stdio` if it isn't resolvable on PATH (the written configs depend on it).

## Error handling

- Tool not installed → tile shows "Not installed", no Connect.
- Config file unreadable / malformed JSON → clear error, never overwrite; offer the manual snippet as fallback.
- Permission denied on a target → report path + reason; other edits for that tool are rolled back (all-or-nothing per connect).
- Re-connect is idempotent (JSON key replaced not duplicated; marked block replaced).
- `mnemos-mcp-stdio` missing → warn before connect (config would reference a missing command).

## Testing

- **Unit (Rust), per connector:** detection (present/absent), `is_present`, JSON-merge idempotency, marked-block insert/replace/strip, malformed-config rejection, backup creation, atomic write, disconnect leaves foreign content intact. Use fixture config files in tempdirs.
- **Integration:** the four REST endpoints against a fixture HOME (env-overridden paths).
- **UI:** tile states (each badge), preview-confirm flow, manual tile, error surfacing.

## Antigravity / Gemini specifics

- Create `adapters/antigravity-cli/` (README + config template) targeting Antigravity's `mcp_config.json`; verify the exact path/format during implementation (Antigravity is new + closed-source — confirm against its current docs before finalizing the descriptor).
- `gemini-cli` connector: `deprecated = "Gemini CLI shuts down 2026-06-18; migrate to Antigravity CLI"`, de-emphasized in the UI, still connectable if detected.

## Scope boundary / out of scope

- Auto-connect for the 4 detectable CLIs (claude-code, codex, antigravity-cli, gemini-cli-legacy); manual tiles for generic-mcp / openai-functions / hermes / openclaw.
- Session-start hint included for tools with an instruction-file convention; tools without one get only the MCP edit.
- NOT in scope: managing tool installation (we detect, never install tools); non-CLI IDE plugins beyond what the adapters cover; Windows/macOS path specifics (Linux-first, consistent with the current release).

## Dependencies / sources

- Gemini→Antigravity transition: [Google Developers Blog](https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/), [The Register (2026-05-20)](https://www.theregister.com/ai-ml/2026/05/20/bye-bye-gemini-cli-google-nudges-devs-toward-antigravity/5243605).
- Reuses existing `adapters/*` config templates as the source of truth for each tool's snippet.
