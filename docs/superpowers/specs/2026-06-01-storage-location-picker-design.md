# Storage Location Picker (UI-driven vault relocation) — Design

- **Date:** 2026-06-01
- **Status:** Approved (brainstorming) — pending spec review → implementation plan
- **Component:** `desktop/` (Tauri shell + React UI), `crates/mnemos_daemon` (config)
- **Goal:** Let the user choose/relocate their vault storage location entirely
  from the desktop UI — no `config.toml`/env editing. First concrete step toward
  the broader principle that **all setup/configuration is doable in the frontend**.

## Problem

Today the vault path (`vault.root`) is only configurable via the `MNEMOS_VAULT`
env var or `config.toml`. The first-run wizard tells the user they can change
the location "in Settings", but Settings has no such control — the copy
over-promises. There is also no native folder picker, and the desktop app does
not manage the daemon at all (the Rust shell only exposes `read_token`), so even
if the path changed, nothing could apply it.

## Decisions (locked during brainstorming)

1. **Semantics = move/migrate.** Changing location relocates the current vault's
   data (markdown files + SQLite index) to the new directory and points the
   daemon there. Single-vault mental model — no multi-vault "which is current?"
   ambiguity. (Switching/opening other vaults is explicitly out of scope.)
2. **App manages the daemon.** The desktop shell gains daemon lifecycle control
   so a move can: stop → relocate → restart at the new path → reconnect, fully
   in-UI. The `mnemos-daemon` sidecar is already declared in `externalBin` but
   currently unused; this wires it up.
3. **Shell orchestrates the move** (not a daemon self-move endpoint). The shell
   owns lifecycle + has filesystem access, so it is the natural orchestrator.
   The daemon stays simple.

## Architecture

```
┌─────────────── Desktop app (Tauri) ───────────────┐
│  React UI (Settings → Storage)                     │
│     │  invoke()                                     │
│     ▼                                               │
│  Rust shell                                         │
│   • daemon supervisor (adopt-or-spawn, stop/start)  │
│   • move_vault orchestration (validate→move→apply)  │
│   • tauri-plugin-dialog (native folder picker)      │
└───────────────────────┬─────────────────────────────┘
                         │ spawns/supervises + REST :7423
                         ▼
                  mnemos-daemon (sidecar)
                         │
                         ▼
                  vault dir (markdown + SQLite)
```

### Components

1. **Daemon supervisor (Rust shell, new module)**
   - On launch: probe `:7423` health + PID file. If a healthy daemon exists,
     **adopt** it (do not spawn a second — avoids port conflict with a
     user-run/systemd daemon). Otherwise **spawn** the `mnemos-daemon` sidecar
     and supervise it (restart-on-crash optional, out of scope for v1).
   - Internal API: `start()`, `stop()` (graceful SIGTERM, wait for exit + port
     release), `status()` (running/adopted/spawned + healthy).
   - On app exit: only stop daemons this shell spawned; never kill an adopted
     (externally-managed) daemon.

2. **`tauri-plugin-dialog`** — added to `Cargo.toml` + a capability entry in
   `desktop/src-tauri/capabilities/default.json`. Provides the native folder
   chooser. Browser/dev fallback: a plain editable text-path field.

3. **New Tauri commands** (in the shell, exposed to the renderer):
   - `pick_vault_dir() -> Option<String>` — opens native folder dialog.
   - `daemon_status() -> DaemonStatus` — for UI state/badges.
   - `move_vault(new_path: String) -> MoveResult` — the orchestration below.

4. **Settings → Storage section (React, new)**
   - Shows current vault path (from `GET /v1/config` `vault.root`).
   - "Change location…" → `pick_vault_dir()` → confirmation dialog showing
     source → target.
   - Progress states: validating → stopping daemon → moving → starting →
     reconnecting → done; plus explicit error states. Disabled/loading states
     throughout (no happy-path-only UI).

## Data flow — the move

`move_vault(newPath)` in the shell:

1. **Validate**
   - Resolve/normalize `newPath`. Reject if it equals current path.
   - Target must be empty or non-existent (it will be created). A non-empty dir
     or an existing vault there → reject with a clear message.
   - Pre-flight free-space check ≥ current vault size.
2. **Stop daemon** (graceful; wait for port release + PID file removal).
3. **Move** the vault directory (markdown + SQLite + WAL/SHM siblings).
   - Prefer atomic `rename` when same filesystem; else copy-then-verify.
   - **Source is preserved until the new location is verified healthy.**
4. **Persist** `vault.root` by writing `config.toml` **directly from the shell**.
   The daemon is stopped at this point, so the HTTP `PUT /v1/config` route cannot
   be used. The shell shares the daemon's `Config` (de)serialization — the
   `config.rs` `Config` type and `default_config_path()` are lifted into
   `mnemos_core` (or a shared crate) so both the daemon route and the shell write
   identical TOML. Single serialization logic, two callers. (The HTTP route
   continues to serve runtime config edits while the daemon is up; only the
   move flow writes directly because it owns the down-window.)
5. **Start daemon** pointed at the new path; **wait for `/v1/doctor` health**.
6. **Finalize**: on healthy new daemon, remove the now-empty old dir (copy mode)
   or it is already gone (rename mode). Return success.

Revert (failed start at new path) uses the same shell-direct `config.toml` write
to restore the old `vault.root`, then restarts — no daemon dependency.

## Error handling (safety-first — user data)

| Failure | Behavior |
|---------|----------|
| Target non-empty / existing vault present | Reject before any change; clear message. |
| Insufficient disk space | Reject at pre-flight. |
| Move fails midway (copy mode) | Abort; source intact; remove partial target; restart daemon at old path. |
| Daemon won't start at new path | Revert `vault.root` to old path, restart at old path, surface error; source still intact. |
| Adopted (external) daemon present | "Change location" requires app-managed daemon; if adopted, show guidance instead of force-stopping someone else's daemon. |

No destructive step (removing the old dir) happens until the new location is
confirmed healthy.

## Testing

- **Unit (Rust):** path validation (equal/non-empty/no-space), move + rollback
  logic, same-fs rename vs cross-fs copy.
- **Integration (Rust):** supervisor adopt-vs-spawn; full stop→move→start cycle
  against a temp vault; revert path on failed start.
- **E2E (Playwright):** Settings → Storage happy path; non-empty-target
  rejection; reconnect after move. Evidence screenshots per state.

## Out of scope (explicit)

- Multiple/named vaults or "open another vault" (switch semantics).
- Daemon crash-supervision/auto-restart beyond the move flow.
- macOS/Windows packaging specifics (Linux-only release for now).
- Full build-out of "every setting in the UI" — tracked as the follow-up below.

## Follow-up (noted, not specced here)

The existing `GET/PUT /v1/config` Settings screen already covers daemon,
embedder, LLM, OpenAI, retrieval, and reflection. With the new daemon supervisor
in place, the natural next step toward "everything configurable in the UI" is an
in-UI **"apply & restart"** affordance for any setting whose
`restart_required_for` is non-empty (embedder backend, daemon port, etc.), so the
user never needs a terminal. To be specced separately.
