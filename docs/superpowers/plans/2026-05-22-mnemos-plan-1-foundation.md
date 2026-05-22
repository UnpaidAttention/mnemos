# Mnemos Plan 1 — Foundation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a working CLI (`mnemos`) that stores memories as markdown files on disk, indexes them in libSQL with FTS5, and answers BM25-only `recall` queries. Bi-temporal model, audit log, file watcher, and `mnemos rebuild` / `doctor` all functional. This is the v0.0.1 — no vectors, no daemon, no LLM yet. Everything in subsequent plans builds on this foundation.

**Architecture:** Single Cargo workspace with `mnemos_core` (storage, types, file I/O, retrieval) and `mnemos_cli` (the `mnemos` binary). libSQL via the `libsql` crate (API-compatible with rusqlite; gives us embedded replicas later). Files in `~/.local/share/mnemos/files/` are source of truth; DB at `~/.local/share/mnemos/index.db` is a rebuildable derived index. Tokio async throughout.

**Tech Stack:** Rust 2021 edition, tokio, libsql, serde + serde_yaml, clap v4, notify (file watcher), tracing (logs), ulid, anyhow + thiserror, assert_cmd + tempfile (CLI tests).

---

## Plan sequence context

Plan 1 of 7. After this plan lands you have a usable single-user CLI memory tool. Subsequent plans add:
- Plan 2: dense vectors + RRF + rerank
- Plan 3: long-running daemon, MCP server, REST API
- Plan 4: async LLM-driven extraction + resolution pipelines
- Plan 5: HippoRAG PPR + reflection + community detection
- Plan 6: Tauri+React desktop UI
- Plan 7: sync backends, adapters, packaging

The schema designed in this plan is forward-compatible with all subsequent plans — no schema rewrites required.

---

## File structure produced by this plan

```
mnemos/
├── Cargo.toml                          # workspace root
├── rust-toolchain.toml                 # pin stable
├── .gitignore
├── LICENSE                             # Apache-2.0
├── README.md
├── CHANGELOG.md
├── CONTRIBUTING.md
├── .github/workflows/ci.yml            # fmt, clippy, test
├── crates/
│   ├── mnemos_core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # crate entry + re-exports
│   │       ├── error.rs                # MnemosError + Result alias
│   │       ├── id.rs                   # ULID helpers
│   │       ├── types.rs                # Memory, Chunk, Entity, Session, etc.
│   │       ├── tier.rs                 # Tier enum + display/parse
│   │       ├── frontmatter.rs          # YAML frontmatter parser/serializer
│   │       ├── file_io.rs              # read_memory_file / write_memory_file
│   │       ├── paths.rs                # XDG path resolution
│   │       ├── storage/
│   │       │   ├── mod.rs              # Storage struct (libSQL handle + ops)
│   │       │   ├── migrations.rs       # version 1 migration
│   │       │   ├── memory_ops.rs       # insert/update/get/list/forget
│   │       │   ├── chunk_ops.rs        # chunk CRUD (used by future plans)
│   │       │   ├── entity_ops.rs       # entity CRUD (stubs; full use in Plan 4)
│   │       │   ├── audit.rs            # append-only audit log
│   │       │   └── triggers.rs         # SQL triggers (FTS sync, audit lock)
│   │       ├── retrieval/
│   │       │   ├── mod.rs              # Recall trait + RecallOpts
│   │       │   └── bm25.rs             # FTS5-based BM25 retriever
│   │       ├── watcher.rs              # notify crate file watcher
│   │       ├── rebuild.rs              # full reindex from files
│   │       └── doctor.rs               # drift detection
│   └── mnemos_cli/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                 # tokio entry + clap dispatch
│           ├── cli.rs                  # clap definitions
│           └── commands/
│               ├── mod.rs
│               ├── remember.rs
│               ├── recall.rs
│               ├── get.rs
│               ├── list.rs
│               ├── forget.rs
│               ├── rebuild.rs
│               ├── doctor.rs
│               └── status.rs
└── crates/mnemos_core/tests/           # integration tests
    ├── frontmatter_roundtrip.rs
    ├── storage_crud.rs
    ├── bm25_retrieval.rs
    ├── bi_temporal.rs
    ├── audit_log.rs
    ├── rebuild.rs
    └── watcher.rs
```

---

## Conventions used by every task

- Every code change is preceded by a failing test (TDD). Infrastructure-only tasks (scaffolding, config) skip the failing-test step.
- Every task ends with a commit. Commit messages use the form `feat:` / `chore:` / `test:` / `fix:` per CLAUDE.md rules.
- Rust 2021 edition; `cargo fmt` + `cargo clippy -- -D warnings` must pass at every commit.
- All file paths in this plan are **relative to `/home/jons/AntiGravityProjects/mnemos/`**.
- All `cargo` commands assume `cd /home/jons/AntiGravityProjects/mnemos`.
- The libSQL `Connection` type is async; tests use `#[tokio::test]`.

---

## Task 1: Initialize Cargo workspace

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `.gitignore`
- Create: `LICENSE`

- [ ] **Step 1: Write workspace `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = ["crates/mnemos_core", "crates/mnemos_cli"]

[workspace.package]
version = "0.0.1"
edition = "2021"
license = "Apache-2.0"
authors = ["Shaun Jones <sjones@armellini.com>"]
repository = "https://github.com/sjones/mnemos"
rust-version = "1.78"

[workspace.dependencies]
# Async runtime
tokio = { version = "1.40", features = ["rt-multi-thread", "macros", "fs", "io-util", "sync", "time", "signal"] }

# Database
libsql = { version = "0.5", default-features = false, features = ["core", "serde"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
serde_json = "1"

# Time + IDs
chrono = { version = "0.4", features = ["serde"] }
ulid = { version = "1", features = ["serde"] }

# Errors + logs
anyhow = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# CLI
clap = { version = "4", features = ["derive", "env"] }
directories = "5"

# File watching
notify = "6"
notify-debouncer-full = "0.3"

# Test helpers
assert_cmd = "2"
predicates = "3"
tempfile = "3"
insta = { version = "1", features = ["yaml"] }

[profile.release]
lto = "thin"
codegen-units = 1
strip = "symbols"
```

- [ ] **Step 2: Write `rust-toolchain.toml`**

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3: Write `.gitignore`**

```gitignore
/target/
**/*.rs.bk
Cargo.lock
.env
.env.local
*.swp
.DS_Store
/dist/
/.tauri/
/node_modules/
/ui/dist/
/ui/src-tauri/target/
```

- [ ] **Step 4: Write `LICENSE`** (Apache-2.0)

Use the standard Apache-2.0 text from `https://www.apache.org/licenses/LICENSE-2.0.txt`. Save verbatim to `LICENSE`.

- [ ] **Step 5: Verify workspace loads**

Run: `cargo check --workspace`
Expected: warnings about missing member crates (we add them in Task 2). No fatal errors on the workspace manifest.

If the check complains about missing members, that's fine — we add them next.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml rust-toolchain.toml .gitignore LICENSE
git commit -m "chore: initialize Cargo workspace with pinned toolchain"
```

---

## Task 2: Scaffold `mnemos_core` crate

**Files:**
- Create: `crates/mnemos_core/Cargo.toml`
- Create: `crates/mnemos_core/src/lib.rs`

- [ ] **Step 1: Write `crates/mnemos_core/Cargo.toml`**

```toml
[package]
name = "mnemos_core"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
rust-version.workspace = true

[dependencies]
tokio = { workspace = true }
libsql = { workspace = true }
serde = { workspace = true }
serde_yaml = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
ulid = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
notify = { workspace = true }
notify-debouncer-full = { workspace = true }
directories = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
insta = { workspace = true }
```

- [ ] **Step 2: Write `crates/mnemos_core/src/lib.rs`**

```rust
//! Mnemos core: storage, types, file I/O, retrieval.
//!
//! This crate is transport-agnostic. CLI, daemon, and UI all sit on top of it.

#![deny(rust_2018_idioms)]
#![warn(clippy::all)]

pub mod error;
pub mod id;
pub mod paths;
pub mod tier;
pub mod types;
pub mod frontmatter;
pub mod file_io;
pub mod storage;
pub mod retrieval;
pub mod watcher;
pub mod rebuild;
pub mod doctor;

pub use error::{MnemosError, Result};
pub use storage::Storage;
pub use tier::Tier;
pub use types::{Memory, MemoryType};
```

- [ ] **Step 3: Stub out every referenced module with an empty file**

For each `pub mod X;` line above, create the corresponding `src/X.rs` (or `src/X/mod.rs`) as an empty file. We'll fill them in subsequent tasks. Without these stubs `cargo check` will fail.

```bash
cd crates/mnemos_core/src
touch error.rs id.rs paths.rs tier.rs types.rs frontmatter.rs file_io.rs watcher.rs rebuild.rs doctor.rs
mkdir -p storage retrieval
touch storage/mod.rs retrieval/mod.rs
```

- [ ] **Step 4: Verify the crate compiles**

Run: `cargo check -p mnemos_core`
Expected: PASS. Lots of warnings about unused items — fine.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/
git commit -m "feat(core): scaffold mnemos_core crate with module skeleton"
```

---

## Task 3: Error type and Result alias

**Files:**
- Modify: `crates/mnemos_core/src/error.rs`
- Test: `crates/mnemos_core/tests/error_display.rs`

- [ ] **Step 1: Write failing test `crates/mnemos_core/tests/error_display.rs`**

```rust
use mnemos_core::MnemosError;

#[test]
fn invalid_frontmatter_error_includes_path() {
    let err = MnemosError::InvalidFrontmatter {
        path: "/tmp/bad.md".into(),
        reason: "missing 'tier'".into(),
    };
    let msg = format!("{err}");
    assert!(msg.contains("/tmp/bad.md"));
    assert!(msg.contains("missing 'tier'"));
}

#[test]
fn not_found_error_includes_id() {
    let err = MnemosError::MemoryNotFound("mem_01HXTEST".into());
    assert!(format!("{err}").contains("mem_01HXTEST"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test error_display`
Expected: FAIL with "cannot find struct `MnemosError`" or similar.

- [ ] **Step 3: Implement `crates/mnemos_core/src/error.rs`**

```rust
use std::path::PathBuf;
use thiserror::Error;

pub type Result<T, E = MnemosError> = std::result::Result<T, E>;

#[derive(Error, Debug)]
pub enum MnemosError {
    #[error("memory not found: {0}")]
    MemoryNotFound(String),

    #[error("entity not found: {0}")]
    EntityNotFound(String),

    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("invalid frontmatter at {path}: {reason}")]
    InvalidFrontmatter { path: PathBuf, reason: String },

    #[error("malformed memory file at {path}: {reason}")]
    MalformedFile { path: PathBuf, reason: String },

    #[error("path resolution failed: {0}")]
    PathError(String),

    #[error("database error: {0}")]
    Database(#[from] libsql::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("migration {version} failed: {reason}")]
    Migration { version: u32, reason: String },

    #[error("schema drift detected: {0}")]
    SchemaDrift(String),

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("internal: {0}")]
    Internal(String),
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p mnemos_core --test error_display`
Expected: PASS (both tests).

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/error.rs crates/mnemos_core/tests/error_display.rs
git commit -m "feat(core): MnemosError enum with thiserror"
```

---

## Task 4: ULID helpers

**Files:**
- Modify: `crates/mnemos_core/src/id.rs`
- Test: `crates/mnemos_core/tests/id.rs`

- [ ] **Step 1: Write failing test `crates/mnemos_core/tests/id.rs`**

```rust
use mnemos_core::id::{new_memory_id, new_chunk_id, new_session_id, new_entity_id, parse_id};

#[test]
fn memory_id_has_correct_prefix() {
    let id = new_memory_id();
    assert!(id.starts_with("mem_"));
    assert_eq!(id.len(), 4 + 26); // "mem_" + 26-char ULID
}

#[test]
fn chunk_session_entity_ids_have_correct_prefixes() {
    assert!(new_chunk_id().starts_with("chunk_"));
    assert!(new_session_id().starts_with("sess_"));
    assert!(new_entity_id().starts_with("ent_"));
}

#[test]
fn ids_are_sortable_by_creation_time() {
    let a = new_memory_id();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let b = new_memory_id();
    assert!(a < b, "ULIDs should sort chronologically");
}

#[test]
fn parse_id_rejects_bad_input() {
    assert!(parse_id("mem_NOT_A_ULID").is_err());
    assert!(parse_id("no_prefix").is_err());
    assert!(parse_id(&new_memory_id()).is_ok());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test id`
Expected: FAIL — module functions don't exist yet.

- [ ] **Step 3: Implement `crates/mnemos_core/src/id.rs`**

```rust
use crate::error::{MnemosError, Result};
use ulid::Ulid;

pub fn new_memory_id() -> String {
    format!("mem_{}", Ulid::new())
}

pub fn new_chunk_id() -> String {
    format!("chunk_{}", Ulid::new())
}

pub fn new_session_id() -> String {
    format!("sess_{}", Ulid::new())
}

pub fn new_entity_id() -> String {
    format!("ent_{}", Ulid::new())
}

pub fn new_edge_id() -> String {
    format!("edge_{}", Ulid::new())
}

/// Parse an id of the form `<prefix>_<ulid>`; returns the ULID portion.
pub fn parse_id(id: &str) -> Result<Ulid> {
    let (_prefix, ulid_str) = id
        .split_once('_')
        .ok_or_else(|| MnemosError::Validation(format!("id missing prefix: {id}")))?;
    Ulid::from_string(ulid_str)
        .map_err(|e| MnemosError::Validation(format!("invalid ulid in {id}: {e}")))
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_core --test id`
Expected: PASS (all four tests).

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/id.rs crates/mnemos_core/tests/id.rs
git commit -m "feat(core): ULID-based ID helpers with prefixes"
```

---

## Task 5: Tier enum

**Files:**
- Modify: `crates/mnemos_core/src/tier.rs`
- Test: `crates/mnemos_core/tests/tier.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::Tier;
use std::str::FromStr;

#[test]
fn tier_round_trips_through_string() {
    for tier in [Tier::Working, Tier::Episodic, Tier::Semantic, Tier::Procedural, Tier::Reflection] {
        let s = tier.as_str();
        let parsed: Tier = s.parse().unwrap();
        assert_eq!(parsed, tier);
    }
}

#[test]
fn tier_parse_rejects_unknown() {
    assert!("frobnicated".parse::<Tier>().is_err());
}

#[test]
fn tier_directory_names_are_stable() {
    assert_eq!(Tier::Working.dir_name(), "working");
    assert_eq!(Tier::Episodic.dir_name(), "episodic");
    assert_eq!(Tier::Semantic.dir_name(), "semantic");
    assert_eq!(Tier::Procedural.dir_name(), "procedural");
    assert_eq!(Tier::Reflection.dir_name(), "reflections");
}

#[test]
fn tier_serde_uses_kebab_case() {
    let json = serde_json::to_string(&Tier::Working).unwrap();
    assert_eq!(json, "\"working\"");
    let back: Tier = serde_json::from_str(&json).unwrap();
    assert_eq!(back, Tier::Working);
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test tier`
Expected: FAIL — `Tier` not defined.

- [ ] **Step 3: Implement `crates/mnemos_core/src/tier.rs`**

```rust
use crate::error::{MnemosError, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    Working,
    Episodic,
    Semantic,
    Procedural,
    Reflection,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Working => "working",
            Tier::Episodic => "episodic",
            Tier::Semantic => "semantic",
            Tier::Procedural => "procedural",
            Tier::Reflection => "reflection",
        }
    }

    /// Directory name on disk. `Reflection` maps to `reflections/` (pluralized)
    /// for human-friendliness; all others match `as_str`.
    pub fn dir_name(self) -> &'static str {
        match self {
            Tier::Reflection => "reflections",
            other => other.as_str(),
        }
    }

    pub fn all() -> &'static [Tier] {
        &[
            Tier::Working,
            Tier::Episodic,
            Tier::Semantic,
            Tier::Procedural,
            Tier::Reflection,
        ]
    }

    /// Default weight used by retrieval ranking. Tunable later via config.
    pub fn default_weight(self) -> f64 {
        match self {
            Tier::Working => 2.0,
            Tier::Procedural => 1.5,
            Tier::Reflection => 1.2,
            Tier::Semantic => 1.0,
            Tier::Episodic => 0.8,
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Tier {
    type Err = MnemosError;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "working" => Ok(Tier::Working),
            "episodic" => Ok(Tier::Episodic),
            "semantic" => Ok(Tier::Semantic),
            "procedural" => Ok(Tier::Procedural),
            "reflection" | "reflections" => Ok(Tier::Reflection),
            other => Err(MnemosError::Validation(format!("unknown tier: {other}"))),
        }
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_core --test tier`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/tier.rs crates/mnemos_core/tests/tier.rs
git commit -m "feat(core): Tier enum with directory names and default weights"
```

---

## Task 6: XDG path resolution

**Files:**
- Modify: `crates/mnemos_core/src/paths.rs`
- Test: `crates/mnemos_core/tests/paths.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::paths::Paths;
use tempfile::TempDir;

#[test]
fn paths_with_override_uses_given_root() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    assert_eq!(paths.root, tmp.path());
    assert_eq!(paths.files_dir, tmp.path().join("files"));
    assert_eq!(paths.db_path, tmp.path().join("index.db"));
    assert_eq!(paths.tier_dir(mnemos_core::Tier::Working), tmp.path().join("files/working"));
}

#[test]
fn paths_ensure_dirs_creates_all_tier_dirs() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    paths.ensure_dirs().unwrap();
    for tier in mnemos_core::Tier::all() {
        assert!(paths.tier_dir(*tier).is_dir(), "{} dir missing", tier);
    }
    assert!(paths.quarantine_dir.is_dir());
    assert!(paths.archived_dir.is_dir());
    assert!(paths.entities_dir.is_dir());
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test paths`
Expected: FAIL — `Paths` not defined.

- [ ] **Step 3: Implement `crates/mnemos_core/src/paths.rs`**

```rust
use crate::error::{MnemosError, Result};
use crate::tier::Tier;
use directories::ProjectDirs;
use std::path::{Path, PathBuf};

/// Resolved on-disk paths for a Mnemos vault.
#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub files_dir: PathBuf,
    pub db_path: PathBuf,
    pub quarantine_dir: PathBuf,
    pub archived_dir: PathBuf,
    pub entities_dir: PathBuf,
}

impl Paths {
    /// XDG defaults: `~/.local/share/mnemos/`.
    pub fn default_xdg() -> Result<Self> {
        let dirs = ProjectDirs::from("dev", "mnemos", "mnemos")
            .ok_or_else(|| MnemosError::PathError("could not resolve XDG dirs".into()))?;
        Ok(Self::with_root(dirs.data_dir()))
    }

    pub fn with_root(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            files_dir: root.join("files"),
            db_path: root.join("index.db"),
            quarantine_dir: root.join("files").join("quarantine"),
            archived_dir: root.join("files").join("archived"),
            entities_dir: root.join("files").join("entities"),
        }
    }

    pub fn tier_dir(&self, tier: Tier) -> PathBuf {
        self.files_dir.join(tier.dir_name())
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(&self.files_dir)?;
        for tier in Tier::all() {
            std::fs::create_dir_all(self.tier_dir(*tier))?;
        }
        std::fs::create_dir_all(&self.quarantine_dir)?;
        std::fs::create_dir_all(&self.archived_dir)?;
        std::fs::create_dir_all(&self.entities_dir)?;
        Ok(())
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_core --test paths`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/paths.rs crates/mnemos_core/tests/paths.rs
git commit -m "feat(core): XDG-compliant path resolution with vault override"
```

---

## Task 7: Memory + related types

**Files:**
- Modify: `crates/mnemos_core/src/types.rs`
- Test: `crates/mnemos_core/tests/types_serde.rs`

- [ ] **Step 1: Write failing test**

```rust
use chrono::{TimeZone, Utc};
use mnemos_core::types::{Memory, MemoryType, Provenance};
use mnemos_core::Tier;

#[test]
fn memory_serializes_to_frontmatter_yaml() {
    let mem = Memory {
        id: "mem_01HXTEST".into(),
        tier: Tier::Semantic,
        kind: MemoryType::Fact,
        title: "User prefers Tauri".into(),
        body: "Because of small bundle size.".into(),
        tags: vec!["tech-pref".into()],
        entities: vec!["tauri".into()],
        links: vec![],
        provenance: vec![Provenance {
            session: Some("sess_01HX".into()),
            chunks: vec!["chunk_01HA".into()],
        }],
        created_at: Utc.with_ymd_and_hms(2026, 5, 22, 14, 30, 0).unwrap(),
        ingested_at: Utc.with_ymd_and_hms(2026, 5, 22, 14, 30, 5).unwrap(),
        valid_at: Utc.with_ymd_and_hms(2026, 5, 22, 14, 30, 0).unwrap(),
        invalid_at: None,
        superseded_by: None,
        strength: 1.0,
        importance: 0.7,
        last_accessed: Utc.with_ymd_and_hms(2026, 5, 22, 14, 30, 0).unwrap(),
        access_count: 0,
        workspace: None,
        source_tool: None,
        mnemos_version: 1,
    };
    let yaml = serde_yaml::to_string(&mem).unwrap();
    assert!(yaml.contains("id: mem_01HXTEST"));
    assert!(yaml.contains("tier: semantic"));
    assert!(yaml.contains("type: fact"));
    assert!(yaml.contains("strength: 1.0"));
    let back: Memory = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(back.id, mem.id);
    assert_eq!(back.tier, mem.tier);
    assert_eq!(back.strength, mem.strength);
}

#[test]
fn memory_type_serializes_kebab_case() {
    let json = serde_json::to_string(&MemoryType::CommunitySummary).unwrap();
    assert_eq!(json, "\"community-summary\"");
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test types_serde`
Expected: FAIL — types not defined.

- [ ] **Step 3: Implement `crates/mnemos_core/src/types.rs`**

```rust
use crate::tier::Tier;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MemoryType {
    Fact,
    Episode,
    Reflection,
    Rule,
    Identity,
    Project,
    Entity,
    CommunitySummary,
}

/// Provenance link: which session and chunks the memory was derived from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub session: Option<String>,
    #[serde(default)]
    pub chunks: Vec<String>,
}

/// One memory = one markdown file. The struct mirrors the YAML frontmatter
/// exactly; the body is held separately.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub tier: Tier,
    #[serde(rename = "type")]
    pub kind: MemoryType,
    pub title: String,
    #[serde(skip)]
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub entities: Vec<String>,
    #[serde(default)]
    pub links: Vec<String>,
    #[serde(default)]
    pub provenance: Vec<Provenance>,
    pub created_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
    pub valid_at: DateTime<Utc>,
    #[serde(default)]
    pub invalid_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub superseded_by: Option<String>,
    pub strength: f64,
    pub importance: f64,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u64,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub source_tool: Option<String>,
    #[serde(default = "default_mnemos_version")]
    pub mnemos_version: u32,
}

fn default_mnemos_version() -> u32 {
    1
}

impl Memory {
    pub fn new_now(id: String, tier: Tier, kind: MemoryType, title: String, body: String) -> Self {
        let now = Utc::now();
        Self {
            id, tier, kind, title, body,
            tags: vec![], entities: vec![], links: vec![], provenance: vec![],
            created_at: now, ingested_at: now, valid_at: now,
            invalid_at: None, superseded_by: None,
            strength: 1.0, importance: 0.5,
            last_accessed: now, access_count: 0,
            workspace: None, source_tool: None, mnemos_version: 1,
        }
    }

    pub fn is_valid(&self, at: DateTime<Utc>) -> bool {
        self.valid_at <= at && self.invalid_at.map_or(true, |iv| at < iv)
    }
}

/// Raw conversation chunk preserved verbatim (anti-mem0 design).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub session_id: String,
    pub speaker: Option<String>,
    pub ordinal: u32,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub source_tool: Option<String>,
    pub source_meta: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub source_tool: Option<String>,
    pub workspace: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: Option<String>,
    pub file_path: Option<String>,
    pub created_at: DateTime<Utc>,
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_core --test types_serde`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/types.rs crates/mnemos_core/tests/types_serde.rs
git commit -m "feat(core): Memory, Chunk, Session, Entity types with serde"
```

---

## Task 8: YAML frontmatter parser

**Files:**
- Modify: `crates/mnemos_core/src/frontmatter.rs`
- Test: `crates/mnemos_core/tests/frontmatter_roundtrip.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::frontmatter::{parse_frontmatter, serialize_with_frontmatter};
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::Tier;

const SAMPLE: &str = "---
id: mem_01HXEXAMPLE
tier: semantic
type: fact
title: \"User prefers Tauri\"
tags:
- tech-pref
entities:
- tauri
links: []
provenance: []
created_at: 2026-05-22T14:30:00Z
ingested_at: 2026-05-22T14:30:05Z
valid_at: 2026-05-22T14:30:00Z
invalid_at: null
superseded_by: null
strength: 1.0
importance: 0.7
last_accessed: 2026-05-22T14:30:00Z
access_count: 0
mnemos_version: 1
---

Body content goes here.

Second paragraph.
";

#[test]
fn parses_frontmatter_and_body() {
    let (mem, body) = parse_frontmatter(SAMPLE).unwrap();
    assert_eq!(mem.id, "mem_01HXEXAMPLE");
    assert_eq!(mem.tier, Tier::Semantic);
    assert_eq!(mem.kind, MemoryType::Fact);
    assert_eq!(mem.strength, 1.0);
    assert!(body.contains("Body content goes here."));
    assert!(body.contains("Second paragraph."));
    assert!(!body.starts_with('\n'), "leading blank line should be trimmed");
}

#[test]
fn roundtrip_preserves_data() {
    let (mem_in, body_in) = parse_frontmatter(SAMPLE).unwrap();
    let mut mem = mem_in.clone();
    mem.body = body_in.clone();
    let serialized = serialize_with_frontmatter(&mem).unwrap();
    let (mem_out, body_out) = parse_frontmatter(&serialized).unwrap();
    assert_eq!(mem_in.id, mem_out.id);
    assert_eq!(mem_in.tier, mem_out.tier);
    assert_eq!(mem_in.created_at, mem_out.created_at);
    assert_eq!(mem_in.strength, mem_out.strength);
    assert_eq!(body_in.trim(), body_out.trim());
}

#[test]
fn parse_rejects_missing_delimiter() {
    let result = parse_frontmatter("no frontmatter here");
    assert!(result.is_err());
}

#[test]
fn parse_rejects_truncated_frontmatter() {
    let result = parse_frontmatter("---\nid: mem_X\nno closing");
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test frontmatter_roundtrip`
Expected: FAIL — functions not defined.

- [ ] **Step 3: Implement `crates/mnemos_core/src/frontmatter.rs`**

```rust
use crate::error::{MnemosError, Result};
use crate::types::Memory;
use std::path::Path;

const DELIM: &str = "---";

/// Parse a markdown file with YAML frontmatter into a `Memory` (frontmatter)
/// and the body text. The returned `Memory.body` field is left as-is from
/// frontmatter (empty by default); the body string is the second tuple element.
pub fn parse_frontmatter(text: &str) -> Result<(Memory, String)> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text); // strip BOM
    let rest = text
        .strip_prefix(DELIM)
        .ok_or_else(|| MnemosError::Validation("missing opening '---' delimiter".into()))?;
    // First newline after the opening delimiter
    let rest = rest.trim_start_matches('\r').strip_prefix('\n')
        .ok_or_else(|| MnemosError::Validation("expected newline after opening '---'".into()))?;
    // Closing delimiter on its own line
    let end_idx = rest
        .find("\n---")
        .ok_or_else(|| MnemosError::Validation("missing closing '---' delimiter".into()))?;
    let yaml_part = &rest[..end_idx];
    let after = &rest[end_idx + 4..]; // skip "\n---"
    let body = after.trim_start_matches(|c: char| c == '\r' || c == '\n').to_string();

    let mut mem: Memory = serde_yaml::from_str(yaml_part)?;
    mem.body = body.clone();
    Ok((mem, body))
}

/// Inverse of `parse_frontmatter`. Uses `mem.body` for the body.
pub fn serialize_with_frontmatter(mem: &Memory) -> Result<String> {
    let yaml = serde_yaml::to_string(mem)?;
    Ok(format!("---\n{yaml}---\n\n{}", mem.body))
}

/// Parse with a `path` for richer error reporting.
pub fn parse_frontmatter_at(text: &str, path: &Path) -> Result<(Memory, String)> {
    parse_frontmatter(text).map_err(|e| match e {
        MnemosError::Validation(reason) => MnemosError::InvalidFrontmatter {
            path: path.to_path_buf(),
            reason,
        },
        other => other,
    })
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_core --test frontmatter_roundtrip`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/frontmatter.rs crates/mnemos_core/tests/frontmatter_roundtrip.rs
git commit -m "feat(core): YAML frontmatter parser with roundtrip"
```

---

## Task 9: File I/O — read & atomic write

**Files:**
- Modify: `crates/mnemos_core/src/file_io.rs`
- Test: `crates/mnemos_core/tests/file_io.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::file_io::{read_memory_file, write_memory_file, content_hash};
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::{Tier, paths::Paths, id::new_memory_id};
use tempfile::TempDir;

#[tokio::test]
async fn write_then_read_roundtrips() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    paths.ensure_dirs().unwrap();

    let mem = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "Round trip test".into(),
        "body text here".into(),
    );
    let file_path = write_memory_file(&paths, &mem).await.unwrap();
    assert!(file_path.exists());

    let (loaded, body) = read_memory_file(&file_path).await.unwrap();
    assert_eq!(loaded.id, mem.id);
    assert_eq!(loaded.title, mem.title);
    assert_eq!(body.trim(), "body text here");
}

#[tokio::test]
async fn atomic_write_uses_temp_then_rename() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    paths.ensure_dirs().unwrap();

    let mem = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "Atomic".into(),
        "x".into(),
    );
    let path = write_memory_file(&paths, &mem).await.unwrap();
    // After write, no stray .tmp files
    let tier_dir = paths.tier_dir(Tier::Semantic);
    let mut tmp_files = 0;
    let mut dir = tokio::fs::read_dir(&tier_dir).await.unwrap();
    while let Some(e) = dir.next_entry().await.unwrap() {
        if e.path().extension().and_then(|s| s.to_str()) == Some("tmp") {
            tmp_files += 1;
        }
    }
    assert_eq!(tmp_files, 0);
    assert!(path.starts_with(&tier_dir));
}

#[test]
fn content_hash_is_stable_and_collision_resistant() {
    let h1 = content_hash("abc");
    let h2 = content_hash("abc");
    let h3 = content_hash("abd");
    assert_eq!(h1, h2);
    assert_ne!(h1, h3);
    assert_eq!(h1.len(), 64); // hex sha256
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test file_io`
Expected: FAIL — functions don't exist.

- [ ] **Step 3: Add `sha2` to the workspace and crate**

Edit `Cargo.toml` (workspace dependencies):

```toml
sha2 = "0.10"
```

Edit `crates/mnemos_core/Cargo.toml` dependencies, add:

```toml
sha2 = { workspace = true }
```

- [ ] **Step 4: Implement `crates/mnemos_core/src/file_io.rs`**

```rust
use crate::error::Result;
use crate::frontmatter::{parse_frontmatter_at, serialize_with_frontmatter};
use crate::paths::Paths;
use crate::types::Memory;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// Read a memory file from disk. Returns the Memory (with body populated)
/// and the body string (also held by the Memory, for ergonomic access).
pub async fn read_memory_file(path: &Path) -> Result<(Memory, String)> {
    let text = tokio::fs::read_to_string(path).await?;
    parse_frontmatter_at(&text, path)
}

/// Write a memory to disk atomically (tmp + rename). Returns the path.
/// Path layout: `<files>/<tier_dir>/<id>.md` for most tiers; entity-scoped
/// semantic memories live at `<files>/semantic/<entity_slug>/<id>.md` and
/// can be relocated later — this function uses the flat path by default.
pub async fn write_memory_file(paths: &Paths, mem: &Memory) -> Result<PathBuf> {
    let dir = paths.tier_dir(mem.tier);
    tokio::fs::create_dir_all(&dir).await?;
    let final_path = dir.join(format!("{}.md", mem.id));
    let tmp_path = dir.join(format!("{}.md.tmp", mem.id));

    let serialized = serialize_with_frontmatter(mem)?;
    {
        let mut f = tokio::fs::File::create(&tmp_path).await?;
        f.write_all(serialized.as_bytes()).await?;
        f.sync_data().await?;
    }
    tokio::fs::rename(&tmp_path, &final_path).await?;
    Ok(final_path)
}

/// Stable SHA-256 hex hash of any text (used to detect file ↔ DB drift).
pub fn content_hash(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    format!("{:x}", digest)
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p mnemos_core --test file_io`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/mnemos_core/src/file_io.rs crates/mnemos_core/tests/file_io.rs Cargo.toml crates/mnemos_core/Cargo.toml
git commit -m "feat(core): atomic file write + sha256 content hash"
```

---

## Task 10: Storage struct + libSQL connection

**Files:**
- Modify: `crates/mnemos_core/src/storage/mod.rs`
- Test: `crates/mnemos_core/tests/storage_open.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn opens_fresh_db_and_reports_schema_version() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let storage = Storage::open(&db_path).await.unwrap();
    assert!(db_path.exists());
    assert_eq!(storage.schema_version().await.unwrap(), 1);
}

#[tokio::test]
async fn reopening_existing_db_does_not_double_migrate() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    {
        let s = Storage::open(&db_path).await.unwrap();
        assert_eq!(s.schema_version().await.unwrap(), 1);
    }
    {
        let s = Storage::open(&db_path).await.unwrap();
        assert_eq!(s.schema_version().await.unwrap(), 1);
    }
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test storage_open`
Expected: FAIL — `Storage` not defined.

- [ ] **Step 3: Implement `crates/mnemos_core/src/storage/mod.rs`**

```rust
pub mod migrations;
pub mod memory_ops;
pub mod chunk_ops;
pub mod entity_ops;
pub mod audit;
pub mod triggers;

use crate::error::Result;
use libsql::{Builder, Connection, Database};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// libSQL handle plus a serialized-write mutex. Reads can go through the
/// `Connection` directly; writes acquire `write_lock` to serialize against
/// each other (SQLite is single-writer anyway).
#[derive(Clone)]
pub struct Storage {
    db: Arc<Database>,
    write_lock: Arc<Mutex<()>>,
}

impl Storage {
    pub async fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let db = Builder::new_local(db_path).build().await?;
        let storage = Self {
            db: Arc::new(db),
            write_lock: Arc::new(Mutex::new(())),
        };
        storage.apply_migrations().await?;
        Ok(storage)
    }

    /// Returns a fresh connection. Each pool checkout in libsql is cheap.
    pub fn conn(&self) -> Result<Connection> {
        Ok(self.db.connect()?)
    }

    /// Acquire the write lock and return a guarded connection.
    pub async fn write_conn(&self) -> Result<(Connection, tokio::sync::MutexGuard<'_, ()>)> {
        let guard = self.write_lock.lock().await;
        Ok((self.conn()?, guard))
    }

    pub async fn schema_version(&self) -> Result<u32> {
        let conn = self.conn()?;
        let mut rows = conn.query("SELECT MAX(version) FROM schema_migrations", ()).await?;
        let row = rows.next().await?.ok_or_else(|| crate::error::MnemosError::Internal(
            "schema_migrations table empty".into()
        ))?;
        let v: i64 = row.get(0)?;
        Ok(v as u32)
    }
}
```

- [ ] **Step 4: Stub `migrations.rs` so the crate compiles**

Add minimal content to `crates/mnemos_core/src/storage/migrations.rs`:

```rust
use crate::error::Result;
use crate::storage::Storage;

impl Storage {
    pub(crate) async fn apply_migrations(&self) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
            (),
        ).await?;
        // Task 11 fills in v1.
        conn.execute(
            "INSERT OR IGNORE INTO schema_migrations (version) VALUES (1)",
            (),
        ).await?;
        Ok(())
    }
}
```

Add stubs to the other modules so they compile:

```bash
# Each is intentionally empty for now.
echo "// Populated in later tasks." > crates/mnemos_core/src/storage/memory_ops.rs
echo "// Populated in later tasks." > crates/mnemos_core/src/storage/chunk_ops.rs
echo "// Populated in later tasks." > crates/mnemos_core/src/storage/entity_ops.rs
echo "// Populated in later tasks." > crates/mnemos_core/src/storage/audit.rs
echo "// Populated in later tasks." > crates/mnemos_core/src/storage/triggers.rs
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p mnemos_core --test storage_open`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/mnemos_core/src/storage/
git commit -m "feat(core): Storage struct opens libSQL DB and tracks schema version"
```

---

## Task 11: Schema migration v1 (memories, chunks, sessions, entities, edges, links, audit)

**Files:**
- Modify: `crates/mnemos_core/src/storage/migrations.rs`
- Test: `crates/mnemos_core/tests/schema_v1.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn migration_v1_creates_all_expected_tables() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("v1.db")).await.unwrap();
    let conn = storage.conn().unwrap();

    let expected = [
        "memories", "chunks", "sessions",
        "entities", "entity_mentions", "entity_edges",
        "memory_links", "memory_chunks",
        "audit_log", "schema_migrations",
    ];
    for table in expected {
        let mut rows = conn
            .query(
                "SELECT name FROM sqlite_master WHERE type='table' AND name=?",
                libsql::params![table],
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap();
        assert!(row.is_some(), "missing table: {table}");
    }
}

#[tokio::test]
async fn migration_v1_creates_fts5_tables() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("fts.db")).await.unwrap();
    let conn = storage.conn().unwrap();

    for vt in ["memory_fts", "chunk_fts"] {
        let mut rows = conn
            .query(
                "SELECT name FROM sqlite_master WHERE name=?",
                libsql::params![vt],
            )
            .await
            .unwrap();
        assert!(rows.next().await.unwrap().is_some(), "missing virtual table: {vt}");
    }
}

#[tokio::test]
async fn migration_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("idem.db");
    let _ = Storage::open(&path).await.unwrap();
    let _ = Storage::open(&path).await.unwrap();
    let s = Storage::open(&path).await.unwrap();
    assert_eq!(s.schema_version().await.unwrap(), 1);
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test schema_v1`
Expected: FAIL — most tables missing.

- [ ] **Step 3: Implement `crates/mnemos_core/src/storage/migrations.rs`**

Replace the stub from Task 10 with the full v1 migration:

```rust
use crate::error::Result;
use crate::storage::Storage;

impl Storage {
    pub(crate) async fn apply_migrations(&self) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
            (),
        ).await?;

        let mut rows = conn.query("SELECT MAX(version) FROM schema_migrations", ()).await?;
        let current: i64 = rows.next().await?.and_then(|r| r.get::<i64>(0).ok()).unwrap_or(0);
        drop(rows);

        if current < 1 {
            migration_v1(&conn).await?;
            conn.execute(
                "INSERT OR IGNORE INTO schema_migrations (version) VALUES (1)",
                (),
            ).await?;
        }
        Ok(())
    }
}

async fn migration_v1(conn: &libsql::Connection) -> Result<()> {
    for stmt in V1_STATEMENTS {
        conn.execute(stmt, ()).await?;
    }
    Ok(())
}

const V1_STATEMENTS: &[&str] = &[
    // ── memories ─────────────────────────────────────────────────────────
    "CREATE TABLE IF NOT EXISTS memories (
        id              TEXT PRIMARY KEY,
        tier            TEXT NOT NULL CHECK(tier IN
            ('working','episodic','semantic','procedural','reflection')),
        kind            TEXT NOT NULL,
        title           TEXT NOT NULL,
        body            TEXT NOT NULL,
        file_path       TEXT NOT NULL UNIQUE,
        content_hash    TEXT NOT NULL,
        tags_json       TEXT NOT NULL DEFAULT '[]',
        entities_json   TEXT NOT NULL DEFAULT '[]',
        links_json      TEXT NOT NULL DEFAULT '[]',
        provenance_json TEXT NOT NULL DEFAULT '[]',
        created_at      TEXT NOT NULL,
        ingested_at     TEXT NOT NULL,
        valid_at        TEXT NOT NULL,
        invalid_at      TEXT,
        superseded_by   TEXT,
        strength        REAL NOT NULL DEFAULT 1.0,
        importance      REAL NOT NULL DEFAULT 0.5,
        last_accessed   TEXT NOT NULL,
        access_count    INTEGER NOT NULL DEFAULT 0,
        workspace       TEXT,
        source_tool     TEXT,
        mnemos_version  INTEGER NOT NULL DEFAULT 1,
        version         INTEGER NOT NULL DEFAULT 1
    )",
    "CREATE INDEX IF NOT EXISTS idx_memories_tier      ON memories(tier)",
    "CREATE INDEX IF NOT EXISTS idx_memories_valid     ON memories(valid_at, invalid_at)",
    "CREATE INDEX IF NOT EXISTS idx_memories_strength  ON memories(strength)",
    "CREATE INDEX IF NOT EXISTS idx_memories_workspace ON memories(workspace)",

    // ── sessions ─────────────────────────────────────────────────────────
    "CREATE TABLE IF NOT EXISTS sessions (
        id           TEXT PRIMARY KEY,
        source_tool  TEXT,
        workspace    TEXT,
        started_at   TEXT NOT NULL,
        ended_at     TEXT,
        summary      TEXT
    )",

    // ── chunks ───────────────────────────────────────────────────────────
    "CREATE TABLE IF NOT EXISTS chunks (
        id          TEXT PRIMARY KEY,
        session_id  TEXT NOT NULL,
        speaker     TEXT,
        ordinal     INTEGER NOT NULL,
        body        TEXT NOT NULL,
        created_at  TEXT NOT NULL,
        source_tool TEXT,
        source_meta TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_chunks_session ON chunks(session_id, ordinal)",

    // ── entities + mentions + edges ──────────────────────────────────────
    "CREATE TABLE IF NOT EXISTS entities (
        id          TEXT PRIMARY KEY,
        name        TEXT NOT NULL UNIQUE,
        kind        TEXT NOT NULL,
        aliases     TEXT NOT NULL DEFAULT '[]',
        description TEXT,
        file_path   TEXT,
        created_at  TEXT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS entity_mentions (
        memory_id TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        PRIMARY KEY (memory_id, entity_id),
        FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE,
        FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE
    )",
    "CREATE INDEX IF NOT EXISTS idx_entity_mentions_entity ON entity_mentions(entity_id)",
    "CREATE TABLE IF NOT EXISTS entity_edges (
        id               TEXT PRIMARY KEY,
        source_entity_id TEXT NOT NULL,
        target_entity_id TEXT NOT NULL,
        relation         TEXT NOT NULL,
        created_at       TEXT NOT NULL,
        valid_at         TEXT NOT NULL,
        invalid_at       TEXT,
        weight           REAL NOT NULL DEFAULT 1.0,
        source_memory_ids TEXT NOT NULL DEFAULT '[]'
    )",
    "CREATE INDEX IF NOT EXISTS idx_edges_source   ON entity_edges(source_entity_id)",
    "CREATE INDEX IF NOT EXISTS idx_edges_target   ON entity_edges(target_entity_id)",
    "CREATE INDEX IF NOT EXISTS idx_edges_relation ON entity_edges(relation)",

    // ── links + chunk provenance ────────────────────────────────────────
    "CREATE TABLE IF NOT EXISTS memory_links (
        source_id TEXT NOT NULL,
        target_id TEXT NOT NULL,
        kind      TEXT NOT NULL,
        PRIMARY KEY (source_id, target_id, kind)
    )",
    "CREATE INDEX IF NOT EXISTS idx_links_target ON memory_links(target_id)",
    "CREATE TABLE IF NOT EXISTS memory_chunks (
        memory_id TEXT NOT NULL,
        chunk_id  TEXT NOT NULL,
        PRIMARY KEY (memory_id, chunk_id)
    )",

    // ── audit log (append-only enforced via trigger in Task 15) ─────────
    "CREATE TABLE IF NOT EXISTS audit_log (
        id        INTEGER PRIMARY KEY AUTOINCREMENT,
        ts        TEXT NOT NULL,
        actor     TEXT NOT NULL,
        action    TEXT NOT NULL,
        memory_id TEXT,
        details   TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_audit_memory ON audit_log(memory_id)",
    "CREATE INDEX IF NOT EXISTS idx_audit_ts     ON audit_log(ts)",

    // ── FTS5 virtual tables ─────────────────────────────────────────────
    "CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
        memory_id UNINDEXED, title, body,
        tokenize='porter unicode61'
    )",
    "CREATE VIRTUAL TABLE IF NOT EXISTS chunk_fts USING fts5(
        chunk_id UNINDEXED, body,
        tokenize='porter unicode61'
    )",
];
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_core --test schema_v1`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/storage/migrations.rs crates/mnemos_core/tests/schema_v1.rs
git commit -m "feat(core): schema v1 — memories/chunks/entities/edges/links/audit/FTS5"
```

---

## Task 12: Memory insert + FTS sync

**Files:**
- Modify: `crates/mnemos_core/src/storage/memory_ops.rs`
- Test: `crates/mnemos_core/tests/memory_insert.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::storage::memory_ops::{insert_memory, get_memory};
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::{Storage, Tier, id::new_memory_id};
use tempfile::TempDir;

#[tokio::test]
async fn insert_then_get_roundtrips() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("t.db")).await.unwrap();

    let mut mem = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "Insert test".into(),
        "body content for insertion".into(),
    );
    mem.tags = vec!["tag-a".into(), "tag-b".into()];
    let file_path = format!("/tmp/{}.md", mem.id);
    let content_hash = "abc123".to_string();

    insert_memory(&storage, &mem, &file_path, &content_hash).await.unwrap();

    let loaded = get_memory(&storage, &mem.id).await.unwrap();
    assert_eq!(loaded.id, mem.id);
    assert_eq!(loaded.title, mem.title);
    assert_eq!(loaded.body, mem.body);
    assert_eq!(loaded.tags, vec!["tag-a", "tag-b"]);
    assert_eq!(loaded.tier, Tier::Semantic);
}

#[tokio::test]
async fn insert_writes_to_fts() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("fts.db")).await.unwrap();

    let mut mem = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "Unique findable title".into(),
        "Distinctive body phrase about platypus".into(),
    );
    let _ = mem; // silence unused-mut
    let mem = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "Unique findable title".into(),
        "Distinctive body phrase about platypus".into(),
    );
    insert_memory(&storage, &mem, "/tmp/a.md", "h").await.unwrap();

    let conn = storage.conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT memory_id FROM memory_fts WHERE memory_fts MATCH 'platypus'",
            (),
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap();
    assert!(row.is_some(), "FTS did not index the body");
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test memory_insert`
Expected: FAIL — `insert_memory` / `get_memory` not defined.

- [ ] **Step 3: Implement `crates/mnemos_core/src/storage/memory_ops.rs`**

```rust
use crate::error::{MnemosError, Result};
use crate::storage::Storage;
use crate::tier::Tier;
use crate::types::{Memory, MemoryType};
use chrono::{DateTime, Utc};
use libsql::{params, Row};
use std::str::FromStr;

pub async fn insert_memory(
    storage: &Storage,
    mem: &Memory,
    file_path: &str,
    content_hash: &str,
) -> Result<()> {
    let (conn, _guard) = storage.write_conn().await?;
    let tx = conn.transaction().await?;

    tx.execute(
        "INSERT INTO memories
            (id, tier, kind, title, body, file_path, content_hash,
             tags_json, entities_json, links_json, provenance_json,
             created_at, ingested_at, valid_at, invalid_at, superseded_by,
             strength, importance, last_accessed, access_count,
             workspace, source_tool, mnemos_version, version)
         VALUES
            (?1, ?2, ?3, ?4, ?5, ?6, ?7,
             ?8, ?9, ?10, ?11,
             ?12, ?13, ?14, ?15, ?16,
             ?17, ?18, ?19, ?20,
             ?21, ?22, ?23, ?24)",
        params![
            mem.id.clone(),
            mem.tier.as_str().to_string(),
            serde_json::to_string(&mem.kind)?.trim_matches('"').to_string(),
            mem.title.clone(),
            mem.body.clone(),
            file_path.to_string(),
            content_hash.to_string(),
            serde_json::to_string(&mem.tags)?,
            serde_json::to_string(&mem.entities)?,
            serde_json::to_string(&mem.links)?,
            serde_json::to_string(&mem.provenance)?,
            mem.created_at.to_rfc3339(),
            mem.ingested_at.to_rfc3339(),
            mem.valid_at.to_rfc3339(),
            mem.invalid_at.map(|d| d.to_rfc3339()),
            mem.superseded_by.clone(),
            mem.strength,
            mem.importance,
            mem.last_accessed.to_rfc3339(),
            mem.access_count as i64,
            mem.workspace.clone(),
            mem.source_tool.clone(),
            mem.mnemos_version as i64,
            1_i64,
        ],
    ).await?;

    tx.execute(
        "INSERT INTO memory_fts (memory_id, title, body) VALUES (?1, ?2, ?3)",
        params![mem.id.clone(), mem.title.clone(), mem.body.clone()],
    ).await?;

    for link in &mem.links {
        tx.execute(
            "INSERT OR IGNORE INTO memory_links (source_id, target_id, kind) VALUES (?, ?, 'link')",
            params![mem.id.clone(), link.clone()],
        ).await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn get_memory(storage: &Storage, id: &str) -> Result<Memory> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT id, tier, kind, title, body,
                    tags_json, entities_json, links_json, provenance_json,
                    created_at, ingested_at, valid_at, invalid_at, superseded_by,
                    strength, importance, last_accessed, access_count,
                    workspace, source_tool, mnemos_version
             FROM memories WHERE id = ?",
            params![id.to_string()],
        )
        .await?;
    let row = rows.next().await?.ok_or_else(|| MnemosError::MemoryNotFound(id.into()))?;
    row_to_memory(&row)
}

pub(crate) fn row_to_memory(row: &Row) -> Result<Memory> {
    let tier_str: String = row.get(1)?;
    let kind_str: String = row.get(2)?;
    let kind: MemoryType = serde_json::from_str(&format!("\"{kind_str}\""))?;
    Ok(Memory {
        id: row.get(0)?,
        tier: Tier::from_str(&tier_str)?,
        kind,
        title: row.get(3)?,
        body: row.get(4)?,
        tags: serde_json::from_str(&row.get::<String>(5)?)?,
        entities: serde_json::from_str(&row.get::<String>(6)?)?,
        links: serde_json::from_str(&row.get::<String>(7)?)?,
        provenance: serde_json::from_str(&row.get::<String>(8)?)?,
        created_at: parse_ts(&row.get::<String>(9)?)?,
        ingested_at: parse_ts(&row.get::<String>(10)?)?,
        valid_at: parse_ts(&row.get::<String>(11)?)?,
        invalid_at: row.get::<Option<String>>(12)?.map(|s| parse_ts(&s)).transpose()?,
        superseded_by: row.get(13)?,
        strength: row.get(14)?,
        importance: row.get(15)?,
        last_accessed: parse_ts(&row.get::<String>(16)?)?,
        access_count: row.get::<i64>(17)? as u64,
        workspace: row.get(18)?,
        source_tool: row.get(19)?,
        mnemos_version: row.get::<i64>(20)? as u32,
    })
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| MnemosError::Validation(format!("bad timestamp '{s}': {e}")))
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_core --test memory_insert`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/storage/memory_ops.rs crates/mnemos_core/tests/memory_insert.rs
git commit -m "feat(core): insert/get memory with FTS5 sync and link extraction"
```

---

## Task 13: Memory update (bi-temporal supersede)

**Files:**
- Modify: `crates/mnemos_core/src/storage/memory_ops.rs`
- Test: `crates/mnemos_core/tests/bi_temporal.rs`

- [ ] **Step 1: Write failing test**

```rust
use chrono::Utc;
use mnemos_core::storage::memory_ops::{insert_memory, supersede_memory, get_memory};
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::{Storage, Tier, id::new_memory_id};
use tempfile::TempDir;

#[tokio::test]
async fn supersede_invalidates_old_and_links_new() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("bt.db")).await.unwrap();

    let old = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "User uses Vue".into(),
        "User said they prefer Vue.".into(),
    );
    insert_memory(&storage, &old, "/tmp/old.md", "h1").await.unwrap();

    let mut new = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "User uses React".into(),
        "User now prefers React.".into(),
    );
    new.valid_at = Utc::now();
    insert_memory(&storage, &new, "/tmp/new.md", "h2").await.unwrap();

    supersede_memory(&storage, &old.id, &new.id, new.valid_at).await.unwrap();

    let old_loaded = get_memory(&storage, &old.id).await.unwrap();
    assert!(old_loaded.invalid_at.is_some(), "old memory should be invalidated");
    assert_eq!(old_loaded.superseded_by.as_deref(), Some(new.id.as_str()));

    let conn = storage.conn().unwrap();
    let mut rows = conn.query(
        "SELECT COUNT(*) FROM memory_links WHERE source_id = ? AND target_id = ? AND kind = 'supersedes'",
        libsql::params![new.id.clone(), old.id.clone()],
    ).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let count: i64 = row.get(0).unwrap();
    assert_eq!(count, 1);
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test bi_temporal`
Expected: FAIL — `supersede_memory` not defined.

- [ ] **Step 3: Add `supersede_memory` and `soft_invalidate` to `memory_ops.rs`**

Append to `crates/mnemos_core/src/storage/memory_ops.rs`:

```rust
use chrono::{DateTime, Utc};

pub async fn supersede_memory(
    storage: &Storage,
    old_id: &str,
    new_id: &str,
    invalid_at: DateTime<Utc>,
) -> Result<()> {
    let (conn, _guard) = storage.write_conn().await?;
    let tx = conn.transaction().await?;

    let affected = tx.execute(
        "UPDATE memories
            SET invalid_at = ?, superseded_by = ?
          WHERE id = ? AND invalid_at IS NULL",
        params![invalid_at.to_rfc3339(), new_id.to_string(), old_id.to_string()],
    ).await?;
    if affected == 0 {
        return Err(MnemosError::MemoryNotFound(old_id.into()));
    }

    tx.execute(
        "INSERT OR IGNORE INTO memory_links (source_id, target_id, kind)
            VALUES (?, ?, 'supersedes')",
        params![new_id.to_string(), old_id.to_string()],
    ).await?;

    tx.commit().await?;
    Ok(())
}

pub async fn soft_invalidate(
    storage: &Storage,
    id: &str,
    at: DateTime<Utc>,
) -> Result<()> {
    let (conn, _guard) = storage.write_conn().await?;
    let affected = conn.execute(
        "UPDATE memories SET invalid_at = ? WHERE id = ? AND invalid_at IS NULL",
        params![at.to_rfc3339(), id.to_string()],
    ).await?;
    if affected == 0 {
        return Err(MnemosError::MemoryNotFound(id.into()));
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_core --test bi_temporal`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/storage/memory_ops.rs crates/mnemos_core/tests/bi_temporal.rs
git commit -m "feat(core): bi-temporal supersede + soft-invalidate operations"
```

---

## Task 14: List + filter memories

**Files:**
- Modify: `crates/mnemos_core/src/storage/memory_ops.rs`
- Test: `crates/mnemos_core/tests/memory_list.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::storage::memory_ops::{insert_memory, list_memories, ListFilter};
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::{Storage, Tier, id::new_memory_id};
use tempfile::TempDir;

#[tokio::test]
async fn list_filters_by_tier_and_invalidation() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("l.db")).await.unwrap();

    let a = Memory::new_now(new_memory_id(), Tier::Semantic, MemoryType::Fact, "A".into(), "body A".into());
    let b = Memory::new_now(new_memory_id(), Tier::Working,  MemoryType::Identity, "B".into(), "body B".into());
    let c = Memory::new_now(new_memory_id(), Tier::Semantic, MemoryType::Fact, "C".into(), "body C".into());
    insert_memory(&storage, &a, "/tmp/a.md", "h").await.unwrap();
    insert_memory(&storage, &b, "/tmp/b.md", "h").await.unwrap();
    insert_memory(&storage, &c, "/tmp/c.md", "h").await.unwrap();

    let all = list_memories(&storage, ListFilter::default()).await.unwrap();
    assert_eq!(all.len(), 3);

    let semantic = list_memories(&storage, ListFilter {
        tiers: Some(vec![Tier::Semantic]),
        ..Default::default()
    }).await.unwrap();
    assert_eq!(semantic.len(), 2);

    mnemos_core::storage::memory_ops::soft_invalidate(&storage, &a.id, chrono::Utc::now()).await.unwrap();
    let valid_only = list_memories(&storage, ListFilter { tiers: Some(vec![Tier::Semantic]), ..Default::default() }).await.unwrap();
    assert_eq!(valid_only.len(), 1, "soft-invalidated memory should be hidden by default");

    let incl_invalid = list_memories(&storage, ListFilter {
        tiers: Some(vec![Tier::Semantic]),
        include_invalid: true,
        ..Default::default()
    }).await.unwrap();
    assert_eq!(incl_invalid.len(), 2);
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test memory_list`
Expected: FAIL.

- [ ] **Step 3: Add `ListFilter` and `list_memories` to `memory_ops.rs`**

Append to `crates/mnemos_core/src/storage/memory_ops.rs`:

```rust
#[derive(Debug, Default, Clone)]
pub struct ListFilter {
    pub tiers: Option<Vec<Tier>>,
    pub workspace: Option<String>,
    pub include_invalid: bool,
    pub limit: Option<usize>,
}

pub async fn list_memories(storage: &Storage, filter: ListFilter) -> Result<Vec<Memory>> {
    let conn = storage.conn()?;
    let mut sql = String::from(
        "SELECT id, tier, kind, title, body,
                tags_json, entities_json, links_json, provenance_json,
                created_at, ingested_at, valid_at, invalid_at, superseded_by,
                strength, importance, last_accessed, access_count,
                workspace, source_tool, mnemos_version
         FROM memories WHERE 1=1",
    );
    let mut args: Vec<libsql::Value> = vec![];

    if !filter.include_invalid {
        sql.push_str(" AND invalid_at IS NULL");
    }
    if let Some(ws) = filter.workspace.as_ref() {
        sql.push_str(" AND (workspace IS NULL OR workspace = ?)");
        args.push(ws.clone().into());
    }
    if let Some(tiers) = filter.tiers.as_ref() {
        if !tiers.is_empty() {
            let placeholders = vec!["?"; tiers.len()].join(",");
            sql.push_str(&format!(" AND tier IN ({placeholders})"));
            for t in tiers {
                args.push(t.as_str().to_string().into());
            }
        }
    }
    sql.push_str(" ORDER BY created_at DESC");
    if let Some(limit) = filter.limit {
        sql.push_str(&format!(" LIMIT {limit}"));
    }

    let mut rows = conn.query(&sql, args).await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(row_to_memory(&row)?);
    }
    Ok(out)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_core --test memory_list`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/storage/memory_ops.rs crates/mnemos_core/tests/memory_list.rs
git commit -m "feat(core): list_memories with tier/workspace/invalid filters"
```

---

## Task 15: Audit log append-only enforcement

**Files:**
- Modify: `crates/mnemos_core/src/storage/audit.rs`
- Modify: `crates/mnemos_core/src/storage/migrations.rs` (add trigger to v1)
- Test: `crates/mnemos_core/tests/audit_log.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::storage::audit::{write_audit, list_audit};
use mnemos_core::Storage;
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn write_audit_entry_and_list_it() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("a.db")).await.unwrap();

    write_audit(&storage, "mnemos-cli", "create", Some("mem_X"),
        Some(json!({"title": "test"}))).await.unwrap();

    let entries = list_audit(&storage, Some("mem_X")).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].action, "create");
    assert_eq!(entries[0].actor, "mnemos-cli");
}

#[tokio::test]
async fn audit_log_rejects_update() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("a.db")).await.unwrap();

    write_audit(&storage, "cli", "create", Some("mem_X"), None).await.unwrap();

    let conn = storage.conn().unwrap();
    let result = conn.execute(
        "UPDATE audit_log SET action = 'tampered' WHERE memory_id = 'mem_X'",
        (),
    ).await;
    assert!(result.is_err(), "audit_log UPDATE should be blocked by trigger");
}

#[tokio::test]
async fn audit_log_rejects_delete() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("a.db")).await.unwrap();

    write_audit(&storage, "cli", "create", Some("mem_X"), None).await.unwrap();

    let conn = storage.conn().unwrap();
    let result = conn.execute(
        "DELETE FROM audit_log WHERE memory_id = 'mem_X'",
        (),
    ).await;
    assert!(result.is_err(), "audit_log DELETE should be blocked by trigger");
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test audit_log`
Expected: FAIL — `write_audit`/`list_audit` not defined.

- [ ] **Step 3: Add the audit triggers to migration v1**

Edit `crates/mnemos_core/src/storage/migrations.rs` — append to the end of the `V1_STATEMENTS` array (just before the closing `];`):

```rust
    // Append-only enforcement on audit_log
    "CREATE TRIGGER IF NOT EXISTS audit_log_no_update
        BEFORE UPDATE ON audit_log
        BEGIN
            SELECT RAISE(ABORT, 'audit_log is append-only: UPDATE not allowed');
        END",
    "CREATE TRIGGER IF NOT EXISTS audit_log_no_delete
        BEFORE DELETE ON audit_log
        BEGIN
            SELECT RAISE(ABORT, 'audit_log is append-only: DELETE not allowed');
        END",
```

- [ ] **Step 4: Implement `crates/mnemos_core/src/storage/audit.rs`**

Replace the stub:

```rust
use crate::error::Result;
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use libsql::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub ts: DateTime<Utc>,
    pub actor: String,
    pub action: String,
    pub memory_id: Option<String>,
    pub details: Option<Value>,
}

pub async fn write_audit(
    storage: &Storage,
    actor: &str,
    action: &str,
    memory_id: Option<&str>,
    details: Option<Value>,
) -> Result<()> {
    let (conn, _guard) = storage.write_conn().await?;
    let details_str = details.map(|v| v.to_string());
    conn.execute(
        "INSERT INTO audit_log (ts, actor, action, memory_id, details)
            VALUES (?, ?, ?, ?, ?)",
        params![
            Utc::now().to_rfc3339(),
            actor.to_string(),
            action.to_string(),
            memory_id.map(String::from),
            details_str,
        ],
    ).await?;
    Ok(())
}

pub async fn list_audit(storage: &Storage, memory_id: Option<&str>) -> Result<Vec<AuditEntry>> {
    let conn = storage.conn()?;
    let (sql, args): (&str, Vec<libsql::Value>) = match memory_id {
        Some(id) => (
            "SELECT id, ts, actor, action, memory_id, details
               FROM audit_log WHERE memory_id = ? ORDER BY id ASC",
            vec![id.to_string().into()],
        ),
        None => (
            "SELECT id, ts, actor, action, memory_id, details
               FROM audit_log ORDER BY id ASC",
            vec![],
        ),
    };
    let mut rows = conn.query(sql, args).await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        let ts_str: String = row.get(1)?;
        let details_str: Option<String> = row.get(5)?;
        out.push(AuditEntry {
            id: row.get(0)?,
            ts: DateTime::parse_from_rfc3339(&ts_str)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| crate::error::MnemosError::Validation(e.to_string()))?,
            actor: row.get(2)?,
            action: row.get(3)?,
            memory_id: row.get(4)?,
            details: details_str.map(|s| serde_json::from_str(&s)).transpose()?,
        });
    }
    Ok(out)
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p mnemos_core --test audit_log`
Expected: PASS (all three tests).

- [ ] **Step 6: Commit**

```bash
git add crates/mnemos_core/src/storage/audit.rs crates/mnemos_core/src/storage/migrations.rs crates/mnemos_core/tests/audit_log.rs
git commit -m "feat(core): append-only audit log via SQL triggers"
```

---

## Task 16: BM25 retrieval

**Files:**
- Modify: `crates/mnemos_core/src/retrieval/mod.rs`
- Modify: `crates/mnemos_core/src/retrieval/bm25.rs`
- Test: `crates/mnemos_core/tests/bm25_retrieval.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::retrieval::{bm25::bm25_recall, RecallOpts};
use mnemos_core::storage::memory_ops::insert_memory;
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::{Storage, Tier, id::new_memory_id};
use tempfile::TempDir;

async fn seed(storage: &Storage, title: &str, body: &str) -> String {
    let mem = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        title.into(),
        body.into(),
    );
    let path = format!("/tmp/{}.md", mem.id);
    insert_memory(storage, &mem, &path, "h").await.unwrap();
    mem.id
}

#[tokio::test]
async fn bm25_finds_distinct_terms() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("b.db")).await.unwrap();
    let id_a = seed(&storage, "Tauri preference", "User uses Tauri for desktop apps").await;
    let _id_b = seed(&storage, "React notes", "React is a JS framework").await;
    let _id_c = seed(&storage, "SQL trivia", "Postgres has window functions").await;

    let hits = bm25_recall(&storage, "tauri", RecallOpts::default()).await.unwrap();
    assert!(!hits.is_empty(), "expected at least one hit for 'tauri'");
    assert_eq!(hits[0].memory.id, id_a);
}

#[tokio::test]
async fn bm25_respects_tier_filter() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("b.db")).await.unwrap();
    let _ = seed(&storage, "Procedural rule", "always use TDD").await;
    let id_sem = seed(&storage, "Semantic fact", "user prefers TDD methodology").await;

    let opts = RecallOpts {
        tiers: Some(vec![Tier::Semantic]),
        ..Default::default()
    };
    let hits = bm25_recall(&storage, "tdd", opts).await.unwrap();
    assert!(hits.iter().all(|h| h.memory.tier == Tier::Semantic));
    assert!(hits.iter().any(|h| h.memory.id == id_sem));
}

#[tokio::test]
async fn bm25_hides_invalidated_by_default() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("b.db")).await.unwrap();
    let id = seed(&storage, "Old belief", "User likes Vue").await;
    mnemos_core::storage::memory_ops::soft_invalidate(&storage, &id, chrono::Utc::now()).await.unwrap();

    let hits = bm25_recall(&storage, "vue", RecallOpts::default()).await.unwrap();
    assert!(hits.iter().all(|h| h.memory.id != id), "invalidated memory should be hidden");

    let hits_all = bm25_recall(&storage, "vue", RecallOpts { include_invalid: true, ..Default::default() }).await.unwrap();
    assert!(hits_all.iter().any(|h| h.memory.id == id));
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test bm25_retrieval`
Expected: FAIL — module empty.

- [ ] **Step 3: Implement `crates/mnemos_core/src/retrieval/mod.rs`**

```rust
pub mod bm25;

use crate::tier::Tier;
use crate::types::Memory;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallOpts {
    pub k: usize,
    pub tiers: Option<Vec<Tier>>,
    pub workspace: Option<String>,
    pub include_invalid: bool,
}

impl Default for RecallOpts {
    fn default() -> Self {
        Self {
            k: 10,
            tiers: None,
            workspace: None,
            include_invalid: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RecallHit {
    pub memory: Memory,
    pub score: f64,
    pub bm25_rank: Option<usize>,
}
```

- [ ] **Step 4: Implement `crates/mnemos_core/src/retrieval/bm25.rs`**

```rust
use crate::error::Result;
use crate::retrieval::{RecallHit, RecallOpts};
use crate::storage::Storage;
use crate::storage::memory_ops::row_to_memory;
use libsql::Value;

/// FTS5 BM25 recall. Returns up to `opts.k` hits sorted by bm25 score (best first).
pub async fn bm25_recall(storage: &Storage, query: &str, opts: RecallOpts) -> Result<Vec<RecallHit>> {
    let conn = storage.conn()?;

    let fts_query = escape_fts5_query(query);
    let mut sql = String::from(
        "SELECT m.id, m.tier, m.kind, m.title, m.body,
                m.tags_json, m.entities_json, m.links_json, m.provenance_json,
                m.created_at, m.ingested_at, m.valid_at, m.invalid_at, m.superseded_by,
                m.strength, m.importance, m.last_accessed, m.access_count,
                m.workspace, m.source_tool, m.mnemos_version,
                bm25(memory_fts) AS s
           FROM memory_fts
           JOIN memories m ON m.id = memory_fts.memory_id
          WHERE memory_fts MATCH ?",
    );
    let mut args: Vec<Value> = vec![fts_query.into()];

    if !opts.include_invalid {
        sql.push_str(" AND m.invalid_at IS NULL");
    }
    if let Some(ws) = opts.workspace.as_ref() {
        sql.push_str(" AND (m.workspace IS NULL OR m.workspace = ?)");
        args.push(ws.clone().into());
    }
    if let Some(tiers) = opts.tiers.as_ref() {
        if !tiers.is_empty() {
            let placeholders = vec!["?"; tiers.len()].join(",");
            sql.push_str(&format!(" AND m.tier IN ({placeholders})"));
            for t in tiers {
                args.push(t.as_str().to_string().into());
            }
        }
    }
    // BM25 returns lower-is-better; sort ascending then invert.
    sql.push_str(" ORDER BY s ASC LIMIT ?");
    args.push((opts.k as i64).into());

    let mut rows = conn.query(&sql, args).await?;
    let mut hits = Vec::new();
    let mut rank = 0;
    while let Some(row) = rows.next().await? {
        rank += 1;
        let memory = row_to_memory(&row)?;
        let raw: f64 = row.get(21)?;
        hits.push(RecallHit {
            memory,
            score: -raw, // higher = better
            bm25_rank: Some(rank),
        });
    }
    Ok(hits)
}

/// Escape FTS5 special characters in a free-form user query. Quotes the
/// query as a phrase if it contains punctuation that FTS5 would reject.
fn escape_fts5_query(q: &str) -> String {
    let trimmed = q.trim();
    if trimmed.is_empty() {
        return "\"\"".into();
    }
    // FTS5 allows alphanumeric tokens, AND/OR/NOT, parens, quotes.
    // For simplicity in Plan 1: wrap the whole query in quotes (phrase mode)
    // and escape inner double-quotes by doubling.
    let escaped = trimmed.replace('"', "\"\"");
    format!("\"{escaped}\"")
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p mnemos_core --test bm25_retrieval`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/mnemos_core/src/retrieval/ crates/mnemos_core/tests/bm25_retrieval.rs
git commit -m "feat(core): BM25 recall via FTS5 with tier/workspace/invalid filters"
```

---

## Task 17: High-level `vault` ops (file + DB together)

**Files:**
- Modify: `crates/mnemos_core/src/lib.rs` (add `vault` module export)
- Create: `crates/mnemos_core/src/vault.rs`
- Test: `crates/mnemos_core/tests/vault_ops.rs`

This wraps "write the file AND the DB row together" so commands and the daemon don't have to coordinate two operations themselves.

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::vault::{Vault, RememberOpts};
use mnemos_core::{Tier, paths::Paths, types::MemoryType};
use tempfile::TempDir;

#[tokio::test]
async fn remember_writes_file_and_db_row() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();

    let id = vault.remember("body text", RememberOpts {
        title: Some("hello".into()),
        tier: Tier::Semantic,
        kind: MemoryType::Fact,
        tags: vec!["t1".into()],
        importance: Some(0.6),
        ..Default::default()
    }).await.unwrap();

    assert!(id.starts_with("mem_"));
    let file = tmp.path().join("files/semantic").join(format!("{id}.md"));
    assert!(file.exists(), "memory file should exist");

    let loaded = vault.get(&id).await.unwrap();
    assert_eq!(loaded.title, "hello");
    assert_eq!(loaded.body.trim(), "body text");
    assert_eq!(loaded.tags, vec!["t1"]);
}

#[tokio::test]
async fn forget_invalidates_memory_and_audits() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();

    let id = vault.remember("delete me", RememberOpts {
        title: Some("trash".into()),
        ..Default::default()
    }).await.unwrap();

    vault.forget(&id, Some("test reason")).await.unwrap();
    let mem = vault.get(&id).await.unwrap();
    assert!(mem.invalid_at.is_some());

    let entries = mnemos_core::storage::audit::list_audit(vault.storage(), Some(&id)).await.unwrap();
    assert!(entries.iter().any(|e| e.action == "create"));
    assert!(entries.iter().any(|e| e.action == "forget"));
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test vault_ops`
Expected: FAIL — `Vault` not defined.

- [ ] **Step 3: Add `pub mod vault;` to `crates/mnemos_core/src/lib.rs`**

Insert after the existing `pub mod` lines, before the `pub use ...` block:

```rust
pub mod vault;
```

And add this to the re-exports:

```rust
pub use vault::{Vault, RememberOpts};
```

- [ ] **Step 4: Implement `crates/mnemos_core/src/vault.rs`**

```rust
use crate::error::Result;
use crate::file_io::{content_hash, read_memory_file, write_memory_file};
use crate::frontmatter::parse_frontmatter;
use crate::id::new_memory_id;
use crate::paths::Paths;
use crate::storage::audit::write_audit;
use crate::storage::memory_ops::{
    get_memory, insert_memory, list_memories, soft_invalidate, ListFilter,
};
use crate::storage::Storage;
use crate::tier::Tier;
use crate::types::{Memory, MemoryType};
use chrono::Utc;
use serde_json::json;

#[derive(Clone)]
pub struct Vault {
    paths: Paths,
    storage: Storage,
}

#[derive(Debug, Clone, Default)]
pub struct RememberOpts {
    pub title: Option<String>,
    pub tier: Tier,
    pub kind: MemoryType,
    pub tags: Vec<String>,
    pub importance: Option<f64>,
    pub workspace: Option<String>,
    pub source_tool: Option<String>,
}

impl Default for Tier {
    fn default() -> Self { Tier::Semantic }
}

impl Default for MemoryType {
    fn default() -> Self { MemoryType::Fact }
}

impl Vault {
    /// Open a vault: ensure dirs, open DB, run migrations.
    pub async fn open(paths: Paths) -> Result<Self> {
        paths.ensure_dirs()?;
        let storage = Storage::open(&paths.db_path).await?;
        Ok(Self { paths, storage })
    }

    pub fn storage(&self) -> &Storage { &self.storage }
    pub fn paths(&self) -> &Paths { &self.paths }

    pub async fn remember(&self, body: &str, opts: RememberOpts) -> Result<String> {
        let id = new_memory_id();
        let now = Utc::now();
        let title = opts.title.unwrap_or_else(|| auto_title(body));
        let mem = Memory {
            id: id.clone(),
            tier: opts.tier,
            kind: opts.kind,
            title,
            body: body.to_string(),
            tags: opts.tags,
            entities: vec![],
            links: vec![],
            provenance: vec![],
            created_at: now, ingested_at: now, valid_at: now,
            invalid_at: None, superseded_by: None,
            strength: 1.0,
            importance: opts.importance.unwrap_or(0.5),
            last_accessed: now,
            access_count: 0,
            workspace: opts.workspace,
            source_tool: opts.source_tool,
            mnemos_version: 1,
        };
        let file_path = write_memory_file(&self.paths, &mem).await?;
        let hash = content_hash(body);
        insert_memory(
            &self.storage,
            &mem,
            file_path.to_string_lossy().as_ref(),
            &hash,
        ).await?;
        write_audit(
            &self.storage,
            opts_actor(&mem),
            "create",
            Some(&id),
            Some(json!({"tier": mem.tier.as_str(), "title": mem.title})),
        ).await?;
        Ok(id)
    }

    pub async fn get(&self, id: &str) -> Result<Memory> {
        get_memory(&self.storage, id).await
    }

    pub async fn forget(&self, id: &str, reason: Option<&str>) -> Result<()> {
        soft_invalidate(&self.storage, id, Utc::now()).await?;
        write_audit(
            &self.storage,
            "mnemos-cli",
            "forget",
            Some(id),
            Some(json!({"reason": reason})),
        ).await?;
        Ok(())
    }

    pub async fn list(&self, filter: ListFilter) -> Result<Vec<Memory>> {
        list_memories(&self.storage, filter).await
    }

    /// Reload a memory's body from disk, bypassing the DB cache. Useful when
    /// the user has externally edited the file and we want the truth.
    pub async fn read_from_disk(&self, path: &std::path::Path) -> Result<(Memory, String)> {
        read_memory_file(path).await
    }
}

fn auto_title(body: &str) -> String {
    let line = body.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        "Untitled memory".into()
    } else if line.len() <= 80 {
        line.into()
    } else {
        format!("{}…", &line[..77])
    }
}

fn opts_actor(_mem: &Memory) -> &'static str {
    // Plan 3 will set this from the calling MCP client; for now we tag CLI.
    "mnemos-cli"
}

/// Parse a markdown file via the vault (re-export convenience).
pub fn parse_file(text: &str) -> Result<(Memory, String)> {
    parse_frontmatter(text)
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p mnemos_core --test vault_ops`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/mnemos_core/src/vault.rs crates/mnemos_core/src/lib.rs crates/mnemos_core/tests/vault_ops.rs
git commit -m "feat(core): Vault facade for remember/get/forget/list with audit"
```

---

## Task 18: Rebuild from files

**Files:**
- Modify: `crates/mnemos_core/src/rebuild.rs`
- Test: `crates/mnemos_core/tests/rebuild.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::rebuild::rebuild_index;
use mnemos_core::vault::{Vault, RememberOpts};
use mnemos_core::{Tier, paths::Paths};
use tempfile::TempDir;

#[tokio::test]
async fn rebuild_recreates_index_from_files() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());

    // Create three memories
    let ids = {
        let vault = Vault::open(paths.clone()).await.unwrap();
        let mut ids = vec![];
        for i in 0..3 {
            let id = vault.remember(&format!("body {i}"), RememberOpts {
                title: Some(format!("Title {i}")),
                tier: Tier::Semantic,
                ..Default::default()
            }).await.unwrap();
            ids.push(id);
        }
        ids
    };

    // Wipe the DB; files remain
    tokio::fs::remove_file(&paths.db_path).await.unwrap();

    // Rebuild
    let stats = rebuild_index(&paths).await.unwrap();
    assert_eq!(stats.memories_indexed, 3);
    assert_eq!(stats.errors, 0);

    // Verify
    let vault = Vault::open(paths.clone()).await.unwrap();
    for id in &ids {
        let mem = vault.get(id).await.unwrap();
        assert!(mem.title.starts_with("Title "));
    }
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test rebuild`
Expected: FAIL.

- [ ] **Step 3: Implement `crates/mnemos_core/src/rebuild.rs`**

```rust
use crate::error::Result;
use crate::file_io::{content_hash, read_memory_file};
use crate::paths::Paths;
use crate::storage::memory_ops::insert_memory;
use crate::storage::Storage;
use crate::tier::Tier;
use serde::Serialize;
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Debug, Default, Serialize)]
pub struct RebuildStats {
    pub memories_indexed: usize,
    pub errors: usize,
    pub error_paths: Vec<PathBuf>,
}

pub async fn rebuild_index(paths: &Paths) -> Result<RebuildStats> {
    paths.ensure_dirs()?;
    // Open Storage which will create a fresh DB + run migrations
    let storage = Storage::open(&paths.db_path).await?;
    let mut stats = RebuildStats::default();

    for tier in Tier::all() {
        let dir = paths.tier_dir(*tier);
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            match index_single_file(&storage, &path).await {
                Ok(()) => stats.memories_indexed += 1,
                Err(e) => {
                    warn!("failed to index {}: {}", path.display(), e);
                    stats.errors += 1;
                    stats.error_paths.push(path);
                }
            }
        }
    }
    info!(
        "rebuild complete: {} indexed, {} errors",
        stats.memories_indexed, stats.errors
    );
    Ok(stats)
}

async fn index_single_file(storage: &Storage, path: &std::path::Path) -> Result<()> {
    let (mem, body) = read_memory_file(path).await?;
    let hash = content_hash(&body);
    insert_memory(storage, &mem, path.to_string_lossy().as_ref(), &hash).await?;
    Ok(())
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_core --test rebuild`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/rebuild.rs crates/mnemos_core/tests/rebuild.rs
git commit -m "feat(core): rebuild index from files dir, idempotent"
```

---

## Task 19: Doctor — detect file/DB drift

**Files:**
- Modify: `crates/mnemos_core/src/doctor.rs`
- Test: `crates/mnemos_core/tests/doctor.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::doctor::{diagnose, DriftKind};
use mnemos_core::vault::{Vault, RememberOpts};
use mnemos_core::{Tier, paths::Paths};
use tempfile::TempDir;

#[tokio::test]
async fn doctor_clean_vault_returns_no_issues() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths.clone()).await.unwrap();
    let _ = vault.remember("clean body", RememberOpts { title: Some("ok".into()), ..Default::default() }).await.unwrap();

    let report = diagnose(&paths).await.unwrap();
    assert!(report.issues.is_empty(), "expected no issues, got {:?}", report.issues);
}

#[tokio::test]
async fn doctor_detects_orphaned_file() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let _vault = Vault::open(paths.clone()).await.unwrap();

    // Write a stray file directly that the DB doesn't know about
    let stray = paths.tier_dir(Tier::Semantic).join("mem_01HXSTRAY.md");
    tokio::fs::write(&stray, "---\nid: mem_01HXSTRAY\ntier: semantic\ntype: fact\ntitle: orphan\ncreated_at: 2026-05-22T14:30:00Z\ningested_at: 2026-05-22T14:30:00Z\nvalid_at: 2026-05-22T14:30:00Z\nstrength: 1.0\nimportance: 0.5\nlast_accessed: 2026-05-22T14:30:00Z\naccess_count: 0\n---\n\nbody\n").await.unwrap();

    let report = diagnose(&paths).await.unwrap();
    assert!(report.issues.iter().any(|i| matches!(i.kind, DriftKind::FileNotInDb)));
}

#[tokio::test]
async fn doctor_detects_db_row_missing_file() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths.clone()).await.unwrap();
    let id = vault.remember("body", RememberOpts { title: Some("ok".into()), ..Default::default() }).await.unwrap();
    let path = paths.tier_dir(Tier::Semantic).join(format!("{id}.md"));
    tokio::fs::remove_file(&path).await.unwrap();

    let report = diagnose(&paths).await.unwrap();
    assert!(report.issues.iter().any(|i| matches!(i.kind, DriftKind::DbRowNoFile)));
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test doctor`
Expected: FAIL.

- [ ] **Step 3: Implement `crates/mnemos_core/src/doctor.rs`**

```rust
use crate::error::Result;
use crate::paths::Paths;
use crate::storage::Storage;
use crate::tier::Tier;
use libsql::params;
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub enum DriftKind {
    FileNotInDb,    // .md exists, no DB row
    DbRowNoFile,    // DB row exists, .md missing
    HashMismatch,   // both exist but content_hash != hash(file body)
    QuarantineFile, // file is parked in quarantine
}

#[derive(Debug, Serialize)]
pub struct DriftIssue {
    pub kind: DriftKind,
    pub path: Option<PathBuf>,
    pub memory_id: Option<String>,
    pub detail: String,
}

#[derive(Debug, Serialize, Default)]
pub struct DoctorReport {
    pub files_scanned: usize,
    pub db_rows: usize,
    pub issues: Vec<DriftIssue>,
}

pub async fn diagnose(paths: &Paths) -> Result<DoctorReport> {
    paths.ensure_dirs()?;
    let storage = Storage::open(&paths.db_path).await?;
    let mut report = DoctorReport::default();

    // Gather DB ids -> file_path
    let mut db_files: std::collections::HashMap<String, String> = Default::default();
    {
        let conn = storage.conn()?;
        let mut rows = conn.query("SELECT id, file_path FROM memories", ()).await?;
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            let p: String = row.get(1)?;
            db_files.insert(id, p);
        }
    }
    report.db_rows = db_files.len();
    let db_paths: HashSet<String> = db_files.values().cloned().collect();

    // Walk file tiers
    let mut seen_paths: HashSet<String> = HashSet::new();
    for tier in Tier::all() {
        let dir = paths.tier_dir(*tier);
        if !dir.exists() { continue; }
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") { continue; }
            report.files_scanned += 1;
            let p_str = path.to_string_lossy().to_string();
            seen_paths.insert(p_str.clone());
            if !db_paths.contains(&p_str) {
                report.issues.push(DriftIssue {
                    kind: DriftKind::FileNotInDb,
                    path: Some(path),
                    memory_id: None,
                    detail: "file present but not indexed".into(),
                });
            }
        }
    }

    // Quarantine files
    if paths.quarantine_dir.exists() {
        let mut entries = tokio::fs::read_dir(&paths.quarantine_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            report.issues.push(DriftIssue {
                kind: DriftKind::QuarantineFile,
                path: Some(entry.path()),
                memory_id: None,
                detail: "quarantined file awaiting review".into(),
            });
        }
    }

    // DB rows pointing at missing files
    for (id, p_str) in &db_files {
        if !seen_paths.contains(p_str) {
            report.issues.push(DriftIssue {
                kind: DriftKind::DbRowNoFile,
                path: Some(PathBuf::from(p_str)),
                memory_id: Some(id.clone()),
                detail: "DB row references missing file".into(),
            });
        }
    }

    // Hash mismatches
    let conn = storage.conn()?;
    let mut rows = conn.query(
        "SELECT id, file_path, content_hash FROM memories",
        (),
    ).await?;
    while let Some(row) = rows.next().await? {
        let id: String = row.get(0)?;
        let p: String = row.get(1)?;
        let stored_hash: String = row.get(2)?;
        if let Ok(body) = tokio::fs::read_to_string(&p).await {
            let live = crate::file_io::content_hash(&body);
            if live != stored_hash {
                report.issues.push(DriftIssue {
                    kind: DriftKind::HashMismatch,
                    path: Some(PathBuf::from(p)),
                    memory_id: Some(id),
                    detail: "file content differs from indexed hash".into(),
                });
            }
        }
    }
    drop(rows);
    let _ = params!(); // keep import
    Ok(report)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_core --test doctor`
Expected: PASS (all three tests).

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/doctor.rs crates/mnemos_core/tests/doctor.rs
git commit -m "feat(core): doctor detects file/DB drift + orphans + hash mismatches"
```

---

## Task 20: File watcher reindexes on external edit

**Files:**
- Modify: `crates/mnemos_core/src/watcher.rs`
- Test: `crates/mnemos_core/tests/watcher.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::watcher::{watch_vault, WatchEvent};
use mnemos_core::vault::{Vault, RememberOpts};
use mnemos_core::{Tier, paths::Paths};
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_edit_emits_changed_event() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths.clone()).await.unwrap();

    let id = vault.remember("original", RememberOpts { title: Some("t".into()), ..Default::default() }).await.unwrap();
    let file = paths.tier_dir(Tier::Semantic).join(format!("{id}.md"));

    let (tx, mut rx) = mpsc::channel(16);
    let _handle = watch_vault(&paths, tx).await.unwrap();

    // Edit the file externally
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut content = tokio::fs::read_to_string(&file).await.unwrap();
    content.push_str("\nappended.\n");
    tokio::fs::write(&file, content).await.unwrap();

    let event = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await
        .expect("watcher should emit event within 3s")
        .expect("channel should not close");
    match event {
        WatchEvent::Changed(p) => assert_eq!(p, file),
        other => panic!("expected Changed, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_core --test watcher`
Expected: FAIL.

- [ ] **Step 3: Implement `crates/mnemos_core/src/watcher.rs`**

```rust
use crate::error::{MnemosError, Result};
use crate::paths::Paths;
use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, FileIdMap};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tracing::warn;

#[derive(Debug, Clone)]
pub enum WatchEvent {
    Changed(PathBuf),
    Removed(PathBuf),
    Created(PathBuf),
}

/// Start watching the vault's files dir. The returned `Debouncer` must be
/// held alive — dropping it stops the watcher.
pub async fn watch_vault(
    paths: &Paths,
    tx: Sender<WatchEvent>,
) -> Result<Debouncer<notify::RecommendedWatcher, FileIdMap>> {
    let files_dir = paths.files_dir.clone();
    let mut debouncer = new_debouncer(
        Duration::from_millis(150),
        None,
        move |res: DebounceEventResult| {
            let events = match res {
                Ok(e) => e,
                Err(errs) => {
                    for e in errs { warn!("watcher error: {e}"); }
                    return;
                }
            };
            for de in events {
                for path in de.event.paths {
                    if path.extension().and_then(|s| s.to_str()) != Some("md") { continue; }
                    use notify::EventKind::*;
                    let we = match de.event.kind {
                        Create(_) => WatchEvent::Created(path),
                        Modify(_) => WatchEvent::Changed(path),
                        Remove(_) => WatchEvent::Removed(path),
                        _ => continue,
                    };
                    // Try blocking send; if channel closed, drop event.
                    let _ = tx.try_send(we);
                }
            }
        },
    ).map_err(|e| MnemosError::Internal(format!("debouncer init: {e}")))?;

    debouncer.watcher()
        .watch(&files_dir, RecursiveMode::Recursive)
        .map_err(|e| MnemosError::Internal(format!("watch start: {e}")))?;
    Ok(debouncer)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_core --test watcher`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_core/src/watcher.rs crates/mnemos_core/tests/watcher.rs
git commit -m "feat(core): debounced file watcher emits typed change events"
```

---

## Task 21: Scaffold `mnemos_cli` binary

**Files:**
- Create: `crates/mnemos_cli/Cargo.toml`
- Create: `crates/mnemos_cli/src/main.rs`
- Create: `crates/mnemos_cli/src/cli.rs`
- Create: `crates/mnemos_cli/src/commands/mod.rs`

- [ ] **Step 1: Write `crates/mnemos_cli/Cargo.toml`**

```toml
[package]
name = "mnemos_cli"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
rust-version.workspace = true

[[bin]]
name = "mnemos"
path = "src/main.rs"

[dependencies]
mnemos_core = { path = "../mnemos_core" }
tokio = { workspace = true }
clap = { workspace = true }
anyhow = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
directories = { workspace = true }
chrono = { workspace = true }

[dev-dependencies]
assert_cmd = { workspace = true }
predicates = { workspace = true }
tempfile = { workspace = true }
```

- [ ] **Step 2: Write `crates/mnemos_cli/src/cli.rs`**

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "mnemos", version, about = "Local-first AI memory provider")]
pub struct Cli {
    /// Override vault root. Defaults to ~/.local/share/mnemos/
    #[arg(long, global = true, env = "MNEMOS_VAULT")]
    pub vault: Option<PathBuf>,

    /// Emit machine-readable JSON output where supported.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Store something new.
    Remember(RememberArgs),
    /// Search memories (BM25 only in Plan 1).
    Recall(RecallArgs),
    /// Print a single memory by ID.
    Get { id: String },
    /// List memories with filters.
    List(ListArgs),
    /// Soft-invalidate a memory.
    Forget { id: String, #[arg(long)] reason: Option<String> },
    /// Rebuild the DB index from files on disk.
    Rebuild,
    /// Diagnose file/DB drift and quarantine entries.
    Doctor,
    /// Quick vault health summary.
    Status,
}

#[derive(clap::Args, Debug)]
pub struct RememberArgs {
    /// Body text. If absent, read from stdin.
    pub body: Option<String>,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long, default_value = "semantic")]
    pub tier: String,
    #[arg(long, value_delimiter = ',')]
    pub tags: Vec<String>,
    #[arg(long)]
    pub importance: Option<f64>,
    #[arg(long)]
    pub workspace: Option<String>,
    #[arg(long)]
    pub source_tool: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct RecallArgs {
    pub query: String,
    #[arg(short, long, default_value_t = 10)]
    pub k: usize,
    #[arg(long, value_delimiter = ',')]
    pub tier: Vec<String>,
    #[arg(long)]
    pub workspace: Option<String>,
    #[arg(long)]
    pub include_invalid: bool,
}

#[derive(clap::Args, Debug)]
pub struct ListArgs {
    #[arg(long, value_delimiter = ',')]
    pub tier: Vec<String>,
    #[arg(long)]
    pub workspace: Option<String>,
    #[arg(long)]
    pub include_invalid: bool,
    #[arg(short, long, default_value_t = 50)]
    pub limit: usize,
}
```

- [ ] **Step 3: Write `crates/mnemos_cli/src/commands/mod.rs`**

```rust
pub mod remember;
pub mod recall;
pub mod get;
pub mod list;
pub mod forget;
pub mod rebuild;
pub mod doctor;
pub mod status;

use anyhow::Result;
use mnemos_core::{paths::Paths, vault::Vault};
use std::path::PathBuf;

pub async fn open_vault(vault_override: Option<PathBuf>) -> Result<Vault> {
    let paths = match vault_override {
        Some(p) => Paths::with_root(&p),
        None => Paths::default_xdg()?,
    };
    Ok(Vault::open(paths).await?)
}
```

Create empty stubs for each subcommand:

```bash
for f in remember recall get list forget rebuild doctor status; do
    echo "// Populated in Task $((22))..." > crates/mnemos_cli/src/commands/$f.rs
done
```

- [ ] **Step 4: Write `crates/mnemos_cli/src/main.rs`**

```rust
mod cli;
mod commands;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Cmd};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let args = Cli::parse();
    match args.command {
        Cmd::Remember(a) => commands::remember::run(args.vault, args.json, a).await,
        Cmd::Recall(a)   => commands::recall::run(args.vault, args.json, a).await,
        Cmd::Get { id }  => commands::get::run(args.vault, args.json, id).await,
        Cmd::List(a)     => commands::list::run(args.vault, args.json, a).await,
        Cmd::Forget { id, reason } => commands::forget::run(args.vault, args.json, id, reason).await,
        Cmd::Rebuild     => commands::rebuild::run(args.vault, args.json).await,
        Cmd::Doctor      => commands::doctor::run(args.vault, args.json).await,
        Cmd::Status      => commands::status::run(args.vault, args.json).await,
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_env("MNEMOS_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
```

- [ ] **Step 5: Verify it compiles (will fail because subcommand stubs are not real functions yet)**

Add minimal stubs so the binary at least compiles. Replace each `commands/<name>.rs` stub with:

```rust
// Each file gets this skeleton:
use anyhow::Result;
use std::path::PathBuf;
use crate::cli::{RememberArgs, RecallArgs, ListArgs};

pub async fn run(_vault: Option<PathBuf>, _json: bool /* and per-cmd args */) -> Result<()> {
    anyhow::bail!("not yet implemented")
}
```

That signature won't match for every command. Use these minimal but signature-correct stubs:

`crates/mnemos_cli/src/commands/remember.rs`:
```rust
use anyhow::Result;
use std::path::PathBuf;
use crate::cli::RememberArgs;
pub async fn run(_vault: Option<PathBuf>, _json: bool, _args: RememberArgs) -> Result<()> {
    anyhow::bail!("remember: not yet implemented")
}
```

`crates/mnemos_cli/src/commands/recall.rs`:
```rust
use anyhow::Result;
use std::path::PathBuf;
use crate::cli::RecallArgs;
pub async fn run(_vault: Option<PathBuf>, _json: bool, _args: RecallArgs) -> Result<()> {
    anyhow::bail!("recall: not yet implemented")
}
```

`crates/mnemos_cli/src/commands/get.rs`:
```rust
use anyhow::Result;
use std::path::PathBuf;
pub async fn run(_vault: Option<PathBuf>, _json: bool, _id: String) -> Result<()> {
    anyhow::bail!("get: not yet implemented")
}
```

`crates/mnemos_cli/src/commands/list.rs`:
```rust
use anyhow::Result;
use std::path::PathBuf;
use crate::cli::ListArgs;
pub async fn run(_vault: Option<PathBuf>, _json: bool, _args: ListArgs) -> Result<()> {
    anyhow::bail!("list: not yet implemented")
}
```

`crates/mnemos_cli/src/commands/forget.rs`:
```rust
use anyhow::Result;
use std::path::PathBuf;
pub async fn run(_vault: Option<PathBuf>, _json: bool, _id: String, _reason: Option<String>) -> Result<()> {
    anyhow::bail!("forget: not yet implemented")
}
```

`crates/mnemos_cli/src/commands/rebuild.rs`:
```rust
use anyhow::Result;
use std::path::PathBuf;
pub async fn run(_vault: Option<PathBuf>, _json: bool) -> Result<()> {
    anyhow::bail!("rebuild: not yet implemented")
}
```

`crates/mnemos_cli/src/commands/doctor.rs`:
```rust
use anyhow::Result;
use std::path::PathBuf;
pub async fn run(_vault: Option<PathBuf>, _json: bool) -> Result<()> {
    anyhow::bail!("doctor: not yet implemented")
}
```

`crates/mnemos_cli/src/commands/status.rs`:
```rust
use anyhow::Result;
use std::path::PathBuf;
pub async fn run(_vault: Option<PathBuf>, _json: bool) -> Result<()> {
    anyhow::bail!("status: not yet implemented")
}
```

- [ ] **Step 6: Verify the binary builds**

Run: `cargo build -p mnemos_cli`
Expected: PASS. Lots of "unused" warnings, fine.

Run: `cargo run -p mnemos_cli -- --help`
Expected: clap prints usage.

- [ ] **Step 7: Commit**

```bash
git add crates/mnemos_cli/
git commit -m "feat(cli): scaffold mnemos binary with clap subcommand surface"
```

---

## Task 22: `mnemos remember` command

**Files:**
- Modify: `crates/mnemos_cli/src/commands/remember.rs`
- Test: `crates/mnemos_cli/tests/cli_remember.rs`

- [ ] **Step 1: Write failing test**

```rust
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn remember_writes_file_and_prints_id() {
    let tmp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("mnemos").unwrap();
    cmd.env("MNEMOS_VAULT", tmp.path())
       .args(["remember", "the body", "--title", "my title", "--tier", "semantic"])
       .assert()
       .success()
       .stdout(predicate::str::contains("mem_"));

    // verify file exists
    let semantic_dir = tmp.path().join("files/semantic");
    let count = std::fs::read_dir(&semantic_dir).unwrap().count();
    assert_eq!(count, 1);
}

#[test]
fn remember_emits_json_when_flagged() {
    let tmp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("mnemos").unwrap();
    let out = cmd.env("MNEMOS_VAULT", tmp.path())
       .args(["--json", "remember", "body", "--title", "j"])
       .assert()
       .success()
       .get_output().stdout.clone();
    let s = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
    assert!(v["id"].as_str().unwrap().starts_with("mem_"));
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_cli --test cli_remember`
Expected: FAIL — command bails with "not yet implemented".

- [ ] **Step 3: Implement `crates/mnemos_cli/src/commands/remember.rs`**

```rust
use crate::cli::RememberArgs;
use crate::commands::open_vault;
use anyhow::{Context, Result};
use mnemos_core::types::MemoryType;
use mnemos_core::vault::RememberOpts;
use mnemos_core::Tier;
use std::io::Read;
use std::path::PathBuf;
use std::str::FromStr;

pub async fn run(vault: Option<PathBuf>, json: bool, args: RememberArgs) -> Result<()> {
    let body = match args.body {
        Some(b) if !b.is_empty() => b,
        _ => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).context("read stdin")?;
            buf.trim().to_string()
        }
    };
    if body.is_empty() {
        anyhow::bail!("empty body — pass text as argument or via stdin");
    }
    let tier = Tier::from_str(&args.tier).context("invalid --tier")?;
    let vault = open_vault(vault).await?;
    let id = vault.remember(&body, RememberOpts {
        title: args.title,
        tier,
        kind: MemoryType::Fact,
        tags: args.tags,
        importance: args.importance,
        workspace: args.workspace,
        source_tool: args.source_tool,
    }).await?;
    if json {
        println!("{}", serde_json::json!({"id": id}));
    } else {
        println!("{id}");
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_cli --test cli_remember`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_cli/src/commands/remember.rs crates/mnemos_cli/tests/cli_remember.rs
git commit -m "feat(cli): mnemos remember command writes file + DB row"
```

---

## Task 23: `mnemos recall` command

**Files:**
- Modify: `crates/mnemos_cli/src/commands/recall.rs`
- Test: `crates/mnemos_cli/tests/cli_recall.rs`

- [ ] **Step 1: Write failing test**

```rust
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn recall_returns_matching_memory_by_keyword() {
    let tmp = TempDir::new().unwrap();
    let bin = || {
        let mut c = Command::cargo_bin("mnemos").unwrap();
        c.env("MNEMOS_VAULT", tmp.path());
        c
    };
    bin().args(["remember", "User uses Tauri for the desktop UI", "--title", "Tauri choice"]).assert().success();
    bin().args(["remember", "React is a JS UI framework",            "--title", "React notes"]).assert().success();

    bin().args(["recall", "tauri"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tauri"));
}

#[test]
fn recall_json_includes_score_and_id() {
    let tmp = TempDir::new().unwrap();
    let mut c = Command::cargo_bin("mnemos").unwrap();
    c.env("MNEMOS_VAULT", tmp.path());
    c.args(["remember", "platypus body", "--title", "Platypus fact"]).assert().success();

    let mut c = Command::cargo_bin("mnemos").unwrap();
    let out = c.env("MNEMOS_VAULT", tmp.path())
        .args(["--json", "recall", "platypus"])
        .assert()
        .success()
        .get_output().stdout.clone();
    let s = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
    let arr = v["hits"].as_array().unwrap();
    assert!(!arr.is_empty(), "no hits");
    assert!(arr[0]["score"].is_number());
    assert!(arr[0]["memory"]["id"].as_str().unwrap().starts_with("mem_"));
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_cli --test cli_recall`
Expected: FAIL.

- [ ] **Step 3: Implement `crates/mnemos_cli/src/commands/recall.rs`**

```rust
use crate::cli::RecallArgs;
use crate::commands::open_vault;
use anyhow::{Context, Result};
use mnemos_core::retrieval::{bm25::bm25_recall, RecallOpts};
use mnemos_core::Tier;
use std::path::PathBuf;
use std::str::FromStr;

pub async fn run(vault: Option<PathBuf>, json: bool, args: RecallArgs) -> Result<()> {
    let tiers = if args.tier.is_empty() {
        None
    } else {
        let mut v = Vec::with_capacity(args.tier.len());
        for t in &args.tier { v.push(Tier::from_str(t).context("invalid tier")?); }
        Some(v)
    };
    let vault = open_vault(vault).await?;
    let opts = RecallOpts {
        k: args.k,
        tiers,
        workspace: args.workspace,
        include_invalid: args.include_invalid,
    };
    let hits = bm25_recall(vault.storage(), &args.query, opts).await?;
    if json {
        println!("{}", serde_json::json!({"hits": hits}));
    } else {
        if hits.is_empty() {
            println!("no matches");
            return Ok(());
        }
        for (i, hit) in hits.iter().enumerate() {
            println!(
                "{:>2}. [{:.3}] {}  ({})",
                i + 1, hit.score, hit.memory.title, hit.memory.id
            );
            let snippet: String = hit.memory.body.chars().take(140).collect();
            println!("    {snippet}{}", if hit.memory.body.chars().count() > 140 { "…" } else { "" });
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mnemos_cli --test cli_recall`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mnemos_cli/src/commands/recall.rs crates/mnemos_cli/tests/cli_recall.rs
git commit -m "feat(cli): mnemos recall command with BM25 results + JSON output"
```

---

## Task 24: `get`, `list`, `forget` commands

**Files:**
- Modify: `crates/mnemos_cli/src/commands/get.rs`
- Modify: `crates/mnemos_cli/src/commands/list.rs`
- Modify: `crates/mnemos_cli/src/commands/forget.rs`
- Test: `crates/mnemos_cli/tests/cli_get_list_forget.rs`

- [ ] **Step 1: Write failing test**

```rust
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn cmd(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("mnemos").unwrap();
    c.env("MNEMOS_VAULT", tmp.path());
    c
}

fn seed(tmp: &TempDir, title: &str, body: &str) -> String {
    let out = cmd(tmp).args(["remember", body, "--title", title]).assert().success().get_output().stdout.clone();
    String::from_utf8(out).unwrap().trim().to_string()
}

#[test]
fn get_prints_memory_contents() {
    let tmp = TempDir::new().unwrap();
    let id = seed(&tmp, "Greeting", "hello world");
    cmd(&tmp).args(["get", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Greeting"))
        .stdout(predicate::str::contains("hello world"));
}

#[test]
fn list_filters_by_tier() {
    let tmp = TempDir::new().unwrap();
    let _ = seed(&tmp, "A", "a");
    cmd(&tmp).args(["remember", "rule body", "--title", "Rule", "--tier", "procedural"]).assert().success();

    let out = cmd(&tmp).args(["--json", "list", "--tier", "procedural"]).assert().success().get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let memories = v["memories"].as_array().unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0]["title"], "Rule");
}

#[test]
fn forget_then_list_omits_invalidated() {
    let tmp = TempDir::new().unwrap();
    let id = seed(&tmp, "Doomed", "to be forgotten");
    cmd(&tmp).args(["forget", &id, "--reason", "test"]).assert().success();

    let out = cmd(&tmp).args(["--json", "list"]).assert().success().get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["memories"].as_array().unwrap().len(), 0);

    let out = cmd(&tmp).args(["--json", "list", "--include-invalid"]).assert().success().get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["memories"].as_array().unwrap().len(), 1);
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_cli --test cli_get_list_forget`
Expected: FAIL.

- [ ] **Step 3: Implement `crates/mnemos_cli/src/commands/get.rs`**

```rust
use crate::commands::open_vault;
use anyhow::Result;
use std::path::PathBuf;

pub async fn run(vault: Option<PathBuf>, json: bool, id: String) -> Result<()> {
    let vault = open_vault(vault).await?;
    let mem = vault.get(&id).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&mem)?);
    } else {
        println!("ID:         {}", mem.id);
        println!("Tier:       {}", mem.tier);
        println!("Type:       {:?}", mem.kind);
        println!("Title:      {}", mem.title);
        println!("Tags:       {}", mem.tags.join(", "));
        println!("Created:    {}", mem.created_at);
        println!("Valid at:   {}", mem.valid_at);
        if let Some(inv) = mem.invalid_at {
            println!("Invalid at: {inv}");
        }
        println!("Strength:   {:.3}", mem.strength);
        println!("Importance: {:.3}", mem.importance);
        println!("---");
        println!("{}", mem.body);
    }
    Ok(())
}
```

- [ ] **Step 4: Implement `crates/mnemos_cli/src/commands/list.rs`**

```rust
use crate::cli::ListArgs;
use crate::commands::open_vault;
use anyhow::{Context, Result};
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::Tier;
use std::path::PathBuf;
use std::str::FromStr;

pub async fn run(vault: Option<PathBuf>, json: bool, args: ListArgs) -> Result<()> {
    let tiers = if args.tier.is_empty() {
        None
    } else {
        let mut v = Vec::with_capacity(args.tier.len());
        for t in &args.tier { v.push(Tier::from_str(t).context("invalid tier")?); }
        Some(v)
    };
    let vault = open_vault(vault).await?;
    let memories = vault.list(ListFilter {
        tiers,
        workspace: args.workspace,
        include_invalid: args.include_invalid,
        limit: Some(args.limit),
    }).await?;
    if json {
        println!("{}", serde_json::json!({"memories": memories}));
    } else {
        if memories.is_empty() {
            println!("no memories");
            return Ok(());
        }
        for m in memories {
            let inv = if m.invalid_at.is_some() { " [invalidated]" } else { "" };
            println!("{}  {:10}  {}{}", m.id, m.tier, m.title, inv);
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Implement `crates/mnemos_cli/src/commands/forget.rs`**

```rust
use crate::commands::open_vault;
use anyhow::Result;
use std::path::PathBuf;

pub async fn run(vault: Option<PathBuf>, json: bool, id: String, reason: Option<String>) -> Result<()> {
    let vault = open_vault(vault).await?;
    vault.forget(&id, reason.as_deref()).await?;
    if json {
        println!("{}", serde_json::json!({"id": id, "status": "invalidated"}));
    } else {
        println!("invalidated {id}");
    }
    Ok(())
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p mnemos_cli --test cli_get_list_forget`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/mnemos_cli/src/commands/get.rs \
        crates/mnemos_cli/src/commands/list.rs \
        crates/mnemos_cli/src/commands/forget.rs \
        crates/mnemos_cli/tests/cli_get_list_forget.rs
git commit -m "feat(cli): get/list/forget commands with bi-temporal aware listing"
```

---

## Task 25: `rebuild`, `doctor`, `status` commands

**Files:**
- Modify: `crates/mnemos_cli/src/commands/rebuild.rs`
- Modify: `crates/mnemos_cli/src/commands/doctor.rs`
- Modify: `crates/mnemos_cli/src/commands/status.rs`
- Test: `crates/mnemos_cli/tests/cli_admin.rs`

- [ ] **Step 1: Write failing test**

```rust
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn cmd(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("mnemos").unwrap();
    c.env("MNEMOS_VAULT", tmp.path());
    c
}

#[test]
fn rebuild_reports_indexed_count() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp).args(["remember", "body", "--title", "t"]).assert().success();
    cmd(&tmp).args(["remember", "body2","--title", "t2"]).assert().success();
    std::fs::remove_file(tmp.path().join("index.db")).unwrap();

    cmd(&tmp).args(["rebuild"])
        .assert()
        .success()
        .stdout(predicate::str::contains("indexed: 2"));
}

#[test]
fn doctor_reports_clean_vault() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp).args(["remember", "x", "--title", "y"]).assert().success();
    cmd(&tmp).args(["doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no issues"));
}

#[test]
fn status_shows_memory_count() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp).args(["remember", "body", "--title", "t"]).assert().success();
    cmd(&tmp).args(["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("memories: 1"));
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test -p mnemos_cli --test cli_admin`
Expected: FAIL.

- [ ] **Step 3: Implement `crates/mnemos_cli/src/commands/rebuild.rs`**

```rust
use anyhow::Result;
use mnemos_core::{paths::Paths, rebuild::rebuild_index};
use std::path::PathBuf;

pub async fn run(vault: Option<PathBuf>, json: bool) -> Result<()> {
    let paths = match vault {
        Some(p) => Paths::with_root(&p),
        None => Paths::default_xdg()?,
    };
    let stats = rebuild_index(&paths).await?;
    if json {
        println!("{}", serde_json::json!({
            "indexed": stats.memories_indexed,
            "errors": stats.errors,
            "error_paths": stats.error_paths,
        }));
    } else {
        println!("rebuild complete — indexed: {}  errors: {}",
            stats.memories_indexed, stats.errors);
        for p in stats.error_paths {
            eprintln!("  ERR {}", p.display());
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Implement `crates/mnemos_cli/src/commands/doctor.rs`**

```rust
use anyhow::Result;
use mnemos_core::{doctor::diagnose, paths::Paths};
use std::path::PathBuf;

pub async fn run(vault: Option<PathBuf>, json: bool) -> Result<()> {
    let paths = match vault {
        Some(p) => Paths::with_root(&p),
        None => Paths::default_xdg()?,
    };
    let report = diagnose(&paths).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "files scanned: {}\nindexed memories: {}",
            report.files_scanned, report.db_rows
        );
        if report.issues.is_empty() {
            println!("no issues");
        } else {
            println!("{} issue(s):", report.issues.len());
            for issue in report.issues {
                println!(
                    "  [{:?}] {} {}",
                    issue.kind,
                    issue.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
                    issue.detail
                );
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Implement `crates/mnemos_cli/src/commands/status.rs`**

```rust
use crate::commands::open_vault;
use anyhow::Result;
use mnemos_core::storage::memory_ops::ListFilter;
use std::path::PathBuf;

pub async fn run(vault: Option<PathBuf>, json: bool) -> Result<()> {
    let vault = open_vault(vault).await?;
    let active = vault.list(ListFilter { include_invalid: false, ..Default::default() }).await?;
    let all = vault.list(ListFilter { include_invalid: true, ..Default::default() }).await?;
    if json {
        println!("{}", serde_json::json!({
            "memories_active": active.len(),
            "memories_total":  all.len(),
            "vault_root":      vault.paths().root,
        }));
    } else {
        println!("vault:    {}", vault.paths().root.display());
        println!("memories: {} active / {} total",
            active.len(), all.len());
    }
    Ok(())
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p mnemos_cli --test cli_admin`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/mnemos_cli/src/commands/rebuild.rs \
        crates/mnemos_cli/src/commands/doctor.rs \
        crates/mnemos_cli/src/commands/status.rs \
        crates/mnemos_cli/tests/cli_admin.rs
git commit -m "feat(cli): rebuild, doctor, and status commands"
```

---

## Task 26: GitHub Actions CI

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Write `.github/workflows/ci.yml`**

```yaml
name: ci

on:
  push:
    branches: [main, master]
  pull_request:

jobs:
  test:
    name: cargo test (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - name: fmt
        run: cargo fmt --all -- --check
      - name: clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: test
        run: cargo test --workspace --all-features
```

- [ ] **Step 2: Verify YAML syntax locally**

If `yq` is available: `yq eval . .github/workflows/ci.yml > /dev/null`
Otherwise: open the file in an editor and visually check.

- [ ] **Step 3: Run the actual checks locally to make sure they pass before pushing**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: ALL PASS. If clippy fails, fix lints inline; if fmt fails, run `cargo fmt --all` and commit.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: cargo fmt + clippy + test on linux/macos/windows"
```

---

## Task 27: README, CHANGELOG, CONTRIBUTING, and v0.0.1 tag

**Files:**
- Create: `README.md`
- Create: `CHANGELOG.md`
- Create: `CONTRIBUTING.md`

- [ ] **Step 1: Write `README.md`**

```markdown
# mnemos

Local-first, file-as-source-of-truth memory provider for AI tools. Plan 1
ships the CLI foundation — vectors, daemon, MCP, and UI come in later
plans.

## What works today (v0.0.1)

- `mnemos remember "<body>"` — store a memory (markdown file on disk + DB index).
- `mnemos recall "<query>"` — BM25 search via SQLite FTS5.
- `mnemos get <id>` / `mnemos list` / `mnemos forget <id>` — basic CRUD with
  bi-temporal soft invalidation.
- `mnemos rebuild` — reconstruct the DB index from files.
- `mnemos doctor` — detect file / DB drift, orphans, hash mismatches.
- `mnemos status` — vault summary.

## Install (from source)

```bash
git clone https://github.com/sjones/mnemos
cd mnemos
cargo install --path crates/mnemos_cli
```

## Vault layout

```
~/.local/share/mnemos/
├── files/
│   ├── working/
│   ├── episodic/
│   ├── semantic/
│   ├── procedural/
│   └── reflections/
└── index.db
```

Override the location with `--vault <path>` or `MNEMOS_VAULT=<path>`.

## Design

See `docs/superpowers/specs/2026-05-22-mnemos-memory-provider-design.md`.

## License

Apache-2.0
```

- [ ] **Step 2: Write `CHANGELOG.md`**

```markdown
# Changelog

All notable changes to this project are recorded here.

## [0.0.1] - 2026-05-22

### Added
- Cargo workspace with `mnemos_core` + `mnemos_cli` crates.
- Markdown files as source of truth with YAML frontmatter.
- libSQL + FTS5 derived index, schema v1.
- Bi-temporal model: `valid_at` / `invalid_at` / `superseded_by` on every memory.
- BM25 retrieval with tier / workspace / invalidation filters.
- `Vault` facade and `Storage` abstraction.
- CLI: `remember`, `recall`, `get`, `list`, `forget`, `rebuild`, `doctor`, `status`.
- Append-only audit log enforced via SQL triggers.
- File watcher emits typed events on external edits.
- GitHub Actions CI: fmt + clippy + test on Linux/macOS/Windows.
```

- [ ] **Step 3: Write `CONTRIBUTING.md`**

```markdown
# Contributing to mnemos

Mnemos is in early development. Before opening a PR:

1. `cargo fmt --all`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`

All three must pass. New features must come with tests — TDD is the rule, not the exception.

Commit messages follow `<type>: <subject>` form (`feat:`, `fix:`, `chore:`,
`docs:`, `test:`, `refactor:`). Reference the relevant Plan + Task in the
body when applicable.
```

- [ ] **Step 4: Final sanity sweep before tagging**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all green.

- [ ] **Step 5: Commit and tag**

```bash
git add README.md CHANGELOG.md CONTRIBUTING.md
git commit -m "docs: README, CHANGELOG, CONTRIBUTING for v0.0.1"
git tag -a v0.0.1 -m "mnemos v0.0.1 — Plan 1 (Foundation) complete"
```

- [ ] **Step 6: Smoke test the binary end-to-end**

```bash
cargo build --release -p mnemos_cli
export PATH="$PWD/target/release:$PATH"
export MNEMOS_VAULT="/tmp/mnemos-smoke"
rm -rf "$MNEMOS_VAULT"

mnemos remember "User prefers Tauri over Electron" --title "Tauri choice"
mnemos remember "User likes Rust" --title "Rust pref"
mnemos list
mnemos recall "tauri"
mnemos status
mnemos doctor
```

Expected: every command succeeds; `recall tauri` returns the Tauri memory; `doctor` reports "no issues"; `status` reports 2 memories.

If any command fails, debug it now — Plan 2 builds on a working v0.0.1 foundation.

---

## Plan 1 self-review

### Spec coverage

This plan implements the **storage layer**, **bi-temporal model**, **audit log**, **BM25 retrieval**, **file ↔ DB sync (rebuild, watcher, doctor)**, and **CLI surface (BM25-only)** sections of the spec. Sections explicitly **deferred** to subsequent plans:

| Spec section | Plan |
|---|---|
| Dense vectors (sqlite-vec) + RRF + cross-encoder rerank | 2 |
| HippoRAG PPR retrieval | 5 |
| Daemon (REST/WS/MCP) | 3 |
| MCP tools, resources, prompts | 3 |
| Async pipelines (extract/resolve/entity-link/graph-update/decay) | 4 |
| Reflection + community detection | 5 |
| Tauri+React UI | 6 |
| Sync backends (git/Syncthing/S3/Turso) | 7 |
| Reference adapters (Claude Code, Gemini, Codex, Hermes, Openclaw) | 7 |
| Packaging (Tauri bundle, brew, deb, Docker, systemd) | 7 |
| Secret detection at ingestion | 4 |
| Encrypt-at-rest option | 7 |
| First-run wizard | 7 |
| LLM eval suite | 4 |

Plan 1 produces a usable single-user CLI with the spec's load-bearing storage decisions locked in: markdown files as source of truth, bi-temporal everywhere, append-only audit log, FTS5 hybrid-ready schema, file watcher in place. Subsequent plans are additive — no schema rewrites required.

### Placeholder scan

The only "to-be-filled" content in Plan 1 is the bodies of subsequent plans (intentional — Plan 1 is self-contained). All code blocks are complete. Each task's expected outputs and verification commands are explicit. No `TBD`, `TODO`, `// add error handling`, or "similar to Task N" hand-waves remain.

### Type / signature consistency

Cross-task signature audit:

- `Memory` (Task 7): all fields appear unchanged in Tasks 8, 10, 12, 13, 14, 16, 17, 18.
- `Tier::from_str` returns `Result<Tier>` consistently (Tasks 5, 10, 14, 22, 23).
- `Storage::open(&Path) → Result<Self>` consistent (Tasks 10, 11, 18, 19).
- `Storage::conn() → Result<Connection>` and `Storage::write_conn() → Result<(Connection, MutexGuard<'_, ()>)>` used identically in Tasks 11, 12, 13, 14, 15.
- `Vault::open(Paths) → Result<Self>` consistent (Task 17 onward).
- `RecallOpts` shape (k, tiers, workspace, include_invalid) consistent between Tasks 16 and 23.
- `row_to_memory(&Row) → Result<Memory>` used by Task 12 (defined) and Task 14, 16 (consumers).
- CLI command function signatures match `main.rs` dispatch in Task 21 throughout Tasks 22-25.

### Known follow-on cleanup (Plan 2)

These are not bugs — they're forward-compatible choices Plan 1 makes that Plan 2 will revise:

- `Memory.entities` stored as JSON in `entities_json` column. Plan 4 will populate `entity_mentions` table when the entity-linking pipeline lands.
- `Memory.body` is denormalized into the DB. Plan 6 (UI) will rely on this for fast inspector rendering; Plan 4 may revisit if storage cost matters.
- `Vault::remember` does not yet trigger an extraction pipeline. Plan 4 wires that in.
- `bm25_recall` returns a single retriever's results. Plan 2 introduces the fused-retrieval entry point that calls BM25, Dense, and (Plan 5) PPR.

---

## Execution

Plan 1 done — 27 tasks, ~120 steps total, ~10–15 hours of focused work for an engineer following the plan.

Subsequent plans (2 through 7) will be written after Plan 1 lands.
