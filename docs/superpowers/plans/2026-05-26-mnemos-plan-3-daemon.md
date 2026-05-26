# Mnemos Plan 3 — Daemon, REST API, WebSocket, MCP Server

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `mnemosd` — a long-running daemon that exposes the same `mnemos_core` Vault over (a) REST + WebSocket on `127.0.0.1:7423`, (b) MCP over Streamable HTTP at the same port, and (c) MCP over stdio for legacy clients. CLI continues to work standalone, but prefers the daemon when one is running. End state: `claude code` can `remember`/`recall` against Mnemos via MCP without writing custom integration code. Also closes five Plan 2 forward-compat gaps (Embedder::model_id, dim runtime check, OllamaEmbedder batching, MNEMOS_EMBEDDER_DIM → config.toml, --rerank wiring).

**Architecture:** New `mnemos_daemon` crate hosts an `axum` HTTP server. Auth is a 32-byte bearer token at `~/.config/mnemos/token` (mode 0600), required on every endpoint except `/health`. Configuration lives in `~/.config/mnemos/config.toml` (TOML; formalizes Plan 2's env vars). The daemon owns one `Vault`; CLI clients talk to it over REST. MCP-HTTP is implemented as another axum route group on the same server; MCP-stdio is a thin subprocess wrapper that forwards stdio frames to MCP-HTTP. WebSocket emits typed events whenever the Vault changes. CLI gains a small "client mode" that talks to the daemon transparently; falls back to direct vault access when no daemon is running (so `mnemos remember` keeps working without any daemon installed).

**Tech Stack:** Rust 2021, **axum 0.8** (HTTP+WebSocket), **tower-http** (auth + tracing middleware), **tokio**, **serde + toml 0.8** (config), **rand 0.8** (token bytes), **directories** (XDG paths), **rmcp 0.1** (Rust MCP SDK; if API drift, fall back to a thin custom impl), reqwest (CLI HTTP client).

---

## Plan sequence context

Plan 3 of 7. Subsequent:
- Plan 4: async LLM-driven extraction + resolution pipelines (uses the daemon's background runtime)
- Plan 5: HippoRAG PPR retriever + reflection + community detection
- Plan 6: Tauri + React desktop UI (consumes daemon's REST + WebSocket)
- Plan 7: sync backends, adapters, packaging

Plan 3 produces **v0.2.0** — daemon is online; CLI talks to it transparently; Claude Code can integrate via MCP. Nothing in Plan 4-7 requires schema changes to v0.2.0.

---

## Plan 2 carry-forwards closed by this plan

The Plan 2 final review identified five forward-compat gaps. They're addressed inside Plan 3:

| # | Gap from Plan 2 review | Closed by |
|---|---|---|
| 1 | `--rerank` CLI flag is dead — calls `hybrid_recall`, not `hybrid_recall_with_rerank` | Task 15 — Reranker loaded from `config.toml.[reranker]`; daemon routes through `hybrid_recall_with_rerank` |
| 2 | Schema dim hardcoded 768; switching embedder model silently corrupts KNN | Task 4 — `vault_meta` table stores embedder dim + model_id; `Vault::open_with_embedder` errors on mismatch |
| 3 | `Embedder::model_id()` missing | Task 4 — default method added on trait; all implementations override |
| 4 | `OllamaEmbedder::embed_batch` uses serial default | Task 4 — override with concurrent fan-out (Ollama supports parallel HTTP) |
| 5 | `MNEMOS_EMBEDDER_DIM` env var undocumented | Task 2 — graduates to `config.toml [embedder]`; env vars become overrides documented in README |

---

## Hard prerequisites before starting

- Plan 2 (`v0.1.0`) shipped and on master.
- Rust 1.78+ (already pinned in `rust-toolchain.toml`).
- `gh` CLI authenticated (only needed at Task 19 push).
- Ollama optional — all CI/integration tests use `MockEmbedder` or HTTP fixtures.

---

## File structure produced by this plan

```
crates/
├── mnemos_core/                       # unchanged shape; Task 4 adds 1 trait method + small schema migration
│   └── src/
│       ├── providers/
│       │   ├── mod.rs                 # MODIFIED: add fn model_id(&self) -> &str default method
│       │   ├── mock.rs                # MODIFIED: override model_id() = "mock"
│       │   └── ollama.rs              # MODIFIED: override model_id() + concurrent embed_batch
│       ├── storage/
│       │   └── migrations.rs          # MODIFIED: v3 = vault_meta table (dim, model_id)
│       └── vault.rs                   # MODIFIED: open_with_embedder asserts dim/model_id match
├── mnemos_daemon/                     # NEW crate
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                    # tokio entry + clap (`mnemosd serve|status`)
│       ├── lib.rs                     # re-exports for integration tests
│       ├── config.rs                  # Config struct + loader; reads ~/.config/mnemos/config.toml
│       ├── auth.rs                    # token issuance + Bearer middleware
│       ├── state.rs                   # AppState { vault, config, event_bus, reranker }
│       ├── error.rs                   # ApiError + IntoResponse impl
│       ├── routes/
│       │   ├── mod.rs                 # router builder
│       │   ├── health.rs              # GET /health (no auth)
│       │   ├── memories.rs            # CRUD + search + time-travel + audit
│       │   ├── sessions.rs            # session lifecycle
│       │   ├── entities.rs            # entity + entity graph (stubs in Plan 3; full in Plan 5)
│       │   ├── working.rs             # GET /v1/working (mnemos://working resource)
│       │   └── events.rs              # WS /v1/events
│       ├── mcp/
│       │   ├── mod.rs                 # MCP server scaffold
│       │   ├── tools.rs               # remember, recall, forget, …
│       │   ├── resources.rs           # mnemos://working, mnemos://memory/{id}, …
│       │   └── prompts.rs             # mnemos.context-for, mnemos.session-resume
│       ├── events.rs                  # EventBus + Event types
│       └── pid.rs                     # PID file mgmt
├── mnemos_client/                     # NEW crate — shared HTTP client for CLI + future UI
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                     # Client struct + methods (remember, recall, …)
│       ├── error.rs
│       └── transport.rs               # reqwest + Bearer token loader
└── mnemos_cli/                        # MODIFIED: gains client-mode + daemon subcommand
    └── src/
        ├── cli.rs                     # MODIFIED: Daemon subcommand
        ├── main.rs                    # MODIFIED: dispatch new arms
        └── commands/
            ├── mod.rs                 # MODIFIED: open_vault → open_session
            │                          # (returns ClientOrVault, transparently)
            ├── daemon.rs              # NEW: start | stop | status | logs
            └── (others)               # MODIFIED to use ClientOrVault

adapters/                              # NEW dir
├── claude-code/
│   ├── README.md                      # one-page setup guide
│   ├── CLAUDE.md.fragment             # drop-in fragment users append to ~/.claude/CLAUDE.md
│   └── claude_mcp_config.json         # MCP server registration JSON

Cargo.toml                              # workspace.members += mnemos_daemon, mnemos_client
README.md                               # Plan 3 surface added
CHANGELOG.md                            # 0.2.0 entry
```

---

## Conventions (same as Plans 1-2)

- **TDD**: failing test → confirm-fail → impl → confirm-pass → commit. Apply to every task that has testable behavior; pure scaffolding (Cargo.toml, README) skips the failing-test step.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` green at every commit.
- Commit message format: `<type>: <subject>`. Reference Plan 3 / Task N in the body when useful.
- All paths relative to `/home/jons/AntiGravityProjects/mnemos/`.
- Tokio async throughout.
- All HTTP fixtures (axum integration tests) use `axum::Router::into_make_service()` + `hyper::client::conn` so tests don't need a real socket. The pattern is shown in Task 5.
- CLI integration tests continue to set `MNEMOS_EMBEDDER=mock` and now `MNEMOS_DAEMON=off` for direct-vault mode.

---

## Task 1: Scaffold `mnemos_daemon` and `mnemos_client` crates

**Files:**
- Modify: `Cargo.toml` (workspace members + new workspace.dependencies for axum/tower/toml)
- Create: `crates/mnemos_daemon/Cargo.toml`
- Create: `crates/mnemos_daemon/src/{main.rs,lib.rs}`
- Create: `crates/mnemos_client/Cargo.toml`
- Create: `crates/mnemos_client/src/lib.rs`

- [ ] **Step 1: Update workspace `Cargo.toml`**

Add to `[workspace] members`:

```toml
members = [
    "crates/mnemos_core",
    "crates/mnemos_cli",
    "crates/mnemos_daemon",
    "crates/mnemos_client",
]
```

Add to `[workspace.dependencies]`:

```toml
axum = { version = "0.8", features = ["ws", "macros"] }
tower = { version = "0.5", features = ["util", "timeout"] }
tower-http = { version = "0.6", features = ["trace", "cors", "auth", "limit"] }
hyper = { version = "1", features = ["client", "http1"] }
hyper-util = { version = "0.1", features = ["client", "http1", "tokio"] }
http-body-util = "0.1"
toml = "0.8"
rand = "0.8"
futures = "0.3"
url = "2"
```

- [ ] **Step 2: Create `crates/mnemos_daemon/Cargo.toml`**

```toml
[package]
name = "mnemos_daemon"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
rust-version.workspace = true

[[bin]]
name = "mnemosd"
path = "src/main.rs"

[dependencies]
mnemos_core = { path = "../mnemos_core" }
tokio = { workspace = true }
axum = { workspace = true }
tower = { workspace = true }
tower-http = { workspace = true }
hyper = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
toml = { workspace = true }
clap = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
directories = { workspace = true }
rand = { workspace = true }
chrono = { workspace = true }
futures = { workspace = true }
async-trait = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
reqwest = { workspace = true }
hyper-util = { workspace = true }
http-body-util = { workspace = true }
```

- [ ] **Step 3: Create `crates/mnemos_daemon/src/main.rs` (minimal)**

```rust
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("mnemosd: scaffold only (Task 1)");
    Ok(())
}
```

- [ ] **Step 4: Create `crates/mnemos_daemon/src/lib.rs`**

```rust
//! Mnemos daemon: long-running HTTP + WebSocket + MCP server over the
//! `mnemos_core::Vault`. Re-exported here for integration tests.

#![deny(rust_2018_idioms)]
#![warn(clippy::all)]

// Modules populated by subsequent tasks.
```

- [ ] **Step 5: Create `crates/mnemos_client/Cargo.toml`**

```toml
[package]
name = "mnemos_client"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
rust-version.workspace = true

[dependencies]
mnemos_core = { path = "../mnemos_core" }
tokio = { workspace = true }
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
url = { workspace = true }
directories = { workspace = true }
async-trait = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 6: Create `crates/mnemos_client/src/lib.rs` (minimal)**

```rust
//! Mnemos HTTP client for the daemon. Used by CLI and future UI/adapters.
//! Methods populated in Task 16.

#![deny(rust_2018_idioms)]
#![warn(clippy::all)]
```

- [ ] **Step 7: Verify both crates compile**

```bash
cargo check -p mnemos_daemon
cargo check -p mnemos_client
cargo check --workspace
```

All three pass. Lots of unused-dep warnings — fine for now.

- [ ] **Step 8: Verify the binary at least runs**

```bash
cargo run -p mnemos_daemon -- 2>&1 | head -5
```

Expected output: `mnemosd: scaffold only (Task 1)`.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml crates/mnemos_daemon/ crates/mnemos_client/
git commit -m "chore: scaffold mnemos_daemon + mnemos_client crates"
```

---

## Task 2: `Config` struct + TOML loader

**Files:**
- Create: `crates/mnemos_daemon/src/config.rs`
- Modify: `crates/mnemos_daemon/src/lib.rs` (`pub mod config;`)
- Test: `crates/mnemos_daemon/tests/config.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_daemon::config::{Config, EmbedderKind, RerankerKind};
use tempfile::TempDir;

#[test]
fn config_loads_from_toml_file() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    std::fs::write(&path, r#"
[daemon]
host = "127.0.0.1"
port = 9999

[vault]
root = "/tmp/test-vault"

[embedder]
kind = "mock"
dim = 384

[reranker]
enabled = false
"#).unwrap();
    let cfg = Config::load_from(&path).unwrap();
    assert_eq!(cfg.daemon.port, 9999);
    assert!(matches!(cfg.embedder.kind, EmbedderKind::Mock));
    assert_eq!(cfg.embedder.dim, 384);
    assert!(!cfg.reranker.enabled);
    assert!(matches!(cfg.reranker.kind, RerankerKind::None));
}

#[test]
fn config_defaults_when_file_absent() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("does-not-exist.toml");
    let cfg = Config::load_from(&path).unwrap();   // fallback to defaults
    assert_eq!(cfg.daemon.port, 7423);
    assert!(matches!(cfg.embedder.kind, EmbedderKind::Ollama));
    assert_eq!(cfg.embedder.dim, 768);
    assert!(!cfg.reranker.enabled);
}

#[test]
fn env_overrides_take_precedence() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    std::fs::write(&path, r#"
[embedder]
kind = "ollama"
url = "http://localhost:11434"
"#).unwrap();
    std::env::set_var("MNEMOS_OLLAMA_URL", "http://override:11434");
    let cfg = Config::load_from(&path).unwrap();
    assert_eq!(cfg.embedder.url, "http://override:11434");
    std::env::remove_var("MNEMOS_OLLAMA_URL");
}

#[test]
fn reweight_defaults_match_recall_opts() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    std::fs::write(&path, "").unwrap();
    let cfg = Config::load_from(&path).unwrap();
    assert_eq!(cfg.retrieval.reweight.recency_decay, 0.02);
    assert_eq!(cfg.retrieval.reweight.tier_weight_working, 2.0);
}
```

- [ ] **Step 2: Run test to verify fail**

`cargo test -p mnemos_daemon --test config`
Expected: FAIL — module empty.

- [ ] **Step 3: Implement `crates/mnemos_daemon/src/config.rs`**

```rust
//! `config.toml` schema and loader.
//!
//! Resolution order: file values → environment-variable overrides → defaults.

use anyhow::{Context, Result};
use mnemos_core::retrieval::reweight::ReweightConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub daemon: DaemonConfig,
    pub vault: VaultConfig,
    pub embedder: EmbedderConfig,
    pub reranker: RerankerConfig,
    pub retrieval: RetrievalConfig,
    pub mcp: McpConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub host: String,
    pub port: u16,
    /// If true, CLI auto-spawns the daemon when one is not running.
    pub auto_start: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VaultConfig {
    /// Vault root. Tilde expansion applied at load time.
    pub root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbedderConfig {
    pub kind: EmbedderKind,
    pub url: String,
    pub model: String,
    pub dim: usize,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EmbedderKind {
    Ollama,
    Mock,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RerankerConfig {
    pub enabled: bool,
    pub kind: RerankerKind,
    pub model_path: Option<PathBuf>,
    pub tokenizer_path: Option<PathBuf>,
    pub max_seq_len: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RerankerKind {
    None,
    Onnx,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RetrievalConfig {
    pub default_k: usize,
    pub rrf_k: usize,
    pub reweight: ReweightConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    pub enabled: bool,
    /// Try to use MCP sampling (Plan 4 uses this for extraction LLM).
    pub sampling_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    /// "json" or "compact"
    pub format: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
            vault: VaultConfig::default(),
            embedder: EmbedderConfig::default(),
            reranker: RerankerConfig::default(),
            retrieval: RetrievalConfig::default(),
            mcp: McpConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self { host: "127.0.0.1".into(), port: 7423, auto_start: true }
    }
}

impl Default for VaultConfig {
    fn default() -> Self {
        let root = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
            .map(|p| p.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("./mnemos-data"));
        Self { root }
    }
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            kind: EmbedderKind::Ollama,
            url: "http://localhost:11434".into(),
            model: "nomic-embed-text".into(),
            dim: 768,
            timeout_secs: 30,
        }
    }
}

impl Default for RerankerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            kind: RerankerKind::None,
            model_path: None,
            tokenizer_path: None,
            max_seq_len: 512,
        }
    }
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self { default_k: 10, rrf_k: 60, reweight: ReweightConfig::default() }
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self { enabled: true, sampling_enabled: true }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self { level: "info".into(), format: "compact".into() }
    }
}

impl Config {
    /// Load from a TOML file; if absent, use defaults. Environment vars override.
    pub fn load_from(path: &Path) -> Result<Self> {
        let mut cfg: Config = if path.exists() {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("read {}", path.display()))?;
            toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?
        } else {
            Config::default()
        };
        apply_env_overrides(&mut cfg);
        expand_paths(&mut cfg)?;
        Ok(cfg)
    }

    /// Load from the default XDG location (~/.config/mnemos/config.toml).
    pub fn load_default() -> Result<Self> {
        let path = default_config_path()?;
        Self::load_from(&path)
    }
}

fn default_config_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .context("could not resolve XDG config dir")?;
    Ok(dirs.config_dir().join("config.toml"))
}

fn apply_env_overrides(cfg: &mut Config) {
    // Plan 2 env vars graduate to overrides on this config.
    if let Ok(v) = std::env::var("MNEMOS_EMBEDDER") {
        cfg.embedder.kind = match v.as_str() {
            "mock"   => EmbedderKind::Mock,
            "none"   => EmbedderKind::None,
            _        => EmbedderKind::Ollama,
        };
    }
    if let Ok(v) = std::env::var("MNEMOS_OLLAMA_URL")   { cfg.embedder.url = v; }
    if let Ok(v) = std::env::var("MNEMOS_OLLAMA_MODEL") { cfg.embedder.model = v; }
    if let Ok(v) = std::env::var("MNEMOS_EMBEDDER_DIM") {
        if let Ok(n) = v.parse::<usize>() { cfg.embedder.dim = n; }
    }
    if let Ok(v) = std::env::var("MNEMOS_VAULT") { cfg.vault.root = PathBuf::from(v); }
    if let Ok(v) = std::env::var("MNEMOS_DAEMON_PORT") {
        if let Ok(p) = v.parse::<u16>() { cfg.daemon.port = p; }
    }
    if let Ok(v) = std::env::var("MNEMOS_LOG") { cfg.logging.level = v; }
}

fn expand_paths(cfg: &mut Config) -> Result<()> {
    cfg.vault.root = expand_tilde(&cfg.vault.root)?;
    if let Some(p) = cfg.reranker.model_path.as_mut() {
        *p = expand_tilde(p)?;
    }
    if let Some(p) = cfg.reranker.tokenizer_path.as_mut() {
        *p = expand_tilde(p)?;
    }
    Ok(())
}

fn expand_tilde(p: &Path) -> Result<PathBuf> {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/") {
        let home = directories::UserDirs::new()
            .and_then(|u| Some(u.home_dir().to_path_buf()))
            .context("could not resolve home dir for ~/ expansion")?;
        Ok(home.join(rest))
    } else {
        Ok(p.to_path_buf())
    }
}
```

- [ ] **Step 4: Add `pub mod config;` to `crates/mnemos_daemon/src/lib.rs`**

```rust
pub mod config;
```

- [ ] **Step 5: Run tests**

`cargo test -p mnemos_daemon --test config` → 4 pass.

- [ ] **Step 6: Verify**

```bash
cargo fmt --check -p mnemos_daemon
cargo clippy -p mnemos_daemon --all-targets -- -D warnings
```

Both clean.

- [ ] **Step 7: Commit**

```bash
git add crates/mnemos_daemon/src/{lib.rs,config.rs} crates/mnemos_daemon/tests/config.rs
git commit -m "feat(daemon): Config struct + TOML loader with env overrides"
```

---

## Task 3: Auth token

**Files:**
- Create: `crates/mnemos_daemon/src/auth.rs`
- Modify: `crates/mnemos_daemon/src/lib.rs` (`pub mod auth;`)
- Test: `crates/mnemos_daemon/tests/auth.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_daemon::auth::{ensure_token, load_token, validate_token};
use tempfile::TempDir;

#[test]
fn ensure_token_writes_32_byte_file_with_mode_0600() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("token");
    let token = ensure_token(&path).unwrap();
    assert_eq!(token.len(), 64); // hex of 32 bytes
    assert!(path.exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }
}

#[test]
fn ensure_token_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("token");
    let a = ensure_token(&path).unwrap();
    let b = ensure_token(&path).unwrap();
    assert_eq!(a, b);
}

#[test]
fn validate_token_uses_constant_time_compare() {
    let token = "abcdef0123456789".repeat(4); // 64 chars
    assert!(validate_token(&token, &token));
    assert!(!validate_token(&token, "wrong"));
    assert!(!validate_token(&token, ""));
}

#[test]
fn load_token_reads_file() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("token");
    let written = ensure_token(&path).unwrap();
    let loaded = load_token(&path).unwrap();
    assert_eq!(written, loaded);
}
```

- [ ] **Step 2: Run test to verify fail**

`cargo test -p mnemos_daemon --test auth`
Expected: FAIL — module empty.

- [ ] **Step 3: Implement `crates/mnemos_daemon/src/auth.rs`**

```rust
//! Bearer token issuance + validation. Token lives at
//! `~/.config/mnemos/token` (mode 0600 on Unix).
//!
//! 32 random bytes, hex-encoded → 64-char ASCII string.

use anyhow::{Context, Result};
use rand::RngCore;
use std::path::Path;

const TOKEN_BYTES: usize = 32;

/// Returns the token at `path`, creating it if absent. On Unix, sets mode 0600.
pub fn ensure_token(path: &Path) -> Result<String> {
    if path.exists() {
        return load_token(path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create token dir {}", parent.display()))?;
    }
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    std::fs::write(path, &hex)
        .with_context(|| format!("write token {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(hex)
}

pub fn load_token(path: &Path) -> Result<String> {
    let s = std::fs::read_to_string(path)
        .with_context(|| format!("read token {}", path.display()))?;
    Ok(s.trim().to_string())
}

/// Constant-time string equality. Returns false for length-mismatched inputs.
pub fn validate_token(expected: &str, presented: &str) -> bool {
    let a = expected.as_bytes();
    let b = presented.as_bytes();
    if a.len() != b.len() { return false; }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}
```

- [ ] **Step 4: Wire `pub mod auth;` into `crates/mnemos_daemon/src/lib.rs`**

```rust
pub mod auth;
pub mod config;
```

- [ ] **Step 5: Run tests** → 4 pass.

- [ ] **Step 6: Verify** — fmt + clippy clean.

- [ ] **Step 7: Commit**

```bash
git add crates/mnemos_daemon/src/{auth.rs,lib.rs} crates/mnemos_daemon/tests/auth.rs
git commit -m "feat(daemon): bearer token issuance + constant-time validation"
```

---

## Task 4: Close 4 Plan 2 carry-forwards (Embedder::model_id, OllamaEmbedder::embed_batch override, schema v3 vault_meta, dim runtime check)

**Files:**
- Modify: `crates/mnemos_core/src/providers/mod.rs`
- Modify: `crates/mnemos_core/src/providers/mock.rs`
- Modify: `crates/mnemos_core/src/providers/ollama.rs`
- Modify: `crates/mnemos_core/src/storage/migrations.rs`
- Modify: `crates/mnemos_core/src/storage/mod.rs` (add vault_meta read/write helpers)
- Modify: `crates/mnemos_core/src/vault.rs` (dim/model check in `open_with_embedder`)
- Test: `crates/mnemos_core/tests/dim_mismatch.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::providers::{Embedder, mock::MockEmbedder};
use mnemos_core::vault::Vault;
use mnemos_core::paths::Paths;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn opening_vault_with_mismatched_dim_errors() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let e768: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    {
        let _ = Vault::open_with_embedder(paths.clone(), Some(e768.clone())).await.unwrap();
    }
    let e384: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(384));
    let err = Vault::open_with_embedder(paths, Some(e384)).await;
    assert!(err.is_err(), "different dim must error");
    let msg = format!("{:?}", err.unwrap_err());
    assert!(msg.contains("dim"), "error should mention dim: {msg}");
}

#[tokio::test]
async fn opening_vault_with_no_embedder_skips_dim_check() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let e: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let _ = Vault::open_with_embedder(paths.clone(), Some(e)).await.unwrap();
    let _ = Vault::open(paths).await.unwrap();  // no embedder → no check
}

#[tokio::test]
async fn embedder_model_id_round_trips_through_vault_meta() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let e: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    assert_eq!(e.model_id(), "mock");

    let v = Vault::open_with_embedder(paths.clone(), Some(e)).await.unwrap();
    let meta = v.storage().get_vault_meta().await.unwrap();
    assert_eq!(meta.embedder_dim, Some(768));
    assert_eq!(meta.embedder_model_id, Some("mock".to_string()));
}
```

- [ ] **Step 2: Run test to verify fail**

`cargo test -p mnemos_core --test dim_mismatch`
Expected: FAIL — `model_id` not defined; `get_vault_meta` not defined; dim check not enforced.

- [ ] **Step 3: Add `model_id` default method on `Embedder` trait**

In `crates/mnemos_core/src/providers/mod.rs`:

```rust
#[async_trait]
pub trait Embedder: Send + Sync {
    /// Vector dimension produced by this embedder (e.g. 768 for nomic-embed-text).
    fn dim(&self) -> usize;

    /// Stable identifier for the model (used to detect model swaps).
    /// Override per implementation; default is "unknown".
    fn model_id(&self) -> &str {
        "unknown"
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            out.push(self.embed(t).await?);
        }
        Ok(out)
    }
}
```

- [ ] **Step 4: Override `model_id` on MockEmbedder**

In `crates/mnemos_core/src/providers/mock.rs`, add inside `impl Embedder for MockEmbedder`:

```rust
    fn model_id(&self) -> &str {
        "mock"
    }
```

- [ ] **Step 5: Override `model_id` and `embed_batch` on OllamaEmbedder**

In `crates/mnemos_core/src/providers/ollama.rs`, change `impl Embedder for OllamaEmbedder` to:

```rust
#[async_trait]
impl Embedder for OllamaEmbedder {
    fn dim(&self) -> usize {
        self.config.dim
    }

    fn model_id(&self) -> &str {
        &self.config.model
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // existing body unchanged
        let url = format!("{}/api/embeddings", self.config.base_url.trim_end_matches('/'));
        let resp = self.client.post(&url)
            .json(&EmbedReq { model: &self.config.model, prompt: text })
            .send().await
            .map_err(|e| MnemosError::Internal(format!("ollama HTTP: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MnemosError::Internal(format!("ollama responded {status}: {body}")));
        }
        let parsed: EmbedResp = resp.json().await
            .map_err(|e| MnemosError::Internal(format!("ollama parse: {e}")))?;
        if parsed.embedding.len() != self.config.dim {
            return Err(MnemosError::Internal(format!(
                "ollama returned {}d, expected {}d (model mismatch?)",
                parsed.embedding.len(), self.config.dim
            )));
        }
        Ok(parsed.embedding)
    }

    /// Concurrent fan-out — Ollama serves embeddings in parallel reliably.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        use futures::stream::{self, StreamExt};
        const MAX_CONCURRENT: usize = 8;
        let results: Vec<Result<Vec<f32>>> = stream::iter(texts.iter())
            .map(|t| self.embed(t))
            .buffered(MAX_CONCURRENT)
            .collect()
            .await;
        results.into_iter().collect()
    }
}
```

Note: this requires `futures` as a dep. Add to `crates/mnemos_core/Cargo.toml`:

```toml
futures = { workspace = true }
```

`futures` is already in workspace deps (Task 1 added it).

- [ ] **Step 6: Add `vault_meta` table — schema migration v3**

In `crates/mnemos_core/src/storage/migrations.rs`, extend `apply_migrations`:

```rust
        if current < 3 {
            migration_v3(&conn).await?;
            conn.execute(
                "INSERT OR IGNORE INTO schema_migrations (version) VALUES (3)",
                (),
            ).await?;
        }
        Ok(())
    }
}

async fn migration_v3(conn: &libsql::Connection) -> Result<()> {
    for stmt in V3_STATEMENTS {
        conn.execute(stmt, ()).await?;
    }
    Ok(())
}

const V3_STATEMENTS: &[&str] = &[
    // Single-row vault metadata. PK is a constant so we always upsert in place.
    "CREATE TABLE IF NOT EXISTS vault_meta (
        id                INTEGER PRIMARY KEY CHECK(id = 1),
        embedder_dim      INTEGER,
        embedder_model_id TEXT,
        updated_at        TEXT NOT NULL
    )",
    "INSERT OR IGNORE INTO vault_meta (id, updated_at) VALUES (1, '1970-01-01T00:00:00Z')",
];
```

- [ ] **Step 7: Add `vault_meta` read/write helpers in `crates/mnemos_core/src/storage/mod.rs`**

Append to the existing `impl Storage` block:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct VaultMeta {
    pub embedder_dim: Option<usize>,
    pub embedder_model_id: Option<String>,
}

impl Storage {
    pub async fn get_vault_meta(&self) -> crate::error::Result<VaultMeta> {
        let conn = self.conn()?;
        let mut rows = conn.query(
            "SELECT embedder_dim, embedder_model_id FROM vault_meta WHERE id = 1",
            (),
        ).await?;
        let row = rows.next().await?;
        match row {
            Some(r) => Ok(VaultMeta {
                embedder_dim: r.get::<Option<i64>>(0)?.map(|x| x as usize),
                embedder_model_id: r.get::<Option<String>>(1)?,
            }),
            None => Ok(VaultMeta { embedder_dim: None, embedder_model_id: None }),
        }
    }

    pub async fn set_vault_meta(&self, dim: usize, model_id: &str) -> crate::error::Result<()> {
        let (conn, _g) = self.write_conn().await?;
        conn.execute(
            "UPDATE vault_meta SET embedder_dim = ?, embedder_model_id = ?, updated_at = ? WHERE id = 1",
            libsql::params![dim as i64, model_id.to_string(), chrono::Utc::now().to_rfc3339()],
        ).await?;
        Ok(())
    }
}
```

- [ ] **Step 8: Add dim/model check in `Vault::open_with_embedder`**

In `crates/mnemos_core/src/vault.rs`, modify `open_with_embedder` to verify or initialize the stored metadata:

```rust
    pub async fn open_with_embedder(
        paths: Paths,
        embedder: Option<Arc<dyn Embedder>>,
    ) -> Result<Self> {
        paths.ensure_dirs()?;
        let storage = Storage::open(&paths.db_path).await?;

        if let Some(e) = embedder.as_ref() {
            let meta = storage.get_vault_meta().await?;
            match (meta.embedder_dim, meta.embedder_model_id.as_deref()) {
                (None, _) | (_, None) => {
                    // First time an embedder has touched this vault — record it.
                    storage.set_vault_meta(e.dim(), e.model_id()).await?;
                }
                (Some(stored_dim), Some(stored_model)) => {
                    if stored_dim != e.dim() {
                        return Err(crate::error::MnemosError::Validation(format!(
                            "embedder dim mismatch: vault stored {stored_dim}d, embedder produces {}d (model {} → {})",
                            e.dim(), stored_model, e.model_id()
                        )));
                    }
                    if stored_model != e.model_id() {
                        // Same dim, different model — warn but allow.
                        // Production-grade: write an audit entry.
                        tracing::warn!(
                            "vault model_id changed: {} → {} (dim {} unchanged)",
                            stored_model, e.model_id(), stored_dim
                        );
                        storage.set_vault_meta(e.dim(), e.model_id()).await?;
                    }
                }
            }
        }

        Ok(Self { paths, storage, embedder })
    }
```

- [ ] **Step 9: Run tests**

```bash
cargo test -p mnemos_core --test dim_mismatch          # 3 pass
cargo test -p mnemos_core                              # full crate, no regressions
cargo test --workspace                                 # full workspace
```

The existing tests must still pass. In particular: any test that opens a vault with one MockEmbedder dim and reopens with a different dim would now fail — there's no such test in v0.1.0 (all tests use the same dim per fixture), so the existing suite is unaffected.

- [ ] **Step 10: Verify** — fmt + clippy clean workspace-wide.

- [ ] **Step 11: Commit**

```bash
git add crates/mnemos_core/src/providers/{mod.rs,mock.rs,ollama.rs} \
        crates/mnemos_core/src/storage/{mod.rs,migrations.rs} \
        crates/mnemos_core/src/vault.rs \
        crates/mnemos_core/Cargo.toml \
        crates/mnemos_core/tests/dim_mismatch.rs
git commit -m "feat(core): close Plan 2 carry-forwards (model_id, ollama batch, vault_meta dim check)"
```

---

## Task 5: axum scaffold + auth middleware + health endpoint

**Files:**
- Create: `crates/mnemos_daemon/src/state.rs`
- Create: `crates/mnemos_daemon/src/error.rs`
- Create: `crates/mnemos_daemon/src/routes/mod.rs`
- Create: `crates/mnemos_daemon/src/routes/health.rs`
- Modify: `crates/mnemos_daemon/src/lib.rs`
- Test: `crates/mnemos_daemon/tests/health.rs`

- [ ] **Step 1: Write failing test**

```rust
use axum::http::StatusCode;
use mnemos_daemon::{config::Config, build_app};

#[tokio::test]
async fn health_endpoint_returns_200_without_auth() {
    let (app, _state) = build_app(Config::default(), test_vault().await).await.unwrap();
    let resp = call(app, "GET", "/health", None, "").await;
    assert_eq!(resp.0, StatusCode::OK);
    assert!(resp.1.contains("\"status\":\"ok\""));
}

#[tokio::test]
async fn auth_required_on_v1_routes() {
    let (app, _state) = build_app(Config::default(), test_vault().await).await.unwrap();
    let resp = call(app.clone(), "GET", "/v1/working", None, "").await;
    assert_eq!(resp.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_with_correct_bearer_passes() {
    let (app, state) = build_app(Config::default(), test_vault().await).await.unwrap();
    let token = state.token.clone();
    let resp = call(app, "GET", "/v1/working", Some(&token), "").await;
    // We expect either 200 (route exists later) or 404 (route doesn't yet exist),
    // but NOT 401.
    assert_ne!(resp.0, StatusCode::UNAUTHORIZED);
}

// -- helpers --

use mnemos_core::vault::Vault;
use mnemos_core::paths::Paths;
use tempfile::TempDir;

async fn test_vault() -> Vault {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let paths = Paths::with_root(tmp.path());
    Vault::open(paths).await.unwrap()
}

async fn call(
    app: axum::Router,
    method: &str,
    uri: &str,
    auth: Option<&str>,
    body: &str,
) -> (axum::http::StatusCode, String) {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder()
        .method(method)
        .uri(uri);
    if let Some(t) = auth {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let req = req.body(Body::from(body.to_string())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).to_string();
    (status, text)
}
```

NOTE: tests use `tower::ServiceExt::oneshot`, the canonical pattern for axum integration testing without a real socket.

- [ ] **Step 2: Run test to verify fail**

`cargo test -p mnemos_daemon --test health`
Expected: FAIL — `build_app` doesn't exist.

- [ ] **Step 3: Implement `crates/mnemos_daemon/src/state.rs`**

```rust
//! Shared application state passed to all route handlers.

use mnemos_core::vault::Vault;
use std::sync::Arc;

use crate::config::Config;
use crate::events::EventBus;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub vault: Vault,
    pub token: String,
    pub events: EventBus,
}
```

NOTE: `EventBus` is defined in Task 10 — for Task 5 we add a stub.

Create `crates/mnemos_daemon/src/events.rs` with the minimum:

```rust
//! Placeholder — populated in Task 10.
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct EventBus {
    _inner: Arc<()>,
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }
}
```

- [ ] **Step 4: Implement `crates/mnemos_daemon/src/error.rs`**

```rust
//! API error type implementing `IntoResponse` so handlers can return `Result<T, ApiError>`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, msg)
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, msg)
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, msg)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({ "error": self.message })),
        ).into_response()
    }
}

impl From<mnemos_core::error::MnemosError> for ApiError {
    fn from(e: mnemos_core::error::MnemosError) -> Self {
        use mnemos_core::error::MnemosError::*;
        match e {
            MemoryNotFound(_) | EntityNotFound(_) | SessionNotFound(_) => {
                Self::not_found(e.to_string())
            }
            Validation(_) | InvalidFrontmatter { .. } | MalformedFile { .. } => {
                Self::bad_request(e.to_string())
            }
            _ => Self::internal(e.to_string()),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self::internal(e.to_string())
    }
}
```

- [ ] **Step 5: Implement `crates/mnemos_daemon/src/routes/mod.rs`**

```rust
//! Top-level router. Public routes (e.g. /health) are mounted unauthenticated;
//! /v1/* is gated by the bearer-token middleware.

pub mod health;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    middleware::{from_fn_with_state, Next},
    response::Response,
    Router,
};

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/health", axum::routing::get(health::get_health));

    let v1 = Router::new()
        .route("/v1/working", axum::routing::get(stub_working));

    let v1_with_auth = v1.route_layer(from_fn_with_state(state.clone(), bearer_auth));

    public.merge(v1_with_auth).with_state(state)
}

async fn bearer_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let presented = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match presented {
        Some(tok) if crate::auth::validate_token(&state.token, tok) => Ok(next.run(request).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

// Stub handler so the test passes; Task 9 replaces it with the real working route.
async fn stub_working() -> &'static str { "(working tier placeholder)" }
```

- [ ] **Step 6: Implement `crates/mnemos_daemon/src/routes/health.rs`**

```rust
use axum::Json;
use serde_json::{json, Value};

pub async fn get_health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "mnemosd",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
```

- [ ] **Step 7: Implement `build_app` in `crates/mnemos_daemon/src/lib.rs`**

Replace the contents of `lib.rs`:

```rust
//! Mnemos daemon: long-running HTTP + WebSocket + MCP server.

#![deny(rust_2018_idioms)]
#![warn(clippy::all)]

pub mod auth;
pub mod config;
pub mod error;
pub mod events;
pub mod routes;
pub mod state;

use anyhow::Result;
use mnemos_core::vault::Vault;

use crate::config::Config;
use crate::state::AppState;
use std::sync::Arc;

/// Construct the axum app + state. Used by `main.rs` and integration tests.
pub async fn build_app(config: Config, vault: Vault) -> Result<(axum::Router, AppState)> {
    let token_path = config_token_path()?;
    let token = auth::ensure_token(&token_path)?;
    let state = AppState {
        config: Arc::new(config),
        vault,
        token,
        events: events::EventBus::new(),
    };
    let app = routes::build_router(state.clone());
    Ok((app, state))
}

fn config_token_path() -> Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .ok_or_else(|| anyhow::anyhow!("could not resolve XDG config dir"))?;
    Ok(dirs.config_dir().join("token"))
}
```

- [ ] **Step 8: Run tests** → 3 pass.

- [ ] **Step 9: Verify** — fmt + clippy clean workspace-wide.

- [ ] **Step 10: Commit**

```bash
git add crates/mnemos_daemon/src/{lib.rs,state.rs,error.rs,events.rs,routes/} \
        crates/mnemos_daemon/tests/health.rs
git commit -m "feat(daemon): axum scaffold with Bearer auth middleware + /health"
```

---

## Task 6: REST memory endpoints (CRUD + recall + audit + time-travel)

**Files:**
- Create: `crates/mnemos_daemon/src/routes/memories.rs`
- Modify: `crates/mnemos_daemon/src/routes/mod.rs` (mount the memories router)
- Test: `crates/mnemos_daemon/tests/memories.rs`

- [ ] **Step 1: Write failing test**

```rust
use axum::http::StatusCode;
use mnemos_core::vault::Vault;
use mnemos_core::paths::Paths;
use mnemos_daemon::{config::Config, build_app};
use tempfile::TempDir;

async fn fixture() -> (axum::Router, String) {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    (app, state.token)
}

#[tokio::test]
async fn post_memories_then_get_round_trips() {
    let (app, token) = fixture().await;
    let (s, b) = call(app.clone(), "POST", "/v1/memories", Some(&token),
        r#"{"body":"hello world","title":"hi"}"#).await;
    assert_eq!(s, StatusCode::CREATED, "body: {b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let id = v["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("mem_"));

    let (s2, b2) = call(app, "GET", &format!("/v1/memories/{id}"), Some(&token), "").await;
    assert_eq!(s2, StatusCode::OK);
    let v2: serde_json::Value = serde_json::from_str(&b2).unwrap();
    assert_eq!(v2["title"], "hi");
}

#[tokio::test]
async fn delete_memories_id_invalidates() {
    let (app, token) = fixture().await;
    let (_, b) = call(app.clone(), "POST", "/v1/memories", Some(&token),
        r#"{"body":"doomed","title":"doomed"}"#).await;
    let id = serde_json::from_str::<serde_json::Value>(&b).unwrap()["id"].as_str().unwrap().to_string();
    let (s, _) = call(app.clone(), "DELETE", &format!("/v1/memories/{id}"), Some(&token), "").await;
    assert_eq!(s, StatusCode::OK);
    let (s2, b2) = call(app, "GET", &format!("/v1/memories/{id}"), Some(&token), "").await;
    let v2: serde_json::Value = serde_json::from_str(&b2).unwrap();
    assert_eq!(s2, StatusCode::OK);
    assert!(v2["invalid_at"].as_str().is_some());
}

#[tokio::test]
async fn post_memories_search_returns_hits() {
    let (app, token) = fixture().await;
    for body in ["Tauri desktop UI", "React JS framework"] {
        call(app.clone(), "POST", "/v1/memories", Some(&token),
            &format!(r#"{{"body":"{body}","title":"x"}}"#)).await;
    }
    let (s, b) = call(app, "POST", "/v1/memories/search", Some(&token),
        r#"{"query":"tauri","k":3}"#).await;
    assert_eq!(s, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let hits = v["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
}

#[tokio::test]
async fn get_memories_id_audit_returns_create_entry() {
    let (app, token) = fixture().await;
    let (_, b) = call(app.clone(), "POST", "/v1/memories", Some(&token),
        r#"{"body":"x","title":"x"}"#).await;
    let id = serde_json::from_str::<serde_json::Value>(&b).unwrap()["id"].as_str().unwrap().to_string();
    let (s, b2) = call(app, "GET", &format!("/v1/memories/{id}/audit"), Some(&token), "").await;
    assert_eq!(s, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&b2).unwrap();
    let entries = v["entries"].as_array().unwrap();
    assert!(entries.iter().any(|e| e["action"] == "create"));
}

// -- helpers (same shape as Task 5 test) --
async fn call(app: axum::Router, method: &str, uri: &str, auth: Option<&str>, body: &str) -> (StatusCode, String) {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder().method(method).uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = auth { req = req.header("authorization", format!("Bearer {t}")); }
    let req = req.body(Body::from(body.to_string())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).to_string())
}
```

- [ ] **Step 2: Run test to verify fail**

`cargo test -p mnemos_daemon --test memories` → FAIL on missing routes.

- [ ] **Step 3: Implement `crates/mnemos_daemon/src/routes/memories.rs`**

```rust
//! REST endpoints over the memory CRUD + retrieval surface.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use mnemos_core::retrieval::{hybrid::hybrid_recall, RecallOpts};
use mnemos_core::storage::audit::list_audit;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::vault::RememberOpts;
use mnemos_core::Tier;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/memories", post(post_memory).get(list_memories))
        .route("/v1/memories/search", post(search))
        .route("/v1/memories/time-travel", post(time_travel))
        .route("/v1/memories/{id}", get(get_memory).patch(patch_memory).delete(delete_memory))
        .route("/v1/memories/{id}/audit", get(audit))
}

#[derive(Debug, Deserialize)]
struct PostMemoryReq {
    body: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default = "default_tier")]
    tier: String,
    #[serde(default = "default_kind")]
    kind: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    importance: Option<f64>,
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    source_tool: Option<String>,
}

fn default_tier() -> String { "semantic".into() }
fn default_kind() -> String { "fact".into() }

#[derive(Debug, Serialize)]
struct PostMemoryResp { id: String }

async fn post_memory(
    State(state): State<AppState>,
    Json(req): Json<PostMemoryReq>,
) -> Result<(StatusCode, Json<PostMemoryResp>), ApiError> {
    let tier = Tier::from_str(&req.tier)
        .map_err(|e| ApiError::bad_request(format!("invalid tier: {e}")))?;
    let kind: MemoryType = serde_json::from_str(&format!("\"{}\"", req.kind))
        .map_err(|e| ApiError::bad_request(format!("invalid kind: {e}")))?;
    let id = state.vault.remember(&req.body, RememberOpts {
        title: req.title,
        tier,
        kind,
        tags: req.tags,
        importance: req.importance,
        workspace: req.workspace,
        source_tool: req.source_tool,
    }).await?;
    Ok((StatusCode::CREATED, Json(PostMemoryResp { id })))
}

async fn get_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Memory>, ApiError> {
    let mem = state.vault.get(&id).await?;
    Ok(Json(mem))
}

#[derive(Debug, Deserialize)]
struct PatchMemoryReq {
    #[serde(default)] tags: Option<Vec<String>>,
    #[serde(default)] importance: Option<f64>,
    // Body / title patch is intentionally NOT exposed in Plan 3 — files are source of truth.
    // External editors are the path to body updates; that triggers the watcher in Plan 1.
}

async fn patch_memory(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
    Json(_req): Json<PatchMemoryReq>,
) -> Result<StatusCode, ApiError> {
    // Plan 3 ships the route shape; field-by-field PATCH lands in Plan 4 with the
    // pipeline that touches files + DB transactionally. For now: bail.
    Err(ApiError::new(StatusCode::NOT_IMPLEMENTED, "PATCH lands in Plan 4 — use file edits for now"))
}

#[derive(Debug, Deserialize)]
struct DeleteQuery { #[serde(default)] reason: Option<String> }

async fn delete_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<DeleteQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.vault.forget(&id, q.reason.as_deref()).await?;
    Ok(Json(serde_json::json!({ "id": id, "status": "invalidated" })))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default)] tier: Option<Vec<String>>,
    #[serde(default)] workspace: Option<String>,
    #[serde(default)] include_invalid: bool,
    #[serde(default = "default_limit")] limit: usize,
}
fn default_limit() -> usize { 50 }

async fn list_memories(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tiers = q.tier.as_ref().map(|ts| {
        ts.iter().filter_map(|t| Tier::from_str(t).ok()).collect::<Vec<_>>()
    });
    let memories = state.vault.list(ListFilter {
        tiers, workspace: q.workspace, include_invalid: q.include_invalid, limit: Some(q.limit),
    }).await?;
    Ok(Json(serde_json::json!({ "memories": memories })))
}

#[derive(Debug, Deserialize)]
struct SearchReq {
    query: String,
    #[serde(default = "default_k")] k: usize,
    #[serde(default)] tier: Option<Vec<String>>,
    #[serde(default)] workspace: Option<String>,
    #[serde(default)] include_invalid: bool,
    #[serde(default)] explain: bool,
    #[serde(default)] rerank: bool,
}
fn default_k() -> usize { 10 }

async fn search(
    State(state): State<AppState>,
    Json(req): Json<SearchReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tiers = req.tier.as_ref().map(|ts| {
        ts.iter().filter_map(|t| Tier::from_str(t).ok()).collect::<Vec<_>>()
    });
    let opts = RecallOpts {
        k: req.k,
        tiers,
        workspace: req.workspace,
        include_invalid: req.include_invalid,
        explain: req.explain,
        rerank: req.rerank,
        ..Default::default()
    };
    let embedder = state.vault.embedder().cloned();
    let embedder_ref = embedder.as_deref();
    let hits = hybrid_recall(state.vault.storage(), embedder_ref, &req.query, opts).await?;
    Ok(Json(serde_json::json!({ "hits": hits })))
}

#[derive(Debug, Deserialize)]
struct TimeTravelReq {
    query: String,
    #[allow(dead_code)] as_of: String,   // Plan 4 wires this through; Plan 3 returns 501
    #[serde(default = "default_k")] k: usize,
}

async fn time_travel(
    State(_state): State<AppState>,
    Json(_req): Json<TimeTravelReq>,
) -> Result<StatusCode, ApiError> {
    Err(ApiError::new(StatusCode::NOT_IMPLEMENTED, "time-travel lands in Plan 4"))
}

async fn audit(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let entries = list_audit(state.vault.storage(), Some(&id)).await?;
    Ok(Json(serde_json::json!({ "entries": entries })))
}
```

- [ ] **Step 4: Mount `memories::router()` in `crates/mnemos_daemon/src/routes/mod.rs`**

Replace the existing `v1` block with a merged router:

```rust
    let v1 = Router::new()
        .merge(memories::router())
        .route("/v1/working", axum::routing::get(stub_working));
```

And add `pub mod memories;` at the top of `routes/mod.rs`.

- [ ] **Step 5: Run tests**

```bash
cargo test -p mnemos_daemon --test memories
cargo test --workspace
```

All pass.

- [ ] **Step 6: Verify** — fmt + clippy clean workspace-wide.

- [ ] **Step 7: Commit**

```bash
git add crates/mnemos_daemon/src/routes/{mod.rs,memories.rs} \
        crates/mnemos_daemon/tests/memories.rs
git commit -m "feat(daemon): REST memories endpoints (POST/GET/PATCH/DELETE/search/audit)"
```

---

## Task 7: REST session endpoints (placeholder shape; Plan 4 fills in extraction)

**Files:**
- Create: `crates/mnemos_daemon/src/routes/sessions.rs`
- Modify: `crates/mnemos_daemon/src/routes/mod.rs`
- Test: `crates/mnemos_daemon/tests/sessions.rs`

Plan 3 lands the endpoint *shape* (start/add_chunk/end/get) so MCP and UI can be wired now. The bodies write to `sessions` and `chunks` tables — both exist in v1 schema. Plan 4 wires the async extraction trigger off `end_session`.

- [ ] **Step 1: Write failing test**

```rust
use axum::http::StatusCode;
use mnemos_core::vault::Vault;
use mnemos_core::paths::Paths;
use mnemos_daemon::{config::Config, build_app};
use tempfile::TempDir;

async fn fixture() -> (axum::Router, String) {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    (app, state.token)
}

#[tokio::test]
async fn start_add_end_session_lifecycle() {
    let (app, token) = fixture().await;
    let (s, b) = call(app.clone(), "POST", "/v1/sessions", Some(&token),
        r#"{"source_tool":"claude-code","workspace":"/tmp/x"}"#).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let id = serde_json::from_str::<serde_json::Value>(&b).unwrap()["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("sess_"));

    let (s2, _) = call(app.clone(), "POST", &format!("/v1/sessions/{id}/chunks"), Some(&token),
        r#"{"speaker":"user","ordinal":1,"body":"hello"}"#).await;
    assert_eq!(s2, StatusCode::CREATED);

    let (s3, _) = call(app.clone(), "POST", &format!("/v1/sessions/{id}/end"), Some(&token),
        r#"{"summary":"test session"}"#).await;
    assert_eq!(s3, StatusCode::OK);

    let (s4, b4) = call(app, "GET", &format!("/v1/sessions/{id}"), Some(&token), "").await;
    assert_eq!(s4, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&b4).unwrap();
    assert_eq!(v["session"]["id"], id);
    assert_eq!(v["session"]["summary"], "test session");
    assert_eq!(v["chunks"].as_array().unwrap().len(), 1);
}

// reuse call() from Task 6's test pattern — copied here as it's a separate test crate file
async fn call(app: axum::Router, method: &str, uri: &str, auth: Option<&str>, body: &str) -> (StatusCode, String) {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder().method(method).uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = auth { req = req.header("authorization", format!("Bearer {t}")); }
    let req = req.body(Body::from(body.to_string())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let s = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (s, String::from_utf8_lossy(&bytes).to_string())
}
```

- [ ] **Step 2: Run test to verify fail**

`cargo test -p mnemos_daemon --test sessions` → FAIL.

- [ ] **Step 3: Implement `crates/mnemos_daemon/src/routes/sessions.rs`**

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use libsql::params;
use mnemos_core::id::{new_chunk_id, new_session_id};
use mnemos_core::types::{Chunk, Session};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/sessions", post(start_session))
        .route("/v1/sessions/{id}", get(get_session))
        .route("/v1/sessions/{id}/chunks", post(add_chunk))
        .route("/v1/sessions/{id}/end", post(end_session))
}

#[derive(Debug, Deserialize)]
struct StartSessionReq {
    #[serde(default)] source_tool: Option<String>,
    #[serde(default)] workspace: Option<String>,
}

#[derive(Debug, Serialize)]
struct StartSessionResp { id: String }

async fn start_session(
    State(state): State<AppState>,
    Json(req): Json<StartSessionReq>,
) -> Result<(StatusCode, Json<StartSessionResp>), ApiError> {
    let id = new_session_id();
    let (conn, _g) = state.vault.storage().write_conn().await?;
    conn.execute(
        "INSERT INTO sessions (id, source_tool, workspace, started_at) VALUES (?, ?, ?, ?)",
        params![id.clone(), req.source_tool, req.workspace, Utc::now().to_rfc3339()],
    ).await.map_err(mnemos_core::error::MnemosError::from)?;
    Ok((StatusCode::CREATED, Json(StartSessionResp { id })))
}

#[derive(Debug, Deserialize)]
struct AddChunkReq {
    #[serde(default)] speaker: Option<String>,
    #[serde(default)] ordinal: Option<u32>,
    body: String,
    #[serde(default)] source_meta: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct AddChunkResp { chunk_id: String }

async fn add_chunk(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<AddChunkReq>,
) -> Result<(StatusCode, Json<AddChunkResp>), ApiError> {
    let chunk_id = new_chunk_id();
    let ordinal = req.ordinal.unwrap_or(0);
    let source_meta_str = req.source_meta.as_ref().map(|v| v.to_string());
    let (conn, _g) = state.vault.storage().write_conn().await?;
    conn.execute(
        "INSERT INTO chunks (id, session_id, speaker, ordinal, body, created_at, source_meta)
            VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![
            chunk_id.clone(),
            session_id,
            req.speaker,
            ordinal as i64,
            req.body,
            Utc::now().to_rfc3339(),
            source_meta_str,
        ],
    ).await.map_err(mnemos_core::error::MnemosError::from)?;
    Ok((StatusCode::CREATED, Json(AddChunkResp { chunk_id })))
}

#[derive(Debug, Deserialize)]
struct EndSessionReq { #[serde(default)] summary: Option<String> }

async fn end_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<EndSessionReq>,
) -> Result<StatusCode, ApiError> {
    let (conn, _g) = state.vault.storage().write_conn().await?;
    let n = conn.execute(
        "UPDATE sessions SET ended_at = ?, summary = ? WHERE id = ?",
        params![Utc::now().to_rfc3339(), req.summary, id.clone()],
    ).await.map_err(mnemos_core::error::MnemosError::from)?;
    if n == 0 { return Err(ApiError::not_found(format!("session {id}"))); }
    // Plan 4 will trigger extraction here. For Plan 3 this is just a state update.
    Ok(StatusCode::OK)
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state.vault.storage().conn().map_err(mnemos_core::error::MnemosError::from)?;
    let mut rs = conn.query(
        "SELECT id, source_tool, workspace, started_at, ended_at, summary FROM sessions WHERE id = ?",
        params![id.clone()],
    ).await.map_err(mnemos_core::error::MnemosError::from)?;
    let row = rs.next().await.map_err(mnemos_core::error::MnemosError::from)?
        .ok_or_else(|| ApiError::not_found(format!("session {id}")))?;
    use chrono::DateTime;
    let session = Session {
        id: row.get(0).map_err(mnemos_core::error::MnemosError::from)?,
        source_tool: row.get(1).map_err(mnemos_core::error::MnemosError::from)?,
        workspace: row.get(2).map_err(mnemos_core::error::MnemosError::from)?,
        started_at: parse_ts(&row.get::<String>(3).map_err(mnemos_core::error::MnemosError::from)?)?,
        ended_at: row.get::<Option<String>>(4).map_err(mnemos_core::error::MnemosError::from)?
            .map(|s| parse_ts(&s)).transpose()?,
        summary: row.get(5).map_err(mnemos_core::error::MnemosError::from)?,
    };

    let mut cs = conn.query(
        "SELECT id, session_id, speaker, ordinal, body, created_at, source_tool, source_meta
            FROM chunks WHERE session_id = ? ORDER BY ordinal ASC",
        params![id.clone()],
    ).await.map_err(mnemos_core::error::MnemosError::from)?;
    let mut chunks: Vec<Chunk> = Vec::new();
    while let Some(r) = cs.next().await.map_err(mnemos_core::error::MnemosError::from)? {
        chunks.push(Chunk {
            id: r.get(0).map_err(mnemos_core::error::MnemosError::from)?,
            session_id: r.get(1).map_err(mnemos_core::error::MnemosError::from)?,
            speaker: r.get(2).map_err(mnemos_core::error::MnemosError::from)?,
            ordinal: r.get::<i64>(3).map_err(mnemos_core::error::MnemosError::from)? as u32,
            body: r.get(4).map_err(mnemos_core::error::MnemosError::from)?,
            created_at: parse_ts(&r.get::<String>(5).map_err(mnemos_core::error::MnemosError::from)?)?,
            source_tool: r.get(6).map_err(mnemos_core::error::MnemosError::from)?,
            source_meta: r.get::<Option<String>>(7).map_err(mnemos_core::error::MnemosError::from)?
                .map(|s| serde_json::from_str(&s)).transpose().map_err(|e| ApiError::internal(e.to_string()))?,
        });
    }

    Ok(Json(serde_json::json!({ "session": session, "chunks": chunks })))
}

fn parse_ts(s: &str) -> Result<chrono::DateTime<chrono::Utc>, ApiError> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&chrono::Utc))
        .map_err(|e| ApiError::internal(format!("bad ts '{s}': {e}")))
}
```

- [ ] **Step 4: Mount `sessions::router()` and add `pub mod sessions;` in `routes/mod.rs`**

```rust
pub mod memories;
pub mod sessions;
// ...
    let v1 = Router::new()
        .merge(memories::router())
        .merge(sessions::router())
        .route("/v1/working", axum::routing::get(stub_working));
```

- [ ] **Step 5: Run tests** → 1 pass.

- [ ] **Step 6: Verify** — fmt + clippy clean workspace-wide.

- [ ] **Step 7: Commit**

```bash
git add crates/mnemos_daemon/src/routes/{mod.rs,sessions.rs} \
        crates/mnemos_daemon/tests/sessions.rs
git commit -m "feat(daemon): REST session endpoints (start/add_chunk/end/get)"
```

---

## Task 8: REST entity + working tier endpoints

**Files:**
- Create: `crates/mnemos_daemon/src/routes/entities.rs`
- Create: `crates/mnemos_daemon/src/routes/working.rs`
- Modify: `crates/mnemos_daemon/src/routes/mod.rs` (remove stub_working; mount new routers)
- Test: `crates/mnemos_daemon/tests/entities_and_working.rs`

- [ ] **Step 1: Write failing test**

```rust
use axum::http::StatusCode;
use mnemos_core::vault::{Vault, RememberOpts};
use mnemos_core::paths::Paths;
use mnemos_core::Tier;
use mnemos_core::types::MemoryType;
use mnemos_daemon::{config::Config, build_app};
use tempfile::TempDir;

async fn fixture_with_working_memory() -> (axum::Router, String, String) {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let id = vault.remember("user is Shaun", RememberOpts {
        title: Some("identity".into()),
        tier: Tier::Working,
        kind: MemoryType::Identity,
        ..Default::default()
    }).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    (app, state.token, id)
}

#[tokio::test]
async fn get_v1_working_returns_working_memories() {
    let (app, token, id) = fixture_with_working_memory().await;
    let (s, b) = call(app, "GET", "/v1/working", Some(&token), "").await;
    assert_eq!(s, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let mems = v["memories"].as_array().unwrap();
    assert!(mems.iter().any(|m| m["id"] == id));
    assert!(mems.iter().all(|m| m["tier"] == "working"));
}

#[tokio::test]
async fn get_v1_entities_returns_list() {
    let (app, token, _) = fixture_with_working_memory().await;
    let (s, b) = call(app, "GET", "/v1/entities", Some(&token), "").await;
    assert_eq!(s, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert!(v["entities"].is_array());
}

async fn call(app: axum::Router, method: &str, uri: &str, auth: Option<&str>, body: &str) -> (StatusCode, String) {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder().method(method).uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = auth { req = req.header("authorization", format!("Bearer {t}")); }
    let req = req.body(Body::from(body.to_string())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let st = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (st, String::from_utf8_lossy(&bytes).to_string())
}
```

- [ ] **Step 2: Run test to verify fail**

`cargo test -p mnemos_daemon --test entities_and_working` → FAIL.

- [ ] **Step 3: Implement `crates/mnemos_daemon/src/routes/working.rs`**

```rust
use axum::{extract::State, routing::get, Json, Router};
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::Tier;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/working", get(get_working))
}

async fn get_working(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let memories = state.vault.list(ListFilter {
        tiers: Some(vec![Tier::Working]),
        workspace: None,
        include_invalid: false,
        limit: Some(64),
    }).await?;
    Ok(Json(serde_json::json!({ "memories": memories })))
}
```

- [ ] **Step 4: Implement `crates/mnemos_daemon/src/routes/entities.rs`**

```rust
//! Entity routes. Plan 3 ships the surface; Plan 4 (entity-linking pipeline)
//! and Plan 5 (PPR retrieval) populate it. For Plan 3 the list endpoint queries
//! the `entities` table directly — empty until Plan 4 starts writing rows.

use axum::{extract::{Path, Query, State}, routing::get, Json, Router};
use libsql::params;
use mnemos_core::types::Entity;
use serde::Deserialize;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/entities", get(list_entities))
        .route("/v1/entities/{id}", get(get_entity))
        .route("/v1/entities/{id}/graph", get(entity_graph_stub))
}

#[derive(Debug, Deserialize)]
struct ListQuery { #[serde(default = "default_limit")] limit: usize }
fn default_limit() -> usize { 50 }

async fn list_entities(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state.vault.storage().conn().map_err(mnemos_core::error::MnemosError::from)?;
    let mut rows = conn.query(
        "SELECT id, name, kind, aliases, description, file_path, created_at
            FROM entities ORDER BY created_at DESC LIMIT ?",
        params![q.limit as i64],
    ).await.map_err(mnemos_core::error::MnemosError::from)?;
    let mut entities: Vec<Entity> = Vec::new();
    while let Some(r) = rows.next().await.map_err(mnemos_core::error::MnemosError::from)? {
        let aliases_str: String = r.get(3).map_err(mnemos_core::error::MnemosError::from)?;
        let aliases: Vec<String> = serde_json::from_str(&aliases_str).unwrap_or_default();
        let created_at_str: String = r.get(6).map_err(mnemos_core::error::MnemosError::from)?;
        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|d| d.with_timezone(&chrono::Utc))
            .map_err(|e| ApiError::internal(e.to_string()))?;
        entities.push(Entity {
            id: r.get(0).map_err(mnemos_core::error::MnemosError::from)?,
            name: r.get(1).map_err(mnemos_core::error::MnemosError::from)?,
            kind: r.get(2).map_err(mnemos_core::error::MnemosError::from)?,
            aliases,
            description: r.get(4).map_err(mnemos_core::error::MnemosError::from)?,
            file_path: r.get(5).map_err(mnemos_core::error::MnemosError::from)?,
            created_at,
        });
    }
    Ok(Json(serde_json::json!({ "entities": entities })))
}

async fn get_entity(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state.vault.storage().conn().map_err(mnemos_core::error::MnemosError::from)?;
    let mut rows = conn.query(
        "SELECT id, name, kind, aliases, description, file_path, created_at
            FROM entities WHERE id = ?",
        params![id.clone()],
    ).await.map_err(mnemos_core::error::MnemosError::from)?;
    match rows.next().await.map_err(mnemos_core::error::MnemosError::from)? {
        Some(r) => Ok(Json(serde_json::json!({
            "id": r.get::<String>(0).map_err(mnemos_core::error::MnemosError::from)?,
            "name": r.get::<String>(1).map_err(mnemos_core::error::MnemosError::from)?,
            "kind": r.get::<String>(2).map_err(mnemos_core::error::MnemosError::from)?,
        }))),
        None => Err(ApiError::not_found(format!("entity {id}"))),
    }
}

async fn entity_graph_stub(
    State(_): State<AppState>,
    Path(_): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Plan 5 (PPR) populates this. Plan 3 returns an empty graph.
    Ok(Json(serde_json::json!({ "nodes": [], "edges": [] })))
}
```

- [ ] **Step 5: Replace the stub_working route in `crates/mnemos_daemon/src/routes/mod.rs`**

Update mod.rs:

```rust
pub mod entities;
pub mod health;
pub mod memories;
pub mod sessions;
pub mod working;

// ... bearer_auth unchanged

pub fn build_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/health", axum::routing::get(health::get_health));

    let v1 = Router::new()
        .merge(memories::router())
        .merge(sessions::router())
        .merge(entities::router())
        .merge(working::router());

    let v1_with_auth = v1.route_layer(from_fn_with_state(state.clone(), bearer_auth));

    public.merge(v1_with_auth).with_state(state)
}
```

Delete the `stub_working` function — no longer needed.

- [ ] **Step 6: Run tests**

```bash
cargo test -p mnemos_daemon --test entities_and_working
cargo test --workspace
```

All pass.

- [ ] **Step 7: Verify** — fmt + clippy clean workspace-wide.

- [ ] **Step 8: Commit**

```bash
git add crates/mnemos_daemon/src/routes/{mod.rs,working.rs,entities.rs} \
        crates/mnemos_daemon/tests/entities_and_working.rs
git commit -m "feat(daemon): REST endpoints — /v1/working, /v1/entities, /v1/entities/{id}/graph stub"
```

---

## Task 9: WebSocket event bus

**Files:**
- Modify: `crates/mnemos_daemon/src/events.rs` (replace stub with real bus)
- Create: `crates/mnemos_daemon/src/routes/ws.rs`
- Modify: `crates/mnemos_daemon/src/routes/mod.rs` (mount /v1/events)
- Modify: `crates/mnemos_daemon/src/routes/memories.rs` (emit events on create/delete)
- Test: `crates/mnemos_daemon/tests/ws.rs`

- [ ] **Step 1: Write failing test**

```rust
use futures::{SinkExt, StreamExt};
use mnemos_core::vault::Vault;
use mnemos_core::paths::Paths;
use mnemos_daemon::{config::Config, build_app, events::Event};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::net::TcpListener;

#[tokio::test]
async fn ws_receives_memory_created_event() {
    // We need a real TCP socket for WebSocket; axum's oneshot won't do.
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    let token = state.token.clone();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app.into_make_service()).await.unwrap();
    });

    // Subscribe to events
    let url = format!("ws://{addr}/v1/events?token={token}");
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    // Trigger an event via REST
    let client = reqwest::Client::new();
    let resp = client.post(format!("http://{addr}/v1/memories"))
        .header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "body": "ws test", "title": "ws" }))
        .send().await.unwrap();
    assert!(resp.status().is_success());

    // Receive the event
    let frame = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        ws.next(),
    ).await.expect("ws frame within 2s").unwrap().unwrap();
    let text = frame.into_text().unwrap();
    let event: Event = serde_json::from_str(&text).unwrap();
    assert!(matches!(event, Event::MemoryCreated { .. }));

    let _ = ws.close(None).await;
    server.abort();
    drop(Arc::new(state));
}
```

Add to `crates/mnemos_daemon/Cargo.toml` dev-deps (if not already):

```toml
tokio-tungstenite = "0.24"
futures = { workspace = true }
```

NOTE: `reqwest` is already a dev-dep from Task 5. `futures` is a workspace dep added in Task 1.

- [ ] **Step 2: Run test to verify fail**

`cargo test -p mnemos_daemon --test ws` → FAIL — `events::Event` doesn't exist; `/v1/events` doesn't route.

- [ ] **Step 3: Replace `crates/mnemos_daemon/src/events.rs`**

```rust
//! Event bus broadcasts typed events to all connected WebSocket subscribers.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    MemoryCreated { id: String, title: String, tier: String },
    MemoryUpdated { id: String },
    MemoryInvalidated { id: String, reason: Option<String> },
    SessionStarted { id: String },
    SessionEnded { id: String },
}

#[derive(Clone)]
pub struct EventBus {
    tx: Arc<broadcast::Sender<Event>>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx: Arc::new(tx) }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
    pub fn publish(&self, e: Event) {
        let _ = self.tx.send(e);
    }
}

impl Default for EventBus {
    fn default() -> Self { Self::new() }
}
```

- [ ] **Step 4: Implement `crates/mnemos_daemon/src/routes/ws.rs`**

```rust
//! WebSocket route. Auth is via `?token=...` query string (WS clients can't always set headers).

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::auth::validate_token;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/events", get(ws_handler))
}

#[derive(Debug, Deserialize)]
struct WsQuery { token: String }

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(q): Query<WsQuery>,
) -> impl IntoResponse {
    if !validate_token(&state.token, &q.token) {
        return (axum::http::StatusCode::UNAUTHORIZED, "bad token").into_response();
    }
    ws.on_upgrade(|socket| socket_loop(socket, state))
}

async fn socket_loop(mut socket: WebSocket, state: AppState) {
    let mut rx = state.events.subscribe();
    loop {
        tokio::select! {
            // From client: ignore content, just keep the connection alive.
            client_msg = socket.recv() => match client_msg {
                Some(Ok(_)) => continue,
                _ => break,
            },
            // To client: forward events.
            evt = rx.recv() => match evt {
                Ok(e) => {
                    let text = serde_json::to_string(&e).unwrap_or_default();
                    if socket.send(Message::Text(text.into())).await.is_err() { break; }
                }
                Err(_) => break,
            }
        }
    }
}
```

- [ ] **Step 5: Mount `/v1/events` AND emit events from memory routes**

In `routes/mod.rs`, add `pub mod ws;` and merge into `v1`:

```rust
    let v1 = Router::new()
        .merge(memories::router())
        .merge(sessions::router())
        .merge(entities::router())
        .merge(working::router())
        .merge(ws::router());
```

NOTE: `/v1/events` skips the bearer middleware because auth happens via the `token=` query (axum middleware would 401 before the handler can read the query). The handler does the auth check inline. This is documented in the route module.

In `routes/memories.rs::post_memory`, after the successful `state.vault.remember(...)` call, emit an event:

```rust
    let id = state.vault.remember(/* ... */).await?;
    state.events.publish(crate::events::Event::MemoryCreated {
        id: id.clone(),
        title: /* the title actually stored — fetch back or pass through */,
        tier: req.tier.clone(),
    });
    Ok((StatusCode::CREATED, Json(PostMemoryResp { id })))
```

The cleanest way: fetch the memory back with `state.vault.get(&id)` to get the title (which may be auto-generated). Or pre-compute the title from `req.title.clone().unwrap_or_else(|| /* auto_title equivalent */)`. For Plan 3 simplicity, fetch back:

```rust
    let id = state.vault.remember(/* ... */).await?;
    let mem = state.vault.get(&id).await?;
    state.events.publish(crate::events::Event::MemoryCreated {
        id: id.clone(),
        title: mem.title.clone(),
        tier: mem.tier.as_str().to_string(),
    });
    Ok((StatusCode::CREATED, Json(PostMemoryResp { id })))
```

In `delete_memory`:

```rust
    state.vault.forget(&id, q.reason.as_deref()).await?;
    state.events.publish(crate::events::Event::MemoryInvalidated {
        id: id.clone(),
        reason: q.reason.clone(),
    });
    Ok(Json(serde_json::json!({ "id": id, "status": "invalidated" })))
```

In `routes/sessions.rs::start_session` and `end_session`, emit `SessionStarted` / `SessionEnded` similarly.

- [ ] **Step 6: Run tests** → 1 pass (plus all prior tests).

- [ ] **Step 7: Verify** — fmt + clippy clean workspace-wide.

- [ ] **Step 8: Commit**

```bash
git add crates/mnemos_daemon/src/{events.rs,routes/} crates/mnemos_daemon/tests/ws.rs \
        crates/mnemos_daemon/Cargo.toml
git commit -m "feat(daemon): WebSocket event bus + emit events from memory + session routes"
```

---

## Task 10: Daemon `main.rs` — serve the router

**Files:**
- Modify: `crates/mnemos_daemon/src/main.rs`

This task makes the binary actually serve traffic. Tested via integration smoke in Task 17, plus an explicit test here.

- [ ] **Step 1: Write failing test**

`crates/mnemos_daemon/tests/serve.rs`:

```rust
use mnemos_core::vault::Vault;
use mnemos_core::paths::Paths;
use mnemos_daemon::{build_app, config::Config, serve};
use tempfile::TempDir;

#[tokio::test]
async fn serve_binds_and_responds_to_health() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let mut cfg = Config::default();
    cfg.daemon.port = 0;   // OS-chosen port
    let (app, _state) = build_app(cfg.clone(), vault).await.unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(serve(listener, app));

    let body = reqwest::get(format!("http://{addr}/health")).await.unwrap()
        .text().await.unwrap();
    assert!(body.contains("\"status\":\"ok\""));

    handle.abort();
}
```

- [ ] **Step 2: Run test to verify fail**

`cargo test -p mnemos_daemon --test serve` → FAIL — `serve` doesn't exist.

- [ ] **Step 3: Add `pub async fn serve(...)` to `crates/mnemos_daemon/src/lib.rs`**

```rust
/// Block on the axum service until the listener errors or the future is dropped.
pub async fn serve(listener: tokio::net::TcpListener, app: axum::Router) -> anyhow::Result<()> {
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
```

- [ ] **Step 4: Replace `crates/mnemos_daemon/src/main.rs`**

```rust
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_daemon::config::{Config, EmbedderKind};
use mnemos_daemon::{build_app, serve};
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "mnemosd", version, about = "Mnemos daemon")]
struct Cli {
    /// Path to config.toml (default: XDG)
    #[arg(long)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Start the daemon (default if no subcommand given).
    Serve,
    /// Print the resolved config and exit.
    PrintConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    let cfg = match args.config.as_ref() {
        Some(p) => Config::load_from(p),
        None => Config::load_default(),
    }?;

    init_tracing(&cfg.logging);

    match args.command.unwrap_or(Cmd::Serve) {
        Cmd::Serve => serve_cmd(cfg).await,
        Cmd::PrintConfig => {
            println!("{}", toml::to_string_pretty(&cfg).unwrap_or_else(|e| e.to_string()));
            Ok(())
        }
    }
}

fn init_tracing(cfg: &mnemos_daemon::config::LoggingConfig) {
    let filter = EnvFilter::try_new(&cfg.level).unwrap_or_else(|_| EnvFilter::new("info"));
    let sub = tracing_subscriber::fmt().with_env_filter(filter);
    if cfg.format == "json" {
        sub.json().init();
    } else {
        sub.compact().init();
    }
}

async fn serve_cmd(cfg: Config) -> Result<()> {
    let paths = Paths::with_root(&cfg.vault.root);
    let embedder = build_embedder_for_daemon(&cfg)?;
    let vault = Vault::open_with_embedder(paths, embedder).await
        .context("opening vault")?;
    let bind = format!("{}:{}", cfg.daemon.host, cfg.daemon.port);
    let listener = tokio::net::TcpListener::bind(&bind).await
        .with_context(|| format!("bind {bind}"))?;
    tracing::info!(addr = %listener.local_addr()?, "mnemosd listening");
    let (app, _state) = build_app(cfg, vault).await?;
    serve(listener, app).await
}

fn build_embedder_for_daemon(cfg: &Config) -> Result<Option<Arc<dyn mnemos_core::providers::Embedder>>> {
    use mnemos_core::providers::{mock::MockEmbedder, ollama::{OllamaConfig, OllamaEmbedder}};
    Ok(match cfg.embedder.kind {
        EmbedderKind::None => None,
        EmbedderKind::Mock => Some(Arc::new(MockEmbedder::new(cfg.embedder.dim))),
        EmbedderKind::Ollama => {
            let oc = OllamaConfig {
                base_url: cfg.embedder.url.clone(),
                model: cfg.embedder.model.clone(),
                dim: cfg.embedder.dim,
                timeout_secs: cfg.embedder.timeout_secs,
            };
            Some(Arc::new(OllamaEmbedder::new(oc)))
        }
    })
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p mnemos_daemon --test serve
cargo test --workspace
```

Both pass.

- [ ] **Step 6: Verify** — fmt + clippy clean. Smoke:

```bash
cargo run -p mnemos_daemon -- print-config | head -10
```

Should print the resolved config as TOML.

- [ ] **Step 7: Commit**

```bash
git add crates/mnemos_daemon/src/{main.rs,lib.rs} crates/mnemos_daemon/tests/serve.rs
git commit -m "feat(daemon): mnemosd binary — serve and print-config subcommands"
```

---

## Task 11: MCP — HTTP transport scaffold + tool surface

**Files:**
- Create: `crates/mnemos_daemon/src/mcp/mod.rs`
- Create: `crates/mnemos_daemon/src/mcp/protocol.rs`
- Create: `crates/mnemos_daemon/src/mcp/tools.rs`
- Modify: `crates/mnemos_daemon/src/routes/mod.rs` (mount /mcp)
- Test: `crates/mnemos_daemon/tests/mcp.rs`

MCP over Streamable HTTP is a documented JSON-RPC 2.0-shaped protocol. For Plan 3 we implement just enough of it to (a) advertise `tools/list`, (b) handle `tools/call`, and (c) handle `initialize`. Plan 4 adds `sampling/createMessage` (calling-client extraction) and `resources/*` lookups. The `rmcp` crate is intentionally NOT used here — its surface is still volatile; rolling our own keeps Plan 3 self-contained and well-tested.

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::vault::Vault;
use mnemos_core::paths::Paths;
use mnemos_daemon::{config::Config, build_app};
use tempfile::TempDir;

async fn fixture() -> (axum::Router, String) {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    (app, state.token)
}

#[tokio::test]
async fn mcp_initialize_returns_capabilities() {
    let (app, token) = fixture().await;
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#;
    let (s, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    assert_eq!(s, 200);
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["jsonrpc"], "2.0");
    assert_eq!(v["id"], 1);
    assert!(v["result"]["serverInfo"]["name"].is_string());
    assert!(v["result"]["capabilities"]["tools"].is_object());
}

#[tokio::test]
async fn mcp_tools_list_returns_remember_recall_forget() {
    let (app, token) = fixture().await;
    let body = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let tools = v["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"remember"));
    assert!(names.contains(&"recall"));
    assert!(names.contains(&"forget"));
    assert!(names.contains(&"list_memories"));
    assert!(names.contains(&"get_memory"));
}

#[tokio::test]
async fn mcp_tools_call_remember_then_recall() {
    let (app, token) = fixture().await;
    // call remember
    let body = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"remember","arguments":{"body":"Tauri preference","title":"Tauri"}}}"#;
    let (_, b) = call(app.clone(), "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let result = &v["result"];
    let content = &result["content"][0]["text"];
    let id_json: serde_json::Value = serde_json::from_str(content.as_str().unwrap()).unwrap();
    assert!(id_json["id"].as_str().unwrap().starts_with("mem_"));

    // call recall
    let body2 = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"recall","arguments":{"query":"tauri","k":3}}}"#;
    let (_, b2) = call(app, "POST", "/mcp", Some(&token), body2).await;
    let v2: serde_json::Value = serde_json::from_str(&b2).unwrap();
    let content2 = v2["result"]["content"][0]["text"].as_str().unwrap();
    let hits_json: serde_json::Value = serde_json::from_str(content2).unwrap();
    assert!(!hits_json["hits"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn mcp_unknown_method_returns_method_not_found_error() {
    let (app, token) = fixture().await;
    let body = r#"{"jsonrpc":"2.0","id":5,"method":"sub-zero/finish-him"}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["error"]["code"], -32601);
}

async fn call(app: axum::Router, method: &str, uri: &str, auth: Option<&str>, body: &str) -> (u16, String) {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder().method(method).uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = auth { req = req.header("authorization", format!("Bearer {t}")); }
    let req = req.body(Body::from(body.to_string())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let s = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (s, String::from_utf8_lossy(&bytes).to_string())
}
```

- [ ] **Step 2: Run test to verify fail**

`cargo test -p mnemos_daemon --test mcp` → FAIL — no /mcp route, no tool impls.

- [ ] **Step 3: Implement `crates/mnemos_daemon/src/mcp/protocol.rs`**

```rust
//! Minimal JSON-RPC 2.0 + MCP types. Only what Plan 3 needs.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self { jsonrpc: "2.0", id, result: Some(result), error: None }
    }
    pub fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError { code, message: message.into(), data: None }),
        }
    }
}

// Standard JSON-RPC error codes
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;
```

- [ ] **Step 4: Implement `crates/mnemos_daemon/src/mcp/tools.rs`**

```rust
//! MCP tool implementations. Each wraps the relevant Vault/retrieval call.

use mnemos_core::retrieval::{hybrid::hybrid_recall, RecallOpts};
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::types::MemoryType;
use mnemos_core::vault::RememberOpts;
use mnemos_core::Tier;
use serde_json::{json, Value};
use std::str::FromStr;

use crate::state::AppState;

/// Returns the MCP tool descriptors. Schemas are JSON Schema 2020-12.
pub fn descriptors() -> Vec<Value> {
    vec![
        json!({
            "name": "remember",
            "description": "Store a new memory. Returns its id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "body": { "type": "string" },
                    "title": { "type": "string" },
                    "tier": { "type": "string", "enum": ["working","episodic","semantic","procedural","reflection"], "default": "semantic" },
                    "kind": { "type": "string", "default": "fact" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "importance": { "type": "number" },
                    "workspace": { "type": "string" },
                    "source_tool": { "type": "string" }
                },
                "required": ["body"]
            }
        }),
        json!({
            "name": "recall",
            "description": "Hybrid search (BM25 + dense). Returns ranked hits.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "k": { "type": "integer", "default": 10 },
                    "tier": { "type": "array", "items": { "type": "string" } },
                    "workspace": { "type": "string" },
                    "include_invalid": { "type": "boolean", "default": false },
                    "explain": { "type": "boolean", "default": false },
                    "rerank": { "type": "boolean", "default": false }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "forget",
            "description": "Soft-invalidate a memory by id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string" },
                    "reason": { "type": "string" }
                },
                "required": ["memory_id"]
            }
        }),
        json!({
            "name": "get_memory",
            "description": "Fetch a single memory by id.",
            "inputSchema": {
                "type": "object",
                "properties": { "memory_id": { "type": "string" } },
                "required": ["memory_id"]
            }
        }),
        json!({
            "name": "list_memories",
            "description": "List memories with optional filters.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tier": { "type": "array", "items": { "type": "string" } },
                    "workspace": { "type": "string" },
                    "include_invalid": { "type": "boolean", "default": false },
                    "limit": { "type": "integer", "default": 50 }
                }
            }
        }),
    ]
}

/// Dispatch a tool call. Returns the MCP `content` array.
pub async fn call(state: &AppState, name: &str, args: &Value) -> anyhow::Result<Value> {
    match name {
        "remember" => remember(state, args).await,
        "recall" => recall(state, args).await,
        "forget" => forget(state, args).await,
        "get_memory" => get_memory(state, args).await,
        "list_memories" => list_memories(state, args).await,
        other => Err(anyhow::anyhow!("unknown tool: {other}")),
    }
}

async fn remember(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    let body = args["body"].as_str().ok_or_else(|| anyhow::anyhow!("body required"))?.to_string();
    let tier_str = args["tier"].as_str().unwrap_or("semantic");
    let tier = Tier::from_str(tier_str).map_err(|e| anyhow::anyhow!("invalid tier: {e}"))?;
    let kind_str = args["kind"].as_str().unwrap_or("fact");
    let kind: MemoryType = serde_json::from_str(&format!("\"{kind_str}\""))?;
    let id = state.vault.remember(&body, RememberOpts {
        title: args["title"].as_str().map(String::from),
        tier,
        kind,
        tags: args["tags"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
        importance: args["importance"].as_f64(),
        workspace: args["workspace"].as_str().map(String::from),
        source_tool: args["source_tool"].as_str().map(String::from),
    }).await?;
    Ok(tool_content_json(json!({ "id": id })))
}

async fn recall(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    let query = args["query"].as_str().ok_or_else(|| anyhow::anyhow!("query required"))?;
    let k = args["k"].as_u64().unwrap_or(10) as usize;
    let tiers = args["tier"].as_array().map(|a| {
        a.iter().filter_map(|v| v.as_str()).filter_map(|s| Tier::from_str(s).ok()).collect()
    });
    let opts = RecallOpts {
        k,
        tiers,
        workspace: args["workspace"].as_str().map(String::from),
        include_invalid: args["include_invalid"].as_bool().unwrap_or(false),
        explain: args["explain"].as_bool().unwrap_or(false),
        rerank: args["rerank"].as_bool().unwrap_or(false),
        ..Default::default()
    };
    let embedder = state.vault.embedder().cloned();
    let embedder_ref = embedder.as_deref();
    let hits = hybrid_recall(state.vault.storage(), embedder_ref, query, opts).await?;
    Ok(tool_content_json(json!({ "hits": hits })))
}

async fn forget(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    let id = args["memory_id"].as_str().ok_or_else(|| anyhow::anyhow!("memory_id required"))?;
    let reason = args["reason"].as_str();
    state.vault.forget(id, reason).await?;
    Ok(tool_content_json(json!({ "id": id, "status": "invalidated" })))
}

async fn get_memory(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    let id = args["memory_id"].as_str().ok_or_else(|| anyhow::anyhow!("memory_id required"))?;
    let mem = state.vault.get(id).await?;
    Ok(tool_content_json(serde_json::to_value(mem)?))
}

async fn list_memories(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    let tiers = args["tier"].as_array().map(|a| {
        a.iter().filter_map(|v| v.as_str()).filter_map(|s| Tier::from_str(s).ok()).collect()
    });
    let memories = state.vault.list(ListFilter {
        tiers,
        workspace: args["workspace"].as_str().map(String::from),
        include_invalid: args["include_invalid"].as_bool().unwrap_or(false),
        limit: args["limit"].as_u64().map(|n| n as usize),
    }).await?;
    Ok(tool_content_json(json!({ "memories": memories })))
}

fn tool_content_json(value: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": value.to_string()
        }]
    })
}
```

- [ ] **Step 5: Implement `crates/mnemos_daemon/src/mcp/mod.rs`**

```rust
pub mod protocol;
pub mod tools;

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde_json::{json, Value};

use crate::mcp::protocol::{
    JsonRpcRequest, JsonRpcResponse, INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST,
    METHOD_NOT_FOUND,
};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/mcp", post(handle))
}

async fn handle(
    State(state): State<AppState>,
    Json(req): Json<JsonRpcRequest>,
) -> (StatusCode, Json<JsonRpcResponse>) {
    if req.jsonrpc != "2.0" {
        return (
            StatusCode::OK,
            Json(JsonRpcResponse::error(req.id, INVALID_REQUEST, "jsonrpc must be '2.0'")),
        );
    }
    let resp = match req.method.as_str() {
        "initialize" => initialize(),
        "tools/list" => tools_list(),
        "tools/call" => tools_call(&state, req.params.as_ref().unwrap_or(&Value::Null)).await,
        other => JsonRpcResponse::error(req.id.clone(), METHOD_NOT_FOUND, format!("unknown method: {other}")),
    };
    let mut resp = resp;
    if resp.id.is_none() { resp.id = req.id; }
    (StatusCode::OK, Json(resp))
}

fn initialize() -> JsonRpcResponse {
    JsonRpcResponse::success(None, json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {
            "tools": { "listChanged": false },
            // resources + prompts populated in Task 12; advertised here but stubbed.
            "resources": { "listChanged": false },
            "prompts": { "listChanged": false }
        },
        "serverInfo": { "name": "mnemos", "version": env!("CARGO_PKG_VERSION") }
    }))
}

fn tools_list() -> JsonRpcResponse {
    JsonRpcResponse::success(None, json!({ "tools": tools::descriptors() }))
}

async fn tools_call(state: &AppState, params: &Value) -> JsonRpcResponse {
    let name = match params["name"].as_str() {
        Some(n) => n,
        None => return JsonRpcResponse::error(None, INVALID_PARAMS, "tools/call requires 'name'"),
    };
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    match tools::call(state, name, &args).await {
        Ok(result) => JsonRpcResponse::success(None, result),
        Err(e) => JsonRpcResponse::error(None, INTERNAL_ERROR, e.to_string()),
    }
}
```

- [ ] **Step 6: Mount /mcp in `routes/mod.rs`**

```rust
// Add to top of mod.rs imports:
use crate::mcp;
```

And add `pub mod mcp;` to `lib.rs`.

Then in `build_router`, add the mcp route to `v1`. But MCP is conventionally NOT under `/v1/` — the spec uses `/mcp`. Mount it alongside the public routes BUT with auth:

```rust
    let public = Router::new()
        .route("/health", axum::routing::get(health::get_health));

    let v1 = Router::new()
        .merge(memories::router())
        .merge(sessions::router())
        .merge(entities::router())
        .merge(working::router());

    let mcp_routes = mcp::router();

    let authed = v1.merge(mcp_routes).route_layer(from_fn_with_state(state.clone(), bearer_auth));

    public.merge(authed).merge(ws::router().with_state(state.clone()))
        .with_state(state)
```

Hmm — the existing structure with `with_state(state)` at the bottom complicates layering. Adjust to apply `with_state` last, after merges. Concretely:

```rust
pub fn build_router(state: AppState) -> Router {
    let public: Router<AppState> = Router::new()
        .route("/health", axum::routing::get(health::get_health));

    let ws_router: Router<AppState> = ws::router();   // auth via query param, not middleware

    let authed: Router<AppState> = Router::new()
        .merge(memories::router())
        .merge(sessions::router())
        .merge(entities::router())
        .merge(working::router())
        .merge(crate::mcp::router())
        .route_layer(from_fn_with_state(state.clone(), bearer_auth));

    public.merge(authed).merge(ws_router).with_state(state)
}
```

- [ ] **Step 7: Run tests**

```bash
cargo test -p mnemos_daemon --test mcp
cargo test --workspace
```

All pass.

- [ ] **Step 8: Verify** — fmt + clippy clean workspace-wide.

- [ ] **Step 9: Commit**

```bash
git add crates/mnemos_daemon/src/{lib.rs,mcp/,routes/mod.rs} crates/mnemos_daemon/tests/mcp.rs
git commit -m "feat(daemon): MCP over Streamable HTTP — initialize + tools/list + tools/call"
```

---

## Task 12: MCP resources + prompts

**Files:**
- Create: `crates/mnemos_daemon/src/mcp/resources.rs`
- Create: `crates/mnemos_daemon/src/mcp/prompts.rs`
- Modify: `crates/mnemos_daemon/src/mcp/mod.rs` (route `resources/*` and `prompts/*`)
- Test: `crates/mnemos_daemon/tests/mcp_resources.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::vault::{Vault, RememberOpts};
use mnemos_core::paths::Paths;
use mnemos_core::Tier;
use mnemos_core::types::MemoryType;
use mnemos_daemon::{config::Config, build_app};
use tempfile::TempDir;

async fn fixture_with_working_mem() -> (axum::Router, String) {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    vault.remember("user is Shaun", RememberOpts {
        title: Some("identity".into()),
        tier: Tier::Working,
        kind: MemoryType::Identity,
        ..Default::default()
    }).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    (app, state.token)
}

#[tokio::test]
async fn mcp_resources_list_includes_working() {
    let (app, token) = fixture_with_working_mem().await;
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let resources = v["result"]["resources"].as_array().unwrap();
    assert!(resources.iter().any(|r| r["uri"] == "mnemos://working"));
    assert!(resources.iter().any(|r| r["uri"] == "mnemos://recent"));
}

#[tokio::test]
async fn mcp_resources_read_working_returns_working_memories() {
    let (app, token) = fixture_with_working_mem().await;
    let body = r#"{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"mnemos://working"}}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let contents = v["result"]["contents"].as_array().unwrap();
    let first_text = contents[0]["text"].as_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(first_text).unwrap();
    let mems = parsed["memories"].as_array().unwrap();
    assert!(mems.iter().any(|m| m["title"] == "identity"));
}

#[tokio::test]
async fn mcp_prompts_list_includes_context_for() {
    let (app, token) = fixture_with_working_mem().await;
    let body = r#"{"jsonrpc":"2.0","id":3,"method":"prompts/list"}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let names: Vec<&str> = v["result"]["prompts"].as_array().unwrap().iter()
        .map(|p| p["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"context-for"));
}

#[tokio::test]
async fn mcp_prompts_get_context_for_returns_messages() {
    let (app, token) = fixture_with_working_mem().await;
    let body = r#"{"jsonrpc":"2.0","id":4,"method":"prompts/get","params":{"name":"context-for","arguments":{"workspace":"any"}}}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let msgs = v["result"]["messages"].as_array().unwrap();
    assert!(!msgs.is_empty());
    assert_eq!(msgs[0]["role"], "system");
    let text = msgs[0]["content"]["text"].as_str().unwrap();
    assert!(text.contains("user is Shaun"));
}

async fn call(app: axum::Router, method: &str, uri: &str, auth: Option<&str>, body: &str) -> (u16, String) {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder().method(method).uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = auth { req = req.header("authorization", format!("Bearer {t}")); }
    let req = req.body(Body::from(body.to_string())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let s = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (s, String::from_utf8_lossy(&bytes).to_string())
}
```

- [ ] **Step 2: Run test, verify FAIL**

`cargo test -p mnemos_daemon --test mcp_resources` → FAIL.

- [ ] **Step 3: Implement `crates/mnemos_daemon/src/mcp/resources.rs`**

```rust
//! MCP resource handlers.
//!
//! Plan 3 ships three resources:
//!   - mnemos://working      → full working tier
//!   - mnemos://recent       → last 20 memories created
//!   - mnemos://memory/{id}  → single memory by id (Plan 5+ extends with entity, session)

use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::Tier;
use serde_json::{json, Value};

use crate::state::AppState;

pub fn list_descriptors() -> Vec<Value> {
    vec![
        json!({
            "uri": "mnemos://working",
            "name": "Working memory",
            "description": "Always-loaded memories (identity, current projects, hard constraints).",
            "mimeType": "application/json",
        }),
        json!({
            "uri": "mnemos://recent",
            "name": "Recent memories",
            "description": "Last 20 memories created across all tiers.",
            "mimeType": "application/json",
        }),
    ]
}

pub async fn read(state: &AppState, uri: &str) -> anyhow::Result<Value> {
    if uri == "mnemos://working" {
        let memories = state.vault.list(ListFilter {
            tiers: Some(vec![Tier::Working]),
            include_invalid: false,
            limit: Some(64),
            ..Default::default()
        }).await?;
        return Ok(content_json(uri, json!({ "memories": memories })));
    }
    if uri == "mnemos://recent" {
        let memories = state.vault.list(ListFilter {
            limit: Some(20),
            ..Default::default()
        }).await?;
        return Ok(content_json(uri, json!({ "memories": memories })));
    }
    if let Some(id) = uri.strip_prefix("mnemos://memory/") {
        let mem = state.vault.get(id).await?;
        return Ok(content_json(uri, serde_json::to_value(mem)?));
    }
    Err(anyhow::anyhow!("unknown resource uri: {uri}"))
}

fn content_json(uri: &str, value: Value) -> Value {
    json!({
        "contents": [{
            "uri": uri,
            "mimeType": "application/json",
            "text": value.to_string()
        }]
    })
}
```

- [ ] **Step 4: Implement `crates/mnemos_daemon/src/mcp/prompts.rs`**

```rust
//! MCP prompt templates.
//!
//! `context-for(workspace?)` — composes a system prompt from the working tier
//! (and Plan 5 will add procedural rules + recent reflections).

use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::Tier;
use serde_json::{json, Value};

use crate::state::AppState;

pub fn list_descriptors() -> Vec<Value> {
    vec![
        json!({
            "name": "context-for",
            "description": "Returns a system prompt with the user's working memory + procedural rules.",
            "arguments": [
                { "name": "workspace", "description": "Optional workspace scope", "required": false }
            ]
        }),
    ]
}

pub async fn get(state: &AppState, name: &str, _args: &Value) -> anyhow::Result<Value> {
    match name {
        "context-for" => context_for(state).await,
        other => Err(anyhow::anyhow!("unknown prompt: {other}")),
    }
}

async fn context_for(state: &AppState) -> anyhow::Result<Value> {
    let working = state.vault.list(ListFilter {
        tiers: Some(vec![Tier::Working]),
        include_invalid: false,
        limit: Some(64),
        ..Default::default()
    }).await?;
    let procedural = state.vault.list(ListFilter {
        tiers: Some(vec![Tier::Procedural]),
        include_invalid: false,
        limit: Some(64),
        ..Default::default()
    }).await?;

    let mut text = String::new();
    text.push_str("# Persistent context from Mnemos\n\n");
    if !working.is_empty() {
        text.push_str("## Working memory\n");
        for m in &working {
            text.push_str(&format!("- {} — {}\n", m.title, m.body.lines().next().unwrap_or("")));
        }
        text.push('\n');
    }
    if !procedural.is_empty() {
        text.push_str("## Procedural rules\n");
        for m in &procedural {
            text.push_str(&format!("- {}: {}\n", m.title, m.body.lines().next().unwrap_or("")));
        }
        text.push('\n');
    }

    // Append raw bodies for the model to read in full (working tier only, capped).
    text.push_str("---\n");
    for m in &working {
        text.push_str(&format!("\n[{}]\n{}\n", m.title, m.body));
    }

    Ok(json!({
        "messages": [{
            "role": "system",
            "content": { "type": "text", "text": text }
        }]
    }))
}
```

- [ ] **Step 5: Wire methods in `crates/mnemos_daemon/src/mcp/mod.rs`**

Add to the dispatch in `handle()`:

```rust
        "resources/list" => resources_list(),
        "resources/read" => resources_read(&state, req.params.as_ref().unwrap_or(&Value::Null)).await,
        "prompts/list"   => prompts_list(),
        "prompts/get"    => prompts_get(&state, req.params.as_ref().unwrap_or(&Value::Null)).await,
```

And helper functions inside `mcp/mod.rs`:

```rust
pub mod resources;
pub mod prompts;

fn resources_list() -> JsonRpcResponse {
    JsonRpcResponse::success(None, json!({ "resources": resources::list_descriptors() }))
}

async fn resources_read(state: &AppState, params: &Value) -> JsonRpcResponse {
    let uri = match params["uri"].as_str() {
        Some(u) => u,
        None => return JsonRpcResponse::error(None, INVALID_PARAMS, "resources/read requires 'uri'"),
    };
    match resources::read(state, uri).await {
        Ok(v) => JsonRpcResponse::success(None, v),
        Err(e) => JsonRpcResponse::error(None, INTERNAL_ERROR, e.to_string()),
    }
}

fn prompts_list() -> JsonRpcResponse {
    JsonRpcResponse::success(None, json!({ "prompts": prompts::list_descriptors() }))
}

async fn prompts_get(state: &AppState, params: &Value) -> JsonRpcResponse {
    let name = match params["name"].as_str() {
        Some(n) => n,
        None => return JsonRpcResponse::error(None, INVALID_PARAMS, "prompts/get requires 'name'"),
    };
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    match prompts::get(state, name, &args).await {
        Ok(v) => JsonRpcResponse::success(None, v),
        Err(e) => JsonRpcResponse::error(None, INTERNAL_ERROR, e.to_string()),
    }
}
```

- [ ] **Step 6: Run tests** → 4 pass + all prior.

- [ ] **Step 7: Verify** — fmt + clippy clean.

- [ ] **Step 8: Commit**

```bash
git add crates/mnemos_daemon/src/mcp/ crates/mnemos_daemon/tests/mcp_resources.rs
git commit -m "feat(daemon): MCP resources/list+read and prompts/list+get"
```

---

## Task 13: MCP stdio transport (subprocess wrapper)

**Files:**
- Create: `crates/mnemos_daemon/src/bin/mnemos_mcp_stdio.rs`
- Test: `crates/mnemos_daemon/tests/mcp_stdio.rs`

MCP stdio transport is what Claude Code and other tools spawn when an entry like `{"command": "mnemos-mcp-stdio"}` is in their config. The subprocess reads JSON-RPC frames (Content-Length-framed) from stdin, forwards them to the daemon's `/mcp` HTTP endpoint, and writes responses to stdout. If no daemon is running, the subprocess auto-spawns one (when `daemon.auto_start = true` in config).

For Plan 3 we ship the simpler variant: subprocess REQUIRES a running daemon (else exits with a clear error). Auto-start lands in Task 16 alongside the CLI client.

- [ ] **Step 1: Write failing test**

```rust
use mnemos_core::vault::Vault;
use mnemos_core::paths::Paths;
use mnemos_daemon::{config::Config, build_app, serve};
use std::process::Stdio;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::Command;

#[tokio::test]
async fn stdio_subprocess_forwards_initialize_to_daemon() {
    // Start a real daemon on a random port.
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    let token = state.token.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move { serve(listener, app).await.unwrap(); });

    // Spawn the stdio binary pointed at our daemon.
    let bin = env!("CARGO_BIN_EXE_mnemos_mcp_stdio");
    let mut child = Command::new(bin)
        .env("MNEMOS_DAEMON_URL", format!("http://{addr}"))
        .env("MNEMOS_DAEMON_TOKEN", &token)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // Send an initialize request with Content-Length framing.
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#;
    let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    stdin.write_all(frame.as_bytes()).await.unwrap();
    stdin.flush().await.unwrap();

    // Read one response frame.
    let mut header = String::new();
    while !header.ends_with("\r\n\r\n") {
        let mut byte = [0u8; 1];
        tokio::time::timeout(std::time::Duration::from_secs(3), {
            use tokio::io::AsyncReadExt;
            reader.read_exact(&mut byte)
        }).await.expect("response within 3s").unwrap();
        header.push(byte[0] as char);
    }
    let len: usize = header.lines()
        .find_map(|l| l.strip_prefix("Content-Length: "))
        .and_then(|s| s.trim().parse().ok())
        .unwrap();
    let mut payload = vec![0u8; len];
    use tokio::io::AsyncReadExt;
    reader.read_exact(&mut payload).await.unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    assert_eq!(resp["id"], 1);
    assert!(resp["result"]["serverInfo"]["name"].is_string());

    child.kill().await.ok();
}
```

- [ ] **Step 2: Run test to verify fail**

`cargo test -p mnemos_daemon --test mcp_stdio` → FAIL — binary doesn't exist.

- [ ] **Step 3: Implement `crates/mnemos_daemon/src/bin/mnemos_mcp_stdio.rs`**

```rust
//! MCP stdio transport. Reads Content-Length-framed JSON-RPC from stdin,
//! forwards each request to the daemon's /mcp HTTP endpoint, writes responses
//! to stdout with the same framing.

use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mnemos-mcp-stdio: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    let url = std::env::var("MNEMOS_DAEMON_URL").unwrap_or_else(|_| "http://127.0.0.1:7423".into());
    let token = match std::env::var("MNEMOS_DAEMON_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            let path = mnemos_daemon::token_path()?;
            mnemos_daemon::auth::load_token(&path)
                .with_context(|| format!("read token from {}", path.display()))?
        }
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    let mcp_url = format!("{}/mcp", url.trim_end_matches('/'));

    let stdin = std::io::stdin();
    let stdout_handle = std::io::stdout();

    // Spawn a blocking task to read framed messages from stdin; forward each
    // to /mcp; write framed response to stdout.
    loop {
        let frame = match read_frame(&mut stdin.lock()) {
            Ok(Some(f)) => f,
            Ok(None) => break,                 // EOF — graceful exit
            Err(e) => return Err(e),
        };

        let resp = client.post(&mcp_url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(frame)
            .send().await?;
        let body = resp.bytes().await?;
        write_frame(&mut stdout_handle.lock(), &body)?;
    }
    Ok(())
}

fn read_frame<R: BufRead>(r: &mut R) -> Result<Option<Vec<u8>>> {
    let mut header = String::new();
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line)?;
        if n == 0 { return Ok(None); }       // EOF before header
        if line == "\r\n" || line == "\n" { break; }
        header.push_str(&line);
    }
    let len: usize = header
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length:").map(str::trim))
        .ok_or_else(|| anyhow::anyhow!("missing Content-Length header"))?
        .parse()
        .context("parse Content-Length")?;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    Ok(Some(payload))
}

fn write_frame<W: Write>(w: &mut W, body: &[u8]) -> Result<()> {
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(body)?;
    w.flush()?;
    Ok(())
}
```

- [ ] **Step 4: Add `[[bin]]` entry to `crates/mnemos_daemon/Cargo.toml`**

```toml
[[bin]]
name = "mnemos-mcp-stdio"
path = "src/bin/mnemos_mcp_stdio.rs"
```

(Keep the existing `[[bin]] name = "mnemosd"`.)

- [ ] **Step 5: Add `pub fn token_path()` to `crates/mnemos_daemon/src/lib.rs`**

```rust
/// Resolve the canonical path to the daemon's auth token file.
pub fn token_path() -> anyhow::Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .ok_or_else(|| anyhow::anyhow!("could not resolve XDG config dir"))?;
    Ok(dirs.config_dir().join("token"))
}
```

(The existing private `config_token_path` in `build_app` should delegate to this.)

- [ ] **Step 6: Add `reqwest` to non-dev deps of `mnemos_daemon`**

`crates/mnemos_daemon/Cargo.toml` `[dependencies]`:

```toml
reqwest = { workspace = true }
```

- [ ] **Step 7: Run tests** → 1 pass + all prior.

NOTE: the test uses `env!("CARGO_BIN_EXE_mnemos_mcp_stdio")` which is automatically provided by Cargo when a `[[bin]]` is declared. Verified by `cargo test -p mnemos_daemon --test mcp_stdio --no-run` first.

- [ ] **Step 8: Verify** — fmt + clippy clean.

- [ ] **Step 9: Commit**

```bash
git add crates/mnemos_daemon/Cargo.toml \
        crates/mnemos_daemon/src/{lib.rs,bin/mnemos_mcp_stdio.rs} \
        crates/mnemos_daemon/tests/mcp_stdio.rs
git commit -m "feat(daemon): mnemos-mcp-stdio subprocess — Content-Length frames ↔ /mcp HTTP"
```

---

## Task 14: Reranker wiring (close Plan 2 carry-forward #1)

**Files:**
- Modify: `crates/mnemos_daemon/src/state.rs` (add `reranker: Option<Arc<dyn Reranker>>`)
- Modify: `crates/mnemos_daemon/src/lib.rs` (build_app picks reranker from config)
- Modify: `crates/mnemos_daemon/src/routes/memories.rs::search` (use hybrid_recall_with_rerank when state.reranker is Some)
- Modify: `crates/mnemos_daemon/src/mcp/tools.rs::recall` (same wiring)
- Test: `crates/mnemos_daemon/tests/reranker_wiring.rs`

For Plan 3 we ship the wiring; the actual ONNX reranker requires `cargo build --features rerank-onnx`. With default features, `state.reranker = None` and the wiring is a no-op (matches v0.1.0 behavior). The test injects a stub reranker via a public test hook to verify the wiring is correct without needing ONNX.

- [ ] **Step 1: Write failing test**

```rust
use async_trait::async_trait;
use mnemos_core::error::Result;
use mnemos_core::providers::{Embedder, mock::MockEmbedder, Reranker};
use mnemos_core::vault::Vault;
use mnemos_core::paths::Paths;
use mnemos_daemon::{config::Config, build_app_with_reranker};
use std::sync::Arc;
use tempfile::TempDir;

struct KeywordReranker;
#[async_trait]
impl Reranker for KeywordReranker {
    async fn rerank(&self, q: &str, candidates: &[String]) -> Result<Vec<f32>> {
        let q = q.to_lowercase();
        Ok(candidates.iter().map(|c| if c.to_lowercase().contains(&q) { 1.0 } else { 0.0 }).collect())
    }
}

#[tokio::test]
async fn search_with_rerank_flag_uses_state_reranker() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let emb: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let vault = Vault::open_with_embedder(paths, Some(emb)).await.unwrap();
    use mnemos_core::vault::RememberOpts;
    let _ = vault.remember("apples", RememberOpts { title: Some("a".into()), ..Default::default() }).await.unwrap();
    let id_match = vault.remember("the special-marker is here", RememberOpts { title: Some("b".into()), ..Default::default() }).await.unwrap();
    let _ = vault.remember("bananas", RememberOpts { title: Some("c".into()), ..Default::default() }).await.unwrap();

    let reranker: Option<Arc<dyn Reranker>> = Some(Arc::new(KeywordReranker));
    let (app, state) = build_app_with_reranker(Config::default(), vault, reranker).await.unwrap();
    let token = state.token.clone();

    let body = r#"{"query":"special-marker","k":3,"rerank":true,"explain":true}"#;
    let (s, b) = call(app, "POST", "/v1/memories/search", Some(&token), body).await;
    assert_eq!(s, 200);
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let hits = v["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0]["memory"]["id"], id_match);
    let explain = hits[0]["explain"].as_object().unwrap();
    assert!(explain["rerank_score"].as_f64().is_some());
}

async fn call(app: axum::Router, method: &str, uri: &str, auth: Option<&str>, body: &str) -> (u16, String) {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder().method(method).uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = auth { req = req.header("authorization", format!("Bearer {t}")); }
    let req = req.body(Body::from(body.to_string())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let s = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (s, String::from_utf8_lossy(&bytes).to_string())
}
```

- [ ] **Step 2: Run test to verify fail**

`cargo test -p mnemos_daemon --test reranker_wiring` → FAIL.

- [ ] **Step 3: Modify `crates/mnemos_daemon/src/state.rs`**

```rust
use mnemos_core::providers::Reranker;
use mnemos_core::vault::Vault;
use std::sync::Arc;

use crate::config::Config;
use crate::events::EventBus;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub vault: Vault,
    pub token: String,
    pub events: EventBus,
    pub reranker: Option<Arc<dyn Reranker>>,
}
```

- [ ] **Step 4: Update `crates/mnemos_daemon/src/lib.rs`**

Add a new public constructor `build_app_with_reranker` and have the existing `build_app` delegate with `None`:

```rust
pub async fn build_app(config: Config, vault: Vault) -> Result<(axum::Router, AppState)> {
    build_app_with_reranker(config, vault, None).await
}

pub async fn build_app_with_reranker(
    config: Config,
    vault: Vault,
    reranker: Option<Arc<dyn mnemos_core::providers::Reranker>>,
) -> Result<(axum::Router, AppState)> {
    let token_path_buf = token_path()?;
    let token = auth::ensure_token(&token_path_buf)?;
    let state = AppState {
        config: Arc::new(config),
        vault,
        token,
        events: events::EventBus::new(),
        reranker,
    };
    let app = routes::build_router(state.clone());
    Ok((app, state))
}
```

- [ ] **Step 5: Use the reranker in `routes/memories.rs::search`**

Replace the existing search body's `hybrid_recall(...)` call with conditional `hybrid_recall_with_rerank` when `state.reranker.is_some() && opts.rerank`:

```rust
async fn search(
    State(state): State<AppState>,
    Json(req): Json<SearchReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tiers = req.tier.as_ref().map(|ts| {
        ts.iter().filter_map(|t| Tier::from_str(t).ok()).collect::<Vec<_>>()
    });
    let opts = RecallOpts {
        k: req.k,
        tiers,
        workspace: req.workspace,
        include_invalid: req.include_invalid,
        explain: req.explain,
        rerank: req.rerank,
        ..Default::default()
    };
    let embedder = state.vault.embedder().cloned();
    let embedder_ref = embedder.as_deref();
    let hits = if opts.rerank && state.reranker.is_some() {
        use mnemos_core::retrieval::hybrid::hybrid_recall_with_rerank;
        let rr_arc = state.reranker.clone().unwrap();
        hybrid_recall_with_rerank(state.vault.storage(), embedder_ref, Some(rr_arc.as_ref()), &req.query, opts).await?
    } else {
        hybrid_recall(state.vault.storage(), embedder_ref, &req.query, opts).await?
    };
    Ok(Json(serde_json::json!({ "hits": hits })))
}
```

- [ ] **Step 6: Same wiring in `mcp/tools.rs::recall`**

```rust
    let hits = if opts.rerank && state.reranker.is_some() {
        use mnemos_core::retrieval::hybrid::hybrid_recall_with_rerank;
        let rr_arc = state.reranker.clone().unwrap();
        hybrid_recall_with_rerank(state.vault.storage(), embedder_ref, Some(rr_arc.as_ref()), query, opts).await?
    } else {
        hybrid_recall(state.vault.storage(), embedder_ref, query, opts).await?
    };
```

- [ ] **Step 7: Build a real Reranker in `main.rs` when config says so**

In `crates/mnemos_daemon/src/main.rs::serve_cmd`, build the reranker from `cfg.reranker` and pass to `build_app_with_reranker`:

```rust
    let reranker = build_reranker_for_daemon(&cfg)?;
    let (app, _state) = build_app_with_reranker(cfg, vault, reranker).await?;
```

Add `build_reranker_for_daemon`:

```rust
fn build_reranker_for_daemon(cfg: &Config) -> Result<Option<Arc<dyn mnemos_core::providers::Reranker>>> {
    use mnemos_daemon::config::RerankerKind;
    if !cfg.reranker.enabled || matches!(cfg.reranker.kind, RerankerKind::None) {
        return Ok(None);
    }
    #[cfg(feature = "rerank-onnx")]
    {
        // OnnxReranker lives in mnemos_core::providers::onnx_reranker behind the same feature.
        // Plan 3 just wires the loader; users opting into ONNX must build with the feature.
        use mnemos_core::providers::onnx_reranker::{OnnxReranker, OnnxRerankerConfig};
        let oc = OnnxRerankerConfig {
            model_path: cfg.reranker.model_path.clone()
                .ok_or_else(|| anyhow::anyhow!("reranker.model_path required"))?,
            tokenizer_path: cfg.reranker.tokenizer_path.clone()
                .ok_or_else(|| anyhow::anyhow!("reranker.tokenizer_path required"))?,
            max_seq_len: cfg.reranker.max_seq_len,
        };
        return Ok(Some(Arc::new(OnnxReranker::load(oc)?)));
    }
    #[cfg(not(feature = "rerank-onnx"))]
    {
        anyhow::bail!("reranker.enabled = true but binary was built without --features rerank-onnx");
    }
}
```

NOTE: `mnemos_daemon` needs to surface a `rerank-onnx` feature that turns on the `mnemos_core` feature of the same name. Add to `crates/mnemos_daemon/Cargo.toml`:

```toml
[features]
default = []
rerank-onnx = ["mnemos_core/rerank-onnx"]
```

- [ ] **Step 8: Run tests** → 1 pass + all prior.

- [ ] **Step 9: Verify** — fmt + clippy clean. Default-feature `cargo build --workspace` does NOT pull ort.

- [ ] **Step 10: Commit**

```bash
git add crates/mnemos_daemon/src/{lib.rs,main.rs,state.rs,routes/memories.rs,mcp/tools.rs} \
        crates/mnemos_daemon/Cargo.toml \
        crates/mnemos_daemon/tests/reranker_wiring.rs
git commit -m "feat(daemon): wire Reranker from config; CLI --rerank now actually reranks"
```

---

## Task 15: PID file + graceful shutdown

**Files:**
- Create: `crates/mnemos_daemon/src/pid.rs`
- Modify: `crates/mnemos_daemon/src/lib.rs` (`pub mod pid;`)
- Modify: `crates/mnemos_daemon/src/main.rs::serve_cmd` (write PID; SIGTERM handler removes it)
- Test: `crates/mnemos_daemon/tests/pid.rs`

- [ ] **Step 1: Write failing test**

```rust
use mnemos_daemon::pid::{PidFile, write_pid, read_pid, remove_pid};
use tempfile::TempDir;

#[test]
fn write_pid_creates_file_with_current_process_id() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("mnemosd.pid");
    let _guard = PidFile::acquire(&path).unwrap();
    let pid = read_pid(&path).unwrap();
    assert_eq!(pid, std::process::id());
}

#[test]
fn pidfile_drop_removes_pid() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("mnemosd.pid");
    {
        let _guard = PidFile::acquire(&path).unwrap();
        assert!(path.exists());
    }
    assert!(!path.exists(), "PidFile drop should remove the file");
}

#[test]
fn second_acquire_errors_when_pid_is_alive() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("mnemosd.pid");
    let _g = PidFile::acquire(&path).unwrap();
    let r = PidFile::acquire(&path);
    assert!(r.is_err(), "second acquire must fail while first is alive");
}

#[test]
fn second_acquire_succeeds_when_prior_pid_is_dead() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("mnemosd.pid");
    // Write a PID that we're confident doesn't exist (i32::MAX is never a real PID).
    write_pid(&path, i32::MAX as u32).unwrap();
    let r = PidFile::acquire(&path);
    assert!(r.is_ok(), "stale PID file should be reclaimed");
    drop(r);
    remove_pid(&path).ok();
}
```

- [ ] **Step 2: Run test to verify fail**

`cargo test -p mnemos_daemon --test pid` → FAIL.

- [ ] **Step 3: Implement `crates/mnemos_daemon/src/pid.rs`**

```rust
//! PID file management for `mnemosd`. Used by `mnemos daemon status` to
//! detect a running daemon and by graceful shutdown to clean up.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// On-drop RAII guard: removes the pid file when dropped.
pub struct PidFile {
    path: PathBuf,
}

impl PidFile {
    /// Atomically acquire the PID file at `path`. Fails if a live process already owns it.
    pub fn acquire(path: &Path) -> Result<Self> {
        if path.exists() {
            if let Ok(pid) = read_pid(path) {
                if process_is_alive(pid) {
                    anyhow::bail!("PID file {} already owned by process {pid}", path.display());
                }
                // Stale — fall through and overwrite.
            }
        }
        write_pid(path, std::process::id())?;
        Ok(Self { path: path.to_path_buf() })
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        let _ = remove_pid(&self.path);
    }
}

pub fn write_pid(path: &Path, pid: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create PID dir {}", parent.display()))?;
    }
    std::fs::write(path, pid.to_string())
        .with_context(|| format!("write PID file {}", path.display()))?;
    Ok(())
}

pub fn read_pid(path: &Path) -> Result<u32> {
    let s = std::fs::read_to_string(path)
        .with_context(|| format!("read PID file {}", path.display()))?;
    s.trim().parse().context("parse PID")
}

pub fn remove_pid(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("remove PID file {}", path.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    // kill(pid, 0) returns 0 if process exists, -1 with ESRCH if not.
    // SAFETY: kill is async-signal-safe; signal 0 doesn't actually deliver.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(not(unix))]
fn process_is_alive(_pid: u32) -> bool {
    // Conservative: assume alive on non-Unix. (Plan 7 packaging will adjust.)
    true
}
```

Add `libc = "0.2"` to workspace deps and `libc = { workspace = true }` (target_family = "unix") to crate deps... but for simplicity, just add it unconditionally — libc builds on all platforms (it's a no-op API surface on Windows).

In `crates/mnemos_daemon/Cargo.toml` `[target.'cfg(unix)'.dependencies]`:

```toml
libc = "0.2"
```

This scopes the dep to Unix builds only.

- [ ] **Step 4: Wire PID file into `main.rs::serve_cmd`**

```rust
async fn serve_cmd(cfg: Config) -> Result<()> {
    let paths = Paths::with_root(&cfg.vault.root);
    let embedder = build_embedder_for_daemon(&cfg)?;
    let reranker = build_reranker_for_daemon(&cfg)?;
    let vault = Vault::open_with_embedder(paths, embedder).await
        .context("opening vault")?;
    let bind = format!("{}:{}", cfg.daemon.host, cfg.daemon.port);
    let listener = tokio::net::TcpListener::bind(&bind).await
        .with_context(|| format!("bind {bind}"))?;
    tracing::info!(addr = %listener.local_addr()?, "mnemosd listening");

    let pid_path = mnemos_daemon::pid_path()?;
    let _pid = mnemos_daemon::pid::PidFile::acquire(&pid_path)
        .with_context(|| format!("acquire PID file {}", pid_path.display()))?;
    tracing::info!(pid_file = %pid_path.display(), pid = std::process::id(), "PID file acquired");

    let (app, _state) = build_app_with_reranker(cfg, vault, reranker).await?;

    // Listen for SIGTERM / SIGINT and exit gracefully so Drop removes the PID file.
    let shutdown = async {
        #[cfg(unix)] {
            let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("install SIGTERM handler");
            let mut int  = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).expect("install SIGINT handler");
            tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
        }
        #[cfg(not(unix))] { let _ = tokio::signal::ctrl_c().await; }
    };

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}
```

Add `pub fn pid_path()` to `crates/mnemos_daemon/src/lib.rs`:

```rust
pub fn pid_path() -> anyhow::Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .ok_or_else(|| anyhow::anyhow!("could not resolve XDG state dir"))?;
    let state_dir = dirs.state_dir().unwrap_or(dirs.data_dir());
    Ok(state_dir.join("mnemosd.pid"))
}
```

Also `pub mod pid;` in `lib.rs`.

- [ ] **Step 5: Run tests** → 4 pass + all prior.

- [ ] **Step 6: Verify** — fmt + clippy clean.

- [ ] **Step 7: Commit**

```bash
git add crates/mnemos_daemon/Cargo.toml \
        crates/mnemos_daemon/src/{lib.rs,main.rs,pid.rs} \
        crates/mnemos_daemon/tests/pid.rs
git commit -m "feat(daemon): PID file management + graceful shutdown on SIGTERM/SIGINT"
```

---

## Task 16: `mnemos_client` — HTTP client + CLI client-mode wrapper

**Files:**
- Modify: `crates/mnemos_client/src/lib.rs` (HTTP client methods)
- Create: `crates/mnemos_client/src/transport.rs`
- Create: `crates/mnemos_client/src/error.rs`
- Test: `crates/mnemos_client/tests/client.rs`

For CLI integration with the daemon, we need a typed HTTP client. It must (a) be able to detect whether a daemon is running and (b) fall back gracefully to direct vault access when there's no daemon. Detection is via `GET /health`; fallback is implemented in Task 17 by the CLI.

- [ ] **Step 1: Write failing test**

```rust
use mnemos_client::Client;
use mnemos_core::vault::Vault;
use mnemos_core::paths::Paths;
use mnemos_daemon::{config::Config, build_app, serve};
use tempfile::TempDir;
use tokio::net::TcpListener;

async fn spin_daemon() -> (String, String) {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    let token = state.token.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { serve(listener, app).await.unwrap(); });
    (format!("http://{addr}"), token)
}

#[tokio::test]
async fn client_health_ok() {
    let (url, token) = spin_daemon().await;
    let c = Client::new(&url, &token).unwrap();
    assert!(c.health().await.unwrap());
}

#[tokio::test]
async fn client_remember_then_get() {
    let (url, token) = spin_daemon().await;
    let c = Client::new(&url, &token).unwrap();
    let id = c.remember("body", Default::default()).await.unwrap();
    assert!(id.starts_with("mem_"));
    let mem = c.get_memory(&id).await.unwrap();
    assert_eq!(mem.body, "body");
}

#[tokio::test]
async fn client_recall_returns_hits() {
    let (url, token) = spin_daemon().await;
    let c = Client::new(&url, &token).unwrap();
    c.remember("Tauri choice", Default::default()).await.unwrap();
    let hits = c.recall("tauri", Default::default()).await.unwrap();
    assert!(!hits.is_empty());
}
```

- [ ] **Step 2: Run test, verify FAIL**

`cargo test -p mnemos_client --test client` → FAIL — module empty.

- [ ] **Step 3: Implement `crates/mnemos_client/src/error.rs`**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server returned {status}: {body}")]
    Server { status: u16, body: String },
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type Result<T, E = ClientError> = std::result::Result<T, E>;
```

- [ ] **Step 4: Implement `crates/mnemos_client/src/transport.rs`**

```rust
use reqwest::{Client as Http, Method};
use serde::{de::DeserializeOwned, Serialize};
use url::Url;

use crate::error::{ClientError, Result};

pub struct Transport {
    http: Http,
    base: Url,
    token: String,
}

impl Transport {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        let base = Url::parse(base_url)?;
        let http = Http::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self { http, base, token: token.into() })
    }

    pub async fn request<B: Serialize, R: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        with_auth: bool,
    ) -> Result<R> {
        let url = self.base.join(path)?;
        let mut req = self.http.request(method, url);
        if with_auth {
            req = req.bearer_auth(&self.token);
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            return Err(ClientError::Server {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&bytes).to_string(),
            });
        }
        if bytes.is_empty() {
            // Allow `()` deserialization when there's no body.
            return Ok(serde_json::from_str::<R>("null")?);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
}
```

- [ ] **Step 5: Implement `crates/mnemos_client/src/lib.rs`**

```rust
//! Mnemos HTTP client. Talks to the daemon's REST surface.

#![deny(rust_2018_idioms)]
#![warn(clippy::all)]

pub mod error;
pub mod transport;

pub use error::{ClientError, Result};

use mnemos_core::retrieval::RecallHit;
use mnemos_core::types::Memory;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone)]
pub struct Client {
    tx: std::sync::Arc<transport::Transport>,
}

impl Client {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        Ok(Self { tx: std::sync::Arc::new(transport::Transport::new(base_url, token)?) })
    }

    /// `GET /health` — returns true on 200.
    pub async fn health(&self) -> Result<bool> {
        let v: Value = self.tx.request(Method::GET, "/health", None::<&()>, false).await?;
        Ok(v.get("status").and_then(|s| s.as_str()) == Some("ok"))
    }

    pub async fn remember(&self, body: &str, opts: RememberClientOpts) -> Result<String> {
        let req = RememberReq {
            body: body.to_string(),
            title: opts.title,
            tier: opts.tier.unwrap_or_else(|| "semantic".into()),
            kind: opts.kind.unwrap_or_else(|| "fact".into()),
            tags: opts.tags,
            importance: opts.importance,
            workspace: opts.workspace,
            source_tool: opts.source_tool,
        };
        let v: Value = self.tx.request(Method::POST, "/v1/memories", Some(&req), true).await?;
        Ok(v["id"].as_str().unwrap_or_default().to_string())
    }

    pub async fn get_memory(&self, id: &str) -> Result<Memory> {
        self.tx.request(Method::GET, &format!("/v1/memories/{id}"), None::<&()>, true).await
    }

    pub async fn forget(&self, id: &str, reason: Option<&str>) -> Result<()> {
        let q = reason.map(|r| format!("?reason={}", urlencoding::encode(r))).unwrap_or_default();
        let _: Value = self.tx.request(Method::DELETE, &format!("/v1/memories/{id}{q}"), None::<&()>, true).await?;
        Ok(())
    }

    pub async fn list_memories(&self, opts: ListClientOpts) -> Result<Vec<Memory>> {
        let q = opts.to_query();
        let v: Value = self.tx.request(Method::GET, &format!("/v1/memories{q}"), None::<&()>, true).await?;
        Ok(serde_json::from_value(v["memories"].clone())?)
    }

    pub async fn recall(&self, query: &str, opts: RecallClientOpts) -> Result<Vec<RecallHit>> {
        let req = RecallReq {
            query: query.to_string(),
            k: opts.k.unwrap_or(10),
            tier: opts.tier,
            workspace: opts.workspace,
            include_invalid: opts.include_invalid,
            explain: opts.explain,
            rerank: opts.rerank,
        };
        let v: Value = self.tx.request(Method::POST, "/v1/memories/search", Some(&req), true).await?;
        Ok(serde_json::from_value(v["hits"].clone())?)
    }
}

#[derive(Default, Debug, Clone)]
pub struct RememberClientOpts {
    pub title: Option<String>,
    pub tier: Option<String>,
    pub kind: Option<String>,
    pub tags: Vec<String>,
    pub importance: Option<f64>,
    pub workspace: Option<String>,
    pub source_tool: Option<String>,
}

#[derive(Default, Debug, Clone)]
pub struct RecallClientOpts {
    pub k: Option<usize>,
    pub tier: Option<Vec<String>>,
    pub workspace: Option<String>,
    pub include_invalid: bool,
    pub explain: bool,
    pub rerank: bool,
}

#[derive(Default, Debug, Clone)]
pub struct ListClientOpts {
    pub tier: Option<Vec<String>>,
    pub workspace: Option<String>,
    pub include_invalid: bool,
    pub limit: Option<usize>,
}

impl ListClientOpts {
    fn to_query(&self) -> String {
        let mut parts: Vec<String> = vec![];
        if let Some(ts) = &self.tier {
            for t in ts { parts.push(format!("tier={}", urlencoding::encode(t))); }
        }
        if let Some(ws) = &self.workspace { parts.push(format!("workspace={}", urlencoding::encode(ws))); }
        if self.include_invalid { parts.push("include_invalid=true".into()); }
        if let Some(l) = self.limit { parts.push(format!("limit={l}")); }
        if parts.is_empty() { String::new() } else { format!("?{}", parts.join("&")) }
    }
}

#[derive(Serialize, Deserialize)]
struct RememberReq {
    body: String,
    #[serde(skip_serializing_if = "Option::is_none")] title: Option<String>,
    tier: String,
    kind: String,
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")] importance: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] source_tool: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct RecallReq {
    query: String,
    k: usize,
    #[serde(skip_serializing_if = "Option::is_none")] tier: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] workspace: Option<String>,
    include_invalid: bool,
    explain: bool,
    rerank: bool,
}
```

Add `urlencoding = "2"` to workspace deps and to `crates/mnemos_client/Cargo.toml`:

```toml
urlencoding = "2"
```

- [ ] **Step 6: Run tests** → 3 pass.

- [ ] **Step 7: Verify** — fmt + clippy clean.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml \
        crates/mnemos_client/{Cargo.toml,src/} \
        crates/mnemos_client/tests/client.rs
git commit -m "feat(client): typed Mnemos HTTP client (Client, RememberOpts, RecallOpts, ListOpts)"
```

---

## Task 17: `mnemos daemon` subcommand (start | stop | status | logs)

**Files:**
- Modify: `crates/mnemos_cli/src/cli.rs` (new Daemon subcommand)
- Create: `crates/mnemos_cli/src/commands/daemon.rs`
- Modify: `crates/mnemos_cli/src/commands/mod.rs` (`pub mod daemon;`)
- Modify: `crates/mnemos_cli/src/main.rs` (dispatch)
- Modify: `crates/mnemos_cli/Cargo.toml` (add mnemos_client + mnemos_daemon deps)
- Test: `crates/mnemos_cli/tests/cli_daemon.rs`

`mnemos daemon` is a process-management subcommand. It spawns `mnemosd` as a child, tracks the PID, and provides status/stop/log inspection.

- [ ] **Step 1: Write failing test**

```rust
use assert_cmd::Command;
use predicates::prelude::*;
use std::process::Stdio;
use tempfile::TempDir;

fn cmd(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("mnemos").unwrap();
    c.env("MNEMOS_VAULT", tmp.path())
     .env("MNEMOS_EMBEDDER", "mock")
     .env("XDG_CONFIG_HOME", tmp.path().join("config"))
     .env("XDG_STATE_HOME", tmp.path().join("state"));
    c
}

#[test]
fn daemon_status_when_no_daemon_running_says_so() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp).args(["daemon", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("not running"));
}

#[test]
fn daemon_status_json_when_no_daemon_returns_running_false() {
    let tmp = TempDir::new().unwrap();
    let out = cmd(&tmp).args(["--json", "daemon", "status"]).assert().success().get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["running"], false);
}
```

Note: testing daemon `start` and `stop` requires spawning a real `mnemosd` binary as a subprocess; that's an integration concern best left for the smoke test in Task 19. The unit test here covers the status path (the most common operation).

- [ ] **Step 2: Run test, verify FAIL**

`cargo test -p mnemos_cli --test cli_daemon` → FAIL.

- [ ] **Step 3: Add `Daemon` to the `Cmd` enum in `crates/mnemos_cli/src/cli.rs`**

After `Embed(EmbedArgs)`:

```rust
    /// Daemon process management.
    Daemon(DaemonArgs),
```

And the args type at the bottom:

```rust
#[derive(clap::Args, Debug)]
pub struct DaemonArgs {
    #[command(subcommand)]
    pub action: DaemonAction,
}

#[derive(Subcommand, Debug)]
pub enum DaemonAction {
    /// Spawn `mnemosd` as a background process.
    Start,
    /// Send SIGTERM to the daemon (graceful shutdown).
    Stop,
    /// Print whether a daemon is running, its PID, and its address.
    Status,
    /// Tail the daemon log file.
    Logs { #[arg(long, default_value_t = 100)] lines: usize },
}
```

- [ ] **Step 4: Implement `crates/mnemos_cli/src/commands/daemon.rs`**

```rust
use crate::cli::{DaemonAction, DaemonArgs};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Stdio;

pub async fn run(_vault: Option<PathBuf>, json: bool, args: DaemonArgs) -> Result<()> {
    match args.action {
        DaemonAction::Start => start(json).await,
        DaemonAction::Stop => stop(json).await,
        DaemonAction::Status => status(json).await,
        DaemonAction::Logs { lines } => logs(lines).await,
    }
}

async fn start(json: bool) -> Result<()> {
    let pid_path = mnemos_daemon::pid_path()?;
    if pid_path.exists() {
        if let Ok(pid) = mnemos_daemon::pid::read_pid(&pid_path) {
            if process_alive(pid) {
                if json {
                    println!("{}", serde_json::json!({"started": false, "reason": "already running", "pid": pid}));
                } else {
                    println!("mnemosd already running (pid {pid})");
                }
                return Ok(());
            }
        }
    }
    let log_path = log_path()?;
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let log = std::fs::OpenOptions::new().create(true).append(true).open(&log_path)
        .with_context(|| format!("open log {}", log_path.display()))?;
    let log_err = log.try_clone()?;
    let bin_name = "mnemosd";
    let bin = which::which(bin_name).unwrap_or_else(|_| PathBuf::from(bin_name));
    let child = std::process::Command::new(bin)
        .arg("serve")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .spawn()
        .with_context(|| format!("spawn {bin_name}"))?;
    // Give the daemon a moment to bind / write PID.
    std::thread::sleep(std::time::Duration::from_millis(250));
    if json {
        println!("{}", serde_json::json!({"started": true, "pid": child.id(), "log": log_path}));
    } else {
        println!("mnemosd started (pid {}), logs at {}", child.id(), log_path.display());
    }
    Ok(())
}

async fn stop(json: bool) -> Result<()> {
    let pid_path = mnemos_daemon::pid_path()?;
    if !pid_path.exists() {
        if json {
            println!("{}", serde_json::json!({"stopped": false, "reason": "no PID file"}));
        } else {
            println!("no daemon running (no PID file)");
        }
        return Ok(());
    }
    let pid = mnemos_daemon::pid::read_pid(&pid_path)?;
    #[cfg(unix)] {
        // SAFETY: kill is async-signal-safe; SIGTERM is graceful.
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM); }
    }
    if json {
        println!("{}", serde_json::json!({"stopped": true, "pid": pid}));
    } else {
        println!("sent SIGTERM to pid {pid}");
    }
    Ok(())
}

async fn status(json: bool) -> Result<()> {
    let pid_path = mnemos_daemon::pid_path()?;
    if !pid_path.exists() {
        if json {
            println!("{}", serde_json::json!({"running": false}));
        } else {
            println!("mnemosd not running");
        }
        return Ok(());
    }
    let pid = mnemos_daemon::pid::read_pid(&pid_path)?;
    if !process_alive(pid) {
        if json {
            println!("{}", serde_json::json!({"running": false, "stale_pid": pid}));
        } else {
            println!("mnemosd not running (stale PID file points at {pid})");
        }
        return Ok(());
    }
    // Try to reach the HTTP /health to confirm it's actually serving.
    let cfg = mnemos_daemon::config::Config::load_default().unwrap_or_default();
    let url = format!("http://{}:{}/health", cfg.daemon.host, cfg.daemon.port);
    let healthy = reqwest::Client::builder().timeout(std::time::Duration::from_millis(500))
        .build()?.get(&url).send().await.is_ok();
    if json {
        println!("{}", serde_json::json!({"running": true, "pid": pid, "url": url, "healthy": healthy}));
    } else {
        println!("mnemosd running — pid {pid}, url {url}, healthy={healthy}");
    }
    Ok(())
}

async fn logs(lines: usize) -> Result<()> {
    let path = log_path()?;
    if !path.exists() {
        println!("no log file at {}", path.display());
        return Ok(());
    }
    let s = std::fs::read_to_string(&path)?;
    let mut all: Vec<&str> = s.lines().collect();
    let start = all.len().saturating_sub(lines);
    for l in all.drain(start..) {
        println!("{l}");
    }
    Ok(())
}

fn log_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .ok_or_else(|| anyhow::anyhow!("could not resolve XDG state dir"))?;
    let state_dir = dirs.state_dir().unwrap_or(dirs.data_dir());
    Ok(state_dir.join("logs").join("mnemosd.log"))
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}
#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool { true }
```

- [ ] **Step 5: Add deps to `crates/mnemos_cli/Cargo.toml`**

```toml
[dependencies]
# ... existing
mnemos_client = { path = "../mnemos_client" }
mnemos_daemon = { path = "../mnemos_daemon" }
reqwest = { workspace = true }
which = "6"

[target.'cfg(unix)'.dependencies]
libc = "0.2"
```

Add `which = "6"` to workspace deps too.

- [ ] **Step 6: Wire `pub mod daemon;` in `crates/mnemos_cli/src/commands/mod.rs`** + dispatch in `main.rs`:

```rust
        Cmd::Daemon(a)   => commands::daemon::run(args.vault, args.json, a).await,
```

- [ ] **Step 7: Run tests** → 2 pass + all prior.

- [ ] **Step 8: Verify** — fmt + clippy clean.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml \
        crates/mnemos_cli/Cargo.toml \
        crates/mnemos_cli/src/{cli.rs,main.rs,commands/mod.rs,commands/daemon.rs} \
        crates/mnemos_cli/tests/cli_daemon.rs
git commit -m "feat(cli): mnemos daemon start|stop|status|logs subcommands"
```

---

## Task 18: Claude Code reference adapter

**Files:**
- Create: `adapters/claude-code/README.md`
- Create: `adapters/claude-code/CLAUDE.md.fragment`
- Create: `adapters/claude-code/claude_mcp_config.json`

This is documentation + config, not code — no failing test step. But it must be accurate and copy-pasteable.

- [ ] **Step 1: Write `adapters/claude-code/README.md`**

```markdown
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
```

- [ ] **Step 2: Write `adapters/claude-code/claude_mcp_config.json`**

```json
{
  "mcpServers": {
    "mnemos": {
      "command": "mnemos-mcp-stdio",
      "args": [],
      "env": {
        "MNEMOS_DAEMON_URL": "http://127.0.0.1:7423"
      }
    }
  }
}
```

NOTE: the stdio subprocess reads the auth token from `~/.config/mnemos/token` automatically — no need to embed it in the config.

- [ ] **Step 3: Write `adapters/claude-code/CLAUDE.md.fragment`**

```markdown
## Mnemos persistent memory

This session has a persistent memory server (Mnemos) registered as an MCP
provider. At the start of every session, read the `mnemos://working`
resource — it contains identity facts and active project context.

Available tools:
- `remember(body, title?, tier?, tags?, importance?)` — store a memory.
- `recall(query, k=5, explain?)` — retrieve relevant memories.
- `forget(memory_id, reason?)` — soft-invalidate.
- `get_memory(memory_id)` / `list_memories(...)` — browse.

When the user states a durable preference, project context, or rule that's
not obvious from the codebase, call `remember(...)` with `tier=procedural`
(rules) or `tier=working` (identity/project) so it persists across sessions.
```

- [ ] **Step 4: Verify with a smoke check**

```bash
# Confirm the JSON is valid
cat adapters/claude-code/claude_mcp_config.json | python3 -m json.tool > /dev/null
echo "json OK"

# Confirm the docs read clean
wc -l adapters/claude-code/*.md adapters/claude-code/*.fragment
```

- [ ] **Step 5: Commit**

```bash
git add adapters/claude-code/
git commit -m "docs(adapter): Claude Code MCP integration — README + config JSON + CLAUDE.md fragment"
```

---

## Task 19: README + CHANGELOG + tag v0.2.0

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Update `README.md`**

Insert a new section after "What works today (v0.1.0)" — DO NOT delete the existing section, just retitle and supersede:

Replace the existing "What works today (v0.1.0)" section title with "What works today (v0.2.0)" and update the bullet list:

```markdown
## What works today (v0.2.0)

- **Long-running daemon** (`mnemosd`) — REST + WebSocket + MCP over Streamable HTTP at `127.0.0.1:7423`.
- **CLI talks to the daemon when one is running**, falls back to direct vault otherwise.
- **MCP integration** — `mnemos-mcp-stdio` subprocess speaks the MCP protocol to Claude Code, Gemini CLI, and any MCP-aware client. Reference adapter for Claude Code at `adapters/claude-code/`.
- `mnemos daemon start|stop|status|logs` — process management.
- `mnemos remember "<body>"` — store a memory.
- `mnemos recall "<query>" --rerank --explain` — hybrid retrieval (BM25 + dense + RRF + reweight + optional cross-encoder rerank, wired from `config.toml`).
- `mnemos embed status|backfill` — embedding maintenance.
- `mnemos get <id>` / `mnemos list` / `mnemos forget <id>` — CRUD with bi-temporal soft invalidation.
- `mnemos rebuild` / `mnemos doctor` — diagnostics + recovery.

### Configuration

Settings live in `~/.config/mnemos/config.toml` (created on first run). See
`docs/superpowers/specs/2026-05-22-mnemos-memory-provider-design.md` for the
full schema; key keys:

```toml
[daemon]
host = "127.0.0.1"
port = 7423

[embedder]
kind = "ollama"            # "ollama" | "mock" | "none"
url = "http://localhost:11434"
model = "nomic-embed-text"
dim = 768

[reranker]
enabled = false            # set true + build with --features rerank-onnx to enable

[mcp]
enabled = true
```

Environment variables still override (Plan 2 compat):
`MNEMOS_EMBEDDER`, `MNEMOS_OLLAMA_URL`, `MNEMOS_OLLAMA_MODEL`,
`MNEMOS_EMBEDDER_DIM`, `MNEMOS_VAULT`, `MNEMOS_DAEMON_PORT`, `MNEMOS_LOG`.

### Auth

Daemon endpoints require `Authorization: Bearer <token>`. The token lives at
`~/.config/mnemos/token` (mode 0600), auto-generated on first daemon start.
`/health` is exempt for monitoring.
```

- [ ] **Step 2: Update `CHANGELOG.md`**

Prepend a new entry (use `date -u +%Y-%m-%d` for the actual date):

```markdown
## [0.2.0] - YYYY-MM-DD

### Added
- `mnemos_daemon` crate — long-running HTTP+WebSocket+MCP server.
- `mnemos_client` crate — typed Rust HTTP client for the daemon.
- REST API: `/v1/memories[/{id}/audit|/search|/time-travel]`, `/v1/sessions[/{id}[/chunks|/end]]`, `/v1/entities[/{id}[/graph]]`, `/v1/working`.
- WebSocket `/v1/events` — typed event stream (MemoryCreated, MemoryInvalidated, SessionStarted, SessionEnded).
- MCP over Streamable HTTP at `/mcp` — `initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read`, `prompts/list`, `prompts/get`.
- `mnemos-mcp-stdio` subprocess transport for MCP stdio clients.
- `mnemos daemon start|stop|status|logs` CLI subcommand.
- `config.toml` schema with env-var overrides (Plan 2's env vars graduate to overrides).
- Bearer-token auth, auto-issued at `~/.config/mnemos/token` (mode 0600).
- PID file + graceful shutdown on SIGTERM/SIGINT.
- Claude Code reference adapter (`adapters/claude-code/`).
- Schema v3: `vault_meta` table tracks embedder dim + model_id; `Vault::open_with_embedder` errors on dim mismatch.
- `Embedder::model_id()` default trait method; `OllamaEmbedder` overrides with concurrent `embed_batch` (8-way fanout).

### Changed
- CLI `--rerank` flag now actually reranks when the daemon's `[reranker]` config enables a reranker (was a stderr warning in v0.1.0).
- `Vault::open_with_embedder` enforces embedder dim/model_id consistency against the stored vault metadata.

### Notes
- Daemon binds to `127.0.0.1` by default — exposing publicly requires explicit config (and a TLS terminator).
- ONNX reranker still feature-gated (`cargo build --features rerank-onnx`).
- All test fixtures use `MockEmbedder`; CI does not require Ollama.
```

- [ ] **Step 3: Final sanity sweep**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p mnemos_daemon -p mnemos_cli
```

All must pass.

- [ ] **Step 4: Smoke test end-to-end**

```bash
export PATH="$PWD/target/release:$PATH"
export MNEMOS_VAULT="/tmp/mnemos-plan3-smoke"
export MNEMOS_EMBEDDER="mock"
rm -rf "$MNEMOS_VAULT"

# Standalone CLI mode (no daemon)
mnemos remember "User likes Tauri" --title "Tauri"
mnemos recall "tauri" --json

# Daemon mode
mnemos daemon start
mnemos daemon status                  # running
sleep 1
curl -s http://127.0.0.1:7423/health
TOKEN=$(cat ~/.config/mnemos/token)
curl -s -H "Authorization: Bearer $TOKEN" http://127.0.0.1:7423/v1/working

# MCP over HTTP via direct curl
curl -s -X POST http://127.0.0.1:7423/mcp \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'

# MCP over stdio (manual frame)
printf 'Content-Length: 60\r\n\r\n{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  | MNEMOS_DAEMON_URL=http://127.0.0.1:7423 MNEMOS_DAEMON_TOKEN=$TOKEN \
    mnemos-mcp-stdio | head -c 200

mnemos daemon stop
mnemos daemon status                  # not running
```

Every command should succeed; the MCP responses should contain the tool list.

- [ ] **Step 5: Commit + tag**

```bash
git add README.md CHANGELOG.md
git commit -m "docs: README and CHANGELOG for v0.2.0 — daemon + REST + WS + MCP"
git tag -a v0.2.0 -m "mnemos v0.2.0 — Plan 3 (Daemon + REST + WebSocket + MCP) complete"
```

- [ ] **Step 6: Push**

```bash
git push origin master
git push origin v0.2.0
```

---

## Plan 3 self-review

### Spec coverage

This plan implements the **daemon**, **REST API**, **WebSocket events**, **MCP server (HTTP + stdio)**, **auth + config**, and **reference adapter** sections of the design spec. Also closes all 5 Plan 2 carry-forwards. Sections explicitly deferred to subsequent plans:

| Spec section | Plan |
|---|---|
| Async LLM extraction pipeline (`POST /v1/sessions/{id}/end` triggers it) | 4 |
| Resolution (ADD/UPDATE/DELETE/NOOP) | 4 |
| Entity-linking pipeline (populates `entities` + `entity_mentions`) | 4 |
| Graph-update pipeline (populates `entity_edges` bi-temporally) | 4 |
| Decay daemon (hourly strength updates) | 4 |
| Time-travel handler (returns 501 in Plan 3) | 4 |
| `PATCH /v1/memories/{id}` (returns 501 in Plan 3) | 4 |
| MCP `sampling/createMessage` (extraction via calling client's LLM) | 4 |
| HippoRAG Personalized PageRank retriever | 5 |
| Reflection (importance-triggered) | 5 |
| Community detection / global recall mode | 5 |
| Tauri+React desktop UI | 6 |
| Sync backends (git/Syncthing/Turso/S3) | 7 |
| Additional reference adapters (Gemini CLI, Codex, Hermes, Openclaw) | 7 |

### Placeholder scan

All "implement later" hand-waves were checked. Endpoints that don't have full Plan 3 behavior return `501 Not Implemented` with a clear message naming the responsible plan (`PATCH /v1/memories/{id}`, `POST /v1/memories/time-travel`). The MCP `resources` and `prompts` surfaces return real data; `entity_graph` returns an empty-but-valid graph (because Plan 5 populates the underlying data). No tests assert behavior that's not implemented in Plan 3.

### Type / signature consistency

Cross-task signature audit:

- `AppState { config, vault, token, events, reranker }` — defined in Task 5, expanded in Task 14. Used by every route + the MCP handler.
- `EventBus` — stubbed in Task 5, real in Task 9. The stub's only method `new()` matches the real one.
- `build_app(Config, Vault) -> Result<(Router, AppState)>` — defined in Task 5, kept stable. Task 14 adds `build_app_with_reranker(Config, Vault, Option<Arc<dyn Reranker>>) -> ...` and rewrites `build_app` as a delegation.
- `Config` struct and `EmbedderKind` / `RerankerKind` enums — defined once in Task 2, used in Tasks 10, 14, and `main.rs`.
- `Client::new(base_url: &str, token: &str)` — defined Task 16; CLI uses identically in any client-mode helpers (Task 17 only does daemon process management, not client calls — that intersects in Plan 4 when CLI fully migrates).
- `pid::PidFile::acquire(&Path) -> Result<Self>`, `pid::read_pid(&Path) -> Result<u32>`, `pid::remove_pid(&Path) -> Result<()>`, `pid::write_pid(&Path, u32) -> Result<()>` — defined Task 15; used in Task 17's `daemon.rs`.
- `token_path()` / `pid_path()` — both `pub fn` in `mnemos_daemon::lib`, used in main and CLI consistently.
- `Embedder::model_id(&self) -> &str` — added with default in Task 4; overridden in `MockEmbedder` and `OllamaEmbedder`. Used by `Vault::open_with_embedder`'s metadata check.

### Carry-forward closure

| Plan 2 gap | Plan 3 task | Closed? |
|---|---|---|
| `--rerank` flag dead in CLI | Task 14 + 17 (CLI talks to daemon which has reranker; standalone-mode still warns) | Yes |
| Schema dim hardcoded; model swap silently corrupts KNN | Task 4 — `vault_meta` table + dim/model check | Yes |
| `Embedder::model_id()` missing | Task 4 — default + overrides | Yes |
| `OllamaEmbedder::embed_batch` serial | Task 4 — 8-way concurrent fanout | Yes |
| `MNEMOS_EMBEDDER_DIM` undocumented | Task 2 — graduated to `config.toml` + documented in README | Yes |

### Known follow-on cleanup (Plan 4+)

These are noted in the plan but not flagged as fixes-before-Plan-4:

- `time_travel` returns 501 — Plan 4's bi-temporal query support implements it.
- `PATCH /v1/memories/{id}` returns 501 — Plan 4 wires file+DB transactional updates.
- `entity_graph` returns empty graph — Plan 5's PPR depends on this graph; the route shape is finalized so Plan 5 only updates the body, not the URI.
- CLI's standalone-mode `--rerank` warning still printed (Task 17 does NOT route CLI through the daemon by default — that's a Plan 4 follow-up after MCP-sampling lands and the daemon becomes the central LLM router).
- `auto_start` in `[daemon]` config is read but not implemented (Plan 4 — CLI auto-spawn-on-missing-daemon belongs with the full client-mode migration).
- Audit-actor on REST endpoints hard-codes `"mnemos-cli"` via `Vault::forget`; Task 14 (Plan 4) will plumb the calling client through.

---

## Execution

Plan 3 done — 19 tasks, ~150 steps total, ~12-20 hours of focused work for an engineer following the plan. Schema migration is additive (v3 = vault_meta table); existing v0.1.0 vaults upgrade transparently on first open by the new code.

Subsequent plans (4 through 7) will be written after Plan 3 lands.
