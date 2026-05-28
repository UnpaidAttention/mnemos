# Mnemos Plan 7 — Cloud sync, QoL, and adapters

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn mnemos into a complete personal-knowledge system: keep two machines' vaults in sync (three durable file-sync backends + optional libSQL replica layer), give the user a real Settings view + first-run experience + Doctor view, finish the entity-graph editing surface (merge, promote-to-procedural), let people get their data in and out (zip export/import), and ship the remaining MCP/HTTP adapters so any AI tool can plug in. End state: install on a second machine, point it at the same vault, and pick up where you left off — with a settings panel that exposes every knob, a doctor that explains what's wrong, and adapters for Codex, Hermes Agent, Openclaw, Gemini CLI, generic-MCP, and OpenAI function-calling.

**Architecture:** Sync is **files-first** (the durable record). Three pluggable backends ship in `mnemos_core::sync`: filesystem-sync (point the vault at a Syncthing/Dropbox/iCloud/OneDrive folder; surface sync-conflict files), Git remote (periodic commits + push/pull + a small `mnemos-merge-driver` binary that does YAML-aware frontmatter merges), and S3-compatible (shell out to `rclone` for periodic upload + pull-on-start). Sync runs as a background task in `mnemosd` with `[sync]` config, REST + CLI + WS surface, and a status pill in the top bar. Settings/first-run/doctor are wired into the existing Tauri shell; entity merge + tier promotion add the missing memory-graph edits to the daemon (with audit). Adapters live under `adapters/<name>/` — each a small, copy-pastable integration. **Turso libSQL embedded replicas** (the DB-layer fast-path) are deferred to a later increment: the config knob is in place but the wire-up is a stub with a documented TODO.

**Tech Stack:** Rust 2021 (existing stack), axum 0.8, libsql, plus `walkdir` and `zip` for export/import, `tokio::process` to shell out to `git` and `rclone`, `serde_yaml` for the frontmatter merge driver. Frontend additions: a new Settings view (form-driven), First-Run wizard, Doctor view, Entity-merge dialog, Promote-to-procedural action — all on the existing design system. No new heavy dependencies on the frontend.

---

## Plan sequence context

Plan 7 of 8, producing **v0.6.0**. Built on v0.5.0 (Plan 6: desktop UI). Final plan ahead is **Plan 8 — packaging, installers, code signing, auto-update** (the only thing left to call mnemos "shippable to non-developers"). After Plan 7 the product is feature-complete; Plan 8 is purely distribution.

External prerequisites the user installs themselves (the plan documents this in the new Settings/Doctor views): **`git`** (any modern install) when using the Git-remote backend; **`rclone`** when using the S3-compatible backend; **Ollama** for the default embedder/LLM. None are required for the filesystem-sync backend.

---

## What this plan deliberately defers

| Capability | Why | Target |
|---|---|---|
| **Turso libSQL embedded replicas** (DB-layer fast-path) | Requires a Turso cloud / self-hosted instance to test against. The `[sync.turso]` config knob ships; the daemon wires through to `libsql::Database::open_with_remote_sync(...)` behind a `turso` cargo feature; default off. | A later increment when there's a test target. |
| **Encrypt-at-rest** (libSQL native encryption + `age` for files) | The spec marks it opt-in and notes it breaks Obsidian-direct editing. Distinct security concern; ships separately. | Later. |
| **Secret detection at ingestion** | The spec lists it as a security feature (scan for AWS keys / JWT / SSH / OpenAI-prefix on `remember`). Discrete, needs its own threat-model + allowlist UX. | Later increment (could be Plan 8a). |
| **AI-tool auto-detection in first-run** | We detect Ollama (HTTP probe) and offer the model pull. Auto-dropping config fragments into other tools' directories (`~/.claude/`, `~/.config/codex/`, …) is a per-tool dance with versioning risk. The wizard SHOWS the integration snippets for copy-paste instead. | When a tool versions stabilize. |
| **Native packaging / installers / signing / auto-update** | Plan 8. |
| **Daily community-detection scheduler** | On-demand endpoint already ships (Plan 5). | Trivial follow-up if desired. |

---

## Hard prerequisites

- Plan 6 (`v0.5.0`) shipped; daemon + desktop CI green on `master`.
- Rust toolchain, Node 20+, pnpm 9+.
- `git` (for Git-remote sync tests; also already used by the repo).
- `rclone` (for S3-sync tests) — the plan's S3 tests skip with `#[ignore]` when `rclone` isn't on PATH.

---

## File structure produced by this plan

```
crates/mnemos_core/src/
├── sync/                       # NEW — pluggable sync backends + state
│   ├── mod.rs                  # SyncBackend trait, SyncStatus, SyncEvent
│   ├── filesystem.rs           # filesystem-sync (Syncthing/Dropbox/iCloud/OneDrive) + conflict scan
│   ├── git.rs                  # Git remote: shell out to `git` with custom merge driver
│   ├── s3.rs                   # S3-compatible: shell out to `rclone`
│   └── state.rs                # last_pushed_at, last_pulled_at, conflicts (schema v7)
├── storage/
│   ├── migrations.rs           # MODIFIED: v7 (sync_state, sync_conflicts)
│   ├── memory_ops.rs           # MODIFIED: change_tier(id, new_tier) + audit
│   └── entity_ops.rs           # MODIFIED: merge_entities(source, target)
├── vault.rs                    # MODIFIED: promote(id, new_tier), merge_entities passthrough
└── lib.rs                      # MODIFIED: pub mod sync;

crates/mnemos_daemon/src/
├── routes/
│   ├── sync.rs                 # NEW: GET /v1/sync/status, POST /v1/sync/push|pull
│   ├── config.rs               # NEW: GET/PUT /v1/config (settings)
│   ├── vault.rs                # NEW: POST /v1/vault/export, POST /v1/vault/import
│   ├── doctor.rs               # NEW: GET /v1/doctor
│   ├── memories.rs             # MODIFIED: POST /v1/memories/{id}/promote
│   ├── entities.rs             # MODIFIED: POST /v1/entities/merge
│   └── mod.rs                  # MODIFIED: mount all new routers
├── sync_worker.rs              # NEW: periodic sync task
├── events.rs                   # MODIFIED: SyncStarted/Completed/Failed/Conflict
├── config.rs                   # MODIFIED: [sync] block + [sync.git|fs|s3|turso]
└── main.rs                     # MODIFIED: spawn sync_worker + graceful shutdown

crates/mnemos_cli/src/
├── cli.rs                      # MODIFIED: `sync push|pull|status`, `doctor`, `export|import`
└── commands/
    ├── sync.rs                 # NEW
    ├── export.rs               # NEW
    └── import.rs               # NEW

crates/mnemos_merge_driver/     # NEW — tiny binary registered as git merge driver
├── Cargo.toml
└── src/main.rs                 # YAML-aware frontmatter merge for *.md memory files

desktop/src/
├── views/
│   ├── Settings.tsx            # NEW
│   ├── Doctor.tsx              # NEW
│   ├── FirstRun.tsx            # NEW (modal, shown on first launch)
│   └── EntityProfile.tsx       # MODIFIED: Merge action; promote-to-procedural enabled
├── api/                        # MODIFIED: sync, config, vault, doctor, merge, promote
├── layout/TopBar.tsx           # MODIFIED: real sync status pill
└── store/events.ts             # MODIFIED: sync_* event types

adapters/
├── claude-code/                # already shipped
├── gemini-cli/                 # NEW (or completed)
├── codex/                      # NEW
├── hermes-agent/               # NEW
├── openclaw/                   # NEW
├── generic-mcp/                # NEW
└── openai-functions/           # NEW

README.md / CHANGELOG.md / Cargo.toml   # MODIFIED: v0.6.0
```

---

## Conventions (same as Plans 1-6)

- **Backend (Rust):** TDD — failing test → implement → `cargo fmt`/`clippy -D warnings`/`cargo test` green → commit. Daemon endpoints behind bearer auth (the `authed` router).
- **Frontend (TS):** TDD — Vitest + Testing Library + MSW; `pnpm typecheck`/`lint`/`test` green per task.
- **External tool tests** (`git`, `rclone`): use `which::which` to detect; `#[ignore]` the test if absent. The shipped feature still works for users who have the tool.
- **Commits:** `<type>: <subject>` + Plan 7 / Task N in body. Each task is one commit (sync subtasks may split into a/b for daemon + UI).
- **No placeholders / no AI-slop**: every view consumes design tokens; every state is handled.
- **No pushing.** Tag locally at the end; user reviews + pushes.

Frontend command paths in this plan are unchanged from Plan 6 (`desktop/`). The new `mnemos_merge_driver` crate joins the workspace.

---

# Group A — Cloud sync core

## Task 1: Schema v7 (`sync_state` + `sync_conflicts`)

Persistent bookkeeping for sync: last push/pull timestamps + last error, and a list of detected sync conflicts the UI surfaces.

**Files:**
- Modify: `crates/mnemos_core/src/storage/migrations.rs`
- Test: `crates/mnemos_core/tests/schema_v7.rs` (new)

- [ ] **Step 1: Failing test** — `crates/mnemos_core/tests/schema_v7.rs`:

```rust
use mnemos_core::storage::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn migration_v7_adds_sync_tables() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(&tmp.path().join("v7.db")).await.unwrap();
    assert!(s.schema_version().await.unwrap() >= 7);
    let conn = s.conn().unwrap();
    conn.execute("INSERT INTO sync_conflicts (ts, path, detected_by, resolved_at) VALUES ('2026-05-28T00:00:00+00:00','foo.md','filesystem',NULL)", ()).await.unwrap();
    conn.execute("UPDATE sync_state SET last_pushed_at = '2026-05-28T00:00:00+00:00' WHERE id = 1", ()).await.unwrap();
    let mut rows = conn.query("SELECT last_pushed_at FROM sync_state WHERE id = 1", ()).await.unwrap();
    let r = rows.next().await.unwrap().unwrap();
    let v: Option<String> = r.get(0).unwrap();
    assert_eq!(v.as_deref(), Some("2026-05-28T00:00:00+00:00"));
}
```

- [ ] **Step 2: Run test to verify it fails** — `cargo test -p mnemos_core --test schema_v7` → FAIL.

- [ ] **Step 3: Add migration v7** in `migrations.rs`. After the `current < 6` block:

```rust
        if current < 7 {
            migration_v7(&conn).await?;
            conn.execute(
                "INSERT OR IGNORE INTO schema_migrations (version) VALUES (7)",
                (),
            )
            .await?;
        }
```

```rust
async fn migration_v7(conn: &libsql::Connection) -> Result<()> {
    for stmt in V7_STATEMENTS {
        conn.execute(stmt, ()).await?;
    }
    Ok(())
}

const V7_STATEMENTS: &[&str] = &[
    // Single-row sync bookkeeping.
    "CREATE TABLE IF NOT EXISTS sync_state (
        id                INTEGER PRIMARY KEY CHECK(id = 1),
        last_pushed_at    TEXT,
        last_pulled_at    TEXT,
        last_error        TEXT
    )",
    "INSERT OR IGNORE INTO sync_state (id) VALUES (1)",
    // Detected conflict files (Syncthing-style, etc.) and Git merge conflicts.
    "CREATE TABLE IF NOT EXISTS sync_conflicts (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        ts           TEXT NOT NULL,
        path         TEXT NOT NULL,
        detected_by  TEXT NOT NULL,
        resolved_at  TEXT,
        details      TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_sync_conflicts_unresolved ON sync_conflicts(resolved_at) WHERE resolved_at IS NULL",
];
```

- [ ] **Step 4: Bump stale schema-version assertions** from 6 → 7 in `tests/schema_v1.rs`, `tests/schema_v2.rs`, `tests/storage_open.rs` (grep `schema_version` — there are multiple per file).

- [ ] **Step 5: Pass + commit.**
```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/storage/migrations.rs crates/mnemos_core/tests/schema_v7.rs crates/mnemos_core/tests/schema_v1.rs crates/mnemos_core/tests/schema_v2.rs crates/mnemos_core/tests/storage_open.rs
git commit -m "feat: schema v7 — sync_state + sync_conflicts (Plan 7 Task 1)"
```

---

## Task 2: `SyncBackend` trait + sync state ops

The trait every backend implements, plus typed helpers that read/write `sync_state` and `sync_conflicts`.

**Files:**
- Modify: `crates/mnemos_core/src/lib.rs` (add `pub mod sync;`)
- Create: `crates/mnemos_core/src/sync/mod.rs`
- Create: `crates/mnemos_core/src/sync/state.rs`
- Test: `crates/mnemos_core/tests/sync_state.rs` (new)

- [ ] **Step 1: Failing test** — `crates/mnemos_core/tests/sync_state.rs`:

```rust
use mnemos_core::storage::Storage;
use mnemos_core::sync::state::{
    list_unresolved_conflicts, record_conflict, record_pull, record_push, resolve_conflict,
};
use chrono::Utc;
use tempfile::TempDir;

#[tokio::test]
async fn sync_state_records_and_lists_conflicts() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(&tmp.path().join("sync.db")).await.unwrap();
    let now = Utc::now();

    record_push(&s, now, None).await.unwrap();
    record_pull(&s, now, Some("git remote unreachable")).await.unwrap();
    let id = record_conflict(&s, "memories/mem_x.md", "filesystem", Some("Syncthing")).await.unwrap();

    let open = list_unresolved_conflicts(&s).await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].path, "memories/mem_x.md");

    resolve_conflict(&s, id, now).await.unwrap();
    assert!(list_unresolved_conflicts(&s).await.unwrap().is_empty());
}
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Create `crates/mnemos_core/src/sync/mod.rs`**

```rust
//! Pluggable cloud-sync backends. Files are the durable record; each backend
//! syncs the on-disk vault. The DB is rebuilt from files on pull when needed.

pub mod state;

use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Outcome of a push or pull operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncReport {
    pub files_changed: usize,
    pub conflicts: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStatus {
    pub backend: String,
    pub ready: bool,
    pub detail: String,
}

#[async_trait]
pub trait SyncBackend: Send + Sync {
    fn name(&self) -> &str;
    async fn push(&self, vault_root: &Path) -> Result<SyncReport>;
    async fn pull(&self, vault_root: &Path) -> Result<SyncReport>;
    async fn status(&self) -> Result<BackendStatus>;
}
```

- [ ] **Step 4: Create `crates/mnemos_core/src/sync/state.rs`**

```rust
//! sync_state + sync_conflicts persistence helpers.

use crate::error::Result;
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use libsql::params;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ConflictRow {
    pub id: i64,
    pub ts: String,
    pub path: String,
    pub detected_by: String,
    pub details: Option<String>,
}

pub async fn record_push(storage: &Storage, at: DateTime<Utc>, error: Option<&str>) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "UPDATE sync_state SET last_pushed_at = ?, last_error = ? WHERE id = 1",
        params![at.to_rfc3339(), error.map(|s| s.to_string())],
    )
    .await?;
    Ok(())
}

pub async fn record_pull(storage: &Storage, at: DateTime<Utc>, error: Option<&str>) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "UPDATE sync_state SET last_pulled_at = ?, last_error = ? WHERE id = 1",
        params![at.to_rfc3339(), error.map(|s| s.to_string())],
    )
    .await?;
    Ok(())
}

pub async fn record_conflict(
    storage: &Storage,
    path: &str,
    detected_by: &str,
    details: Option<&str>,
) -> Result<i64> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "INSERT INTO sync_conflicts (ts, path, detected_by, details) VALUES (?, ?, ?, ?)",
        params![
            Utc::now().to_rfc3339(),
            path.to_string(),
            detected_by.to_string(),
            details.map(|s| s.to_string())
        ],
    )
    .await?;
    let mut rows = conn.query("SELECT last_insert_rowid()", ()).await?;
    Ok(rows.next().await?.unwrap().get::<i64>(0)?)
}

pub async fn resolve_conflict(storage: &Storage, id: i64, at: DateTime<Utc>) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "UPDATE sync_conflicts SET resolved_at = ? WHERE id = ?",
        params![at.to_rfc3339(), id],
    )
    .await?;
    Ok(())
}

pub async fn list_unresolved_conflicts(storage: &Storage) -> Result<Vec<ConflictRow>> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT id, ts, path, detected_by, details FROM sync_conflicts
              WHERE resolved_at IS NULL ORDER BY ts DESC",
            (),
        )
        .await?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().await? {
        out.push(ConflictRow {
            id: r.get(0)?,
            ts: r.get(1)?,
            path: r.get(2)?,
            detected_by: r.get(3)?,
            details: r.get(4)?,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncStateRow {
    pub last_pushed_at: Option<String>,
    pub last_pulled_at: Option<String>,
    pub last_error: Option<String>,
}

pub async fn get_sync_state(storage: &Storage) -> Result<SyncStateRow> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query("SELECT last_pushed_at, last_pulled_at, last_error FROM sync_state WHERE id = 1", ())
        .await?;
    let r = rows.next().await?.ok_or_else(|| {
        crate::error::MnemosError::Internal("sync_state row missing".into())
    })?;
    Ok(SyncStateRow {
        last_pushed_at: r.get(0)?,
        last_pulled_at: r.get(1)?,
        last_error: r.get(2)?,
    })
}
```

- [ ] **Step 5: Add `pub mod sync;`** to `lib.rs`. Pass + commit.

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/lib.rs crates/mnemos_core/src/sync/ crates/mnemos_core/tests/sync_state.rs
git commit -m "feat: SyncBackend trait + sync state/conflict ops (Plan 7 Task 2)"
```

---

## Task 3: Filesystem-sync backend (Syncthing/Dropbox/iCloud/OneDrive)

Lowest-friction backend: the vault root already lives in a synced folder; the OS handles bytes. Our job is to **detect conflict files** the sync tool left behind and surface them. Push is a no-op; pull is a scan.

**Files:** `crates/mnemos_core/src/sync/filesystem.rs`, `mod.rs` (declare), `crates/mnemos_core/tests/sync_filesystem.rs`.

- [ ] **Step 1: Failing test** — `tests/sync_filesystem.rs`:

```rust
use mnemos_core::storage::Storage;
use mnemos_core::sync::filesystem::FilesystemSync;
use mnemos_core::sync::state::list_unresolved_conflicts;
use mnemos_core::sync::SyncBackend;
use tempfile::TempDir;
use tokio::fs;

#[tokio::test]
async fn filesystem_pull_detects_syncthing_and_dropbox_conflicts() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("memories")).await.unwrap();
    fs::write(root.join("memories/mem_a.md"), "---\n---\nok").await.unwrap();
    // Syncthing pattern
    fs::write(root.join("memories/mem_a.sync-conflict-20260101-000000-LAPTOP.md"), "x").await.unwrap();
    // Dropbox pattern
    fs::write(root.join("memories/mem_a (Shaun's conflicted copy 2026-05-01).md"), "x").await.unwrap();

    let storage = Storage::open(&root.join(".mnemos.db")).await.unwrap();
    let backend = FilesystemSync::new(storage.clone());
    let report = backend.pull(root).await.unwrap();
    assert_eq!(report.conflicts.len(), 2);

    let open = list_unresolved_conflicts(&storage).await.unwrap();
    assert_eq!(open.len(), 2);

    // push is a no-op but returns Ok
    let r2 = backend.push(root).await.unwrap();
    assert!(r2.message.to_lowercase().contains("no-op") || r2.files_changed == 0);
}
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Create `crates/mnemos_core/src/sync/filesystem.rs`**

```rust
//! Filesystem-sync backend. The vault lives in a Syncthing/Dropbox/iCloud/
//! OneDrive folder; the OS handles bytes. We detect conflict files and surface
//! them through `sync_conflicts` so the UI can present a resolution flow.

use crate::error::Result;
use crate::storage::Storage;
use crate::sync::state::{list_unresolved_conflicts, record_conflict};
use crate::sync::{BackendStatus, SyncBackend, SyncReport};
use async_trait::async_trait;
use std::path::Path;
use walkdir::WalkDir;

pub struct FilesystemSync {
    storage: Storage,
}

impl FilesystemSync {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }
}

/// Heuristics for the four common conflict-file naming conventions.
pub fn is_conflict_file(name: &str) -> bool {
    name.contains(".sync-conflict-")                              // Syncthing
        || name.contains("conflicted copy")                       // Dropbox
        || name.contains(" (Case Conflict")                       // iCloud rare
        || name.ends_with(".collision.md")                        // OneDrive style
}

#[async_trait]
impl SyncBackend for FilesystemSync {
    fn name(&self) -> &str { "filesystem" }

    async fn push(&self, _vault_root: &Path) -> Result<SyncReport> {
        Ok(SyncReport { files_changed: 0, conflicts: vec![], message: "no-op (OS handles sync)".into() })
    }

    async fn pull(&self, vault_root: &Path) -> Result<SyncReport> {
        let mut conflicts: Vec<String> = Vec::new();
        for entry in WalkDir::new(vault_root).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if is_conflict_file(&name) {
                let rel = entry.path().strip_prefix(vault_root).unwrap_or(entry.path()).to_string_lossy().to_string();
                // dedupe against already-known unresolved
                let known = list_unresolved_conflicts(&self.storage).await?;
                if !known.iter().any(|c| c.path == rel) {
                    record_conflict(&self.storage, &rel, "filesystem", Some(&name)).await?;
                }
                conflicts.push(rel);
            }
        }
        Ok(SyncReport {
            files_changed: 0,
            message: format!("detected {} conflict file(s)", conflicts.len()),
            conflicts,
        })
    }

    async fn status(&self) -> Result<BackendStatus> {
        Ok(BackendStatus { backend: "filesystem".into(), ready: true, detail: "OS-managed sync".into() })
    }
}
```

- [ ] **Step 4: Declare module + add `walkdir` dep** to `crates/mnemos_core/Cargo.toml`: `walkdir = "2"`. Declare `pub mod filesystem;` in `sync/mod.rs`.

- [ ] **Step 5: Pass + commit.**
```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/Cargo.toml crates/mnemos_core/src/sync/filesystem.rs crates/mnemos_core/src/sync/mod.rs crates/mnemos_core/tests/sync_filesystem.rs
git commit -m "feat: filesystem-sync backend with conflict detection (Plan 7 Task 3)"
```

---

## Task 4: Git-remote backend + `mnemos-merge-driver` binary

Shell out to `git` from the vault root. A separate small binary (`mnemos-merge-driver`) is registered as a custom merge driver for `*.md` memory files, performing YAML-aware frontmatter merges (latest `valid_at` wins, union of tags, keep any `invalid_at` if either side has one, latest `superseded_by`).

**Files (new crate):** `crates/mnemos_merge_driver/Cargo.toml`, `src/main.rs`. **Modify:** workspace `Cargo.toml` (add member), `crates/mnemos_core/src/sync/git.rs` (new), `mod.rs`, test `tests/sync_git.rs`.

- [ ] **Step 1: Create the merge-driver crate.** Workspace `Cargo.toml` `[workspace] members = [..., "crates/mnemos_merge_driver"]`. `crates/mnemos_merge_driver/Cargo.toml`:

```toml
[package]
name = "mnemos_merge_driver"
version = "0.6.0"
edition = "2021"

[[bin]]
name = "mnemos-merge-driver"
path = "src/main.rs"

[dependencies]
serde = { workspace = true }
serde_yaml = { workspace = true }
anyhow = { workspace = true }
```

`crates/mnemos_merge_driver/src/main.rs`:

```rust
//! Git custom merge driver for mnemos memory `.md` files.
//!
//! Invoked by git as: `mnemos-merge-driver %A %O %B` where %A is the current
//! (ours) version that will be overwritten with the merge result, %O the
//! ancestor, %B theirs. Exit 0 = clean merge; non-zero = conflict.
//!
//! Strategy: parse YAML frontmatter on each side; merge frontmatter field by
//! field with conservative rules; concatenate bodies separated by a marker if
//! both sides changed the body, otherwise take whichever side changed.

use anyhow::{Context, Result};
use serde_yaml::{Mapping, Value};
use std::fs;
use std::path::PathBuf;

fn split(text: &str) -> (Option<String>, String) {
    let trimmed = text.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let fm = rest[..end].to_string();
            let body = rest[end..].strip_prefix("\n---").unwrap_or("").trim_start_matches('\n').to_string();
            return (Some(fm), body);
        }
    }
    (None, text.to_string())
}

fn merge_yaml(a: &Value, b: &Value, base: &Value) -> Value {
    use Value::*;
    match (a, b, base) {
        (Mapping(am), Mapping(bm), Mapping(basem)) => {
            let mut out = am.clone();
            for (k, bv) in bm {
                let av = am.get(k);
                let basev = basem.get(k);
                match (av, basev) {
                    (Some(av), Some(basev)) if av == basev => {
                        // a unchanged → take b
                        out.insert(k.clone(), bv.clone());
                    }
                    (None, None) => {
                        out.insert(k.clone(), bv.clone());
                    }
                    (Some(_), _) => {
                        // both modified — field-specific merge
                        let key = k.as_str().unwrap_or("");
                        out.insert(k.clone(), merge_field(key, av.unwrap(), bv, basev));
                    }
                    _ => { out.insert(k.clone(), bv.clone()); }
                }
            }
            Mapping(out)
        }
        _ => a.clone(),
    }
}

fn merge_field(key: &str, a: &Value, b: &Value, _base: Option<&Value>) -> Value {
    match key {
        // Latest invalidation wins (presence-prefers).
        "invalid_at" => if a.is_null() { b.clone() } else { a.clone() },
        // Latest supersede pointer wins.
        "superseded_by" => if a.is_null() { b.clone() } else { a.clone() },
        // Union of tags.
        "tags" => union_sequences(a, b),
        // Higher strength/importance wins (conservative).
        "strength" | "importance" => max_num(a, b),
        // Latest timestamps win.
        "valid_at" | "ingested_at" | "last_accessed" => later_string(a, b),
        // Default: prefer a (ours).
        _ => a.clone(),
    }
}

fn union_sequences(a: &Value, b: &Value) -> Value {
    let mut out: Vec<Value> = Vec::new();
    if let Some(s) = a.as_sequence() { out.extend(s.clone()); }
    if let Some(s) = b.as_sequence() {
        for v in s {
            if !out.contains(v) { out.push(v.clone()); }
        }
    }
    Value::Sequence(out)
}

fn max_num(a: &Value, b: &Value) -> Value {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => Value::Number(serde_yaml::Number::from(x.max(y))),
        _ => a.clone(),
    }
}

fn later_string(a: &Value, b: &Value) -> Value {
    match (a.as_str(), b.as_str()) {
        (Some(x), Some(y)) => if x >= y { a.clone() } else { b.clone() },
        _ => a.clone(),
    }
}

fn render(fm: &Value, body: &str) -> Result<String> {
    let fm_str = serde_yaml::to_string(fm)?;
    Ok(format!("---\n{}---\n{}", fm_str, body))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: mnemos-merge-driver %A %O %B");
        std::process::exit(2);
    }
    let a_path = PathBuf::from(&args[0]);
    let o_path = PathBuf::from(&args[1]);
    let b_path = PathBuf::from(&args[2]);
    let a = fs::read_to_string(&a_path).context("read A")?;
    let o = fs::read_to_string(&o_path).context("read O")?;
    let b = fs::read_to_string(&b_path).context("read B")?;

    let (fa, ba) = split(&a); let (fo, bo) = split(&o); let (fb, bb) = split(&b);
    let parse = |s: &Option<String>| -> Value { s.as_deref().map(|x| serde_yaml::from_str(x).unwrap_or(Value::Mapping(Mapping::new()))).unwrap_or(Value::Mapping(Mapping::new())) };
    let merged_fm = merge_yaml(&parse(&fa), &parse(&fb), &parse(&fo));
    // Body merge: prefer the side that differs from base; if both differ, concatenate with a marker.
    let merged_body = if ba == bo { bb }
        else if bb == bo { ba }
        else { format!("{}\n\n<!-- mnemos-merge: both sides changed body -->\n\n{}", ba, bb) };
    let out = render(&merged_fm, &merged_body)?;
    fs::write(&a_path, out).context("write A")?;
    Ok(())
}
```

Add `serde_yaml` to the workspace `[workspace.dependencies]` if not already there (Plan 1 added it for `mnemos_core` — verify it's a workspace dep; otherwise add `serde_yaml = "0.9"`).

- [ ] **Step 2: Failing test** — `tests/sync_git.rs`:

```rust
use mnemos_core::storage::Storage;
use mnemos_core::sync::git::GitSync;
use mnemos_core::sync::SyncBackend;
use std::process::Command;
use tempfile::TempDir;
use tokio::fs;

fn have_git() -> bool { which::which("git").is_ok() }

#[tokio::test]
async fn git_push_pull_round_trip_via_local_bare_remote() {
    if !have_git() { eprintln!("git not on PATH; skipping"); return; }
    let tmp = TempDir::new().unwrap();
    let remote = tmp.path().join("remote.git");
    Command::new("git").args(["init", "--bare"]).arg(&remote).status().unwrap();
    let local = tmp.path().join("local");
    fs::create_dir_all(local.join("memories")).await.unwrap();
    fs::write(local.join("memories/mem_a.md"), "---\nid: mem_a\n---\nhello").await.unwrap();
    for args in [
        vec!["-C", local.to_str().unwrap(), "init"],
        vec!["-C", local.to_str().unwrap(), "remote", "add", "origin", remote.to_str().unwrap()],
        vec!["-C", local.to_str().unwrap(), "config", "user.email", "t@t.test"],
        vec!["-C", local.to_str().unwrap(), "config", "user.name", "Test"],
    ] { Command::new("git").args(&args).status().unwrap(); }

    let storage = Storage::open(&local.join(".mnemos.db")).await.unwrap();
    let backend = GitSync::new(storage, remote.to_string_lossy().to_string(), "main".into());
    let r = backend.push(&local).await.unwrap();
    assert!(r.message.to_lowercase().contains("pushed") || r.files_changed >= 1);

    // clone into a second checkout and pull
    let other = tmp.path().join("other");
    Command::new("git").args(["clone", remote.to_str().unwrap(), other.to_str().unwrap()]).status().unwrap();
    assert!(other.join("memories/mem_a.md").exists());
}
```

- [ ] **Step 3: Verify fail.**

- [ ] **Step 4: Create `crates/mnemos_core/src/sync/git.rs`** (shells out via `tokio::process`)

```rust
//! Git-remote sync backend. Shells out to `git` from the vault root.
//!
//! Push:  git add . → git commit -m "mnemos sync …" → git push origin <branch>
//! Pull:  git pull --rebase origin <branch>
//!
//! The first push initializes `.gitattributes` to route `*.md` merges through
//! the `mnemos-merge-driver` binary the user has on PATH.

use crate::error::{MnemosError, Result};
use crate::storage::Storage;
use crate::sync::{BackendStatus, SyncBackend, SyncReport};
use async_trait::async_trait;
use chrono::Utc;
use std::path::Path;
use tokio::fs;
use tokio::process::Command;

pub struct GitSync {
    #[allow(dead_code)] storage: Storage,
    remote: String,
    branch: String,
}

impl GitSync {
    pub fn new(storage: Storage, remote: String, branch: String) -> Self {
        Self { storage, remote, branch }
    }
}

async fn run(cwd: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").current_dir(cwd).args(args).output().await
        .map_err(|e| MnemosError::Internal(format!("git invocation failed: {e}")))?;
    if !out.status.success() {
        return Err(MnemosError::Internal(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

async fn ensure_gitattributes(root: &Path) -> Result<()> {
    let path = root.join(".gitattributes");
    let line = "*.md merge=mnemos-frontmatter\n";
    if !path.exists() {
        fs::write(&path, line).await?;
        return Ok(());
    }
    let cur = fs::read_to_string(&path).await?;
    if !cur.contains("mnemos-frontmatter") {
        fs::write(&path, format!("{cur}\n{line}")).await?;
    }
    Ok(())
}

async fn ensure_merge_driver_config(root: &Path) -> Result<()> {
    // Register the custom merge driver locally; users must have
    // `mnemos-merge-driver` on PATH (installed alongside `mnemos`).
    let _ = run(root, &["config", "merge.mnemos-frontmatter.name", "mnemos memory frontmatter merge"]).await;
    let _ = run(root, &["config", "merge.mnemos-frontmatter.driver", "mnemos-merge-driver %A %O %B"]).await;
    Ok(())
}

#[async_trait]
impl SyncBackend for GitSync {
    fn name(&self) -> &str { "git" }

    async fn push(&self, vault_root: &Path) -> Result<SyncReport> {
        ensure_gitattributes(vault_root).await?;
        ensure_merge_driver_config(vault_root).await?;
        run(vault_root, &["add", "."]).await?;
        let status = run(vault_root, &["status", "--porcelain"]).await?;
        if status.trim().is_empty() {
            return Ok(SyncReport { files_changed: 0, conflicts: vec![], message: "nothing to push".into() });
        }
        let msg = format!("mnemos sync {}", Utc::now().to_rfc3339());
        run(vault_root, &["commit", "-m", &msg]).await?;
        // ensure remote exists; if already configured, this is a no-op fail we ignore
        let _ = run(vault_root, &["remote", "add", "origin", &self.remote]).await;
        run(vault_root, &["push", "origin", &self.branch]).await?;
        Ok(SyncReport {
            files_changed: status.lines().count(),
            conflicts: vec![],
            message: format!("pushed to {}", self.remote),
        })
    }

    async fn pull(&self, vault_root: &Path) -> Result<SyncReport> {
        ensure_merge_driver_config(vault_root).await?;
        let _ = run(vault_root, &["remote", "add", "origin", &self.remote]).await;
        match run(vault_root, &["pull", "--rebase", "origin", &self.branch]).await {
            Ok(out) => Ok(SyncReport { files_changed: 0, conflicts: vec![], message: out.lines().last().unwrap_or("pulled").to_string() }),
            Err(e) => Err(e),
        }
    }

    async fn status(&self) -> Result<BackendStatus> {
        let ready = which::which("git").is_ok();
        Ok(BackendStatus { backend: "git".into(), ready, detail: if ready { format!("remote {}", self.remote) } else { "git not on PATH".into() } })
    }
}
```

Add `which = { workspace = true }` to `crates/mnemos_core/Cargo.toml` if not already there (Plan 1 added `which` to the workspace).

- [ ] **Step 5: Declare module + pass + commit.** Add `pub mod git;` in `sync/mod.rs`.
```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add Cargo.toml crates/mnemos_merge_driver/ crates/mnemos_core/Cargo.toml crates/mnemos_core/src/sync/git.rs crates/mnemos_core/src/sync/mod.rs crates/mnemos_core/tests/sync_git.rs
git commit -m "feat: Git-remote sync backend + mnemos-merge-driver (Plan 7 Task 4)"
```

---

## Task 5: S3-compatible backend (rclone)

Shell out to `rclone sync` for both directions. The user configures an `rclone` remote (`rclone config`) and points the `[sync.s3]` config at `remote:bucket/path`.

**Files:** `crates/mnemos_core/src/sync/s3.rs`, `mod.rs` (declare), `tests/sync_s3.rs` (`#[ignore]`-d if `rclone` absent).

- [ ] **Step 1: Failing test** — `tests/sync_s3.rs`:

```rust
use mnemos_core::storage::Storage;
use mnemos_core::sync::s3::S3Sync;
use mnemos_core::sync::SyncBackend;
use tempfile::TempDir;

#[tokio::test]
async fn s3_status_reports_rclone_presence() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join(".mnemos.db")).await.unwrap();
    let backend = S3Sync::new(storage, "missing-remote:bucket/path".into());
    let s = backend.status().await.unwrap();
    assert_eq!(s.backend, "s3");
    // ready iff rclone is on PATH
    assert_eq!(s.ready, which::which("rclone").is_ok());
}

#[tokio::test]
#[ignore = "needs a configured rclone remote"]
async fn s3_push_pull_live() {
    // Run manually with a real `rclone` remote configured.
}
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Create `crates/mnemos_core/src/sync/s3.rs`**

```rust
//! S3-compatible sync backend via `rclone`.

use crate::error::{MnemosError, Result};
use crate::storage::Storage;
use crate::sync::{BackendStatus, SyncBackend, SyncReport};
use async_trait::async_trait;
use std::path::Path;
use tokio::process::Command;

pub struct S3Sync {
    #[allow(dead_code)] storage: Storage,
    /// rclone-style target, e.g. "myremote:bucket-name/path".
    remote: String,
}

impl S3Sync {
    pub fn new(storage: Storage, remote: String) -> Self {
        Self { storage, remote }
    }
}

async fn rclone(args: &[&str]) -> Result<String> {
    let out = Command::new("rclone").args(args).output().await
        .map_err(|e| MnemosError::Internal(format!("rclone invocation failed: {e}")))?;
    if !out.status.success() {
        return Err(MnemosError::Internal(format!(
            "rclone {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[async_trait]
impl SyncBackend for S3Sync {
    fn name(&self) -> &str { "s3" }

    async fn push(&self, vault_root: &Path) -> Result<SyncReport> {
        let local = vault_root.to_string_lossy().to_string();
        rclone(&["sync", "--fast-list", &local, &self.remote]).await?;
        Ok(SyncReport { files_changed: 0, conflicts: vec![], message: format!("rclone sync → {}", self.remote) })
    }

    async fn pull(&self, vault_root: &Path) -> Result<SyncReport> {
        let local = vault_root.to_string_lossy().to_string();
        rclone(&["sync", "--fast-list", &self.remote, &local]).await?;
        Ok(SyncReport { files_changed: 0, conflicts: vec![], message: format!("rclone sync ← {}", self.remote) })
    }

    async fn status(&self) -> Result<BackendStatus> {
        let ready = which::which("rclone").is_ok();
        Ok(BackendStatus { backend: "s3".into(), ready, detail: if ready { format!("rclone target {}", self.remote) } else { "rclone not on PATH — install rclone and run `rclone config`".into() } })
    }
}
```

- [ ] **Step 4: Declare + pass + commit.** Add `pub mod s3;` in `sync/mod.rs`.
```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/sync/s3.rs crates/mnemos_core/src/sync/mod.rs crates/mnemos_core/tests/sync_s3.rs
git commit -m "feat: S3-compatible sync backend via rclone (Plan 7 Task 5)"
```

---

## Task 6: Sync config, worker, REST, WS events

Daemon glue: `[sync]` config block, a `sync_worker` periodic tokio task spawned in `build_app_full`, REST endpoints (`GET /v1/sync/status`, `POST /v1/sync/push`, `POST /v1/sync/pull`, `GET /v1/sync/conflicts`), and WS events.

**Files:** `crates/mnemos_daemon/src/config.rs` (add), `events.rs` (variants), `routes/sync.rs` (new), `routes/mod.rs` (mount), `sync_worker.rs` (new), `lib.rs` (spawn in `build_app_full` + handle in shutdown), test `tests/sync.rs`.

- [ ] **Step 1: Failing test** — `tests/sync.rs`:

```rust
use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

#[tokio::test]
async fn sync_status_endpoint_returns_backend_summary() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let mut cfg = Config::default();
    cfg.sync.kind = mnemos_daemon::config::SyncKind::Filesystem;
    let (app, state) = build_app(cfg, vault).await.unwrap();
    let (s, b) = call(app, "GET", "/v1/sync/status", Some(&state.token), "").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["backend"], "filesystem");
    assert_eq!(v["ready"], true);
}

async fn call(app: axum::Router, method: &str, uri: &str, auth: Option<&str>, body: &str) -> (StatusCode, String) {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder().method(method).uri(uri).header("content-type", "application/json");
    if let Some(t) = auth { req = req.header("authorization", format!("Bearer {t}")); }
    let req = req.body(Body::from(body.to_string())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let s = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (s, String::from_utf8_lossy(&bytes).to_string())
}
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Add `[sync]` config** to `crates/mnemos_daemon/src/config.rs`. Add the `pub sync: SyncConfig,` field and:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    pub kind: SyncKind,
    /// Periodic push/pull interval in seconds (0 = disabled, manual only).
    pub interval_secs: u64,
    pub git: GitSyncConfig,
    pub s3: S3SyncConfig,
    /// Reserved for Turso embedded replicas (Plan 7+). Not wired in v0.6.0.
    pub turso: TursoSyncConfig,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SyncKind { None, Filesystem, Git, S3 }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GitSyncConfig { pub remote: String, pub branch: String }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct S3SyncConfig { pub remote: String }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TursoSyncConfig { pub enabled: bool, pub url: String, pub auth_token: String }

impl Default for SyncConfig {
    fn default() -> Self {
        Self { kind: SyncKind::None, interval_secs: 0, git: GitSyncConfig { branch: "main".into(), ..Default::default() }, s3: S3SyncConfig::default(), turso: TursoSyncConfig::default() }
    }
}
```

- [ ] **Step 4: Add events** to `events.rs`:

```rust
    SyncStarted { backend: String, direction: String },
    SyncCompleted { backend: String, direction: String, files_changed: usize },
    SyncFailed { backend: String, direction: String, error: String },
    SyncConflict { path: String, detected_by: String },
```

- [ ] **Step 5: Create `crates/mnemos_daemon/src/routes/sync.rs`**

```rust
//! `GET /v1/sync/status`, `POST /v1/sync/push|pull`, `GET /v1/sync/conflicts`.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use mnemos_core::sync::{state, SyncBackend};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::config::SyncKind;
use crate::error::ApiError;
use crate::events::Event;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/sync/status", get(status))
        .route("/v1/sync/push", post(push))
        .route("/v1/sync/pull", post(pull))
        .route("/v1/sync/conflicts", get(conflicts))
}

fn make_backend(state: &AppState) -> Option<Arc<dyn SyncBackend>> {
    use mnemos_core::sync::{filesystem::FilesystemSync, git::GitSync, s3::S3Sync};
    let s = state.vault.storage().clone();
    match state.config.sync.kind {
        SyncKind::None => None,
        SyncKind::Filesystem => Some(Arc::new(FilesystemSync::new(s))),
        SyncKind::Git => Some(Arc::new(GitSync::new(s, state.config.sync.git.remote.clone(), state.config.sync.git.branch.clone()))),
        SyncKind::S3 => Some(Arc::new(S3Sync::new(s, state.config.sync.s3.remote.clone()))),
    }
}

async fn status(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    match make_backend(&state) {
        None => Ok(Json(json!({ "backend": "none", "ready": false, "detail": "sync disabled" }))),
        Some(b) => {
            let st = b.status().await?;
            let row = state::get_sync_state(state.vault.storage()).await.ok();
            Ok(Json(json!({
                "backend": st.backend, "ready": st.ready, "detail": st.detail,
                "last_pushed_at": row.as_ref().and_then(|r| r.last_pushed_at.clone()),
                "last_pulled_at": row.as_ref().and_then(|r| r.last_pulled_at.clone()),
                "last_error":     row.as_ref().and_then(|r| r.last_error.clone()),
            })))
        }
    }
}

async fn push(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let b = make_backend(&state).ok_or_else(|| ApiError::new(axum::http::StatusCode::CONFLICT, "sync disabled"))?;
    state.events.publish(Event::SyncStarted { backend: b.name().into(), direction: "push".into() });
    match b.push(state.vault.paths().files_root()).await {
        Ok(r) => {
            state::record_push(state.vault.storage(), Utc::now(), None).await?;
            state.events.publish(Event::SyncCompleted { backend: b.name().into(), direction: "push".into(), files_changed: r.files_changed });
            Ok(Json(json!({ "files_changed": r.files_changed, "message": r.message, "conflicts": r.conflicts })))
        }
        Err(e) => {
            state::record_push(state.vault.storage(), Utc::now(), Some(&e.to_string())).await?;
            state.events.publish(Event::SyncFailed { backend: b.name().into(), direction: "push".into(), error: e.to_string() });
            Err(e.into())
        }
    }
}

async fn pull(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let b = make_backend(&state).ok_or_else(|| ApiError::new(axum::http::StatusCode::CONFLICT, "sync disabled"))?;
    state.events.publish(Event::SyncStarted { backend: b.name().into(), direction: "pull".into() });
    match b.pull(state.vault.paths().files_root()).await {
        Ok(r) => {
            state::record_pull(state.vault.storage(), Utc::now(), None).await?;
            for c in &r.conflicts {
                state.events.publish(Event::SyncConflict { path: c.clone(), detected_by: b.name().into() });
            }
            state.events.publish(Event::SyncCompleted { backend: b.name().into(), direction: "pull".into(), files_changed: r.files_changed });
            Ok(Json(json!({ "files_changed": r.files_changed, "message": r.message, "conflicts": r.conflicts })))
        }
        Err(e) => {
            state::record_pull(state.vault.storage(), Utc::now(), Some(&e.to_string())).await?;
            state.events.publish(Event::SyncFailed { backend: b.name().into(), direction: "pull".into(), error: e.to_string() });
            Err(e.into())
        }
    }
}

async fn conflicts(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let rows = state::list_unresolved_conflicts(state.vault.storage()).await?;
    Ok(Json(json!({ "conflicts": rows })))
}
```

> If `Paths` doesn't have `files_root()`, add it: `pub fn files_root(&self) -> &std::path::Path { &self.root }` (the vault root is the files dir).

- [ ] **Step 6: Create `crates/mnemos_daemon/src/sync_worker.rs`** (periodic worker, modeled on the decay worker from Plan 4):

```rust
//! Periodic sync worker. Runs `pull` then `push` on the configured interval.

use crate::state::AppState;
use std::sync::Arc;
use tokio::sync::watch;

pub struct SyncHandle {
    pub(crate) join: tokio::task::JoinHandle<()>,
    pub(crate) shutdown: watch::Sender<bool>,
}

impl SyncHandle {
    pub async fn shutdown(self) { let _ = self.shutdown.send(true); let _ = self.join.await; }
}

pub fn spawn(state: AppState) -> Option<SyncHandle> {
    let interval_secs = state.config.sync.interval_secs;
    if interval_secs == 0 { return None; }
    use crate::config::SyncKind;
    if matches!(state.config.sync.kind, SyncKind::None) { return None; }

    let (tx, mut rx) = watch::channel(false);
    let join = tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        tick.tick().await; // consume the immediate first tick
        loop {
            tokio::select! {
                _ = rx.changed() => { if *rx.borrow() { break; } }
                _ = tick.tick() => {
                    let _ = run_once(&state).await;
                }
            }
        }
    });
    Some(SyncHandle { join, shutdown: tx })
}

async fn run_once(state: &AppState) -> anyhow::Result<()> {
    use mnemos_core::sync::SyncBackend;
    use mnemos_core::sync::{filesystem::FilesystemSync, git::GitSync, s3::S3Sync};
    let backend: Arc<dyn SyncBackend> = match state.config.sync.kind {
        crate::config::SyncKind::None => return Ok(()),
        crate::config::SyncKind::Filesystem => Arc::new(FilesystemSync::new(state.vault.storage().clone())),
        crate::config::SyncKind::Git => Arc::new(GitSync::new(
            state.vault.storage().clone(),
            state.config.sync.git.remote.clone(),
            state.config.sync.git.branch.clone(),
        )),
        crate::config::SyncKind::S3 => Arc::new(S3Sync::new(state.vault.storage().clone(), state.config.sync.s3.remote.clone())),
    };
    let files_root = state.vault.paths().files_root().to_path_buf();
    if let Ok(r) = backend.pull(&files_root).await {
        for c in &r.conflicts {
            state.events.publish(crate::events::Event::SyncConflict { path: c.clone(), detected_by: backend.name().into() });
        }
    }
    let _ = backend.push(&files_root).await;
    Ok(())
}
```

- [ ] **Step 7: Wire `sync_worker::spawn` into `build_app_full`** (alongside the pipeline runner), bundling its handle into the returned tuple as an additional `Option<SyncHandle>`, and join it on shutdown in `main.rs`. Mount the router in `routes/mod.rs`. Run tests + commit.

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/ crates/mnemos_daemon/tests/sync.rs
git commit -m "feat: sync config, worker, REST endpoints, WS events (Plan 7 Task 6)"
```

---

## Task 7: `mnemos sync` CLI commands

`mnemos sync push | pull | status` — local CLI that hits the daemon's sync endpoints (or runs the backend in-process if no daemon).

**Files:** `crates/mnemos_cli/src/cli.rs` (add subcommand), `commands/sync.rs` (new), `commands/mod.rs` (declare), `main.rs` (dispatch).

- [ ] **Step 1: Failing test** — `crates/mnemos_cli/src/commands/sync.rs` `#[cfg(test)]` smoke (no daemon required — runs in-process filesystem backend on a temp vault):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    #[tokio::test]
    async fn sync_status_on_empty_vault_is_disabled() {
        std::env::set_var("MNEMOS_EMBEDDER", "none");
        let tmp = TempDir::new().unwrap();
        run(Some(tmp.path().to_path_buf()), true, SyncAction::Status).await.unwrap();
    }
}
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Add to `cli.rs`:**

```rust
    /// Sync the vault with the configured backend.
    Sync(SyncArgs),
```

```rust
#[derive(clap::Args, Debug)] pub struct SyncArgs { #[command(subcommand)] pub action: SyncAction }
#[derive(Subcommand, Debug)] pub enum SyncAction { Push, Pull, Status }
```

- [ ] **Step 4: `crates/mnemos_cli/src/commands/sync.rs`** — in-process implementation (the CLI opens the vault directly, then runs the configured backend):

```rust
use crate::cli::SyncAction;
use crate::commands::open_vault;
use anyhow::Result;
use mnemos_core::sync::{filesystem::FilesystemSync, git::GitSync, s3::S3Sync, SyncBackend};
use std::path::PathBuf;

fn backend_from_env(storage: mnemos_core::storage::Storage) -> Option<Box<dyn SyncBackend>> {
    let kind = std::env::var("MNEMOS_SYNC_KIND").unwrap_or_else(|_| "none".into());
    match kind.as_str() {
        "filesystem" => Some(Box::new(FilesystemSync::new(storage))),
        "git" => {
            let remote = std::env::var("MNEMOS_SYNC_GIT_REMOTE").ok()?;
            let branch = std::env::var("MNEMOS_SYNC_GIT_BRANCH").unwrap_or_else(|_| "main".into());
            Some(Box::new(GitSync::new(storage, remote, branch)))
        }
        "s3" => {
            let remote = std::env::var("MNEMOS_SYNC_S3_REMOTE").ok()?;
            Some(Box::new(S3Sync::new(storage, remote)))
        }
        _ => None,
    }
}

pub async fn run(vault: Option<PathBuf>, json: bool, action: SyncAction) -> Result<()> {
    let v = open_vault(vault).await?;
    let backend = backend_from_env(v.storage().clone());
    let report = match (backend, action) {
        (None, _) => { println!("sync disabled (set MNEMOS_SYNC_KIND or use the daemon's [sync] config)"); return Ok(()); }
        (Some(b), SyncAction::Status) => {
            let s = b.status().await?;
            if json { println!("{}", serde_json::to_string(&s)?); } else { println!("backend: {}  ready: {}  detail: {}", s.backend, s.ready, s.detail); }
            return Ok(());
        }
        (Some(b), SyncAction::Push) => b.push(v.paths().files_root()).await?,
        (Some(b), SyncAction::Pull) => b.pull(v.paths().files_root()).await?,
    };
    if json { println!("{}", serde_json::to_string(&report)?); }
    else { println!("changed {}  conflicts {}  {}", report.files_changed, report.conflicts.len(), report.message); }
    Ok(())
}
```

- [ ] **Step 5: Declare + dispatch + pass + commit.**
```bash
cargo fmt --all && cargo clippy -p mnemos_cli --all-targets -- -D warnings
git add crates/mnemos_cli/src/
git commit -m "feat: 'mnemos sync push|pull|status' CLI (Plan 7 Task 7)"
```

---

# Group B — Entity merge + promote-to-procedural

## Task 8: `merge_entities` core + `POST /v1/entities/merge`

Reassign mentions and edges from `source` to `target`, remove the source entity, audit. Transaction-wrapped.

**Files:** `crates/mnemos_core/src/storage/entity_ops.rs` (add `merge_entities`), `crates/mnemos_daemon/src/routes/entities.rs` (add route), test `crates/mnemos_daemon/tests/entities.rs` (extend).

- [ ] **Step 1: Failing test** — append to `tests/entities.rs`:

```rust
#[tokio::test]
async fn merge_entities_rewrites_mentions_and_edges() {
    let (app, token, a, _mem) = fixture().await;
    // Create a second entity B linked to A by an edge, mentioned by mem_1.
    // (Use the existing fixture's mem + entity A; add B via the merge call.)
    let b = mnemos_core::storage::entity_ops::upsert_entity(
        &mnemos_core::vault::Vault::open(mnemos_core::paths::Paths::with_root(std::env::temp_dir())).await.unwrap().storage().clone(),
        "Tauri", "tool",
    ).await.unwrap();
    let _ = b; // we'll trust the fixture's B from Task 2 instead — use the same setup
    let (s, body) = call(app, "POST", "/v1/entities/merge", Some(&token),
        &format!(r#"{{"source":"{a}","target":"ent_does_not_exist"}}"#)).await;
    // Missing target → 404
    assert_eq!(s, axum::http::StatusCode::NOT_FOUND, "{body}");
}
```

(Keep the happy-path covered by the core unit test below; the HTTP test asserts the wrapping + error case.)

Core unit test in `entity_ops.rs` (in-module `#[cfg(test)]`):

```rust
#[cfg(test)]
mod merge_tests {
    use super::*;
    use crate::paths::Paths;
    use crate::vault::Vault;
    use tempfile::TempDir;

    #[tokio::test]
    async fn merge_moves_mentions_and_edges() {
        let tmp = TempDir::new().unwrap();
        let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
        let a = upsert_entity(v.storage(), "A", "x").await.unwrap();
        let b = upsert_entity(v.storage(), "B", "x").await.unwrap();
        let c = upsert_entity(v.storage(), "C", "x").await.unwrap();
        upsert_edge(v.storage(), &a, &c, "rel", "mem_x", chrono::Utc::now()).await.unwrap();
        link_entity_mention(v.storage(), "mem_x", &a).await.unwrap();

        merge_entities(v.storage(), &a, &b).await.unwrap();

        let conn = v.storage().conn().unwrap();
        // source removed
        let mut r1 = conn.query("SELECT COUNT(*) FROM entities WHERE id = ?", libsql::params![a.clone()]).await.unwrap();
        let n1: i64 = r1.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(n1, 0);
        // mention reassigned
        let mut r2 = conn.query("SELECT COUNT(*) FROM entity_mentions WHERE entity_id = ?", libsql::params![b.clone()]).await.unwrap();
        let n2: i64 = r2.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(n2, 1);
        // edge reassigned
        let mut r3 = conn.query("SELECT COUNT(*) FROM entity_edges WHERE source_entity_id = ? OR target_entity_id = ?", libsql::params![b.clone(), b.clone()]).await.unwrap();
        let n3: i64 = r3.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(n3, 1);
    }
}
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Add `merge_entities`** to `entity_ops.rs`:

```rust
/// Reassign all mentions and edges from `source` to `target`, then delete the
/// source entity row. Self-loops created by the merge are removed. Transaction-
/// wrapped; idempotent if `source` is already gone.
pub async fn merge_entities(storage: &Storage, source: &str, target: &str) -> Result<()> {
    if source == target {
        return Ok(());
    }
    let (conn, _g) = storage.write_conn().await?;
    let tx = conn.transaction().await?;
    // Verify target exists; if source already gone, treat as no-op.
    let mut t = tx.query("SELECT 1 FROM entities WHERE id = ?", params![target.to_string()]).await?;
    if t.next().await?.is_none() {
        return Err(crate::error::MnemosError::EntityNotFound(target.into()));
    }
    drop(t);
    let mut s = tx.query("SELECT 1 FROM entities WHERE id = ?", params![source.to_string()]).await?;
    let source_present = s.next().await?.is_some();
    drop(s);
    if !source_present { return Ok(()); }

    // Mentions: simple UPDATE; clean up dupes that result.
    tx.execute(
        "INSERT OR IGNORE INTO entity_mentions (memory_id, entity_id)
            SELECT memory_id, ? FROM entity_mentions WHERE entity_id = ?",
        params![target.to_string(), source.to_string()],
    ).await?;
    tx.execute("DELETE FROM entity_mentions WHERE entity_id = ?", params![source.to_string()]).await?;

    // Edges: rewrite endpoints; drop self-loops created by the merge.
    tx.execute(
        "UPDATE entity_edges SET source_entity_id = ? WHERE source_entity_id = ?",
        params![target.to_string(), source.to_string()],
    ).await?;
    tx.execute(
        "UPDATE entity_edges SET target_entity_id = ? WHERE target_entity_id = ?",
        params![target.to_string(), source.to_string()],
    ).await?;
    tx.execute("DELETE FROM entity_edges WHERE source_entity_id = target_entity_id", ()).await?;

    // Entity communities + alias carry-over.
    tx.execute("UPDATE entity_communities SET entity_id = ? WHERE entity_id = ?", params![target.to_string(), source.to_string()]).await?;
    // Append the source's name as an alias of the target.
    let mut nrows = tx.query("SELECT name FROM entities WHERE id = ?", params![source.to_string()]).await?;
    if let Some(r) = nrows.next().await? {
        let source_name: String = r.get(0)?;
        drop(nrows);
        let mut arows = tx.query("SELECT aliases FROM entities WHERE id = ?", params![target.to_string()]).await?;
        let aliases_json: String = arows.next().await?.unwrap().get(0)?;
        drop(arows);
        let mut aliases: Vec<String> = serde_json::from_str(&aliases_json).unwrap_or_default();
        if !aliases.iter().any(|a| a == &source_name) { aliases.push(source_name); }
        tx.execute(
            "UPDATE entities SET aliases = ? WHERE id = ?",
            params![serde_json::to_string(&aliases)?, target.to_string()],
        ).await?;
    }

    // Finally remove the source row.
    tx.execute("DELETE FROM entities WHERE id = ?", params![source.to_string()]).await?;
    tx.commit().await?;
    Ok(())
}
```

- [ ] **Step 4: Add the daemon endpoint** in `routes/entities.rs`. Add to the router: `.route("/v1/entities/merge", post(merge_route))`. Handler:

```rust
use axum::routing::post;
use serde::Deserialize;
use mnemos_core::storage::entity_ops::merge_entities;

#[derive(Debug, Deserialize)]
struct MergeReq { source: String, target: String }

async fn merge_route(
    State(state): State<AppState>,
    Json(req): Json<MergeReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    merge_entities(state.vault.storage(), &req.source, &req.target).await?;
    mnemos_core::storage::audit::write_audit(
        state.vault.storage(), "mnemos-cli", "entity_merge", None,
        Some(serde_json::json!({ "source": req.source, "target": req.target })),
    ).await?;
    Ok(Json(serde_json::json!({ "source": req.source, "target": req.target, "status": "merged" })))
}
```

- [ ] **Step 5: Pass + commit.**
```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/mnemos_core/src/storage/entity_ops.rs crates/mnemos_daemon/src/routes/entities.rs crates/mnemos_daemon/tests/entities.rs
git commit -m "feat: merge_entities core + POST /v1/entities/merge (Plan 7 Task 8)"
```

---

## Task 9: `Vault::promote` + `POST /v1/memories/{id}/promote`

Re-tier a memory: update the DB row, rewrite + move the file to the new tier directory, audit.

**Files:** `crates/mnemos_core/src/vault.rs` (add `promote`), `crates/mnemos_daemon/src/routes/memories.rs` (add route), test `crates/mnemos_daemon/tests/memories.rs`.

- [ ] **Step 1: Failing test** — append to `tests/memories.rs`:

```rust
#[tokio::test]
async fn promote_moves_memory_to_target_tier() {
    let (app, token) = fixture().await;
    let (s, b) = call(app.clone(), "POST", "/v1/memories", Some(&token),
        r#"{"body":"promote target","tier":"semantic"}"#).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let id = serde_json::from_str::<serde_json::Value>(&b).unwrap()["id"].as_str().unwrap().to_string();
    let (s2, b2) = call(app.clone(), "POST", &format!("/v1/memories/{id}/promote"), Some(&token),
        r#"{"tier":"procedural"}"#).await;
    assert_eq!(s2, StatusCode::OK, "{b2}");
    let (_, b3) = call(app, "GET", &format!("/v1/memories/{id}"), Some(&token), "").await;
    assert_eq!(serde_json::from_str::<serde_json::Value>(&b3).unwrap()["tier"], "procedural");
}
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Add `Vault::promote`** (inside `impl Vault`, after `patch`):

```rust
    /// Re-tier a memory: update DB row + rewrite/move the file + audit.
    pub async fn promote(&self, id: &str, new_tier: Tier) -> Result<Memory> {
        use crate::file_io::content_hash;
        let mut mem = get_memory(&self.storage, id).await?;
        if mem.tier == new_tier { return Ok(mem); }
        // Find the old file path before changing tier.
        let old_file_path: Option<String> = {
            let conn = self.storage.conn()?;
            let mut r = conn.query(
                "SELECT file_path FROM memories WHERE id = ?",
                libsql::params![id.to_string()],
            ).await?;
            r.next().await?.and_then(|row| row.get::<String>(0).ok())
        };
        mem.tier = new_tier;
        let new_path = write_memory_file(&self.paths, &mem).await?;
        if let Some(old) = old_file_path.as_deref() {
            if old != new_path.to_string_lossy() {
                let _ = tokio::fs::remove_file(old).await;
            }
        }
        let new_hash = content_hash(&mem.body);
        {
            let (conn, _g) = self.storage.write_conn().await?;
            conn.execute(
                "UPDATE memories SET tier = ?, file_path = ?, content_hash = ? WHERE id = ?",
                libsql::params![
                    mem.tier.as_str().to_string(),
                    new_path.to_string_lossy().to_string(),
                    new_hash,
                    id.to_string()
                ],
            ).await?;
        }
        write_audit(
            &self.storage, opts_actor(), "promote", Some(id),
            Some(json!({ "tier": mem.tier.as_str() })),
        ).await?;
        get_memory(&self.storage, id).await
    }
```

- [ ] **Step 4: Add the endpoint** in `routes/memories.rs`. In `router()`: `.route("/v1/memories/{id}/promote", post(promote_memory))`. Handler:

```rust
#[derive(Debug, Deserialize)]
struct PromoteReq { tier: String }

async fn promote_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<PromoteReq>,
) -> Result<Json<mnemos_core::types::Memory>, ApiError> {
    let tier = Tier::from_str(&req.tier).map_err(|e| ApiError::bad_request(format!("invalid tier: {e}")))?;
    let mem = state.vault.promote(&id, tier).await?;
    state.events.publish(crate::events::Event::MemoryUpdated { id: id.clone() });
    Ok(Json(mem))
}
```

(`use axum::routing::post;` is already imported by other handlers.)

- [ ] **Step 5: Pass + commit.**
```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/mnemos_core/src/vault.rs crates/mnemos_daemon/src/routes/memories.rs crates/mnemos_daemon/tests/memories.rs
git commit -m "feat: Vault::promote + POST /v1/memories/{id}/promote (Plan 7 Task 9)"
```

---

## Task 10: UI — Entity merge dialog + Promote-to-procedural action

Wire up the existing disabled controls. The Entity profile gets a "Merge into…" action (typeahead over `useEntities()` excluding self); Reflections' "Promote to procedural" actually calls `POST /v1/memories/{id}/promote`.

**Files:** `desktop/src/api/client.ts` (+ `mergeEntities`, `promoteMemory`); `desktop/src/components/MergeDialog.tsx` (new); `desktop/src/views/EntityProfile.tsx` (mount Merge action); `desktop/src/views/Reflections.tsx` (enable Promote); tests.

- [ ] **Step 1: Failing tests** — `MergeDialog.test.tsx` + extend `Reflections.test.tsx` to assert a Promote button click hits the endpoint (capture via MSW).

```tsx
// MergeDialog.test.tsx
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { setupServer } from "msw/node";
import { renderWithQuery } from "../test/renderWithQuery";
import { MergeDialog } from "./MergeDialog";

let merged: unknown = null;
const server = setupServer(
  http.get("http://localhost:7423/v1/entities", () => HttpResponse.json({ entities: [{ id: "ent_a", name: "Rust" }, { id: "ent_b", name: "Tauri" }] })),
  http.post("http://localhost:7423/v1/entities/merge", async ({ request }) => { merged = await request.json(); return HttpResponse.json({ status: "merged" }); }),
);
beforeAll(() => server.listen()); afterEach(() => server.resetHandlers()); afterAll(() => server.close());

test("merges source into the picked target", async () => {
  renderWithQuery(<MergeDialog open source={{ id: "ent_a", name: "Rust" }} onClose={() => {}} />);
  await userEvent.click(await screen.findByText("Tauri"));
  await userEvent.click(screen.getByRole("button", { name: /merge/i }));
  await waitFor(() => expect(merged).toMatchObject({ source: "ent_a", target: "ent_b" }));
});
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Client additions** to `desktop/src/api/client.ts`:

```ts
  mergeEntities(source: string, target: string) {
    return this.req<{ status: string }>("POST", "/v1/entities/merge", { source, target });
  }
  promoteMemory(id: string, tier: string) {
    return this.req<Memory>("POST", `/v1/memories/${id}/promote`, { tier });
  }
```

Add `useEntities()` to `queries.ts`: `useQuery({ queryKey: ["entities"], queryFn: () => client.listEntities() })`.

- [ ] **Step 4: `desktop/src/components/MergeDialog.tsx`**

```tsx
import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useEntities } from "../api/queries";
import { client } from "../api/client";
import { Button } from "../design/primitives";

interface Props {
  open: boolean;
  source: { id: string; name: string };
  onClose: () => void;
}

export function MergeDialog({ open, source, onClose }: Props) {
  const qc = useQueryClient();
  const { data: entities } = useEntities();
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<{ id: string; name: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const filtered = useMemo(
    () => (entities ?? []).filter((e) => e.id !== source.id && e.name.toLowerCase().includes(query.toLowerCase())).slice(0, 12),
    [entities, query, source.id],
  );
  if (!open) return null;

  const submit = async () => {
    if (!picked) return;
    setBusy(true);
    try {
      await client.mergeEntities(source.id, picked.id);
      await qc.invalidateQueries({ queryKey: ["entities"] });
      await qc.invalidateQueries({ queryKey: ["entity", source.id] });
      await qc.invalidateQueries({ queryKey: ["graph"] });
      onClose();
    } finally { setBusy(false); }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/30 pt-28" onClick={onClose}>
      <div role="dialog" aria-label="Merge entity" className="w-[34rem] rounded-lg bg-surface-raised shadow-floating border border-border p-4 space-y-3" onClick={(e) => e.stopPropagation()}>
        <div>
          <div className="label">Merge entity</div>
          <h2 className="display text-lg">{source.name}</h2>
          <p className="text-text-muted text-sm">All mentions and edges from <span className="mono">{source.id}</span> will move to the picked target. The source name is added as an alias.</p>
        </div>
        <input autoFocus className="w-full bg-surface border border-border rounded-md px-2 py-1 mono text-sm"
          placeholder="search target…" value={query} onChange={(e) => setQuery(e.target.value)} />
        <ul className="max-h-56 overflow-y-auto border border-border rounded-md">
          {filtered.map((e) => (
            <li key={e.id}>
              <button onClick={() => setPicked(e)} className={`w-full px-3 py-1.5 text-left text-sm hover:bg-surface ${picked?.id === e.id ? "bg-surface-raised" : ""}`}>
                <span className="font-body">{e.name}</span> <span className="mono text-text-muted text-xs">{e.id}</span>
              </button>
            </li>
          ))}
          {!filtered.length && <li className="px-3 py-2 text-sm text-text-muted">no matches</li>}
        </ul>
        <div className="flex justify-end gap-2">
          <button onClick={onClose} className="label text-text-muted">Cancel</button>
          <Button onClick={submit} disabled={!picked || busy}>{busy ? "Merging…" : "Merge"}</Button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 5: Mount the Merge action in `EntityProfile.tsx`** (above the relationships card):

```tsx
import { MergeDialog } from "../components/MergeDialog";
// inside the component:
const [mergeOpen, setMergeOpen] = useState(false);
// ... render:
<div className="flex items-center gap-2">
  <Button variant="ghost" onClick={() => setMergeOpen(true)}>Merge into…</Button>
</div>
<MergeDialog open={mergeOpen} source={{ id, name: data.name }} onClose={() => setMergeOpen(false)} />
```

- [ ] **Step 6: Enable Promote-to-procedural** in `Reflections.tsx`. Replace the disabled button with:

```tsx
const promote = async (id: string) => {
  try {
    await client.promoteMemory(id, "procedural");
    await qc.invalidateQueries({ queryKey: ["reflections"] });
    await qc.invalidateQueries({ queryKey: ["memories"] });
  } catch { /* show toast in a future iteration */ }
};
// ...
<button onClick={() => promote(r.id)} className="label text-accent">Promote to procedural</button>
```

- [ ] **Step 7: Pass + commit.**
```bash
cd desktop && pnpm typecheck && pnpm lint && pnpm test && cd ..
git add desktop/src/api/client.ts desktop/src/api/queries.ts desktop/src/components/MergeDialog.tsx desktop/src/components/MergeDialog.test.tsx desktop/src/views/EntityProfile.tsx desktop/src/views/Reflections.tsx
git commit -m "feat(ui): entity merge dialog + active promote-to-procedural (Plan 7 Task 10)"
```

---

# Group C — Config endpoint, Settings view, real sync status pill

## Task 11: `GET/PUT /v1/config`

Returns the resolved daemon config; PUT writes a partial update back to `~/.config/mnemos/config.toml` and reloads. Auth-gated.

**Files:** `crates/mnemos_daemon/src/routes/config.rs` (new), `routes/mod.rs` (mount), test `tests/config_endpoint.rs` (new).

- [ ] **Step 1: Failing test** — GET returns the current config; PUT with `{"daemon":{"port":7423}}` is accepted and idempotent.

- [ ] **Step 2: Implement.** Use `toml::to_string_pretty` + `tokio::fs::write` to the path returned by `default_config_path` (already in `config.rs`); on PUT, merge the partial JSON into the existing TOML by parsing both as `toml::Value`s and overwriting matching keys. Re-read on next request — the daemon picks up changes when `AppState::config` is reloaded (for v0.6.0, the response includes a "restart required" hint if changes touch daemon.host/port; non-restart-impacting blocks like `[sync]`/`[reflection]` apply on next worker tick).

```rust
//! `GET /v1/config`, `PUT /v1/config`. Bearer-auth gated.

use axum::{extract::State, routing::get, Json, Router};
use serde_json::Value;

use crate::config::Config;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/config", get(get_cfg).put(put_cfg))
}

async fn get_cfg(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::to_value(&*state.config).map_err(|e| ApiError::internal(e.to_string()))?))
}

async fn put_cfg(State(_state): State<AppState>, Json(patch): Json<Value>) -> Result<Json<Value>, ApiError> {
    use mnemos_daemon::config::default_config_path;
    // Load current file (if any), merge, write back, validate by parsing.
    let path = default_config_path().map_err(|e| ApiError::internal(e.to_string()))?;
    let existing: toml::Value = if path.exists() {
        let text = tokio::fs::read_to_string(&path).await.map_err(|e| ApiError::internal(e.to_string()))?;
        toml::from_str(&text).map_err(|e| ApiError::bad_request(format!("config parse: {e}")))?
    } else {
        toml::Value::Table(Default::default())
    };
    let merged = merge_value(existing, json_to_toml(patch));
    let text = toml::to_string_pretty(&merged).map_err(|e| ApiError::internal(e.to_string()))?;
    // Validate by deserializing into Config (rejects garbage).
    let _: Config = toml::from_str(&text).map_err(|e| ApiError::bad_request(format!("config invalid: {e}")))?;
    if let Some(parent) = path.parent() { tokio::fs::create_dir_all(parent).await.ok(); }
    tokio::fs::write(&path, text).await.map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({
        "saved": true,
        "path": path.to_string_lossy(),
        "restart_required_for": ["daemon.host", "daemon.port"],
    })))
}

fn json_to_toml(v: Value) -> toml::Value {
    match v {
        Value::Null => toml::Value::String(String::new()),
        Value::Bool(b) => toml::Value::Boolean(b),
        Value::Number(n) => n.as_i64().map(toml::Value::Integer).or_else(|| n.as_f64().map(toml::Value::Float)).unwrap_or(toml::Value::String(n.to_string())),
        Value::String(s) => toml::Value::String(s),
        Value::Array(a) => toml::Value::Array(a.into_iter().map(json_to_toml).collect()),
        Value::Object(o) => { let mut t = toml::map::Map::new(); for (k, v) in o { t.insert(k, json_to_toml(v)); } toml::Value::Table(t) }
    }
}

fn merge_value(base: toml::Value, patch: toml::Value) -> toml::Value {
    match (base, patch) {
        (toml::Value::Table(mut b), toml::Value::Table(p)) => {
            for (k, v) in p {
                let merged = match b.remove(&k) { Some(bv) => merge_value(bv, v), None => v };
                b.insert(k, merged);
            }
            toml::Value::Table(b)
        }
        (_, p) => p,
    }
}
```

> `default_config_path` is currently private in `config.rs` — change to `pub`.

- [ ] **Step 3: Mount + pass + commit.** Add `pub mod config;` + `.merge(config::router())` in `routes/mod.rs`.
```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/config.rs crates/mnemos_daemon/src/routes/mod.rs crates/mnemos_daemon/src/config.rs crates/mnemos_daemon/tests/config_endpoint.rs
git commit -m "feat: GET/PUT /v1/config endpoint (Plan 7 Task 11)"
```

---

## Task 12: Settings view (sectioned form)

A long, sectioned form rendering the resolved config: Daemon, Embedder, LLM, Reranker, Retrieval/PPR, Decay (`reweight`), Reflection, Community, Sync (with sub-fields per backend kind). Save → `PUT /v1/config`. The form is built generically from a small declarative schema so it's not 600 lines of JSX.

**Files:** `desktop/src/views/Settings.tsx` (new), `desktop/src/api/client.ts` (+ `getConfig`/`putConfig`), router entry, test.

- [ ] **Step 1: Failing test** — render Settings, expect the "Save settings" button and a section heading "Sync".

- [ ] **Step 2: Implement** — sectioned form: each section is a card; each field is one of `{text, number, range, boolean, select, password}`. The schema:

```ts
type Field =
  | { key: string; label: string; kind: "text" | "password" }
  | { key: string; label: string; kind: "number"; min?: number; max?: number; step?: number }
  | { key: string; label: string; kind: "range"; min: number; max: number; step: number }
  | { key: string; label: string; kind: "boolean" }
  | { key: string; label: string; kind: "select"; options: string[] };

type Section = { title: string; path: string[]; fields: Field[] };

const SCHEMA: Section[] = [
  { title: "Daemon", path: ["daemon"], fields: [{ key: "host", label: "Host", kind: "text" }, { key: "port", label: "Port", kind: "number", min: 1024, max: 65535 }] },
  { title: "Embedder", path: ["embedder"], fields: [{ key: "kind", label: "Backend", kind: "select", options: ["ollama", "mock", "none"] }, { key: "url", label: "URL", kind: "text" }, { key: "model", label: "Model", kind: "text" }, { key: "dim", label: "Dim", kind: "number" }, { key: "timeout_secs", label: "Timeout (s)", kind: "number" }] },
  { title: "LLM", path: ["llm"], fields: [{ key: "kind", label: "Backend", kind: "select", options: ["ollama", "mock", "none"] }, { key: "url", label: "URL", kind: "text" }, { key: "model", label: "Model", kind: "text" }, { key: "timeout_secs", label: "Timeout (s)", kind: "number" }] },
  { title: "Retrieval", path: ["retrieval"], fields: [{ key: "default_k", label: "Default k", kind: "number" }, { key: "rrf_k", label: "RRF k", kind: "number" }, { key: "ppr_alpha", label: "PPR α", kind: "range", min: 0.5, max: 0.95, step: 0.05 }, { key: "ppr_iterations", label: "PPR iters", kind: "number", min: 1, max: 200 }] },
  { title: "Decay (reweight)", path: ["retrieval", "reweight"], fields: [{ key: "recency_decay", label: "Recency decay/day", kind: "range", min: 0, max: 0.2, step: 0.005 }, { key: "importance_weight", label: "Importance weight", kind: "range", min: 0, max: 3, step: 0.05 }] },
  { title: "Reflection", path: ["reflection"], fields: [{ key: "salience_threshold", label: "Salience threshold", kind: "range", min: 0, max: 50, step: 0.5 }, { key: "max_sources", label: "Max sources", kind: "number", min: 1, max: 100 }] },
  { title: "Community", path: ["community"], fields: [{ key: "min_community_size", label: "Min community size", kind: "number", min: 2, max: 50 }] },
  { title: "Sync", path: ["sync"], fields: [{ key: "kind", label: "Backend", kind: "select", options: ["none", "filesystem", "git", "s3"] }, { key: "interval_secs", label: "Interval (s)", kind: "number", min: 0, max: 86400 }] },
  { title: "Sync · Git", path: ["sync", "git"], fields: [{ key: "remote", label: "Remote URL", kind: "text" }, { key: "branch", label: "Branch", kind: "text" }] },
  { title: "Sync · S3 (rclone)", path: ["sync", "s3"], fields: [{ key: "remote", label: "Remote (rclone target)", kind: "text" }] },
];
```

A small `get(path)` / `set(path, value)` pair walks the JSON config object. Render each field with token-driven styles (no ad-hoc colors). "Save settings" calls `client.putConfig(diff)` (or the whole object) and shows the "restart required for" hint when relevant. Sections collapse via `<details>`.

```tsx
import { useEffect, useMemo, useState } from "react";
import { client } from "../api/client";
import { Button, Card, Skeleton } from "../design/primitives";

// SCHEMA + helpers omitted for brevity above; paste them here.
function getAt(obj: any, path: string[]): any { return path.reduce((acc, k) => (acc == null ? acc : acc[k]), obj); }
function setAt(obj: any, path: string[], value: any): any {
  const out = Array.isArray(obj) ? [...obj] : { ...(obj || {}) };
  let cur: any = out;
  for (let i = 0; i < path.length - 1; i++) { cur[path[i]] = { ...(cur[path[i]] || {}) }; cur = cur[path[i]]; }
  cur[path[path.length - 1]] = value;
  return out;
}

export function Settings() {
  const [cfg, setCfg] = useState<any>(null);
  const [saving, setSaving] = useState(false);
  const [savedAt, setSavedAt] = useState<string | null>(null);
  useEffect(() => { client.getConfig().then(setCfg); }, []);
  if (!cfg) return <div className="p-6"><Skeleton className="h-64 w-full" /></div>;
  const save = async () => { setSaving(true); try { await client.putConfig(cfg); setSavedAt(new Date().toISOString()); } finally { setSaving(false); } };
  return (
    <div className="p-6 max-w-3xl space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="display text-xl">Settings</h1>
        <div className="flex items-center gap-3">
          {savedAt && <span className="label text-text-muted">saved {savedAt.slice(11, 16)}</span>}
          <Button onClick={save} disabled={saving}>{saving ? "Saving…" : "Save settings"}</Button>
        </div>
      </div>
      {SCHEMA.map((section) => (
        <Card key={section.title} className="p-4">
          <details open className="space-y-3">
            <summary className="display text-base cursor-pointer">{section.title}</summary>
            <div className="grid grid-cols-2 gap-3 pt-2">
              {section.fields.map((f) => {
                const path = [...section.path, f.key];
                const v = getAt(cfg, path);
                const onChange = (val: any) => setCfg(setAt(cfg, path, val));
                return (
                  <label key={f.key} className="flex flex-col gap-1">
                    <span className="label">{f.label}</span>
                    {f.kind === "text" && <input className="bg-surface border border-border rounded-md px-2 py-1 mono text-sm" value={v ?? ""} onChange={(e) => onChange(e.target.value)} />}
                    {f.kind === "password" && <input type="password" className="bg-surface border border-border rounded-md px-2 py-1 mono text-sm" value={v ?? ""} onChange={(e) => onChange(e.target.value)} />}
                    {f.kind === "number" && <input type="number" className="bg-surface border border-border rounded-md px-2 py-1 mono text-sm" value={Number(v ?? 0)} min={f.min} max={f.max} step={f.step} onChange={(e) => onChange(Number(e.target.value))} />}
                    {f.kind === "range" && (<><input type="range" min={f.min} max={f.max} step={f.step} value={Number(v ?? f.min)} onChange={(e) => onChange(Number(e.target.value))} className="accent-accent" /><span className="mono text-xs text-text-muted">{Number(v ?? f.min).toFixed(2)}</span></>)}
                    {f.kind === "boolean" && <input type="checkbox" checked={!!v} onChange={(e) => onChange(e.target.checked)} />}
                    {f.kind === "select" && <select className="bg-surface border border-border rounded-md px-2 py-1 text-sm" value={String(v ?? "")} onChange={(e) => onChange(e.target.value)}>{f.options.map((o) => <option key={o} value={o}>{o}</option>)}</select>}
                  </label>
                );
              })}
            </div>
          </details>
        </Card>
      ))}
    </div>
  );
}
```

- [ ] **Step 3: Client + route + sidebar entry.** Add `getConfig()`/`putConfig(patch)` to `client.ts`; mount `/settings` in `router.tsx` + add to LeftSidebar NAV.

- [ ] **Step 4: Pass + commit.**
```bash
cd desktop && pnpm typecheck && pnpm lint && pnpm test && cd ..
git add desktop/src/views/Settings.tsx desktop/src/api/client.ts desktop/src/router.tsx desktop/src/layout/LeftSidebar.tsx desktop/src/views/index.tsx desktop/src/views/Settings.test.tsx
git commit -m "feat(ui): Settings view — sectioned form over /v1/config (Plan 7 Task 12)"
```

---

## Task 13: Top-bar sync status pill (real)

Replace the placeholder "closed/connecting/open" indicator with a `sync_status` pill that shows the configured backend + last sync time + a click-to-pull action.

**Files:** `desktop/src/layout/TopBar.tsx`, `desktop/src/api/queries.ts` (+`useSyncStatus`), test.

- [ ] Implement: `useSyncStatus` polls `/v1/sync/status` every 10s and is invalidated on `SyncCompleted`/`SyncFailed` events (extend `INVALIDATE` in `api/ws.ts`). Pill shows `<backend> · <relative time>`; clicking it dispatches `mnemos:sync-pull` which Shell catches and calls `client.runSyncPull()`. On failure, the pill turns brick-red with the error in `title`.

- [ ] Commit: `feat(ui): real sync-status pill in top bar (Plan 7 Task 13)`

---

# Group D — Doctor + vault export/import

## Task 14: `GET /v1/doctor`

Runs a battery of checks: schema version current, file/DB drift count (memories whose `file_path` doesn't exist on disk vs files not in DB), embedder reachable + dim matches `vault_meta`, LLM reachable, sync backend `status().ready`, audit-log integrity (trigger present), disk space, vault writable. Returns `{ checks: [{ name, status: "ok"|"warn"|"fail", detail }] }`.

**Files:** `crates/mnemos_daemon/src/routes/doctor.rs` (new), mount, test.

- [ ] Implement each check as a small fn returning `{ name, status, detail }`. Use the existing `doctor.rs` in `mnemos_core` for file/DB drift if present (Plan 1 added one — re-use). The endpoint composes them.

- [ ] Commit: `feat: GET /v1/doctor diagnostics endpoint (Plan 7 Task 14)`

---

## Task 15: Vault export/import zip

`POST /v1/vault/export` → returns a zip of the vault root (memories, reflections, entities files; NOT the DB — it rebuilds from files) with a `mnemos-vault.json` manifest at the root (workspace, version, exported_at). `POST /v1/vault/import` accepts a multipart zip, extracts to the vault root (or a sub-temp + atomic swap), then calls `rebuild`.

**Files:** `crates/mnemos_daemon/src/routes/vault.rs` (new), mount; add `zip = "2"` to workspace deps; test.

- [ ] Implement export by streaming a `ZipWriter` over the vault root via `walkdir`; skip `.mnemos.db` (the DB rebuilds from files on import) and hidden dirs. Import: receive the zip body (`axum::body::Body`), write to a tempfile, extract via `zip::ZipArchive`, then trigger `rebuild`. Cap zip size at 500 MB by default.

- [ ] Commit: `feat: POST /v1/vault/export and /v1/vault/import (Plan 7 Task 15)`

---

## Task 16: UI — Doctor view + Export/Import actions + CLI `doctor`/`export`/`import`

UI Doctor view renders the `/v1/doctor` checks (green/yellow/red dots, expandable detail). Settings view (Task 12) gets a "Vault" section with "Export…" (download) and "Import…" (file picker). CLI `mnemos doctor` (already exists? — extend to call the daemon endpoint via mnemos_client) and `mnemos export <path>` / `mnemos import <path>`.

**Files:** `desktop/src/views/Doctor.tsx` (new), `desktop/src/components/VaultIO.tsx` (export/import buttons mounted in Settings), `crates/mnemos_cli/src/commands/{export,import}.rs` (new), `cli.rs` + `main.rs` (dispatch).

- [ ] Implement UI Doctor as a list of cards (one per check) with a `RefreshCcw` icon button to re-run. Implement Vault export by hitting `/v1/vault/export` and triggering a Blob download. Import via a hidden file input, POST multipart, show progress.

- [ ] Commit: `feat(ui): Doctor view + Export/Import actions + CLI commands (Plan 7 Task 16)`

---

# Group E — First-run wizard

## Task 17: First-run wizard

A one-time modal on first launch (no `mnemos.first_run_completed_at` in the daemon's `vault_meta` table — extend that meta row or add a tiny `app_state` table). The wizard does three steps:

1. **Welcome + vault path** — shows the resolved vault root, lets the user change it (writes `[vault].root` via `PUT /v1/config`).
2. **Ollama check + model pull** — probes `GET {embedder.url}/api/tags` (default `http://localhost:11434`). If reachable, lists the installed models; if `nomic-embed-text` is missing, offers a one-click pull (`POST {url}/api/pull` with `{"name":"nomic-embed-text"}` and progress streaming).
3. **Integration snippets** — copy-pasteable code for Claude Code, Codex, Cursor, generic MCP, and the OpenAI-functions schema. Does NOT auto-write to other tools' config dirs (the spec deferral — versioning risk).

On finish, the daemon stamps `first_run_completed_at = now`; subsequent launches don't show the wizard. The whole component is one file with a small step machine; no router changes (it overlays the shell).

**Files:** `crates/mnemos_daemon/src/routes/firstrun.rs` (`GET/POST /v1/first-run`), `crates/mnemos_core/src/storage/migrations.rs` (extend `vault_meta` with `first_run_completed_at TEXT`), `desktop/src/views/FirstRun.tsx`, mount in App.

- [ ] **Step 1: Failing test (daemon)** — `tests/firstrun.rs`: `GET /v1/first-run` returns `{ completed_at: null }` initially; `POST /v1/first-run/complete` stamps it; the second `GET` returns the timestamp.

- [ ] **Step 2: Daemon** — extend `vault_meta` (migration v8) with `first_run_completed_at TEXT NULL`. Add `routes/firstrun.rs`:

```rust
use axum::{extract::State, routing::{get, post}, Json, Router};
use serde_json::{json, Value};
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/first-run", get(get_state))
        .route("/v1/first-run/complete", post(complete))
}

async fn get_state(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    use mnemos_core::error::MnemosError;
    let conn = state.vault.storage().conn()?;
    let mut rows = conn.query("SELECT first_run_completed_at FROM vault_meta WHERE id = 1", ()).await.map_err(MnemosError::from)?;
    let v: Option<String> = rows.next().await.map_err(MnemosError::from)?.and_then(|r| r.get::<Option<String>>(0).ok()).flatten();
    Ok(Json(json!({ "completed_at": v })))
}

async fn complete(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let (conn, _g) = state.vault.storage().write_conn().await?;
    conn.execute("UPDATE vault_meta SET first_run_completed_at = ? WHERE id = 1",
        libsql::params![chrono::Utc::now().to_rfc3339()]).await.map_err(mnemos_core::error::MnemosError::from)?;
    Ok(Json(json!({ "completed": true })))
}
```

Mount in `routes/mod.rs`. Schema v8 adds the column (`ALTER TABLE vault_meta ADD COLUMN first_run_completed_at TEXT`) + bump stale schema-version asserts 7→8.

- [ ] **Step 3: Frontend** — `desktop/src/views/FirstRun.tsx`. Pseudocode:

```tsx
import { useEffect, useState } from "react";
import { client } from "../api/client";
import { Button, Card } from "../design/primitives";

type Step = 0 | 1 | 2 | 3;

export function FirstRun({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState<Step>(0);
  const [ollamaModels, setOllamaModels] = useState<string[] | null>(null);
  const [ollamaError, setOllamaError] = useState<string | null>(null);
  const [pulling, setPulling] = useState(false);

  // Probe Ollama once when entering step 1.
  useEffect(() => {
    if (step !== 1) return;
    void (async () => {
      try {
        const cfg = await client.getConfig() as { embedder: { url: string } };
        const res = await fetch(`${cfg.embedder.url}/api/tags`);
        if (!res.ok) throw new Error(`Ollama responded ${res.status}`);
        const j = await res.json() as { models?: { name: string }[] };
        setOllamaModels((j.models ?? []).map((m) => m.name));
      } catch (e) {
        setOllamaError(e instanceof Error ? e.message : "unreachable");
      }
    })();
  }, [step]);

  const pullEmbed = async () => {
    setPulling(true);
    try {
      const cfg = await client.getConfig() as { embedder: { url: string } };
      await fetch(`${cfg.embedder.url}/api/pull`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ name: "nomic-embed-text" }) });
      setOllamaModels((m) => (m ?? []).concat("nomic-embed-text"));
    } finally { setPulling(false); }
  };

  const finish = async () => { await client.completeFirstRun(); onClose(); };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <Card className="w-[40rem] p-6 space-y-4">
        <div className="label">Welcome · step {step + 1} of 3</div>
        {step === 0 && (
          <>
            <h1 className="display text-2xl">Set up your memory vault</h1>
            <p className="text-text-muted font-body">mnemos keeps a local-first vault of your AI conversations. Memories live as markdown files in <span className="mono">~/.local/share/mnemos/</span> (you can change this in Settings).</p>
            <div className="flex justify-end"><Button onClick={() => setStep(1)}>Continue</Button></div>
          </>
        )}
        {step === 1 && (
          <>
            <h1 className="display text-xl">Embedder · Ollama</h1>
            {ollamaModels === null && !ollamaError && <p className="text-text-muted">Checking Ollama…</p>}
            {ollamaError && <p className="text-tier-procedural">Ollama isn't running. Install from ollama.com and start it, then click Retry.</p>}
            {ollamaModels && (
              <div>
                <p className="text-text-muted">Found {ollamaModels.length} installed model{ollamaModels.length === 1 ? "" : "s"}.</p>
                {!ollamaModels.includes("nomic-embed-text") ? (
                  <Button onClick={pullEmbed} disabled={pulling}>{pulling ? "Pulling nomic-embed-text…" : "Pull nomic-embed-text"}</Button>
                ) : <p className="label">✓ nomic-embed-text installed</p>}
              </div>
            )}
            <div className="flex justify-between">
              <button className="label text-text-muted" onClick={() => setStep(0)}>Back</button>
              <Button onClick={() => setStep(2)}>Continue</Button>
            </div>
          </>
        )}
        {step === 2 && (
          <>
            <h1 className="display text-xl">Connect your AI tools</h1>
            <p className="text-text-muted font-body">Copy a snippet into each tool's config to use mnemos as its memory provider.</p>
            <details open>
              <summary className="display text-base cursor-pointer">Claude Code</summary>
              <pre className="mono text-xs bg-surface border border-border rounded-md p-3 overflow-x-auto">{`{"mcpServers":{"mnemos":{"command":"mnemos-mcp-stdio"}}}`}</pre>
            </details>
            <details>
              <summary className="display text-base cursor-pointer">Codex / OpenAI function-calling</summary>
              <pre className="mono text-xs bg-surface border border-border rounded-md p-3 overflow-x-auto">{`see adapters/openai-functions/schema.json`}</pre>
            </details>
            <details>
              <summary className="display text-base cursor-pointer">Generic MCP</summary>
              <pre className="mono text-xs bg-surface border border-border rounded-md p-3 overflow-x-auto">{`see adapters/generic-mcp/example.json`}</pre>
            </details>
            <div className="flex justify-between">
              <button className="label text-text-muted" onClick={() => setStep(1)}>Back</button>
              <Button onClick={finish}>Finish setup</Button>
            </div>
          </>
        )}
      </Card>
    </div>
  );
}
```

Add `completeFirstRun()` to the client. In `App.tsx`, after `QueryClientProvider` mounts: `GET /v1/first-run` → if `completed_at == null` render `<FirstRun onClose={() => setShown(false)} />` over the shell.

- [ ] **Step 4: Pass + commit.**
```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cd desktop && pnpm typecheck && pnpm lint && pnpm test && cd ..
git add crates/mnemos_core/src/storage/migrations.rs crates/mnemos_daemon/src/routes/firstrun.rs crates/mnemos_daemon/src/routes/mod.rs desktop/src/views/FirstRun.tsx desktop/src/api/client.ts desktop/src/App.tsx
git commit -m "feat: first-run wizard (vault path / Ollama check / integration snippets) (Plan 7 Task 17)"
```

---

# Group F — Reference adapters

## Task 18: Six small adapter packages

Each adapter is a self-contained folder under `adapters/` with a README + one or two config snippets. These are integration templates, not production code, and stay <200 lines each per spec. Group them in one commit — they share no source dependencies.

**Folders to create (or complete):** `adapters/gemini-cli/`, `adapters/codex/`, `adapters/hermes-agent/`, `adapters/openclaw/`, `adapters/generic-mcp/`, `adapters/openai-functions/`.

- [ ] **Each adapter ships:**
  - `README.md` (one paragraph "what this does" + the exact install/config steps).
  - The minimal config/glue file (JSON snippet, MCP server entry, shell wrapper, function-call schema, etc.) — see spec lines 634-647.

- [ ] **Specifically:**

  - **`adapters/gemini-cli/`** — A `GEMINI.md` fragment and an `mcp.json` snippet pointing at `mnemos-mcp-stdio`. (The repo's `GEMINI.md` already exists per CLAUDE.md global rules; this is the importable fragment.)
  - **`adapters/codex/`** — `codex.config.json` snippet showing `mnemos-mcp-stdio` as a tool provider; README documents Codex CLI's tool-use schema.
  - **`adapters/hermes-agent/`** — `hermes-mnemos.py` (~120 lines): a tiny Python REST client (`requests`) that wraps `remember`/`recall`, plus a 20-line README on bridging Hermes' non-MCP interface to it.
  - **`adapters/openclaw/`** — `openclaw-wrapper.sh` shell script that intercepts the `openclaw` CLI and pipes its session transcript through `mnemos remember`; README explains the `~/.bashrc` install.
  - **`adapters/generic-mcp/`** — `client.example.ts` (~80 lines): a minimal `@modelcontextprotocol/sdk` Node client that connects to `http://localhost:7423/mcp` and demonstrates `tools/list` + `tools/call` for `remember` and `recall`. README walks through `npm i @modelcontextprotocol/sdk`.
  - **`adapters/openai-functions/`** — `schema.json`: the OpenAI function-calling schema for the five mnemos MCP tools (`remember`, `recall`, `forget`, `get_memory`, `list_memories`), copy-pasteable into any OpenAI tool-use request. README shows a `curl` example.

- [ ] **Test/verify:** Lint the JSON snippets with `python -c "import json; json.load(open('<file>'))"` for each. Run the generic-mcp `client.example.ts` against a local daemon manually (note in README as the QA step; not automated CI).

- [ ] **Commit:**
```bash
git add adapters/
git commit -m "feat: reference adapters (gemini-cli, codex, hermes-agent, openclaw, generic-mcp, openai-functions) (Plan 7 Task 18)"
```

---

# Group G — Release v0.6.0

## Task 19: Release v0.6.0 — version, README, CHANGELOG, tag

**Files:** `Cargo.toml` (workspace `version = "0.6.0"`), `desktop/package.json` + `desktop/src-tauri/Cargo.toml` + `desktop/src-tauri/tauri.conf.json` (all → 0.6.0), `README.md`, `CHANGELOG.md`.

- [ ] **Step 1:** Bump all four version stamps (workspace, frontend package, src-tauri Cargo.toml, tauri.conf.json) to `0.6.0`.

- [ ] **Step 2: README** — add a "Sync, settings, doctor, adapters (v0.6.0)" section:

```markdown
## Sync, settings, doctor, adapters (v0.6.0)

mnemos is now multi-machine. Pick a backend:

| Backend | When | How |
|---|---|---|
| **Filesystem** | Vault sits in Syncthing/Dropbox/iCloud/OneDrive | nothing to configure; mnemos detects conflict files |
| **Git remote** | You want audit history + branches | `mnemos sync` shells out to `git`; ships with `mnemos-merge-driver` for YAML-aware frontmatter merges |
| **S3-compatible** | NAS or B2/MinIO | shells out to `rclone` (configure a remote first via `rclone config`) |

Plus: a **Settings view** that edits every knob over `PUT /v1/config`; a **First-run wizard** (Ollama probe + integration snippets); a **Doctor view** at `/doctor` and `GET /v1/doctor` reporting schema/file-DB drift/dep reachability/sync health; **vault export/import** as zip; **entity merge** (`POST /v1/entities/merge` + UI dialog) and a working **Promote to procedural** action on reflections; six new **reference adapters** under `adapters/`.

CLI additions: `mnemos sync push|pull|status`, `mnemos doctor`, `mnemos export <zip>`, `mnemos import <zip>`.

Turso libSQL embedded replicas (the DB-layer fast path) — config knob ships, wire-up deferred to a future increment.
```

- [ ] **Step 3: CHANGELOG** — add at the top:

```markdown
## [0.6.0] - 2026-05-28

### Added
- Cloud sync with three backends: filesystem-sync (Syncthing/Dropbox/iCloud/OneDrive
  conflict-file detection), Git remote (periodic push/pull + the new
  `mnemos-merge-driver` binary for YAML-aware memory frontmatter merges), and
  S3-compatible (shells out to `rclone`). `[sync]` config block + periodic
  `sync_worker` + `GET /v1/sync/status`, `POST /v1/sync/push|pull`,
  `GET /v1/sync/conflicts` + WS events + `mnemos sync push|pull|status` CLI.
- Schema v7 (`sync_state`, `sync_conflicts`) and v8 (first-run timestamp).
- Entity merge (`POST /v1/entities/merge`, in-place `merge_entities` core,
  Entity-profile "Merge into…" dialog).
- Tier promotion (`Vault::promote`, `POST /v1/memories/{id}/promote`); Reflections'
  "Promote to procedural" action wired up.
- Settings view + `GET/PUT /v1/config` (sectioned form over every config block).
- First-run wizard (Ollama probe + `nomic-embed-text` pull + integration snippets).
- Doctor view + `GET /v1/doctor` (schema/file-DB drift/dep reachability/sync state).
- Vault export/import (`POST /v1/vault/export|import` + UI + `mnemos export|import` CLI).
- Reference adapters under `adapters/` for gemini-cli, codex, hermes-agent,
  openclaw, generic-mcp, openai-functions.
- Real sync-status pill in the top bar; live `sync_*` events.

### Deferred
- Turso libSQL embedded replicas (DB-layer fast-path) — config knob exists,
  wire-up TBD when a test target is available.
- Encrypt-at-rest, secret-detection-at-ingest, AI-tool auto-detection in first-run.
- Native packaging / installers / signing / auto-update — Plan 8.
```

- [ ] **Step 4: Release gate**
```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd desktop && pnpm install --frozen-lockfile && pnpm typecheck && pnpm lint && pnpm test && pnpm build && cd ..
```
All green. (`pnpm tauri build` remains a Plan-8 / manual step.)

- [ ] **Step 5: Commit + tag** (local only)
```bash
git add Cargo.toml desktop/package.json desktop/src-tauri/Cargo.toml desktop/src-tauri/tauri.conf.json README.md CHANGELOG.md
git commit -m "chore: release v0.6.0 — sync, settings, doctor, adapters (Plan 7 Task 19)"
git tag -a v0.6.0 -m "v0.6.0 — cloud sync + settings + doctor + adapters"
```

(Do NOT push — user reviews and pushes.)

---

## Done

After all tasks: mnemos is multi-machine (three durable sync backends + audit log of conflicts), every knob has a UI (Settings + Doctor + First-run), the entity graph is editable (merge + promote), data is portable (zip export/import), and the integration story is real (six reference adapters for the major AI clients). The only thing left to make it ship-able to non-developers is **packaging** — that's Plan 8.
