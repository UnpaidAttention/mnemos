# Mnemos Autonomy Layer (Claude Code first) — Design

- **Date:** 2026-06-04
- **Status:** Approved (brainstorming) — pending spec review → implementation plan
- **Components:** `crates/mnemos_cli` (new `hook` subcommands), `crates/mnemos_daemon` (session-end distill+prune, recall for hooks), packaging (systemd user service), `crates/mnemos_daemon/src/connectors` (install hooks + service), `desktop/` (management surface + wizard), `adapters/`
- **Goal:** Make Mnemos set-it-and-forget-it. After install + first-run wizards (storage/embedder + connect tools), the user does nothing else: the daemon runs in the background, and every connected tool automatically receives relevant memory at session start + per prompt and has its conversations captured and learned from — with no manual tool calls. The desktop app is the management/review surface, never required for operation.

## Problem

Mnemos today is a working "brain" (bundled embedder, recall, `/v1/sessions` ingestion, reflection/decay/correction-mining, MCP tools) but it is neither **always running** nor **automatically engaged**:
1. The daemon must be started manually (`mnemos daemon start`); no service/autostart.
2. MCP tools are **model-invoked** — there is no MCP mechanism to push memory into a session or force `recall`/`remember`. So MCP alone can never be autonomous.
3. Nothing feeds the ingestion API or injects recall; memory only forms if the model chooses to call `remember`.

The fix is an **autonomy layer** built on each AI tool's **hook system**. Claude Code's hooks (verified against current docs, 2026-06-04) provide exactly the needed points:
- **SessionStart** + **UserPromptSubmit** inject context via `hookSpecificOutput.additionalContext` (no model action).
- **Stop / SessionEnd** expose the full conversation via `transcript_path` (JSONL).
- Hooks live in `~/.claude/settings.json`, are shell commands (JSON on stdin, JSON on stdout, exit 0), and array-merge safely across config scopes.
- MCP resources do **not** auto-load — hooks are the only reliable auto-inject mechanism.

## Decisions (locked in brainstorming)

1. **Hook commands = `mnemos hook <event>` subcommands** of the installed CLI (not loose scripts, not `mcp_tool` hooks). Versioned with the daemon, unit-testable, single install artifact.
2. **Daemon always-on = systemd user service** (`Restart=always`, auto-`enable --now` at package install) + **lazy-spawn fallback** in every `mnemos hook` / `mnemos-mcp-stdio` invocation.
3. **Capture-all → distill → prune raw.** Every connected-tool session is ingested; the pipeline distills durable memories/reflections/corrections; raw chunks are pruned after distillation (bounded vault, no verbatim hoard).
4. **Recall cadence = session-start working set + capped per-prompt relevance recall** (token-budgeted, tunable/disable-able in the app).
5. **Claude Code end-to-end in v1.** Other tools reuse the same `mnemos hook` commands wired to their hook systems (follow-on).

## Architecture

```
Claude Code session                      Mnemos
─────────────────────                    ────────────────────────────
SessionStart  ──► mnemos hook session-start ──► GET working set ──► additionalContext
UserPromptSubmit ─► mnemos hook user-prompt ──► recall(prompt, cap) ─► additionalContext
…conversation…
SessionEnd/Stop ─► mnemos hook session-end ──► POST transcript chunks
                                              ──► /v1/sessions/.../end
                                                    └► reflect + mine_corrections + PRUNE raw
                          (every hook: ensure daemon up — lazy-spawn if down — else fail-open)
```

The daemon runs as a **systemd user service** independent of the desktop app.

## Components

### 1. Daemon always-on
- **`packaging/systemd/mnemosd.service`** (user unit): `ExecStart=/usr/bin/mnemos-daemon`, `Restart=always`, `WantedBy=default.target`, env for bundled embedder. Installed to the per-user systemd dir; package post-install (or a `mnemos service install` CLI subcommand the wizard calls) runs `systemctl --user enable --now mnemosd`. (Packaging can't enable a *user* service from a root rpm scriptlet, so the **wizard/CLI** performs the enable on first run — see First-run.)
- **Lazy-spawn**: a shared `ensure_daemon()` helper (in `mnemos_client` or the CLI) used by `mnemos hook *` and `mnemos-mcp-stdio`: probe `/health`; if down, spawn `mnemos-daemon` detached + wait for health (bounded), else proceed assuming up.

### 2. `mnemos hook` subcommands (`crates/mnemos_cli`)
A new `hook` command group; each reads the Claude Code hook JSON on stdin and writes hook JSON on stdout, **fail-open** (any error → empty/zero context, exit 0):
- **`mnemos hook session-start`** — parse `cwd`/`source`; ensure daemon; fetch working set (identity + `mnemos:hardened` rules + workspace context) scoped to the `cwd` workspace; print `{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext": "<rendered>"}}`. Skip heavy work on `source == "clear"` if desired.
- **`mnemos hook user-prompt`** — parse `prompt` + `cwd`; ensure daemon; `recall(prompt, k, workspace)` with a **token budget** (default ~300 tokens, configurable); print `additionalContext` with the top hits. Empty output if nothing relevant.
- **`mnemos hook session-end`** — parse `transcript_path` + `cwd`; read the JSONL transcript; map turns → chunks; `POST /v1/sessions` (with workspace) → `/{id}/chunks` (batched) → `/{id}/end`; ingestion triggers reflect + mine + prune. Idempotent per `session_id` (don't double-ingest a transcript already captured).

### 3. Capture → distill → prune pipeline (`crates/mnemos_daemon`)
- `/v1/sessions/.../end` already triggers reflect + (PR #3) mine_corrections. Add a **retention/prune step**: after distillation, delete the session's raw chunks (and their vectors) so the vault stores distilled knowledge, not full transcripts. Keep a small per-session summary (existing `sessions.summary`) for provenance.
- **Workspace scoping**: chunks/sessions carry the `cwd`-derived workspace; distilled memories inherit it; recall is workspace + global.
- **Privacy guard**: a config-level capture toggle (`autonomy.capture = on|paused`), per-workspace exclude list, and a redaction pass that drops chunks matching obvious secret patterns before storage.

### 4. Connector evolution (extends PR #2 connectors)
A connector descriptor gains a **hooks edit-set** + a **service requirement**. "Connect Claude Code" installs (one click): the MCP entry (existing) + the 3 hook entries in `~/.claude/settings.json` (JSON-merge, marker-tagged for clean removal) + ensures the daemon service (enable user unit). Status becomes **Autonomous ✓** when MCP + hooks + service are all present (vs. **Connected** = MCP only). "Disconnect" removes hooks + MCP entry; offers to disable the service.

### 5. App = management/review surface
Builds on existing memory list / Settings / Connections / Storage:
- **Knowledge view** — browse / search / edit / delete captured memories, hardened rules, and corrections (the "review knowledge entries" surface). Mostly reuses existing memory APIs + the new `/v1/corrections`.
- **Autonomy controls** — capture pause/resume, per-workspace include/exclude, recall budget + per-prompt-recall toggle, "what was captured recently" feed.
- **Connections** — per-tool Autonomous/Connected/Not-installed status; connect/disconnect; service status.
The engine (daemon + hooks) runs with the app closed; the app is the cockpit.

### 6. First-run wizard
Install → launch → wizard: (a) confirm storage/embedder [exists]; (b) **enable background service** (`systemctl --user enable --now mnemosd`, with a clear explanation + fallback to lazy-spawn if systemd unavailable); (c) **Connect your tools** → installs hooks + MCP per tool; (d) "You're set — just use your tools." Matches the user's exact flow.

## Error handling (never break a tool session)
- Every `mnemos hook` command is **fail-open**: daemon unreachable / lazy-spawn failed / recall error / timeout → emit empty `additionalContext` (or nothing) and exit 0. A Mnemos problem must never block or slow a Claude Code session beyond the small hook budget.
- `session-end` capture failures are logged, never block the hook.
- Hook commands are time-bounded well under Claude Code's hook timeout.
- Hook config installation is idempotent + marker-scoped so re-connect/disconnect is clean and never corrupts the user's `settings.json`.

## Testing
- **Unit (CLI):** each `mnemos hook` subcommand — stdin JSON → expected stdout `additionalContext`; fail-open when the daemon is down; transcript JSONL → chunks mapping; session-end idempotency.
- **Unit (daemon):** prune-after-distill removes raw chunks but keeps distilled memories + summary; capture toggle/redaction.
- **Integration:** full loop against a live daemon — session-start injects working set; user-prompt injects recall; session-end ingests a fixture transcript → a memory is formed → raw chunks pruned.
- **Connector:** hook-install JSON-merge into a fixture `settings.json`; Autonomous status; disconnect removes exactly the mnemos hooks.

## Scope / decomposition
Sizable — the implementation plan phases it:
- **Phase A — daemon always-on:** systemd user unit + `ensure_daemon()` lazy-spawn + `mnemos service install/enable`.
- **Phase B — `mnemos hook` commands + capture/distill/prune + workspace scoping + privacy guard.**
- **Phase C — connector installs hooks/service + Autonomous status + app autonomy controls + wizard glue.**
v1 = Claude Code end-to-end. **Out of scope (v1):** Codex/Antigravity hook wiring (reuse the same `mnemos hook` commands later); macOS/Windows service managers (Linux/systemd first, lazy-spawn covers the gap elsewhere); cross-device sync of captured memory.

## Honest limits
"Brain-like" autonomy is bounded by the host's hook points: SessionStart + per-prompt injection is the maximum Claude Code exposes (no true mid-token streaming recall). Captured memory quality depends on the distillation LLM (the bundled embedder handles vectors; reflection/mining need an LLM — without one configured, capture still stores+prunes but distills less). The redaction guard is best-effort, not a guarantee — the privacy posture is "distill + prune raw," not "never sees sensitive text."
