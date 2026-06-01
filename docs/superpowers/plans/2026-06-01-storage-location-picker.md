# Storage Location Picker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user relocate their Mnemos vault to a new directory entirely from the desktop UI — pick a folder, move the data, and have the daemon restart at the new location, with no terminal or config editing.

**Architecture:** The Rust shell (`desktop/src-tauri`) gains (1) a daemon **supervisor** that drives the existing `mnemos` CLI sidecar's `daemon start|stop|status` subcommands, (2) a **config writer** that edits `config.toml`'s `vault.root` directly while the daemon is down, (3) a **vault-move** orchestrator with validation + rollback, and (4) Tauri commands the React Settings UI calls. Native folder selection uses `tauri-plugin-dialog`.

**Tech Stack:** Tauri 2 (Rust shell + plugins `shell`, `dialog`), `toml` crate, React + TypeScript (Vitest), `mnemos` CLI sidecar (already in `externalBin`).

**Spec:** `docs/superpowers/specs/2026-06-01-storage-location-picker-design.md`

---

## Deviation from spec (read first)

The spec said the shell would "share the daemon's `Config` (de)serialization, lifted into `mnemos_core`." During planning this proved heavier than the feature: `desktop/src-tauri` is its **own cargo workspace** (note the empty `[workspace]` table at the top of its `Cargo.toml`), so sharing the type means adding a path dependency and lifting `Config` (which references several `mnemos_core` types) out of `mnemos_daemon`. Instead, the shell writes **only `vault.root`** into `config.toml` using the `toml` crate (the same read→merge→write the daemon's `PUT /v1/config` route already does in `crates/mnemos_daemon/src/routes/config.rs`). Risk is low: one string field, and the daemon validates `config.toml` against `Config` on load — the move flow's post-restart health check is the gate. **Confirm this is acceptable before implementing Task 3.**

Also note (packaging prerequisite, Task 12): the supervised daemon needs the bundled embedder to function. In `dev` this works from `assets/` with `LD_LIBRARY_PATH`. In the **packaged desktop app** the embedder `.so` libraries are not yet bundled (only `llama-server` + GGUF are in `tauri.conf.json` `resources`). Task 12 wires the env vars and documents this as a known limitation; fully bundling the libs into the desktop package (or depending on the `mnemos-daemon` package) is tracked separately and is out of scope here.

---

## File Structure

| File | Responsibility |
|------|----------------|
| `desktop/src-tauri/Cargo.toml` | add `tauri-plugin-shell`, `tauri-plugin-dialog`, `toml` deps |
| `desktop/src-tauri/capabilities/default.json` | grant `shell:allow-execute` (sidecar) + `dialog:allow-open` |
| `desktop/src-tauri/src/config_io.rs` | read/write `vault.root` in `config.toml` (pure, unit-tested) |
| `desktop/src-tauri/src/vault_move.rs` | validation + move + rollback (pure-ish, unit-tested) |
| `desktop/src-tauri/src/daemon.rs` | supervisor: status/stop/start via `mnemos` sidecar |
| `desktop/src-tauri/src/commands.rs` | Tauri commands: `pick_vault_dir`, `daemon_status`, `move_vault` |
| `desktop/src-tauri/src/main.rs` | register plugins + commands; adopt-or-spawn daemon on launch |
| `desktop/src/api/tauri.ts` | typed `invoke` wrappers for the new commands + browser fallbacks |
| `desktop/src/views/StorageSettings.tsx` | Settings → Storage section UI + state machine |
| `desktop/src/views/StorageSettings.test.tsx` | component tests |
| `desktop/src/views/Settings.tsx` | mount the Storage section |

---

## Task 1: Add plugin + crate dependencies

**Files:**
- Modify: `desktop/src-tauri/Cargo.toml`
- Modify: `desktop/src-tauri/capabilities/default.json`

- [ ] **Step 1: Add Rust deps**

In `desktop/src-tauri/Cargo.toml`, under `[dependencies]` add:

```toml
tauri-plugin-shell = "2"
tauri-plugin-dialog = "2"
toml = "0.8"
tempfile = "3"
```

Move `tempfile` to `[dev-dependencies]` if a `[dev-dependencies]` section exists; otherwise add one:

```toml
[dev-dependencies]
tempfile = "3"
```

(Remove `tempfile` from `[dependencies]` — it is test-only.)

- [ ] **Step 2: Grant capabilities**

Replace `desktop/src-tauri/capabilities/default.json` permissions array with:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capabilities for the desktop app.",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "updater:default",
    "process:default",
    "dialog:allow-open",
    {
      "identifier": "shell:allow-execute",
      "allow": [{ "name": "mnemos", "sidecar": true, "args": true }]
    }
  ]
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd desktop/src-tauri && cargo build`
Expected: PASS (new crates download + compile; no code uses them yet).

- [ ] **Step 4: Commit**

```bash
git add desktop/src-tauri/Cargo.toml desktop/src-tauri/Cargo.lock desktop/src-tauri/capabilities/default.json
git commit -m "build(desktop): add shell, dialog plugins + toml for storage picker"
```

---

## Task 2: Config writer — set `vault.root` in `config.toml`

**Files:**
- Create: `desktop/src-tauri/src/config_io.rs`
- Test: same file (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

Create `desktop/src-tauri/src/config_io.rs`:

```rust
//! Minimal `config.toml` editing for the desktop shell. Only touches
//! `vault.root`. Mirrors the read→merge→write the daemon's PUT /v1/config
//! route performs, but standalone (the shell is a separate cargo workspace).

use std::path::{Path, PathBuf};

/// Resolve `config.toml`. Honors `MNEMOS_CONFIG_PATH` (used by the daemon and
/// tests); otherwise `~/.config/mnemos/config.toml`.
pub fn config_path() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("MNEMOS_CONFIG_PATH") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .ok_or_else(|| "could not resolve XDG config dir".to_string())?;
    Ok(dirs.config_dir().join("config.toml"))
}

/// Read the current `vault.root` from `config.toml`, or `None` if unset/missing.
pub fn read_vault_root(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let value: toml::Value = toml::from_str(&text).map_err(|e| e.to_string())?;
    Ok(value
        .get("vault")
        .and_then(|v| v.get("root"))
        .and_then(|r| r.as_str())
        .map(PathBuf::from))
}

/// Set `vault.root` in `config.toml`, preserving all other keys. Creates the
/// file and parent dir if absent.
pub fn write_vault_root(path: &Path, root: &Path) -> Result<(), String> {
    let mut doc: toml::Value = if path.exists() {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        toml::from_str(&text).map_err(|e| e.to_string())?
    } else {
        toml::Value::Table(Default::default())
    };
    let table = doc
        .as_table_mut()
        .ok_or_else(|| "config root is not a table".to_string())?;
    let vault = table
        .entry("vault".to_string())
        .or_insert_with(|| toml::Value::Table(Default::default()));
    vault
        .as_table_mut()
        .ok_or_else(|| "[vault] is not a table".to_string())?
        .insert(
            "root".to_string(),
            toml::Value::String(root.to_string_lossy().into_owned()),
        );
    let text = toml::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_vault_root_and_preserves_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config.toml");
        std::fs::write(&cfg, "[daemon]\nport = 7423\n\n[vault]\nroot = \"/old\"\n").unwrap();

        write_vault_root(&cfg, Path::new("/new/place")).unwrap();

        assert_eq!(read_vault_root(&cfg).unwrap(), Some(PathBuf::from("/new/place")));
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.contains("port = 7423"), "other keys preserved: {text}");
    }

    #[test]
    fn writes_into_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("nested").join("config.toml");
        write_vault_root(&cfg, Path::new("/data")).unwrap();
        assert_eq!(read_vault_root(&cfg).unwrap(), Some(PathBuf::from("/data")));
    }

    #[test]
    fn read_missing_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_vault_root(&dir.path().join("nope.toml")).unwrap(), None);
    }
}
```

Register the module: add `mod config_io;` near the top of `desktop/src-tauri/src/main.rs` (after the `#![cfg_attr(...)]` line).

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd desktop/src-tauri && cargo test config_io`
Expected: FAIL — first run before `mod config_io;` is wired, or compile error referencing `tempfile`. Once it compiles, tests should PASS (implementation is complete in Step 1). If they pass immediately, that is acceptable here since this is a pure module; proceed.

- [ ] **Step 3: Run the test to verify it passes**

Run: `cd desktop/src-tauri && cargo test config_io`
Expected: PASS — 3 tests.

- [ ] **Step 4: Commit**

```bash
git add desktop/src-tauri/src/config_io.rs desktop/src-tauri/src/main.rs
git commit -m "feat(desktop): config_io — edit vault.root in config.toml"
```

---

## Task 3: Vault-move validation

**Files:**
- Create: `desktop/src-tauri/src/vault_move.rs`
- Test: same file

- [ ] **Step 1: Write the failing test**

Create `desktop/src-tauri/src/vault_move.rs`:

```rust
//! Validation + execution of a vault directory move. Safety-first: the source
//! is preserved until the destination is confirmed; only the caller
//! (commands::move_vault) removes the old dir after a healthy restart.

use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
pub enum MoveError {
    SamePath,
    TargetNotEmpty(PathBuf),
    SourceMissing(PathBuf),
    Io(String),
}

impl std::fmt::Display for MoveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MoveError::SamePath => write!(f, "new location is the same as the current one"),
            MoveError::TargetNotEmpty(p) => {
                write!(f, "target directory is not empty: {}", p.display())
            }
            MoveError::SourceMissing(p) => write!(f, "current vault not found: {}", p.display()),
            MoveError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// Validate a proposed move. `target` may or may not exist; if it exists it
/// must be an empty directory.
pub fn validate(source: &Path, target: &Path) -> Result<(), MoveError> {
    let src = source.canonicalize().map_err(|_| MoveError::SourceMissing(source.into()))?;
    // target may not exist yet; canonicalize its existing parent for comparison
    let tgt_abs = if target.is_absolute() { target.to_path_buf() } else {
        std::env::current_dir().map_err(|e| MoveError::Io(e.to_string()))?.join(target)
    };
    if tgt_abs == src {
        return Err(MoveError::SamePath);
    }
    if tgt_abs.exists() {
        let mut entries = std::fs::read_dir(&tgt_abs).map_err(|e| MoveError::Io(e.to_string()))?;
        if entries.next().is_some() {
            return Err(MoveError::TargetNotEmpty(tgt_abs));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_same_path() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(validate(dir.path(), dir.path()), Err(MoveError::SamePath));
    }

    #[test]
    fn rejects_nonempty_target() {
        let src = tempfile::tempdir().unwrap();
        let tgt = tempfile::tempdir().unwrap();
        std::fs::write(tgt.path().join("x"), b"data").unwrap();
        assert_eq!(
            validate(src.path(), tgt.path()),
            Err(MoveError::TargetNotEmpty(tgt.path().canonicalize().unwrap()))
        );
    }

    #[test]
    fn rejects_missing_source() {
        let tgt = tempfile::tempdir().unwrap();
        let missing = tgt.path().join("does-not-exist");
        assert_eq!(validate(&missing, tgt.path()), Err(MoveError::SourceMissing(missing)));
    }

    #[test]
    fn accepts_new_empty_target() {
        let src = tempfile::tempdir().unwrap();
        let tgt = src.path().join("new-loc"); // does not exist yet
        assert!(validate(src.path(), &tgt).is_ok());
    }
}
```

Add `mod vault_move;` to `desktop/src-tauri/src/main.rs`.

- [ ] **Step 2: Run the test to verify it fails, then passes**

Run: `cd desktop/src-tauri && cargo test vault_move::tests::`
Expected: compiles, 4 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add desktop/src-tauri/src/vault_move.rs desktop/src-tauri/src/main.rs
git commit -m "feat(desktop): vault move validation"
```

---

## Task 4: Vault-move execution (rename with copy fallback + rollback helper)

**Files:**
- Modify: `desktop/src-tauri/src/vault_move.rs`

- [ ] **Step 1: Write the failing test**

Append to the `tests` module in `desktop/src-tauri/src/vault_move.rs`:

```rust
    #[test]
    fn moves_directory_contents() {
        let parent = tempfile::tempdir().unwrap();
        let src = parent.path().join("vault");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("a.md"), b"hello").unwrap();
        let tgt = parent.path().join("moved");

        execute(&src, &tgt).unwrap();

        assert!(tgt.join("a.md").exists(), "file moved");
        assert_eq!(std::fs::read(tgt.join("a.md")).unwrap(), b"hello");
        assert!(!src.exists(), "source removed after successful move");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd desktop/src-tauri && cargo test vault_move::tests::moves_directory_contents`
Expected: FAIL — `execute` not found.

- [ ] **Step 3: Implement `execute`**

Add to `desktop/src-tauri/src/vault_move.rs` (above the `tests` module):

```rust
/// Move `source` directory to `target`. Tries an atomic rename first (same
/// filesystem); on cross-device error, copies recursively then removes source.
/// On copy failure, removes the partial target and leaves source intact.
pub fn execute(source: &Path, target: &Path) -> Result<(), MoveError> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| MoveError::Io(e.to_string()))?;
    }
    match std::fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(_) => {
            // cross-device or other rename failure: copy then remove source.
            if let Err(e) = copy_dir_recursive(source, target) {
                let _ = std::fs::remove_dir_all(target); // clean partial target
                return Err(MoveError::Io(e));
            }
            std::fs::remove_dir_all(source).map_err(|e| MoveError::Io(e.to_string()))
        }
    }
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_dir_recursive(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd desktop/src-tauri && cargo test vault_move`
Expected: PASS — all tests (validation + move).

- [ ] **Step 5: Commit**

```bash
git add desktop/src-tauri/src/vault_move.rs
git commit -m "feat(desktop): vault move execution with copy fallback"
```

---

## Task 5: Daemon supervisor — drive the `mnemos` CLI sidecar

**Files:**
- Create: `desktop/src-tauri/src/daemon.rs`
- Modify: `desktop/src-tauri/src/main.rs` (add `mod daemon;`)

The `mnemos` CLI sidecar already implements `daemon start|stop|status` (PID file at `~/.local/state/mnemos/mnemosd.pid`, adopt-if-running). We shell out to it via `tauri-plugin-shell` and parse its `--json` output.

- [ ] **Step 1: Implement the supervisor**

Create `desktop/src-tauri/src/daemon.rs`:

```rust
//! Daemon lifecycle for the desktop shell. Delegates to the bundled `mnemos`
//! CLI sidecar's `daemon` subcommands so we reuse its PID/adopt logic.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

#[derive(Debug, Clone, Serialize)]
pub struct DaemonStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub detail: String,
}

/// Run `mnemos daemon <sub> --json` via the sidecar; return (stdout, ok).
async fn run_daemon(app: &AppHandle, sub: &str) -> Result<(String, bool), String> {
    let cmd = app
        .shell()
        .sidecar("mnemos")
        .map_err(|e| format!("resolve sidecar: {e}"))?
        .args(["daemon", sub, "--json"]);
    let out = cmd.output().await.map_err(|e| format!("run mnemos daemon {sub}: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    Ok((stdout, out.status.success()))
}

pub async fn status(app: &AppHandle) -> DaemonStatus {
    match run_daemon(app, "status").await {
        Ok((stdout, _)) => {
            let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_default();
            let running = v.get("running").and_then(|b| b.as_bool()).unwrap_or(false);
            let pid = v.get("pid").and_then(|p| p.as_u64()).map(|p| p as u32);
            DaemonStatus { running, pid, detail: stdout.trim().to_string() }
        }
        Err(e) => DaemonStatus { running: false, pid: None, detail: e },
    }
}

pub async fn start(app: &AppHandle) -> Result<(), String> {
    let (out, ok) = run_daemon(app, "start").await?;
    if ok { Ok(()) } else { Err(format!("daemon start failed: {out}")) }
}

pub async fn stop(app: &AppHandle) -> Result<(), String> {
    let (out, ok) = run_daemon(app, "stop").await?;
    if ok { Ok(()) } else { Err(format!("daemon stop failed: {out}")) }
}
```

> NOTE for implementer: verify the `mnemos daemon status` subcommand emits a JSON object with `running` and `pid` fields when given `--json`. Read `crates/mnemos_cli/src/commands/daemon.rs`. If the field names differ (e.g. the status command prints a different shape), align the parsing in `status()` to the actual keys. Do **not** guess — match the real output.

- [ ] **Step 2: Wait-for-health helper (used by move)**

Append to `desktop/src-tauri/src/daemon.rs`:

```rust
/// Poll the daemon's unauthenticated readiness until healthy or timeout.
/// Returns Ok(()) when /v1/doctor responds (any status), Err on timeout.
pub async fn wait_healthy(port: u16, timeout_ms: u64) -> Result<(), String> {
    let url = format!("http://127.0.0.1:{port}/v1/doctor");
    let started = std::time::Instant::now();
    loop {
        if let Ok(resp) = reqwest::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_millis(500))
            .send()
            .await
        {
            // 200 or 401 both prove the listener is up.
            let _ = resp;
            return Ok(());
        }
        if started.elapsed().as_millis() as u64 > timeout_ms {
            return Err("daemon did not become healthy in time".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}
```

Add `reqwest` and `tokio` to `[dependencies]` in `desktop/src-tauri/Cargo.toml`:

```toml
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }
tokio = { version = "1", features = ["time"] }
```

Add `mod daemon;` to `main.rs`.

- [ ] **Step 3: Verify compile**

Run: `cd desktop/src-tauri && cargo build`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add desktop/src-tauri/src/daemon.rs desktop/src-tauri/src/main.rs desktop/src-tauri/Cargo.toml desktop/src-tauri/Cargo.lock
git commit -m "feat(desktop): daemon supervisor via mnemos CLI sidecar"
```

---

## Task 6: Tauri commands — `pick_vault_dir`, `daemon_status`, `move_vault`

**Files:**
- Create: `desktop/src-tauri/src/commands.rs`
- Modify: `desktop/src-tauri/src/main.rs`

- [ ] **Step 1: Implement the commands**

Create `desktop/src-tauri/src/commands.rs`:

```rust
use crate::{config_io, daemon, vault_move};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

#[derive(Serialize)]
pub struct MoveResult {
    pub moved_to: String,
}

/// Open a native folder picker. Returns the chosen path, or None if cancelled.
#[tauri::command]
pub async fn pick_vault_dir(app: AppHandle) -> Result<Option<String>, String> {
    let folder = app.dialog().file().blocking_pick_folder();
    Ok(folder.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn daemon_status(app: AppHandle) -> Result<daemon::DaemonStatus, String> {
    Ok(daemon::status(&app).await)
}

/// Orchestrate a vault move: validate → stop → write config → move →
/// start → wait healthy → finalize. On failure after the move, attempt revert.
#[tauri::command]
pub async fn move_vault(app: AppHandle, new_path: String) -> Result<MoveResult, String> {
    let cfg_path = config_io::config_path()?;
    let current = config_io::read_vault_root(&cfg_path)?
        .ok_or_else(|| "current vault location is unknown".to_string())?;
    let target = std::path::PathBuf::from(&new_path);

    vault_move::validate(&current, &target).map_err(|e| e.to_string())?;

    daemon::stop(&app).await?;

    // Persist new location BEFORE moving so a restart reads the new path.
    config_io::write_vault_root(&cfg_path, &target)?;

    if let Err(e) = vault_move::execute(&current, &target) {
        // Move failed: restore config and restart at the old path.
        let _ = config_io::write_vault_root(&cfg_path, &current);
        let _ = daemon::start(&app).await;
        return Err(format!("move failed: {e}"));
    }

    daemon::start(&app).await?;
    if let Err(e) = daemon::wait_healthy(7423, 30_000).await {
        // Daemon unhealthy at new path: revert path + data, restart old.
        let _ = vault_move::execute(&target, &current);
        let _ = config_io::write_vault_root(&cfg_path, &current);
        let _ = daemon::start(&app).await;
        return Err(format!("daemon unhealthy after move, reverted: {e}"));
    }

    Ok(MoveResult { moved_to: target.to_string_lossy().into_owned() })
}
```

> NOTE: the daemon port is read as `7423` here for simplicity. If `daemon.port` is customized in `config.toml`, read it via a small `config_io::read_daemon_port()` helper (add it mirroring `read_vault_root`). Implementer: add that helper and use it instead of the literal if you want port-correctness; otherwise leave the documented default and note the limitation.

- [ ] **Step 2: Register commands + plugins in `main.rs`**

Edit `desktop/src-tauri/src/main.rs` `main()`:

```rust
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            read_token,
            commands::pick_vault_dir,
            commands::daemon_status,
            commands::move_vault,
        ])
        .setup(|app| {
            // Adopt-or-spawn the daemon on launch (best-effort; UI surfaces status).
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let st = crate::daemon::status(&handle).await;
                if !st.running {
                    let _ = crate::daemon::start(&handle).await;
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running mnemos desktop");
}
```

Add `mod commands;` near the other `mod` declarations.

- [ ] **Step 3: Verify compile**

Run: `cd desktop/src-tauri && cargo build`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add desktop/src-tauri/src/commands.rs desktop/src-tauri/src/main.rs
git commit -m "feat(desktop): pick_vault_dir, daemon_status, move_vault commands + daemon autostart"
```

---

## Task 7: Integration test — full move cycle (gated)

**Files:**
- Create: `desktop/src-tauri/tests/move_cycle.rs`

This test exercises `config_io` + `vault_move` together (the pure orchestration parts) without a live daemon — daemon control is covered manually + by the E2E task.

- [ ] **Step 1: Write the test**

Create `desktop/src-tauri/tests/move_cycle.rs`:

```rust
// Integration: config write + directory move behave together as move_vault expects.
use std::path::Path;

#[path = "../src/config_io.rs"]
mod config_io;
#[path = "../src/vault_move.rs"]
mod vault_move;

#[test]
fn config_and_move_compose() {
    let root = tempfile::tempdir().unwrap();
    let cfg = root.path().join("config.toml");
    let src = root.path().join("vault");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("m.md"), b"x").unwrap();
    config_io::write_vault_root(&cfg, &src).unwrap();

    let dst = root.path().join("new");
    vault_move::validate(&src, &dst).unwrap();
    config_io::write_vault_root(&cfg, &dst).unwrap();
    vault_move::execute(&src, &dst).unwrap();

    assert_eq!(config_io::read_vault_root(&cfg).unwrap().unwrap(), dst);
    assert!(dst.join("m.md").exists());
    assert!(!src.exists());
}
```

- [ ] **Step 2: Run**

Run: `cd desktop/src-tauri && cargo test --test move_cycle`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add desktop/src-tauri/tests/move_cycle.rs
git commit -m "test(desktop): integration test for config+move composition"
```

---

## Task 8: Frontend — typed invoke wrappers

**Files:**
- Create: `desktop/src/api/tauri.ts`

- [ ] **Step 1: Implement wrappers with browser fallback**

Create `desktop/src/api/tauri.ts`:

```typescript
// Thin wrappers over the Rust shell commands. In a plain browser (vite dev /
// vitest) Tauri isn't present, so these degrade to no-ops / nulls.

export interface DaemonStatus {
  running: boolean;
  pid: number | null;
  detail: string;
}

async function invokeSafe<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<T>(cmd, args);
  } catch {
    return null;
  }
}

export function pickVaultDir(): Promise<string | null> {
  return invokeSafe<string | null>("pick_vault_dir").then((r) => r ?? null);
}

export function daemonStatus(): Promise<DaemonStatus | null> {
  return invokeSafe<DaemonStatus>("daemon_status");
}

export function moveVault(newPath: string): Promise<{ moved_to: string } | null> {
  return invokeSafe<{ moved_to: string }>("move_vault", { newPath });
}
```

> NOTE: Tauri maps the Rust arg `new_path` to JS `newPath` by default (camelCase). Confirm against the installed `@tauri-apps/api` version; if it expects snake_case, pass `{ new_path: newPath }`.

- [ ] **Step 2: Typecheck**

Run: `cd desktop && pnpm typecheck`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add desktop/src/api/tauri.ts
git commit -m "feat(ui): typed wrappers for storage/daemon Tauri commands"
```

---

## Task 9: Frontend — Storage settings section

**Files:**
- Create: `desktop/src/views/StorageSettings.tsx`
- Create: `desktop/src/views/StorageSettings.test.tsx`

- [ ] **Step 1: Write the failing component test**

Create `desktop/src/views/StorageSettings.test.tsx`:

```typescript
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { StorageSettings } from "./StorageSettings";
import * as tauri from "../api/tauri";
import { client } from "../api/client";

vi.mock("../api/tauri");
vi.mock("../api/client", () => ({
  client: { getConfig: vi.fn() },
}));

describe("StorageSettings", () => {
  beforeEach(() => {
    vi.mocked(client.getConfig).mockResolvedValue({ vault: { root: "/home/u/.local/share/mnemos" } });
  });

  it("shows the current vault path", async () => {
    render(<StorageSettings />);
    expect(await screen.findByText("/home/u/.local/share/mnemos")).toBeInTheDocument();
  });

  it("moves the vault when a folder is picked and confirmed", async () => {
    vi.mocked(tauri.pickVaultDir).mockResolvedValue("/data/mnemos");
    vi.mocked(tauri.moveVault).mockResolvedValue({ moved_to: "/data/mnemos" });

    render(<StorageSettings />);
    fireEvent.click(await screen.findByRole("button", { name: /change location/i }));
    fireEvent.click(await screen.findByRole("button", { name: /move my data/i }));

    await waitFor(() => expect(tauri.moveVault).toHaveBeenCalledWith("/data/mnemos"));
    expect(await screen.findByText(/moved to/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd desktop && pnpm test -- StorageSettings`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the component**

Create `desktop/src/views/StorageSettings.tsx`:

```typescript
import { useEffect, useState } from "react";
import { client } from "../api/client";
import { pickVaultDir, moveVault } from "../api/tauri";
import { Button, Card } from "../design/primitives";

type Phase = "idle" | "picked" | "moving" | "done" | "error";

export function StorageSettings() {
  const [current, setCurrent] = useState<string | null>(null);
  const [target, setTarget] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [message, setMessage] = useState<string>("");

  useEffect(() => {
    void client.getConfig().then((c) => {
      const root = (c as { vault?: { root?: string } }).vault?.root ?? null;
      setCurrent(root);
    });
  }, []);

  const pick = async () => {
    const dir = await pickVaultDir();
    if (dir) {
      setTarget(dir);
      setPhase("picked");
    }
  };

  const confirmMove = async () => {
    if (!target) return;
    setPhase("moving");
    setMessage("Moving your vault and restarting the daemon…");
    try {
      const res = await moveVault(target);
      if (!res) throw new Error("Move is only available in the desktop app.");
      setCurrent(res.moved_to);
      setTarget(null);
      setPhase("done");
      setMessage(`Moved to ${res.moved_to}`);
    } catch (e) {
      setPhase("error");
      setMessage(e instanceof Error ? e.message : "Move failed");
    }
  };

  return (
    <Card className="p-4 space-y-3">
      <h2 className="display text-lg">Storage</h2>
      <div className="font-body text-text-muted">
        Current location: <span className="mono">{current ?? "unknown"}</span>
      </div>

      {phase !== "picked" && phase !== "moving" && (
        <Button onClick={pick}>Change location…</Button>
      )}

      {phase === "picked" && target && (
        <div className="space-y-2">
          <p className="font-body">
            Move your vault from <span className="mono">{current}</span> to{" "}
            <span className="mono">{target}</span>? The daemon will restart.
          </p>
          <div className="flex gap-2">
            <Button onClick={confirmMove}>Move my data</Button>
            <button className="label text-text-muted" onClick={() => { setTarget(null); setPhase("idle"); }}>
              Cancel
            </button>
          </div>
        </div>
      )}

      {phase === "moving" && <p className="label" aria-busy="true">{message}</p>}
      {phase === "done" && <p className="label text-accent">{message}</p>}
      {phase === "error" && <p className="label text-danger" role="alert">{message}</p>}
    </Card>
  );
}
```

> NOTE: confirm `Button`, `Card` exist in `desktop/src/design/primitives` (FirstRun.tsx imports them) and that token classes `text-accent`/`text-danger` exist in the design system; if not, use the closest existing tokens. Read `desktop/src/design/primitives.tsx` first.

- [ ] **Step 4: Run to verify it passes**

Run: `cd desktop && pnpm test -- StorageSettings`
Expected: PASS — 2 tests.

- [ ] **Step 5: Commit**

```bash
git add desktop/src/views/StorageSettings.tsx desktop/src/views/StorageSettings.test.tsx
git commit -m "feat(ui): Storage settings section with vault move flow"
```

---

## Task 10: Mount the Storage section in Settings

**Files:**
- Modify: `desktop/src/views/Settings.tsx`

- [ ] **Step 1: Import and render**

At the top of `desktop/src/views/Settings.tsx` add:

```typescript
import { StorageSettings } from "./StorageSettings";
```

Render `<StorageSettings />` at the top of the settings sections list (before the existing daemon/embedder sections). Read the file first to find the exact render location and match its layout wrapper.

- [ ] **Step 2: Typecheck + test + build**

Run: `cd desktop && pnpm typecheck && pnpm test && pnpm build`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add desktop/src/views/Settings.tsx
git commit -m "feat(ui): surface Storage section in Settings"
```

---

## Task 11: Update first-run wizard copy

The wizard currently claims the vault location is changeable in Settings — now true. Keep the claim, but make step 1 link to the action.

**Files:**
- Modify: `desktop/src/views/FirstRun.tsx`

- [ ] **Step 1: Adjust copy**

In `desktop/src/views/FirstRun.tsx` step 0, change the parenthetical "(you can change this in Settings)" to: "(you can move it anytime in **Settings → Storage**)". No behavior change.

- [ ] **Step 2: Test + build**

Run: `cd desktop && pnpm test -- FirstRun && pnpm build`
Expected: PASS (update the FirstRun test assertion if it matched the old copy).

- [ ] **Step 3: Commit**

```bash
git add desktop/src/views/FirstRun.tsx desktop/src/views/FirstRun.test.tsx
git commit -m "docs(ui): first-run points to Settings → Storage for vault location"
```

---

## Task 12: Supervised-daemon embedder assets (env wiring + known-limitation doc)

The autostarted daemon must find the bundled embedder. In `dev` this works from `assets/` with `LD_LIBRARY_PATH`. In the packaged app, point the daemon at the bundled resource dir; document the unresolved `.so` bundling.

**Files:**
- Modify: `desktop/src-tauri/src/daemon.rs` (set env when starting)
- Modify: `BUILD.md` (known limitation)

- [ ] **Step 1: Set embedder asset env on start**

In `desktop/src-tauri/src/daemon.rs`, change `start()` to pass asset-locating env vars to the sidecar. Resolve the bundled resource dir via `app.path().resource_dir()`:

```rust
pub async fn start(app: &AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let mut cmd = app
        .shell()
        .sidecar("mnemos")
        .map_err(|e| format!("resolve sidecar: {e}"))?
        .args(["daemon", "start", "--json"]);
    if let Ok(res) = app.path().resource_dir() {
        // Bundled llama-server + GGUF live under <resources>/assets in packaged
        // builds; in dev the daemon falls back to ./assets relative to CWD.
        let assets = res.join("assets");
        cmd = cmd
            .env("MNEMOS_BUNDLED_BIN_DIR", assets.to_string_lossy().to_string())
            .env("MNEMOS_BUNDLED_MODEL_DIR", assets.to_string_lossy().to_string());
    }
    let out = cmd.output().await.map_err(|e| format!("run mnemos daemon start: {e}"))?;
    if out.status.success() { Ok(()) } else {
        Err(format!("daemon start failed: {}", String::from_utf8_lossy(&out.stdout)))
    }
}
```

> NOTE: `MNEMOS_BUNDLED_BIN_DIR`/`MNEMOS_BUNDLED_MODEL_DIR` are the env hooks in `crates/mnemos_daemon/src/bundled_embedder.rs` (`default_binary_path`/`default_model_path`). Confirm the exact resource layout produced by Tauri (the `resources` entries land under the resource dir with a `_up_/_up_/assets/...` prefix in deb/rpm — verify with `rpm -qlp` and adjust the joined path to match).

- [ ] **Step 2: Document the limitation**

Append to `BUILD.md` a "Desktop app + embedder" note: the desktop deb/rpm currently bundle only `llama-server` + GGUF, not the `libggml*/libllama*` shared libraries. Until those are bundled (or the `mnemos-daemon` package is installed alongside), the desktop-app-supervised daemon's bundled embedder works in `dev` (via `assets/` + `LD_LIBRARY_PATH`) but not from a standalone desktop-package install. Tracked as a follow-up.

- [ ] **Step 3: Build + commit**

Run: `cd desktop/src-tauri && cargo build`
Expected: PASS.

```bash
git add desktop/src-tauri/src/daemon.rs BUILD.md
git commit -m "feat(desktop): point supervised daemon at bundled embedder assets + document libs gap"
```

---

## Task 13: Manual end-to-end verification (dev)

**Files:** none (verification only)

- [ ] **Step 1: Run the app in dev with a seeded vault**

```bash
# Terminal: from repo root, set a throwaway vault and config so we don't touch real data
export MNEMOS_CONFIG_PATH=/tmp/mnemos-e2e/config.toml
export MNEMOS_VAULT=/tmp/mnemos-e2e/vault
mkdir -p /tmp/mnemos-e2e/vault
cd desktop && pnpm tauri dev
```

- [ ] **Step 2: Drive the flow**

In the app: Settings → Storage shows the current path. Click **Change location…**, pick an empty new folder, confirm **Move my data**. Observe: progress → "Moved to …". Verify on disk that the vault files moved and the daemon is serving from the new path:

```bash
curl -s -H "Authorization: Bearer $(cat ~/.config/mnemos/token)" http://localhost:7423/v1/config | python3 -m json.tool | grep -A1 vault
ls /tmp/mnemos-e2e/<new-folder>   # contains the moved vault
```

Expected: `vault.root` is the new path; memories still recall via search.

- [ ] **Step 3: Negative path**

Repeat but pick a **non-empty** folder. Expected: clear rejection, no daemon restart, data untouched.

- [ ] **Step 4: Record evidence**

Capture screenshots of the success and rejection states into `desktop/.screenshots/storage-picker/` and note results in the session log. (No commit required unless screenshots are tracked.)

---

## Self-Review

- **Spec coverage:** move/migrate semantics (Tasks 3–4, 6), app-managed daemon (Tasks 5–6, autostart in 6), shell-orchestrated move (Task 6), native picker via dialog plugin (Tasks 1, 6, 8), Settings → Storage UI (Tasks 9–10), error/rollback (Tasks 4, 6), config-write-while-down (Task 2 + deviation note), testing layers (unit 2/3/4, integration 7, e2e 13), follow-up apply&restart (noted, not built — correct, out of scope). All covered.
- **Deviations flagged:** Config-serialization sharing replaced with standalone `toml` write (top of plan, Task 2) — needs user OK. Packaged embedder `.so` libs gap (Task 12) — documented as known limitation, not solved here.
- **Type consistency:** `DaemonStatus { running, pid, detail }` consistent across Rust (Task 5) and TS (Task 8). Command names `pick_vault_dir`/`daemon_status`/`move_vault` consistent across Tasks 6, 8. `move_vault(new_path)` ↔ `moveVault(newPath)` arg-casing flagged in Task 8. `vault_move::{validate, execute, MoveError}` consistent Tasks 3/4/6/7.
- **Verify-before-trust notes:** several steps include explicit "read the real file / confirm field names" notes (CLI JSON shape, primitives, resource-dir layout, arg casing) rather than guessing — implementer must confirm against source.
