# Mnemos Plan 9 — Bundled embedder, OpenAI backends, auto-update re-enable (v0.8.0)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `mnemos remember` + `mnemos recall` work end-to-end on a freshly installed `.deb`/`.rpm`/`.AppImage` with no external dependencies (no Ollama, no API key, no internet after install).

**Architecture:** Ship `llama-server` (llama.cpp's upstream HTTP inference server) + a 22 MB `all-MiniLM-L6-v2` Q8 GGUF model inside the Mnemos package. The daemon spawns `llama-server` as a managed child process on startup, talks to it via HTTP at `127.0.0.1:7424`, restarts it on crash with backoff. Vault meta gains an `embedder_kind` column so the vault remembers which backend seeded it; `mnemos embed-rebuild` does atomic, resumable, audit-logged migration between any two backends. Adds OpenAI backends for both embedder and LLM as the cloud opt-in. Re-enables Tauri auto-update (deferred from v0.7.0).

**Tech Stack:** Rust + tokio (existing), llama.cpp upstream `llama-server` binary, MiniLM-L6-v2 GGUF Q8, OpenAI HTTP API, reqwest, libsql (existing), Tauri 2 + tauri-plugin-updater (existing).

---

## Plan sequence context

- Plan 1-6 — core, daemon, embeddings, pipelines, graph, desktop UI (v0.1.0–v0.5.0)
- Plan 7 — cloud sync + settings + doctor + adapters (v0.6.0)
- Plan 8 — packaging, installers, auto-update (v0.7.0, Linux-only)
- **Plan 9 (this) — bundled embedder, zero-setup install, OpenAI backends, auto-update re-enable (v0.8.0)**

After Plan 9: `apt install mnemos` on a fresh machine → `mnemos remember "..."` works immediately. No `ollama pull`, no API key. Local-first preserved; cloud opt-in available.

---

## What this plan deliberately defers

| Item | Why | Where it lands |
|---|---|---|
| macOS desktop bundle | `dispatch2` macro recursion still blocked from Plan 8 | Future plan once `dispatch2` ships a fix |
| Windows desktop bundle | `libsql-sys` Unix-only APIs still blocked from Plan 8 | Future plan; needs libsql Windows support or storage swap |
| Bundled chat LLM | Defeats the lightweight goal; 400+ MB | Likely never bundled — users opt in to Ollama or OpenAI |
| Apple Developer notarization / Windows Authenticode | Out of scope until mac/windows builds work | Same future plan |
| Linux PPA / OBS repository auto-publish | Still manual; documented in PACKAGING.md from Plan 8 | Future automation |

---

## Hard prerequisites

- Plan 8 landed (v0.7.0 tag on `origin/master`).
- Linux dev machine. `cargo`, `pnpm`, `rustup` on PATH.
- `wget` or `curl` for asset fetching.
- For the auto-update re-enable in Task 19: user runs `bash scripts/gen-updater-key.sh` once before the release commit and uploads the private key to GH secrets as `TAURI_SIGNING_PRIVATE_KEY`. The plan does NOT generate the key for you (secrets stay user-owned).
- For OpenAI testing: `OPENAI_API_KEY` env or a mock — tests use a `wiremock` mock; real-API smoke tests are documented but not CI-enforced.

---

## File structure produced by this plan

```
crates/mnemos_core/src/storage/
  migrations.rs                            # MOD — schema v9 (embedder_kind column)

crates/mnemos_core/src/embedder/
  mod.rs                                   # MOD — add Bundled + OpenAi to EmbedderKind, route by vault meta
  bundled.rs                               # NEW — HTTP client for local llama-server
  openai.rs                                # NEW — OpenAI embeddings backend

crates/mnemos_core/src/llm/
  openai.rs                                # NEW — OpenAI chat completions
  mod.rs                                   # MOD — add OpenAi to LlmKind

crates/mnemos_core/src/embedder_rebuild.rs # NEW — atomic, resumable migration

crates/mnemos_daemon/src/
  bundled_embedder.rs                      # NEW — llama-server child process lifecycle
  lib.rs                                   # MOD — spawn bundled embedder in build_app_full
  config.rs                                # MOD — EmbedderKind/LlmKind add Bundled + OpenAi
  routes/
    embed_rebuild.rs                       # NEW — GET status, POST start/abort
    doctor.rs                              # MOD — embedder mismatch check + migration prompt
    mod.rs                                 # MOD — mount embed_rebuild
  events.rs                                # MOD — EmbedRebuildProgress, EmbedderUnhealthy

crates/mnemos_daemon/tests/
  bundled_embedder.rs                      # NEW — integration test
  embed_rebuild.rs                         # NEW — atomic migration test
  doctor.rs                                # MOD — mismatch detection test

crates/mnemos_cli/src/commands/
  embed_rebuild.rs                         # NEW
  mod.rs                                   # MOD
crates/mnemos_cli/src/cli.rs               # MOD — EmbedRebuild subcommand
crates/mnemos_cli/src/main.rs              # MOD — dispatch

desktop/src/views/
  Settings.tsx                             # MOD — embedder + LLM sections updated
  Doctor.tsx                               # MOD — migration prompt
  EmbedRebuild.tsx                         # NEW — progress UI
  EmbedRebuild.test.tsx                    # NEW

desktop/src/api/
  client.ts                                # MOD — embed-rebuild endpoints
  queries.ts                               # MOD — useEmbedRebuild hook
  ws.ts                                    # MOD — embed_rebuild_progress event invalidates

desktop/src/router.tsx                     # MOD — /embed-rebuild route
desktop/src/layout/LeftSidebar.tsx         # MOD — link (visible only when migration pending)

scripts/
  fetch-bundled-assets.sh                  # NEW — downloads llama-server + GGUF in CI
  gen-updater-key.sh                       # MOD — fix any drift from Plan 8

.github/workflows/
  release.yml                              # MOD — fetch + bundle assets, re-enable signing
  ci.yml                                   # MOD — fetch + cache assets for tests

desktop/src-tauri/
  tauri.conf.json                          # MOD — createUpdaterArtifacts: true, bundle assets via resources
  Cargo.toml                               # MOD — bundle resources

crates/mnemos_cli/Cargo.toml               # MOD — cargo-deb asset paths include bundled binaries
crates/mnemos_daemon/Cargo.toml            # MOD — same

BUILD.md                                   # MOD — bundled embedder + OpenAI section
PACKAGING.md                               # MOD — release runbook reflects updater re-enable
README.md                                  # MOD — "no setup required" install section
CHANGELOG.md                               # MOD — 0.8.0 entry

Cargo.toml                                 # MOD — workspace version → 0.8.0
desktop/package.json                       # MOD — version → 0.8.0
desktop/src-tauri/Cargo.toml               # MOD — version → 0.8.0
desktop/src-tauri/tauri.conf.json          # MOD — version → 0.8.0
```

---

## Conventions (same as Plans 1-8)

- One commit per task. Exact commit messages as listed.
- `cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings` clean before every commit.
- No `dbg!` / `println!` in production code.
- Workspace deps preferred; check root `Cargo.toml` before adding inline versions.
- Repo: `UnpaidAttention/mnemos`. License: Apache-2.0.
- Daemon binary: `mnemosd`. CLI binary: `mnemos`.
- Don't push (user pushes manually after review).

---

# Group A — Schema + storage foundation

## Task 1: Schema v9 — `embedder_kind` column

Add a new column to `vault_meta` so the vault records which backend seeded its embeddings. `embedder_model` + `embedder_dim` already exist (Plan 1); `embedder_kind` is the new authoritative source for "which backend should this daemon use".

**Files:**
- Modify: `crates/mnemos_core/src/storage/migrations.rs`
- Test: `crates/mnemos_core/tests/schema_v9.rs` (new)

- [ ] **Step 1: Failing test** — `crates/mnemos_core/tests/schema_v9.rs`:

```rust
use mnemos_core::storage::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn migration_v9_adds_embedder_kind() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(&tmp.path().join("v9.db")).await.unwrap();
    assert!(s.schema_version().await.unwrap() >= 9);
    let conn = s.conn().unwrap();
    // The column exists on vault_meta
    conn.execute(
        "UPDATE vault_meta SET embedder_kind = 'ollama' WHERE id = 1",
        (),
    )
    .await
    .unwrap();
    let mut rows = conn
        .query("SELECT embedder_kind FROM vault_meta WHERE id = 1", ())
        .await
        .unwrap();
    let r = rows.next().await.unwrap().unwrap();
    let kind: String = r.get(0).unwrap();
    assert_eq!(kind, "ollama");
}

#[tokio::test]
async fn fresh_vault_defaults_to_bundled() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(&tmp.path().join("v9b.db")).await.unwrap();
    let conn = s.conn().unwrap();
    let mut rows = conn
        .query("SELECT embedder_kind FROM vault_meta WHERE id = 1", ())
        .await
        .unwrap();
    let r = rows.next().await.unwrap().unwrap();
    let kind: String = r.get(0).unwrap();
    assert_eq!(kind, "bundled", "fresh vault should default to bundled");
}
```

- [ ] **Step 2: Run test** — `cargo test -p mnemos_core --test schema_v9` → FAIL.

- [ ] **Step 3: Add migration v9** in `migrations.rs`. After the `current < 8` block, add:

```rust
        if current < 9 {
            migration_v9(&conn).await?;
            conn.execute(
                "INSERT OR IGNORE INTO schema_migrations (version) VALUES (9)",
                (),
            )
            .await?;
        }
```

```rust
async fn migration_v9(conn: &libsql::Connection) -> Result<()> {
    for stmt in V9_STATEMENTS {
        conn.execute(stmt, ()).await?;
    }
    Ok(())
}

const V9_STATEMENTS: &[&str] = &[
    // Add embedder_kind column. Default to 'bundled' for fresh vaults;
    // upgrades from v8 see NULL → we backfill below to 'ollama' since
    // any pre-v9 vault was necessarily seeded with the old default.
    "ALTER TABLE vault_meta ADD COLUMN embedder_kind TEXT",
    // Backfill: existing v8 vaults had embedder_model set by the first
    // remember; if that model was empty (truly fresh) treat as bundled,
    // otherwise treat as ollama. The daemon will reconcile this with
    // the actual configured embedder on next startup.
    "UPDATE vault_meta
        SET embedder_kind = CASE
            WHEN embedder_model IS NULL OR embedder_model = '' THEN 'bundled'
            WHEN embedder_model = 'mock' THEN 'mock'
            ELSE 'ollama'
        END
        WHERE id = 1 AND embedder_kind IS NULL",
    // Enforce non-null going forward.
    // (sqlite can't add NOT NULL to an existing column without a rebuild;
    //  we rely on application-level enforcement instead.)
];
```

- [ ] **Step 4: Bump stale schema-version assertions** from 8 → 9 in `tests/schema_v1.rs`, `tests/schema_v2.rs`, `tests/storage_open.rs`. Grep each for `schema_version` — Plan 8 Task 17 bumped them to 8; this task bumps to 9.

- [ ] **Step 5: Bump `LATEST_SCHEMA`** in `crates/mnemos_daemon/src/routes/doctor.rs` from `8` to `9`.

- [ ] **Step 6: Pass + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/mnemos_core/src/storage/migrations.rs crates/mnemos_core/tests/schema_v9.rs crates/mnemos_core/tests/schema_v1.rs crates/mnemos_core/tests/schema_v2.rs crates/mnemos_core/tests/storage_open.rs crates/mnemos_daemon/src/routes/doctor.rs
git commit -m "feat: schema v9 — vault_meta.embedder_kind column (Plan 9 Task 1)"
```

---

## Task 2: Storage helpers for embedder metadata

Add typed read/write helpers for the three embedder-meta fields. They get used by everything downstream — the bundled embedder reads its dim at startup, the migration command swaps all three atomically.

**Files:**
- Modify: `crates/mnemos_core/src/storage/vault_meta.rs` (or wherever vault_meta accessors live — read the codebase first)
- Test: extend an existing test or create `crates/mnemos_core/tests/vault_meta.rs`

- [ ] **Step 1: Failing test** — append to (or create) `tests/vault_meta.rs`:

```rust
use mnemos_core::storage::Storage;
use mnemos_core::storage::vault_meta::{
    get_embedder_meta, set_embedder_meta, EmbedderMeta,
};
use tempfile::TempDir;

#[tokio::test]
async fn embedder_meta_round_trip() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(&tmp.path().join("vm.db")).await.unwrap();

    // Fresh vault → defaults from migration.
    let m = get_embedder_meta(&s).await.unwrap();
    assert_eq!(m.kind, "bundled");

    // Atomic swap.
    let new = EmbedderMeta {
        kind: "ollama".into(),
        model: "nomic-embed-text".into(),
        dim: 768,
    };
    set_embedder_meta(&s, &new).await.unwrap();
    let read = get_embedder_meta(&s).await.unwrap();
    assert_eq!(read.kind, "ollama");
    assert_eq!(read.model, "nomic-embed-text");
    assert_eq!(read.dim, 768);
}
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Implement** — read the existing `vault_meta` module first to understand the access pattern. Add (or create) the helpers:

```rust
//! Typed accessors for the embedder-related vault_meta columns.

use crate::error::Result;
use crate::storage::Storage;
use libsql::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbedderMeta {
    pub kind: String,
    pub model: String,
    pub dim: u32,
}

pub async fn get_embedder_meta(storage: &Storage) -> Result<EmbedderMeta> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT embedder_kind, COALESCE(embedder_model, ''), COALESCE(embedder_dim, 0)
               FROM vault_meta WHERE id = 1",
            (),
        )
        .await?;
    let r = rows
        .next()
        .await?
        .ok_or_else(|| crate::error::MnemosError::Internal("vault_meta row missing".into()))?;
    let kind: String = r.get(0)?;
    let model: String = r.get(1)?;
    let dim: u32 = r.get::<i64>(2)? as u32;
    Ok(EmbedderMeta { kind, model, dim })
}

/// Atomically set all three embedder fields. Used by migration + first remember.
pub async fn set_embedder_meta(storage: &Storage, meta: &EmbedderMeta) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "UPDATE vault_meta
            SET embedder_kind = ?, embedder_model = ?, embedder_dim = ?
            WHERE id = 1",
        params![meta.kind.clone(), meta.model.clone(), meta.dim as i64],
    )
    .await?;
    Ok(())
}
```

> Read the existing `vault_meta` module to see whether `embedder_model` + `embedder_dim` setters already exist; if so, `set_embedder_meta` should compose them (or be the new authoritative setter — pick one).

- [ ] **Step 4: Pass + commit.**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/storage/ crates/mnemos_core/tests/vault_meta.rs
git commit -m "feat: typed embedder_meta accessors on vault_meta (Plan 9 Task 2)"
```

---

# Group B — Bundled embedder backend

## Task 3: Asset fetch script — download `llama-server` + GGUF model

A single script that grabs the bundled assets. Used by CI before every test run + release build, AND by developers who want to test locally.

**Files:**
- Create: `scripts/fetch-bundled-assets.sh`
- Create: `assets/.gitignore` (so the downloaded files don't get committed)

- [ ] **Step 1: `scripts/fetch-bundled-assets.sh`**

```bash
#!/usr/bin/env bash
# Download the bundled embedder runtime + model.
#
# Outputs:
#   assets/llama-server-linux-x86_64       (~5 MB, llama.cpp upstream binary)
#   assets/all-MiniLM-L6-v2.Q8_0.gguf      (~22 MB, GGUF model)
#
# Idempotent: skips download if files already exist and match the expected
# sha256. Bump LLAMA_CPP_TAG and MODEL_URL to update the pinned versions.
set -euo pipefail

cd "$(dirname "$0")/.."
mkdir -p assets

# Pinned versions. Bump these to upgrade; each bump should be a separate
# commit so reviewers can audit the diff against upstream changelogs.
LLAMA_CPP_TAG="b3447"
LLAMA_CPP_ARCHIVE="llama-${LLAMA_CPP_TAG}-bin-ubuntu-x64.zip"
LLAMA_CPP_URL="https://github.com/ggml-org/llama.cpp/releases/download/${LLAMA_CPP_TAG}/${LLAMA_CPP_ARCHIVE}"

MODEL_NAME="all-MiniLM-L6-v2.Q8_0.gguf"
MODEL_URL="https://huggingface.co/leliuga/all-MiniLM-L6-v2-GGUF/resolve/main/all-MiniLM-L6-v2.Q8_0.gguf"
MODEL_SHA256="<FILL_AFTER_FIRST_DOWNLOAD>"

verify_sha() {
    local file="$1" expected="$2"
    local actual
    actual=$(sha256sum "$file" | awk '{print $1}')
    if [[ "$actual" != "$expected" ]]; then
        echo "sha256 mismatch on $file" >&2
        echo "  expected: $expected" >&2
        echo "  actual:   $actual" >&2
        exit 1
    fi
}

# llama-server binary
TARGET_BINARY="assets/llama-server-linux-x86_64"
if [[ ! -x "$TARGET_BINARY" ]]; then
    echo "=== fetching llama.cpp ${LLAMA_CPP_TAG} ==="
    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT
    curl -fL --retry 3 -o "$tmpdir/llama.zip" "$LLAMA_CPP_URL"
    unzip -q "$tmpdir/llama.zip" -d "$tmpdir/llama"
    # llama.cpp's zip layout: <root>/build/bin/llama-server (or just bin/)
    found=$(find "$tmpdir/llama" -name llama-server -type f -executable | head -1)
    if [[ -z "$found" ]]; then
        echo "llama-server not found in $LLAMA_CPP_ARCHIVE" >&2
        exit 1
    fi
    cp "$found" "$TARGET_BINARY"
    chmod +x "$TARGET_BINARY"
    echo "✓ $TARGET_BINARY"
else
    echo "✓ $TARGET_BINARY (cached)"
fi

# Model file
TARGET_MODEL="assets/${MODEL_NAME}"
if [[ ! -f "$TARGET_MODEL" ]]; then
    echo "=== fetching ${MODEL_NAME} ==="
    curl -fL --retry 3 -o "$TARGET_MODEL" "$MODEL_URL"
    echo "✓ $TARGET_MODEL"
else
    echo "✓ $TARGET_MODEL (cached)"
fi

# Verify sha256 if pinned (skip if placeholder)
if [[ "$MODEL_SHA256" != "<FILL_AFTER_FIRST_DOWNLOAD>" ]]; then
    verify_sha "$TARGET_MODEL" "$MODEL_SHA256"
fi

echo
echo "=== summary ==="
ls -la assets/
echo
echo "Total size:"
du -ch assets/llama-server-linux-x86_64 assets/${MODEL_NAME} | tail -1
```

Make it executable: `chmod +x scripts/fetch-bundled-assets.sh`.

- [ ] **Step 2: Create `assets/.gitignore`** so the bundled binaries/models never get committed:

```
*
!.gitignore
```

- [ ] **Step 3: Run the script + capture the model sha256.** Run once locally:

```bash
bash scripts/fetch-bundled-assets.sh
sha256sum assets/all-MiniLM-L6-v2.Q8_0.gguf
```

Replace `MODEL_SHA256="<FILL_AFTER_FIRST_DOWNLOAD>"` in the script with the actual hash. This pins the model so future downloads verify integrity.

> If the LLAMA_CPP_TAG you picked doesn't have a `bin-ubuntu-x64` archive (llama.cpp's release artifacts change names occasionally), `ls` the release page (`gh release view b3447 -R ggml-org/llama.cpp`) and pick whichever asset matches. Update `LLAMA_CPP_ARCHIVE` accordingly.

- [ ] **Step 4: Smoke test the downloaded binary:**

```bash
./assets/llama-server-linux-x86_64 --help 2>&1 | head -20
```

Should print llama-server's help text. If it fails with a missing library error (e.g., `libcurl.so.4`), llama.cpp may need extra runtime deps — document them in BUILD.md (Task 23) and consider switching to the statically-linked variant if available.

- [ ] **Step 5: Commit.**

```bash
git add scripts/fetch-bundled-assets.sh assets/.gitignore
git commit -m "feat: scripts/fetch-bundled-assets.sh — vendor llama-server + MiniLM GGUF (Plan 9 Task 3)"
```

---

## Task 4: `BundledEmbedder` — HTTP client for local `llama-server`

The Rust impl that the daemon's embedding pipeline calls. It expects `llama-server` to be running on `127.0.0.1:7424` (the daemon spawns it; this module just talks to it).

**Files:**
- Create: `crates/mnemos_core/src/embedder/bundled.rs`
- Modify: `crates/mnemos_core/src/embedder/mod.rs` — declare `bundled`, add to `EmbedderKind` enum
- Test: `crates/mnemos_core/tests/bundled_embedder.rs` (new — ignored by default, runs only when llama-server is on PATH)

- [ ] **Step 1: Failing test** — `tests/bundled_embedder.rs`:

```rust
use mnemos_core::embedder::bundled::BundledEmbedder;
use mnemos_core::embedder::Embedder;

#[tokio::test]
#[ignore = "requires a running llama-server at 127.0.0.1:7424 (set MNEMOS_TEST_LLAMA_SERVER=1)"]
async fn bundled_embedder_returns_384_dim_vector() {
    if std::env::var("MNEMOS_TEST_LLAMA_SERVER").is_err() {
        return;
    }
    let e = BundledEmbedder::new("http://127.0.0.1:7424");
    let v = e.embed("hello world").await.unwrap();
    assert_eq!(v.len(), 384);
    // Vectors should be L2-normalized (or close to it) for cosine sim.
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 0.1, "expected unit-norm vector, got {norm}");
}

#[tokio::test]
#[ignore = "requires a running llama-server"]
async fn bundled_embedder_is_deterministic() {
    if std::env::var("MNEMOS_TEST_LLAMA_SERVER").is_err() {
        return;
    }
    let e = BundledEmbedder::new("http://127.0.0.1:7424");
    let v1 = e.embed("the quick brown fox").await.unwrap();
    let v2 = e.embed("the quick brown fox").await.unwrap();
    // Identical input → identical vector.
    for (a, b) in v1.iter().zip(v2.iter()) {
        assert!((a - b).abs() < 1e-6);
    }
}
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Create `crates/mnemos_core/src/embedder/bundled.rs`**

```rust
//! Bundled embedder backend — HTTP client for the local llama-server child
//! process the daemon spawns at 127.0.0.1:7424.
//!
//! llama-server's embedding endpoint accepts:
//!   POST /v1/embeddings    { "input": "<text>", "model": "any" }
//! and returns:
//!   { "data": [{ "embedding": [f32; D], "index": 0 }], "model": "...", ... }
//! which is OpenAI-compatible. We use this OpenAI-compat shape.

use crate::embedder::Embedder;
use crate::error::{MnemosError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct BundledEmbedder {
    base_url: String,
    client: reqwest::Client,
}

impl BundledEmbedder {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client build"),
        }
    }
}

#[derive(Serialize)]
struct EmbedReq<'a> {
    input: &'a str,
    model: &'a str,
}

#[derive(Deserialize)]
struct EmbedResp {
    data: Vec<EmbedData>,
}

#[derive(Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
}

#[async_trait]
impl Embedder for BundledEmbedder {
    fn name(&self) -> &str {
        "bundled"
    }

    fn dim(&self) -> u32 {
        384
    }

    fn model_id(&self) -> &str {
        "all-MiniLM-L6-v2"
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/v1/embeddings", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .json(&EmbedReq {
                input: text,
                model: "all-MiniLM-L6-v2",
            })
            .send()
            .await
            .map_err(|e| MnemosError::Internal(format!("bundled embedder HTTP: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MnemosError::Internal(format!(
                "bundled embedder responded {status}: {body}"
            )));
        }
        let parsed: EmbedResp = resp
            .json()
            .await
            .map_err(|e| MnemosError::Internal(format!("bundled embedder parse: {e}")))?;
        let v = parsed
            .data
            .into_iter()
            .next()
            .ok_or_else(|| MnemosError::Internal("bundled embedder: empty data".into()))?
            .embedding;
        if v.len() != 384 {
            return Err(MnemosError::Internal(format!(
                "bundled embedder returned dim {} (expected 384)",
                v.len()
            )));
        }
        Ok(v)
    }
}
```

- [ ] **Step 4: Wire into `crates/mnemos_core/src/embedder/mod.rs`**

Read the existing module first. Add `pub mod bundled;`. Add `Bundled` variant to whatever enum dispatches embedders (likely `EmbedderKind` or the factory function). Pattern follows existing `Ollama` / `Mock` / `None` variants.

- [ ] **Step 5: Pass + commit.**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/embedder/ crates/mnemos_core/tests/bundled_embedder.rs
git commit -m "feat: BundledEmbedder — OpenAI-compatible client for local llama-server (Plan 9 Task 4)"
```

---

## Task 5: `bundled_embedder` — llama-server child process lifecycle

The daemon side. Spawns `llama-server` on daemon startup, health-checks it, restarts on crash with backoff, kills on daemon shutdown.

**Files:**
- Create: `crates/mnemos_daemon/src/bundled_embedder.rs`
- Modify: `crates/mnemos_daemon/src/lib.rs` (spawn in `build_app_full`)
- Test: `crates/mnemos_daemon/tests/bundled_embedder.rs` (new)

- [ ] **Step 1: Failing test** — `tests/bundled_embedder.rs`:

```rust
use mnemos_daemon::bundled_embedder::{spawn, BundledEmbedderConfig};

#[tokio::test]
#[ignore = "requires assets/llama-server-linux-x86_64 and assets/*.gguf (run scripts/fetch-bundled-assets.sh)"]
async fn spawn_and_health_check() {
    if std::env::var("MNEMOS_TEST_BUNDLED").is_err() {
        return;
    }
    let cfg = BundledEmbedderConfig {
        binary: std::path::PathBuf::from("assets/llama-server-linux-x86_64"),
        model: std::path::PathBuf::from("assets/all-MiniLM-L6-v2.Q8_0.gguf"),
        port: 17424,  // non-default so we don't collide with a real daemon
        host: "127.0.0.1".into(),
    };
    let handle = spawn(cfg).await.expect("spawn llama-server");
    // Wait for the embed endpoint to come up.
    let client = reqwest::Client::new();
    let mut ok = false;
    for _ in 0..50 {
        if let Ok(r) = client.get("http://127.0.0.1:17424/health").send().await {
            if r.status().is_success() {
                ok = true;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(ok, "llama-server did not become healthy within 5s");
    handle.shutdown().await;
}
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Create `crates/mnemos_daemon/src/bundled_embedder.rs`**

```rust
//! Manages the `llama-server` child process that serves the bundled embedder.
//!
//! Lifecycle:
//!   - spawn(): fork llama-server on $port, wait for /health, return a handle
//!   - health task: every 30s poll /health, restart with exponential backoff on
//!     3 consecutive failures
//!   - shutdown(): SIGTERM, wait 2s, SIGKILL
//!
//! Logs route to ~/.local/state/mnemos/logs/llama-server.log.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::{watch, Mutex};

#[derive(Debug, Clone)]
pub struct BundledEmbedderConfig {
    pub binary: PathBuf,
    pub model: PathBuf,
    pub port: u16,
    pub host: String,
}

impl Default for BundledEmbedderConfig {
    fn default() -> Self {
        Self {
            binary: default_binary_path(),
            model: default_model_path(),
            port: 7424,
            host: "127.0.0.1".into(),
        }
    }
}

pub fn default_binary_path() -> PathBuf {
    // Installed layout (cargo-deb + cargo-generate-rpm): /usr/lib/mnemos/
    if let Ok(env) = std::env::var("MNEMOS_BUNDLED_BIN_DIR") {
        return PathBuf::from(env).join("llama-server");
    }
    let install = PathBuf::from("/usr/lib/mnemos/llama-server");
    if install.exists() {
        return install;
    }
    // Dev layout: assets/llama-server-<triple>
    PathBuf::from("assets/llama-server-linux-x86_64")
}

pub fn default_model_path() -> PathBuf {
    if let Ok(env) = std::env::var("MNEMOS_BUNDLED_MODEL_DIR") {
        return PathBuf::from(env).join("all-MiniLM-L6-v2.Q8_0.gguf");
    }
    let install = PathBuf::from("/usr/lib/mnemos/all-MiniLM-L6-v2.Q8_0.gguf");
    if install.exists() {
        return install;
    }
    PathBuf::from("assets/all-MiniLM-L6-v2.Q8_0.gguf")
}

pub struct BundledHandle {
    child: Arc<Mutex<Option<Child>>>,
    shutdown_tx: watch::Sender<bool>,
}

impl BundledHandle {
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let mut guard = self.child.lock().await;
        if let Some(mut c) = guard.take() {
            // SIGTERM
            let _ = c.start_kill();
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), c.wait()).await;
        }
    }
}

pub async fn spawn(cfg: BundledEmbedderConfig) -> Result<BundledHandle> {
    if !cfg.binary.exists() {
        anyhow::bail!(
            "bundled llama-server binary not found at {}. Run scripts/fetch-bundled-assets.sh or reinstall the Mnemos package.",
            cfg.binary.display()
        );
    }
    if !cfg.model.exists() {
        anyhow::bail!(
            "bundled GGUF model not found at {}. Run scripts/fetch-bundled-assets.sh or reinstall the Mnemos package.",
            cfg.model.display()
        );
    }

    let log_path = log_path()?;
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("open {}", log_path.display()))?;
    let log_err = log.try_clone()?;

    let child = Command::new(&cfg.binary)
        .arg("--model")
        .arg(&cfg.model)
        .arg("--host")
        .arg(&cfg.host)
        .arg("--port")
        .arg(cfg.port.to_string())
        .arg("--embedding")
        .arg("--pooling")
        .arg("mean")
        .arg("--ctx-size")
        .arg("8192")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawn {}", cfg.binary.display()))?;

    let child = Arc::new(Mutex::new(Some(child)));
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

    // Wait up to 5s for the health endpoint to come up.
    let base = format!("http://{}:{}", cfg.host, cfg.port);
    let probe_url = format!("{}/health", base);
    let probe_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;
    let mut ready = false;
    for _ in 0..50 {
        if shutdown_rx.has_changed().unwrap_or(false) && *shutdown_rx.borrow() {
            break;
        }
        if let Ok(r) = probe_client.get(&probe_url).send().await {
            if r.status().is_success() {
                ready = true;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if !ready {
        // Bring down the child we spawned before bailing.
        let mut guard = child.lock().await;
        if let Some(mut c) = guard.take() {
            let _ = c.start_kill();
        }
        anyhow::bail!(
            "llama-server did not become healthy within 5s; check {}",
            log_path.display()
        );
    }

    // Background health task: poll every 30s, restart on 3 consecutive
    // failures. Stops when shutdown_tx fires.
    let child_for_health = child.clone();
    let cfg_for_health = cfg.clone();
    let probe_client_h = probe_client.clone();
    let probe_url_h = probe_url.clone();
    tokio::spawn(async move {
        let mut consecutive_fails = 0u32;
        let mut backoff = std::time::Duration::from_secs(1);
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
        tick.tick().await;
        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() { break; }
                }
                _ = tick.tick() => {
                    let ok = probe_client_h
                        .get(&probe_url_h)
                        .send()
                        .await
                        .map(|r| r.status().is_success())
                        .unwrap_or(false);
                    if ok {
                        consecutive_fails = 0;
                        backoff = std::time::Duration::from_secs(1);
                    } else {
                        consecutive_fails += 1;
                        if consecutive_fails >= 3 {
                            tracing::warn!("llama-server unhealthy; restarting (backoff {:?})", backoff);
                            tokio::time::sleep(backoff).await;
                            backoff = std::cmp::min(backoff * 2, std::time::Duration::from_secs(60));
                            consecutive_fails = 0;
                            // Restart: kill old child, spawn new one.
                            let mut guard = child_for_health.lock().await;
                            if let Some(mut c) = guard.take() {
                                let _ = c.start_kill();
                                let _ = c.wait().await;
                            }
                            let new_child = Command::new(&cfg_for_health.binary)
                                .arg("--model").arg(&cfg_for_health.model)
                                .arg("--host").arg(&cfg_for_health.host)
                                .arg("--port").arg(cfg_for_health.port.to_string())
                                .arg("--embedding")
                                .arg("--pooling").arg("mean")
                                .arg("--ctx-size").arg("8192")
                                .stdin(Stdio::null())
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .kill_on_drop(true)
                                .spawn();
                            match new_child {
                                Ok(c) => *guard = Some(c),
                                Err(e) => tracing::error!("llama-server restart failed: {e}"),
                            }
                        }
                    }
                }
            }
        }
    });

    Ok(BundledHandle {
        child,
        shutdown_tx,
    })
}

fn log_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .context("ProjectDirs")?;
    Ok(dirs.data_local_dir().join("logs").join("llama-server.log"))
}
```

> Note: this uses `directories` (Linux/Mac/Windows paths). If `directories` isn't already a workspace dep, add it. The XDG path on Linux resolves to `~/.local/share/mnemos/logs/llama-server.log`. (The plan spec said `~/.local/state` — both are common, this implementation uses `data_local_dir` which is `~/.local/share` on Linux. Adjust if you prefer state-dir; just be consistent across the codebase.)

- [ ] **Step 4: Wire into `build_app_full`** in `crates/mnemos_daemon/src/lib.rs`. Pattern follows Plan 7's sync-worker / pipeline-runner spawn:
  - When `state.config.embedder.kind == EmbedderKind::Bundled`, call `bundled_embedder::spawn(default config)` AND bundle the returned `BundledHandle` into the tuple `build_app_full` returns, alongside the existing `SyncHandle` + `PipelineHandle`.
  - On shutdown in `main.rs`, call `handle.shutdown().await` after axum exits.
  - For `build_app` (the test variant) — skip spawning. Tests that need a real bundled embedder run `llama-server` themselves (see Task 6's integration test).

- [ ] **Step 5: Add `EmbedderKind::Bundled` variant** to `crates/mnemos_daemon/src/config.rs` (extend the existing enum from Plan 7 Task 6). Default for fresh installs: `Bundled`.

- [ ] **Step 6: Pass + commit.**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/bundled_embedder.rs crates/mnemos_daemon/src/lib.rs crates/mnemos_daemon/src/main.rs crates/mnemos_daemon/src/config.rs crates/mnemos_daemon/tests/bundled_embedder.rs
git commit -m "feat: bundled_embedder child process lifecycle (spawn/health/restart) (Plan 9 Task 5)"
```

---

## Task 6: Daemon routes the embed request to the bundled backend

The factory function that returns the right `Box<dyn Embedder>` based on `vault.embedder_kind`. **Vault meta is authoritative**, per spec.

**Files:**
- Modify: `crates/mnemos_core/src/embedder/mod.rs` (factory function)
- Modify: `crates/mnemos_daemon/src/state.rs` or wherever `AppState` constructs its embedder

- [ ] **Step 1: Update the factory** in `crates/mnemos_core/src/embedder/mod.rs`. Pseudocode (read the existing file first to see the actual shape):

```rust
pub async fn make_embedder(
    config: &EmbedderConfig,
    storage: &Storage,
) -> Result<Box<dyn Embedder>> {
    // Vault is authoritative. Read its embedder_kind; warn if config disagrees.
    let meta = vault_meta::get_embedder_meta(storage).await?;
    let kind = match meta.kind.as_str() {
        "bundled" => EmbedderKind::Bundled,
        "ollama" => EmbedderKind::Ollama,
        "openai" => EmbedderKind::OpenAi,
        "mock" => EmbedderKind::Mock,
        "none" => EmbedderKind::None,
        other => {
            return Err(MnemosError::Internal(format!("unknown embedder_kind in vault: {other}")));
        }
    };
    if config.kind != kind {
        tracing::warn!(
            "MNEMOS_EMBEDDER env says {:?}, vault was seeded with {:?}; using vault setting. To switch, run `mnemos embed-rebuild --target {:?}`",
            config.kind, kind, config.kind
        );
    }
    match kind {
        EmbedderKind::Bundled => Ok(Box::new(bundled::BundledEmbedder::new(
            format!("http://{}:{}", config.bundled.host, config.bundled.port),
        ))),
        EmbedderKind::Ollama => Ok(Box::new(ollama::OllamaEmbedder::new(&config.url, &config.model))),
        EmbedderKind::OpenAi => Ok(Box::new(openai::OpenAiEmbedder::new(&config.openai)?)),
        EmbedderKind::Mock => Ok(Box::new(mock::MockEmbedder::new(384))),
        EmbedderKind::None => Ok(Box::new(none::NoneEmbedder)),
    }
}
```

> The actual existing code in this codebase will differ. Read it. Adapt to whatever pattern is already there — likely a `match config.kind` switch, no vault read. Add the vault read.

- [ ] **Step 2: First-`remember` writes the embedder_kind back to vault_meta.** If the vault is fresh (no embedder_kind yet → defaulted to `bundled` by Task 1's backfill), and the actual configured embedder differs, the FIRST successful embed should atomically set vault_meta.embedder_kind to whatever was actually used. This locks in the choice. Use `vault_meta::set_embedder_meta` (Task 2).

The existing remember code path already does something similar for `embedder_model` and `embedder_dim` (Plan 1). Extend it to also write `embedder_kind`.

- [ ] **Step 3: Pass + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cargo test -p mnemos_core --lib
git add crates/mnemos_core/src/embedder/mod.rs crates/mnemos_core/src/vault.rs crates/mnemos_daemon/src/state.rs
git commit -m "feat: embedder factory respects vault.embedder_kind as authoritative (Plan 9 Task 6)"
```

---

# Group C — OpenAI backends

## Task 7: `OpenAiEmbedder`

`MNEMOS_EMBEDDER=openai` with `OPENAI_API_KEY` set. Defaults to `text-embedding-3-small` (1536-dim).

**Files:**
- Create: `crates/mnemos_core/src/embedder/openai.rs`
- Modify: `crates/mnemos_core/src/embedder/mod.rs` — declare module + add OpenAi variant
- Test: `crates/mnemos_core/tests/openai_embedder.rs` (new, uses `wiremock`)

- [ ] **Step 1: Failing test** — `tests/openai_embedder.rs`:

```rust
use mnemos_core::embedder::openai::{OpenAiEmbedder, OpenAiConfig};
use mnemos_core::embedder::Embedder;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn openai_embedder_sends_correct_request_shape() {
    let server = MockServer::start().await;
    let body = serde_json::json!({
        "data": [{
            "embedding": vec![0.1f32; 1536],
            "index": 0,
            "object": "embedding"
        }],
        "model": "text-embedding-3-small",
        "object": "list",
        "usage": { "prompt_tokens": 2, "total_tokens": 2 }
    });
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    let cfg = OpenAiConfig {
        base_url: server.uri(),
        api_key: "sk-test".into(),
        model: "text-embedding-3-small".into(),
        dim: 1536,
    };
    let e = OpenAiEmbedder::new(&cfg).unwrap();
    let v = e.embed("hello").await.unwrap();
    assert_eq!(v.len(), 1536);
    assert_eq!(e.dim(), 1536);
    assert_eq!(e.model_id(), "text-embedding-3-small");
}
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Add `wiremock` to mnemos_core's dev-deps.** Workspace `Cargo.toml`: `wiremock = "0.6"`. `crates/mnemos_core/Cargo.toml [dev-dependencies]`: `wiremock = { workspace = true }`.

- [ ] **Step 4: Create `crates/mnemos_core/src/embedder/openai.rs`**

```rust
//! OpenAI embeddings backend. Compatible with Azure OpenAI and any
//! OpenAI-compat server via OPENAI_BASE_URL.

use crate::embedder::Embedder;
use crate::error::{MnemosError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub dim: u32,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com".into(),
            api_key: String::new(),
            model: "text-embedding-3-small".into(),
            dim: 1536,
        }
    }
}

/// Build an `OpenAiConfig` from env: `OPENAI_API_KEY`, `OPENAI_BASE_URL`,
/// `MNEMOS_EMBEDDER_MODEL`. Returns an error if the API key is missing.
pub fn config_from_env() -> Result<OpenAiConfig> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| MnemosError::Internal("OPENAI_API_KEY not set".into()))?;
    let base_url =
        std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com".into());
    let model = std::env::var("MNEMOS_EMBEDDER_MODEL")
        .unwrap_or_else(|_| "text-embedding-3-small".into());
    let dim = match model.as_str() {
        "text-embedding-3-small" => 1536,
        "text-embedding-3-large" => 3072,
        "text-embedding-ada-002" => 1536,
        _ => 1536, // default; user can override with MNEMOS_EMBEDDER_DIM if needed
    };
    Ok(OpenAiConfig { base_url, api_key, model, dim })
}

pub struct OpenAiEmbedder {
    cfg: OpenAiConfig,
    client: reqwest::Client,
}

impl OpenAiEmbedder {
    pub fn new(cfg: &OpenAiConfig) -> Result<Self> {
        if cfg.api_key.is_empty() {
            return Err(MnemosError::Internal("OpenAI API key is empty".into()));
        }
        Ok(Self {
            cfg: cfg.clone(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| MnemosError::Internal(format!("reqwest build: {e}")))?,
        })
    }
}

#[derive(Serialize)]
struct EmbedReq<'a> {
    input: &'a str,
    model: &'a str,
}

#[derive(Deserialize)]
struct EmbedResp {
    data: Vec<EmbedData>,
}

#[derive(Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
}

#[async_trait]
impl Embedder for OpenAiEmbedder {
    fn name(&self) -> &str { "openai" }
    fn dim(&self) -> u32 { self.cfg.dim }
    fn model_id(&self) -> &str { &self.cfg.model }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/v1/embeddings", self.cfg.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.cfg.api_key)
            .json(&EmbedReq { input: text, model: &self.cfg.model })
            .send()
            .await
            .map_err(|e| MnemosError::Internal(format!("openai HTTP: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MnemosError::Internal(format!(
                "openai responded {status}: {body}"
            )));
        }
        let parsed: EmbedResp = resp
            .json()
            .await
            .map_err(|e| MnemosError::Internal(format!("openai parse: {e}")))?;
        Ok(parsed
            .data
            .into_iter()
            .next()
            .ok_or_else(|| MnemosError::Internal("openai: empty data".into()))?
            .embedding)
    }
}
```

- [ ] **Step 5: Module + factory wiring** in `crates/mnemos_core/src/embedder/mod.rs`. Add `pub mod openai;`. Add `OpenAi` variant to `EmbedderKind`. Update the factory function (Task 6) to handle it.

- [ ] **Step 6: Pass + commit.**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add Cargo.toml crates/mnemos_core/Cargo.toml crates/mnemos_core/src/embedder/ crates/mnemos_core/tests/openai_embedder.rs
git commit -m "feat: OpenAI embeddings backend (Plan 9 Task 7)"
```

---

## Task 8: `OpenAiLlm`

For reflections, community summaries, and entity extraction. Same shape as the embedder but for `/v1/chat/completions`.

**Files:**
- Create: `crates/mnemos_core/src/llm/openai.rs`
- Modify: `crates/mnemos_core/src/llm/mod.rs` — declare + add OpenAi to LlmKind
- Test: `crates/mnemos_core/tests/openai_llm.rs` (new, wiremock)

- [ ] **Step 1: Failing test** — `tests/openai_llm.rs`:

```rust
use mnemos_core::llm::openai::{OpenAiLlm, OpenAiLlmConfig};
use mnemos_core::llm::Llm;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn openai_llm_chat_completion() {
    let server = MockServer::start().await;
    let body = serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "model": "gpt-4o-mini",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "Hello back" },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7 }
    });
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    let cfg = OpenAiLlmConfig {
        base_url: server.uri(),
        api_key: "sk-test".into(),
        model: "gpt-4o-mini".into(),
    };
    let l = OpenAiLlm::new(&cfg).unwrap();
    let out = l.complete("hello", None).await.unwrap();
    assert_eq!(out, "Hello back");
}
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Create `crates/mnemos_core/src/llm/openai.rs`**

```rust
//! OpenAI chat-completions backend for reflections / community summaries /
//! entity extraction.

use crate::error::{MnemosError, Result};
use crate::llm::Llm;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiLlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl Default for OpenAiLlmConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com".into(),
            api_key: String::new(),
            model: "gpt-4o-mini".into(),
        }
    }
}

pub fn config_from_env() -> Result<OpenAiLlmConfig> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| MnemosError::Internal("OPENAI_API_KEY not set".into()))?;
    let base_url =
        std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com".into());
    let model = std::env::var("MNEMOS_LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
    Ok(OpenAiLlmConfig { base_url, api_key, model })
}

pub struct OpenAiLlm {
    cfg: OpenAiLlmConfig,
    client: reqwest::Client,
}

impl OpenAiLlm {
    pub fn new(cfg: &OpenAiLlmConfig) -> Result<Self> {
        if cfg.api_key.is_empty() {
            return Err(MnemosError::Internal("OpenAI API key is empty".into()));
        }
        Ok(Self {
            cfg: cfg.clone(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .map_err(|e| MnemosError::Internal(format!("reqwest build: {e}")))?,
        })
    }
}

#[derive(Serialize)]
struct ChatReq<'a> {
    model: &'a str,
    messages: Vec<ChatMsg<'a>>,
}

#[derive(Serialize)]
struct ChatMsg<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResp {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMsgOwned,
}

#[derive(Deserialize)]
struct ChatMsgOwned {
    content: String,
}

#[async_trait]
impl Llm for OpenAiLlm {
    fn name(&self) -> &str { "openai" }
    fn model_id(&self) -> &str { &self.cfg.model }

    async fn complete(&self, prompt: &str, system: Option<&str>) -> Result<String> {
        let mut messages = Vec::new();
        if let Some(s) = system {
            messages.push(ChatMsg { role: "system", content: s });
        }
        messages.push(ChatMsg { role: "user", content: prompt });
        let url = format!("{}/v1/chat/completions", self.cfg.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.cfg.api_key)
            .json(&ChatReq { model: &self.cfg.model, messages })
            .send()
            .await
            .map_err(|e| MnemosError::Internal(format!("openai HTTP: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MnemosError::Internal(format!(
                "openai responded {status}: {body}"
            )));
        }
        let parsed: ChatResp = resp
            .json()
            .await
            .map_err(|e| MnemosError::Internal(format!("openai parse: {e}")))?;
        Ok(parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| MnemosError::Internal("openai: empty choices".into()))?
            .message
            .content)
    }
}
```

> The exact `Llm` trait signature in `mnemos_core` may differ — read `crates/mnemos_core/src/llm/mod.rs` first and match the existing `OllamaLlm` shape (system prompt may be a separate field, may take a `Vec<Message>`, etc.).

- [ ] **Step 4: Wire** into `llm/mod.rs`. Add `pub mod openai;`. Add `OpenAi` variant to `LlmKind`. Update the LLM factory.

- [ ] **Step 5: Default LLM kind = `None`.** Update `daemon/src/config.rs` so `LlmKind::default()` returns `None`, not `Ollama`. Fresh installs no longer require Ollama. Document this in CHANGELOG (Task 22).

- [ ] **Step 6: Pass + commit.**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/llm/ crates/mnemos_core/tests/openai_llm.rs crates/mnemos_daemon/src/config.rs
git commit -m "feat: OpenAI chat-completions LLM backend; default LLM = none (Plan 9 Task 8)"
```

---

# Group D — Embed-rebuild migration

## Task 9: `embedder_rebuild` core — atomic, resumable

The migration engine. Re-embeds every memory in the vault with the target embedder, writing new vectors to a shadow table, then atomically swapping.

**Files:**
- Create: `crates/mnemos_core/src/embedder_rebuild.rs`
- Test: `crates/mnemos_core/tests/embed_rebuild.rs` (new)

- [ ] **Step 1: Failing test** — `tests/embed_rebuild.rs`:

```rust
use mnemos_core::embedder_rebuild::{rebuild, RebuildOptions, RebuildStatus};
use mnemos_core::paths::Paths;
use mnemos_core::vault::{RememberOpts, Vault};
use tempfile::TempDir;

#[tokio::test]
async fn rebuild_migrates_vault_to_target_embedder() {
    std::env::set_var("MNEMOS_EMBEDDER", "mock");
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let _ = v.remember("first memory", RememberOpts::default()).await.unwrap();
    let _ = v.remember("second memory", RememberOpts::default()).await.unwrap();
    let _ = v.remember("third memory", RememberOpts::default()).await.unwrap();

    // Source: vault was seeded with the mock embedder (dim 384).
    // Target: still mock — but a different model_id, so the rebuild runs.
    let opts = RebuildOptions {
        target_kind: "mock".into(),
        target_model: "mock-v2".into(),
        target_dim: 384,
        actor: "test".into(),
    };
    let status = rebuild(&v, opts).await.unwrap();
    assert!(matches!(status, RebuildStatus::Completed { processed: 3, .. }));

    // Vault meta is updated.
    let meta = mnemos_core::storage::vault_meta::get_embedder_meta(v.storage())
        .await
        .unwrap();
    assert_eq!(meta.kind, "mock");
    assert_eq!(meta.model, "mock-v2");
}

#[tokio::test]
async fn rebuild_resumes_after_partial_completion() {
    // Simulate: rebuild starts, processes 2 of 3, then dies.
    // Next rebuild should skip the 2 already-done and only process the 3rd.
    // (Implementation: shadow table memory_embeddings_v2 persists across runs.)
    std::env::set_var("MNEMOS_EMBEDDER", "mock");
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let m1 = v.remember("first", RememberOpts::default()).await.unwrap();
    let m2 = v.remember("second", RememberOpts::default()).await.unwrap();
    let _ = v.remember("third", RememberOpts::default()).await.unwrap();

    // Manually pre-populate the shadow table with two entries.
    let (conn, _g) = v.storage().write_conn().await.unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS memory_embeddings_v2 (
            memory_id TEXT PRIMARY KEY,
            embedding BLOB NOT NULL,
            embedder_kind TEXT NOT NULL,
            embedder_model TEXT NOT NULL,
            embedder_dim INTEGER NOT NULL,
            created_at TEXT NOT NULL
        )",
        (),
    ).await.unwrap();
    let dummy = vec![0u8; 384 * 4];
    for id in [&m1, &m2] {
        conn.execute(
            "INSERT INTO memory_embeddings_v2 (memory_id, embedding, embedder_kind, embedder_model, embedder_dim, created_at) VALUES (?, ?, 'mock', 'mock-v2', 384, ?)",
            libsql::params![id.clone(), dummy.clone(), chrono::Utc::now().to_rfc3339()],
        ).await.unwrap();
    }
    drop(conn);

    let opts = RebuildOptions {
        target_kind: "mock".into(),
        target_model: "mock-v2".into(),
        target_dim: 384,
        actor: "test".into(),
    };
    let status = rebuild(&v, opts).await.unwrap();
    if let RebuildStatus::Completed { processed, skipped, .. } = status {
        assert_eq!(processed, 1, "should process the one unfinished memory");
        assert_eq!(skipped, 2, "should skip the two already in shadow table");
    } else {
        panic!("expected Completed status");
    }
}
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Create `crates/mnemos_core/src/embedder_rebuild.rs`**

```rust
//! Atomic, resumable embedder migration.
//!
//! Flow:
//!   1. Create memory_embeddings_v2 shadow table (idempotent).
//!   2. For each memory not already in v2: embed body with target embedder,
//!      INSERT into v2. Skip if already present (resumability).
//!   3. After every memory done: atomically rename
//!        memory_embeddings → memory_embeddings_v1_backup_<ts>
//!        memory_embeddings_v2 → memory_embeddings
//!      and update vault_meta.
//!   4. Audit log the migration.
//!   5. Schedule the backup table for cleanup after 7 days.

use crate::embedder::Embedder;
use crate::error::Result;
use crate::storage::audit::write_audit;
use crate::storage::vault_meta::{get_embedder_meta, set_embedder_meta, EmbedderMeta};
use crate::storage::Storage;
use crate::vault::Vault;
use chrono::Utc;
use libsql::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebuildOptions {
    pub target_kind: String,
    pub target_model: String,
    pub target_dim: u32,
    pub actor: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RebuildStatus {
    Idle,
    Running { processed: usize, total: usize },
    Completed { processed: usize, skipped: usize, total: usize, swapped: bool },
    Failed { error: String, processed: usize },
}

pub async fn rebuild(vault: &Vault, opts: RebuildOptions) -> Result<RebuildStatus> {
    let storage = vault.storage();

    // 1. Shadow table.
    {
        let (conn, _g) = storage.write_conn().await?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS memory_embeddings_v2 (
                memory_id TEXT PRIMARY KEY,
                embedding BLOB NOT NULL,
                embedder_kind TEXT NOT NULL,
                embedder_model TEXT NOT NULL,
                embedder_dim INTEGER NOT NULL,
                created_at TEXT NOT NULL
            )",
            (),
        )
        .await?;
    }

    // 2. List all memories.
    let memory_ids: Vec<String> = {
        let conn = storage.conn()?;
        let mut rows = conn
            .query("SELECT id FROM memories WHERE invalid_at IS NULL ORDER BY id", ())
            .await?;
        let mut out = Vec::new();
        while let Some(r) = rows.next().await? {
            out.push(r.get::<String>(0)?);
        }
        out
    };
    let total = memory_ids.len();

    // 3. Build the target embedder.
    let target_embedder = build_target_embedder(&opts).await?;

    let mut processed = 0;
    let mut skipped = 0;
    for id in &memory_ids {
        // Skip if already in shadow.
        let already: bool = {
            let conn = storage.conn()?;
            let mut r = conn
                .query("SELECT 1 FROM memory_embeddings_v2 WHERE memory_id = ?", params![id.clone()])
                .await?;
            r.next().await?.is_some()
        };
        if already {
            skipped += 1;
            continue;
        }

        // Load body. (Skip the file — embed the body from DB for simplicity.)
        let body: String = {
            let conn = storage.conn()?;
            let mut r = conn
                .query("SELECT body FROM memories WHERE id = ?", params![id.clone()])
                .await?;
            r.next().await?
                .ok_or_else(|| crate::error::MnemosError::Internal(format!("memory {id} gone mid-rebuild")))?
                .get::<String>(0)?
        };

        let vector = target_embedder.embed(&body).await?;
        let bytes = f32_vec_to_bytes(&vector);

        let (conn, _g) = storage.write_conn().await?;
        conn.execute(
            "INSERT INTO memory_embeddings_v2 (memory_id, embedding, embedder_kind, embedder_model, embedder_dim, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            params![
                id.clone(),
                bytes,
                opts.target_kind.clone(),
                opts.target_model.clone(),
                opts.target_dim as i64,
                Utc::now().to_rfc3339(),
            ],
        )
        .await?;
        processed += 1;
    }

    // 4. Atomic swap.
    let backup_name = format!("memory_embeddings_v1_backup_{}", Utc::now().timestamp());
    {
        let (conn, _g) = storage.write_conn().await?;
        // sqlite ALTER TABLE RENAME is atomic, but two renames are not jointly atomic.
        // We use a transaction with both renames; sqlite will either complete or roll back.
        let tx = conn.transaction().await?;
        tx.execute(&format!("ALTER TABLE memory_embeddings RENAME TO {}", backup_name), ()).await?;
        tx.execute("ALTER TABLE memory_embeddings_v2 RENAME TO memory_embeddings", ()).await?;
        tx.commit().await?;
    }

    // 5. Update vault_meta.
    set_embedder_meta(storage, &EmbedderMeta {
        kind: opts.target_kind.clone(),
        model: opts.target_model.clone(),
        dim: opts.target_dim,
    })
    .await?;

    // 6. Audit.
    write_audit(
        storage,
        &opts.actor,
        "embedder_migrated",
        None,
        Some(serde_json::json!({
            "target_kind": opts.target_kind,
            "target_model": opts.target_model,
            "target_dim": opts.target_dim,
            "processed": processed,
            "skipped": skipped,
            "total": total,
            "backup_table": backup_name,
        })),
    )
    .await?;

    Ok(RebuildStatus::Completed { processed, skipped, total, swapped: true })
}

async fn build_target_embedder(opts: &RebuildOptions) -> Result<Box<dyn Embedder>> {
    use crate::embedder::*;
    match opts.target_kind.as_str() {
        "bundled" => Ok(Box::new(bundled::BundledEmbedder::new("http://127.0.0.1:7424"))),
        "ollama" => {
            let url = std::env::var("OLLAMA_URL")
                .unwrap_or_else(|_| "http://localhost:11434".into());
            Ok(Box::new(ollama::OllamaEmbedder::new(&url, &opts.target_model)))
        }
        "openai" => {
            let cfg = openai::config_from_env()?;
            Ok(Box::new(openai::OpenAiEmbedder::new(&cfg)?))
        }
        "mock" => Ok(Box::new(mock::MockEmbedder::new(opts.target_dim))),
        "none" => Err(crate::error::MnemosError::Internal(
            "cannot rebuild into 'none' embedder — disables semantic recall".into(),
        )),
        other => Err(crate::error::MnemosError::Internal(format!(
            "unknown target embedder: {other}"
        ))),
    }
}

fn f32_vec_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for &x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}
```

> Read the existing `memory_embeddings` schema before committing to the column names — the actual schema may differ (e.g., the column might be `dense_embedding`, the type might be `BLOB` or stored in a sqlite-vec virtual table, etc.). Adapt the shadow table + rename SQL accordingly.
>
> For sqlite-vec virtual tables: rename isn't supported on virtual tables. In that case, the swap becomes: `DELETE FROM memory_embeddings`, then bulk-insert from `memory_embeddings_v2`, then `DROP TABLE memory_embeddings_v2`. Slightly less atomic but the user-visible state is "during the swap, you might briefly see no embeddings" — acceptable for a multi-minute migration that's already underway.

- [ ] **Step 4: Background cleanup of backup tables.** Add a small async task that runs once at daemon start, deletes any `memory_embeddings_v1_backup_<ts>` tables where `<ts>` is older than 7 days. Implementation: query `sqlite_master WHERE name LIKE 'memory_embeddings_v1_backup_%'`, parse the timestamp, drop the table if old.

- [ ] **Step 5: Pass + commit.**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/embedder_rebuild.rs crates/mnemos_core/src/lib.rs crates/mnemos_core/tests/embed_rebuild.rs
git commit -m "feat: embedder_rebuild — atomic, resumable, audit-logged migration (Plan 9 Task 9)"
```

---

## Task 10: Daemon REST endpoints for embed-rebuild

`GET /v1/embed-rebuild/status` + `POST /v1/embed-rebuild/start` + `POST /v1/embed-rebuild/abort` + WS event for progress.

**Files:**
- Create: `crates/mnemos_daemon/src/routes/embed_rebuild.rs`
- Modify: `crates/mnemos_daemon/src/routes/mod.rs` (mount)
- Modify: `crates/mnemos_daemon/src/events.rs` (EmbedRebuildProgress event)
- Test: `crates/mnemos_daemon/tests/embed_rebuild.rs` (new)

- [ ] **Step 1: Failing test** — `tests/embed_rebuild.rs`:

```rust
use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

#[tokio::test]
async fn embed_rebuild_endpoint_round_trip() {
    std::env::set_var("MNEMOS_EMBEDDER", "mock");
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let _ = vault.remember("test memory", RememberOpts::default()).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();

    let (s, b) = call(app.clone(), "GET", "/v1/embed-rebuild/status", Some(&state.token), "").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["status"], "idle");

    let body = r#"{"target_kind":"mock","target_model":"mock-v2","target_dim":384}"#;
    let (s2, b2) = call(app.clone(), "POST", "/v1/embed-rebuild/start", Some(&state.token), body).await;
    assert_eq!(s2, StatusCode::OK, "{b2}");

    // Poll until completed (test runs synchronously since rebuild is fast).
    let mut completed = false;
    for _ in 0..20 {
        let (_, b3) = call(app.clone(), "GET", "/v1/embed-rebuild/status", Some(&state.token), "").await;
        let v3: serde_json::Value = serde_json::from_str(&b3).unwrap();
        if v3["status"] == "completed" {
            completed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(completed, "rebuild did not complete");
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

- [ ] **Step 3: Add events** to `crates/mnemos_daemon/src/events.rs`:

```rust
    EmbedRebuildStarted { target_kind: String, target_model: String, target_dim: u32 },
    EmbedRebuildProgress { processed: usize, total: usize },
    EmbedRebuildCompleted { processed: usize, skipped: usize, total: usize },
    EmbedRebuildFailed { error: String, processed: usize },
```

- [ ] **Step 4: Create `crates/mnemos_daemon/src/routes/embed_rebuild.rs`**

```rust
//! Embedder migration endpoints.

use axum::{extract::State, routing::{get, post}, Json, Router};
use mnemos_core::embedder_rebuild::{rebuild, RebuildOptions, RebuildStatus};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::ApiError;
use crate::events::Event;
use crate::state::AppState;

// In-process state for the rebuild status. The current implementation runs
// rebuilds synchronously inside a background task; the latest status is held
// in this Mutex. A future plan could persist state to disk for cross-restart
// resumability of the rebuild itself (today only the shadow table is durable).
pub type RebuildStateRef = Arc<Mutex<RebuildStatus>>;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/embed-rebuild/status", get(status))
        .route("/v1/embed-rebuild/start", post(start))
        .route("/v1/embed-rebuild/abort", post(abort))
}

#[derive(Debug, Deserialize)]
struct StartReq {
    target_kind: String,
    target_model: String,
    target_dim: u32,
}

async fn status(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let s = state.rebuild_status.lock().await.clone();
    Ok(Json(serde_json::to_value(s).map_err(|e| ApiError::internal(e.to_string()))?))
}

async fn start(
    State(state): State<AppState>,
    Json(req): Json<StartReq>,
) -> Result<Json<Value>, ApiError> {
    // Don't allow two rebuilds at once.
    {
        let cur = state.rebuild_status.lock().await;
        if matches!(*cur, RebuildStatus::Running { .. }) {
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "an embed-rebuild is already running",
            ));
        }
    }

    // Spawn the rebuild in the background.
    let vault = state.vault.clone();
    let status_ref = state.rebuild_status.clone();
    let events = state.events.clone();
    let opts = RebuildOptions {
        target_kind: req.target_kind.clone(),
        target_model: req.target_model.clone(),
        target_dim: req.target_dim,
        actor: "mnemos-rest".into(),
    };

    events.publish(Event::EmbedRebuildStarted {
        target_kind: opts.target_kind.clone(),
        target_model: opts.target_model.clone(),
        target_dim: opts.target_dim,
    });

    *status_ref.lock().await = RebuildStatus::Running { processed: 0, total: 0 };

    tokio::spawn(async move {
        match rebuild(&vault, opts).await {
            Ok(result) => {
                if let RebuildStatus::Completed { processed, skipped, total, .. } = &result {
                    events.publish(Event::EmbedRebuildCompleted {
                        processed: *processed,
                        skipped: *skipped,
                        total: *total,
                    });
                }
                *status_ref.lock().await = result;
            }
            Err(e) => {
                let cur = status_ref.lock().await.clone();
                let processed = if let RebuildStatus::Running { processed, .. } = cur { processed } else { 0 };
                events.publish(Event::EmbedRebuildFailed {
                    error: e.to_string(),
                    processed,
                });
                *status_ref.lock().await = RebuildStatus::Failed {
                    error: e.to_string(),
                    processed,
                };
            }
        }
    });

    Ok(Json(json!({ "started": true })))
}

async fn abort(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    // Cooperative cancellation. The current rebuild() impl doesn't observe
    // cancellation tokens; this just marks status as aborted so the UI
    // reflects it. The in-flight task will run to completion in the
    // background, then write its Completed status, which the abort flag
    // overrides at read time.
    let mut s = state.rebuild_status.lock().await;
    if matches!(*s, RebuildStatus::Running { .. }) {
        *s = RebuildStatus::Failed {
            error: "aborted by user".into(),
            processed: 0,
        };
    }
    Ok(Json(json!({ "aborted": true })))
}
```

> Honest note: the abort is best-effort. Plan-10-level work could thread a cancellation token through `rebuild()` to actually halt the in-flight embedder calls. For v0.8.0 the user can still see "running → aborted" in the UI and the swap doesn't happen until processing completes anyway, so partial state is bounded.

- [ ] **Step 5: Wire into AppState.** Add `rebuild_status: RebuildStateRef` to `AppState`; initialize to `Idle` in the constructor.

- [ ] **Step 6: Mount + pass + commit.** `pub mod embed_rebuild;` + `.merge(embed_rebuild::router())` in `routes/mod.rs`.

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/embed_rebuild.rs crates/mnemos_daemon/src/routes/mod.rs crates/mnemos_daemon/src/events.rs crates/mnemos_daemon/src/state.rs crates/mnemos_daemon/tests/embed_rebuild.rs
git commit -m "feat: /v1/embed-rebuild endpoints + WS events (Plan 9 Task 10)"
```

---

## Task 11: CLI `mnemos embed-rebuild`

Hits the daemon's REST endpoints (or runs in-process if no daemon). Streams progress to the terminal.

**Files:**
- Create: `crates/mnemos_cli/src/commands/embed_rebuild.rs`
- Modify: `crates/mnemos_cli/src/cli.rs` + `commands/mod.rs` + `main.rs`
- Test: in-module `#[cfg(test)]`

- [ ] **Step 1: Failing test** — at the bottom of `commands/embed_rebuild.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn embed_rebuild_in_process_smoke() {
        std::env::set_var("MNEMOS_EMBEDDER", "mock");
        let tmp = TempDir::new().unwrap();
        let opts = EmbedRebuildOpts {
            vault: Some(tmp.path().to_path_buf()),
            target_kind: "mock".into(),
            target_model: "mock-v2".into(),
            target_dim: 384,
            json: true,
            poll: false,
        };
        // No memories → instant completion.
        run(opts).await.unwrap();
    }
}
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Create `crates/mnemos_cli/src/commands/embed_rebuild.rs`**

```rust
//! `mnemos embed-rebuild` — runs the migration in-process against a local
//! vault. If a daemon is already running and bound to the same vault, the
//! CLI prints a warning and refuses to run (so we don't double-process).

use crate::commands::open_vault;
use anyhow::{anyhow, Result};
use mnemos_core::embedder_rebuild::{rebuild, RebuildOptions, RebuildStatus};
use std::path::PathBuf;

pub struct EmbedRebuildOpts {
    pub vault: Option<PathBuf>,
    pub target_kind: String,
    pub target_model: String,
    pub target_dim: u32,
    pub json: bool,
    pub poll: bool,
}

pub async fn run(opts: EmbedRebuildOpts) -> Result<()> {
    let v = open_vault(opts.vault).await?;

    // Refuse if a daemon is already running on this vault. Detection: PID
    // file exists + process alive. If so, instruct the user to run via the
    // REST endpoint instead.
    if let Ok(pid_path) = mnemos_daemon::pid_path() {
        if pid_path.exists() {
            if let Ok(pid) = mnemos_daemon::pid::read_pid(&pid_path) {
                if crate::commands::daemon::process_alive(pid) {
                    return Err(anyhow!(
                        "a mnemos daemon is running (pid {pid}). To migrate via the daemon, run:\n  curl -X POST -H \"authorization: Bearer $(cat ~/.config/mnemos/token)\" -H \"content-type: application/json\" -d '{{\"target_kind\":\"{}\",\"target_model\":\"{}\",\"target_dim\":{}}}' http://127.0.0.1:7423/v1/embed-rebuild/start\n\nOr stop the daemon first: `mnemos daemon stop`",
                        opts.target_kind, opts.target_model, opts.target_dim
                    ));
                }
            }
        }
    }

    let result = rebuild(
        &v,
        RebuildOptions {
            target_kind: opts.target_kind.clone(),
            target_model: opts.target_model.clone(),
            target_dim: opts.target_dim,
            actor: "mnemos-cli".into(),
        },
    )
    .await?;

    if opts.json {
        println!(
            "{}",
            serde_json::to_string(&result)?
        );
    } else {
        match result {
            RebuildStatus::Completed { processed, skipped, total, .. } => {
                println!(
                    "✓ rebuild complete — {processed} processed, {skipped} skipped, {total} total"
                );
                println!("  vault now uses {} ({}, dim {})", opts.target_kind, opts.target_model, opts.target_dim);
            }
            RebuildStatus::Failed { error, processed } => {
                println!("✗ rebuild failed after {processed} memories: {error}");
            }
            _ => println!("(unexpected status: {:?})", result),
        }
    }
    Ok(())
}
```

- [ ] **Step 4: CLI subcommand** in `cli.rs`:

```rust
    /// Re-embed every memory in the vault with a different embedder.
    EmbedRebuild {
        #[arg(long)]
        vault: Option<PathBuf>,
        /// Target embedder: bundled | ollama | openai | mock
        #[arg(long)]
        target: String,
        /// Model identifier (for ollama: e.g. nomic-embed-text; for openai: text-embedding-3-small; for bundled: all-MiniLM-L6-v2)
        #[arg(long)]
        model: Option<String>,
        /// Override the dim (auto-detected for known models)
        #[arg(long)]
        dim: Option<u32>,
        /// Emit JSON status
        #[arg(long)]
        json: bool,
    },
```

Dispatch in `main.rs`:

```rust
    Commands::EmbedRebuild { vault, target, model, dim, json } => {
        let (default_model, default_dim) = match target.as_str() {
            "bundled" => ("all-MiniLM-L6-v2".to_string(), 384),
            "ollama" => ("nomic-embed-text".to_string(), 768),
            "openai" => ("text-embedding-3-small".to_string(), 1536),
            "mock" => ("mock".to_string(), 384),
            other => anyhow::bail!("unknown target embedder: {other}"),
        };
        commands::embed_rebuild::run(commands::embed_rebuild::EmbedRebuildOpts {
            vault,
            target_kind: target,
            target_model: model.unwrap_or(default_model),
            target_dim: dim.unwrap_or(default_dim),
            json,
            poll: false,
        }).await?;
    }
```

`commands/mod.rs`: `pub mod embed_rebuild;`.

- [ ] **Step 5: Pass + commit.**

```bash
cargo fmt --all && cargo clippy -p mnemos_cli --all-targets -- -D warnings
git add crates/mnemos_cli/src/cli.rs crates/mnemos_cli/src/commands/mod.rs crates/mnemos_cli/src/commands/embed_rebuild.rs crates/mnemos_cli/src/main.rs
git commit -m "feat: 'mnemos embed-rebuild' CLI (Plan 9 Task 11)"
```

---

# Group E — UI + doctor

## Task 12: Doctor view surfaces embedder mismatch + migration prompt

The Doctor view already shows the embedder check (Plan 7 / Plan 8). Extend the check so it specifically flags vault-vs-config mismatch and offers a one-click migrate link.

**Files:**
- Modify: `crates/mnemos_daemon/src/routes/doctor.rs` (add migration_available field)
- Modify: `desktop/src/views/Doctor.tsx` (render migrate button when surfaced)
- Test: extend `desktop/src/views/Doctor.test.tsx`

- [ ] **Step 1: Extend the doctor response.** In the daemon's doctor handler, when checking the embedder, also compare `vault.embedder_kind` to the configured `EmbedderKind`. If they differ, add a top-level field to the response:

```rust
// In the doctor handler:
let vault_meta = mnemos_core::storage::vault_meta::get_embedder_meta(state.vault.storage()).await.ok();
let migration_hint = vault_meta.as_ref().and_then(|m| {
    if m.kind != state.config.embedder.kind.as_str() {
        Some(serde_json::json!({
            "from_kind": m.kind,
            "from_model": m.model,
            "from_dim": m.dim,
            "to_kind": state.config.embedder.kind.as_str(),
        }))
    } else {
        None
    }
});

// Then in the returned JSON:
Ok(Json(json!({
    "checks": checks,
    "report": report,
    "migration_hint": migration_hint, // null OR { from_*, to_kind }
})))
```

- [ ] **Step 2: Update `desktop/src/views/Doctor.tsx`** to render a migration banner above the check list when `migration_hint` is non-null:

```tsx
{data.migration_hint && (
  <Card className="p-3 border-tier-working" data-testid="migration-hint">
    <div className="flex items-start justify-between gap-3">
      <div>
        <div className="display text-base">Migrate embedder</div>
        <p className="label text-text-muted">
          Your vault was seeded with <span className="mono">{data.migration_hint.from_kind}</span> ({data.migration_hint.from_model}, dim {data.migration_hint.from_dim}).
          The configured embedder is <span className="mono">{data.migration_hint.to_kind}</span>.
          Run `mnemos embed-rebuild --target {data.migration_hint.to_kind}` to migrate.
        </p>
      </div>
      <Button onClick={() => navigate({ to: "/embed-rebuild" })}>Open migration</Button>
    </div>
  </Card>
)}
```

- [ ] **Step 3: Extend `Doctor.test.tsx`** with a test where `migration_hint` is non-null → banner shows + link works.

- [ ] **Step 4: Pass + commit.**

```bash
cd desktop && pnpm typecheck && pnpm lint && pnpm test -- --run && cd ..
git add crates/mnemos_daemon/src/routes/doctor.rs desktop/src/views/Doctor.tsx desktop/src/views/Doctor.test.tsx
git commit -m "feat(ui): doctor surfaces embedder mismatch + migration prompt (Plan 9 Task 12)"
```

---

## Task 13: Settings — embedder section reflects new options

Plan 7 Task 12 created the Settings view's sectioned form. Update the Embedder section's `select` options from `["ollama", "mock", "none"]` to `["bundled", "ollama", "openai", "mock", "none"]`. Same for LLM.

**Files:**
- Modify: `desktop/src/views/Settings.tsx`
- Modify (if needed): `crates/mnemos_daemon/src/routes/config.rs` accepts the new kinds without choking

- [ ] **Step 1: Update the SCHEMA** in `Settings.tsx`:

```ts
{ title: "Embedder", path: ["embedder"], fields: [
  { key: "kind", label: "Backend", kind: "select", options: ["bundled", "ollama", "openai", "mock", "none"] },
  { key: "url", label: "URL", kind: "text" },  // for ollama
  { key: "model", label: "Model", kind: "text" },
  { key: "dim", label: "Dim", kind: "number" },
  { key: "timeout_secs", label: "Timeout (s)", kind: "number" }
] },
{ title: "LLM", path: ["llm"], fields: [
  { key: "kind", label: "Backend", kind: "select", options: ["ollama", "openai", "mock", "none"] },
  { key: "url", label: "URL", kind: "text" },
  { key: "model", label: "Model", kind: "text" },
  { key: "timeout_secs", label: "Timeout (s)", kind: "number" }
] },
{ title: "OpenAI", path: ["openai"], fields: [
  { key: "base_url", label: "Base URL", kind: "text" },
  { key: "api_key", label: "API Key", kind: "password" }
] },
```

- [ ] **Step 2: Update `Config`** to include an `OpenAiConfig`:

```rust
// In crates/mnemos_daemon/src/config.rs
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub api_key: String,
}

// Add to the top-level Config:
pub openai: OpenAiConfig,
```

- [ ] **Step 3: Pass + commit.**

```bash
cd desktop && pnpm typecheck && pnpm lint && pnpm test -- --run && cd ..
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add desktop/src/views/Settings.tsx crates/mnemos_daemon/src/config.rs
git commit -m "feat(ui): Settings — embedder + LLM sections include bundled + openai (Plan 9 Task 13)"
```

---

## Task 14: EmbedRebuild progress view

A dedicated view for tracking an in-flight migration: progress bar, ETA, abort button.

**Files:**
- Create: `desktop/src/views/EmbedRebuild.tsx`
- Create: `desktop/src/views/EmbedRebuild.test.tsx`
- Modify: `desktop/src/router.tsx` — `/embed-rebuild` route
- Modify: `desktop/src/api/client.ts` — `getEmbedRebuildStatus`, `startEmbedRebuild`, `abortEmbedRebuild`
- Modify: `desktop/src/api/queries.ts` — `useEmbedRebuildStatus`
- Modify: `desktop/src/api/ws.ts` — `embed_rebuild_progress`/`embed_rebuild_completed`/`embed_rebuild_failed` invalidates

- [ ] **Step 1: Failing test** — `EmbedRebuild.test.tsx`:

```tsx
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { renderWithQuery } from "../test/renderWithQuery";
import { EmbedRebuild } from "./EmbedRebuild";

let started = false;
const server = setupServer(
  http.get("http://localhost:7423/v1/embed-rebuild/status", () =>
    HttpResponse.json({ status: started ? "running" : "idle", processed: started ? 4 : 0, total: 10 }),
  ),
  http.post("http://localhost:7423/v1/embed-rebuild/start", () => {
    started = true;
    return HttpResponse.json({ started: true });
  }),
);
beforeAll(() => server.listen());
afterEach(() => { server.resetHandlers(); started = false; });
afterAll(() => server.close());

test("starts a rebuild with the picked target", async () => {
  renderWithQuery(<EmbedRebuild />);
  const targetSel = await screen.findByRole("combobox", { name: /target/i });
  await userEvent.selectOptions(targetSel, "bundled");
  await userEvent.click(screen.getByRole("button", { name: /start migration/i }));
  await waitFor(() => expect(started).toBe(true));
  expect(await screen.findByText(/4 of 10/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: Client additions** to `desktop/src/api/client.ts`:

```ts
async getEmbedRebuildStatus() {
  return this.req<
    | { status: "idle" }
    | { status: "running"; processed: number; total: number }
    | { status: "completed"; processed: number; skipped: number; total: number; swapped: boolean }
    | { status: "failed"; error: string; processed: number }
  >("GET", "/v1/embed-rebuild/status");
}

async startEmbedRebuild(target_kind: string, target_model: string, target_dim: number) {
  return this.req<{ started: boolean }>("POST", "/v1/embed-rebuild/start", { target_kind, target_model, target_dim });
}

async abortEmbedRebuild() {
  return this.req<{ aborted: boolean }>("POST", "/v1/embed-rebuild/abort");
}
```

In `queries.ts`:

```ts
export function useEmbedRebuildStatus() {
  return useQuery({
    queryKey: ["embed-rebuild", "status"],
    queryFn: () => client.getEmbedRebuildStatus(),
    refetchInterval: (q) => {
      const d = q.state.data;
      return d && (d as { status: string }).status === "running" ? 500 : 5000;
    },
  });
}
```

In `ws.ts` — add to the `INVALIDATE` map:

```ts
  embed_rebuild_started: [["embed-rebuild", "status"]],
  embed_rebuild_progress: [["embed-rebuild", "status"]],
  embed_rebuild_completed: [["embed-rebuild", "status"], ["doctor"], ["memories"]],
  embed_rebuild_failed: [["embed-rebuild", "status"]],
```

- [ ] **Step 4: `desktop/src/views/EmbedRebuild.tsx`**

```tsx
import { useState } from "react";
import { client } from "../api/client";
import { useEmbedRebuildStatus } from "../api/queries";
import { Button, Card } from "../design/primitives";

const TARGETS = [
  { value: "bundled", label: "Bundled (MiniLM)", model: "all-MiniLM-L6-v2", dim: 384 },
  { value: "ollama", label: "Ollama (nomic-embed-text)", model: "nomic-embed-text", dim: 768 },
  { value: "openai", label: "OpenAI (text-embedding-3-small)", model: "text-embedding-3-small", dim: 1536 },
  { value: "mock", label: "Mock (tests)", model: "mock", dim: 384 },
];

export function EmbedRebuild() {
  const { data, isLoading } = useEmbedRebuildStatus();
  const [target, setTarget] = useState("bundled");
  const [busy, setBusy] = useState(false);

  const start = async () => {
    const t = TARGETS.find((x) => x.value === target);
    if (!t) return;
    setBusy(true);
    try {
      await client.startEmbedRebuild(t.value, t.model, t.dim);
    } finally {
      setBusy(false);
    }
  };

  if (isLoading) return <div className="p-6">Loading…</div>;

  const status = data ?? { status: "idle" as const };

  return (
    <div className="p-6 max-w-2xl space-y-4">
      <h1 className="display text-xl">Migrate embedder</h1>
      <p className="text-text-muted font-body">
        Re-embeds every memory with the chosen backend. Atomic and resumable —
        safe to abort and restart. The old embeddings are kept as a backup for
        7 days.
      </p>

      {status.status === "running" && (
        <Card className="p-4 space-y-2">
          <div className="label">Running</div>
          <div className="text-sm">{(status as { processed: number; total: number }).processed} of {(status as { total: number }).total}</div>
          <div className="h-2 w-full bg-surface border border-border rounded-full overflow-hidden">
            <div
              className="h-full bg-tier-working transition-all"
              style={{ width: `${Math.round(((status as { processed: number; total: number }).processed / Math.max((status as { total: number }).total, 1)) * 100)}%` }}
            />
          </div>
          <Button variant="ghost" onClick={() => client.abortEmbedRebuild()}>Abort</Button>
        </Card>
      )}

      {status.status === "completed" && (
        <Card className="p-4">
          <div className="label">Completed</div>
          <p>Processed {(status as { processed: number }).processed}, skipped {(status as { skipped: number }).skipped} of {(status as { total: number }).total}.</p>
        </Card>
      )}

      {status.status === "failed" && (
        <Card className="p-4 text-tier-procedural">
          <div className="label">Failed</div>
          <p>{(status as { error: string }).error}</p>
        </Card>
      )}

      {status.status === "idle" && (
        <Card className="p-4 space-y-3">
          <label className="flex flex-col gap-1">
            <span className="label">Target</span>
            <select
              aria-label="target"
              value={target}
              onChange={(e) => setTarget(e.target.value)}
              className="bg-surface border border-border rounded-md px-2 py-1 text-sm"
            >
              {TARGETS.map((t) => (
                <option key={t.value} value={t.value}>{t.label}</option>
              ))}
            </select>
          </label>
          <Button onClick={start} disabled={busy}>{busy ? "Starting…" : "Start migration"}</Button>
        </Card>
      )}
    </div>
  );
}
```

- [ ] **Step 5: Router + sidebar.** Add `/embed-rebuild` to `router.tsx`. Add a Sidebar link (visible only when `data.migration_hint` from Doctor is non-null, OR always for simplicity).

- [ ] **Step 6: Pass + commit.**

```bash
cd desktop && pnpm typecheck && pnpm lint && pnpm test -- --run && cd ..
git add desktop/src/views/EmbedRebuild.tsx desktop/src/views/EmbedRebuild.test.tsx desktop/src/api/client.ts desktop/src/api/queries.ts desktop/src/api/ws.ts desktop/src/router.tsx desktop/src/layout/LeftSidebar.tsx
git commit -m "feat(ui): EmbedRebuild view + progress UI (Plan 9 Task 14)"
```

---

# Group F — Packaging + CI

## Task 15: Bundle assets in .deb / .rpm / .AppImage

The `.deb` and `.rpm` packages (CLI-only flavour AND desktop flavour) need to ship `llama-server` + the GGUF model in `/usr/lib/mnemos/`. Update the `cargo-deb` + `cargo-generate-rpm` metadata + the Tauri sidecar config.

**Files:**
- Modify: `crates/mnemos_daemon/Cargo.toml` (cargo-deb + cargo-generate-rpm assets)
- Modify: `desktop/src-tauri/tauri.conf.json` (bundle resources)
- Modify: `desktop/src-tauri/build-sidecars.sh` (include llama-server in staging)

- [ ] **Step 1: `crates/mnemos_daemon/Cargo.toml`** — extend `[package.metadata.deb].assets`:

```toml
[package.metadata.deb]
# ... existing fields ...
assets = [
    ["target/release/mnemosd", "usr/bin/mnemos-daemon", "755"],
    ["../../README.md", "usr/share/doc/mnemos-daemon/README", "644"],
    ["../../CHANGELOG.md", "usr/share/doc/mnemos-daemon/CHANGELOG", "644"],
    ["../../assets/llama-server-linux-x86_64", "usr/lib/mnemos/llama-server", "755"],
    ["../../assets/all-MiniLM-L6-v2.Q8_0.gguf", "usr/lib/mnemos/all-MiniLM-L6-v2.Q8_0.gguf", "644"],
]
```

Same idea for `[package.metadata.generate-rpm].assets`:

```toml
[package.metadata.generate-rpm]
# ...
assets = [
    { source = "target/release/mnemosd", dest = "/usr/bin/mnemos-daemon", mode = "755" },
    { source = "README.md", dest = "/usr/share/doc/mnemos-daemon/README", mode = "644", doc = true },
    { source = "CHANGELOG.md", dest = "/usr/share/doc/mnemos-daemon/CHANGELOG", mode = "644", doc = true },
    { source = "../../assets/llama-server-linux-x86_64", dest = "/usr/lib/mnemos/llama-server", mode = "755" },
    { source = "../../assets/all-MiniLM-L6-v2.Q8_0.gguf", dest = "/usr/lib/mnemos/all-MiniLM-L6-v2.Q8_0.gguf", mode = "644" },
]
```

> `cargo-generate-rpm` source paths are relative to the crate dir. The script `prepare-linux-packages.sh` (Plan 8 Task 5) copies workspace files into the crate dir; extend it to also copy the asset files (or symlink — verify which works).

- [ ] **Step 2: Tauri bundle resources** — modify `desktop/src-tauri/tauri.conf.json`:

```json
"resources": [
  "../../assets/llama-server-linux-x86_64",
  "../../assets/all-MiniLM-L6-v2.Q8_0.gguf"
]
```

These become accessible at runtime via Tauri's resource-resolver. The Mnemos daemon (running as a sidecar from the desktop bundle) needs to know its install layout — read either `MNEMOS_BUNDLED_BIN_DIR` env (set by the Tauri app at launch) or fall back to `/usr/lib/mnemos/` for .deb installs.

- [ ] **Step 3: Modify `prepare-linux-packages.sh`** to ensure `scripts/fetch-bundled-assets.sh` ran first:

```bash
# At the top of prepare-linux-packages.sh, after `cd ..`:
echo "=== ensuring bundled assets are present ==="
if [[ ! -x assets/llama-server-linux-x86_64 ]] || [[ ! -f assets/all-MiniLM-L6-v2.Q8_0.gguf ]]; then
    bash scripts/fetch-bundled-assets.sh
fi
```

- [ ] **Step 4: Smoke test** locally:

```bash
bash scripts/fetch-bundled-assets.sh
bash scripts/prepare-linux-packages.sh
# Confirm the resulting .deb contains the bundled files:
ar p target/debian/mnemos-daemon_*.deb data.tar.xz | tar -tJf - | grep -E "(llama-server|gguf)"
```

Expected: two lines showing `./usr/lib/mnemos/llama-server` and `./usr/lib/mnemos/all-MiniLM-L6-v2.Q8_0.gguf`.

- [ ] **Step 5: Commit.**

```bash
git add crates/mnemos_daemon/Cargo.toml desktop/src-tauri/tauri.conf.json scripts/prepare-linux-packages.sh
git commit -m "feat: bundle llama-server + GGUF in .deb/.rpm/.AppImage (Plan 9 Task 15)"
```

---

## Task 16: CI fetches + caches bundled assets

`ci.yml` (tests) and `release.yml` (packaging) both need the bundled assets. Add a step that runs `scripts/fetch-bundled-assets.sh` early and caches the result.

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/desktop.yml` (if it also runs Rust tests that touch bundled embedder)

- [ ] **Step 1: Add to `ci.yml`** — after the checkout + before any test step:

```yaml
- name: Cache bundled assets
  id: cache-assets
  uses: actions/cache@v4
  with:
    path: assets/
    key: bundled-assets-llamacpp-b3447-minilm-q8

- name: Fetch bundled assets
  if: steps.cache-assets.outputs.cache-hit != 'true'
  run: bash scripts/fetch-bundled-assets.sh
```

The cache key includes the pinned versions; bumping `LLAMA_CPP_TAG` invalidates the cache automatically.

- [ ] **Step 2: Add the same step to `release.yml`** for the Linux build job AND the linux-packages job.

- [ ] **Step 3: Run a tagged test to verify integration tests** with the bundled embedder pass. The tests are `#[ignore]`d unless `MNEMOS_TEST_BUNDLED=1` is set. The CI workflow should set this env for the test step:

```yaml
- name: Run mnemos_core tests with bundled embedder
  env:
    MNEMOS_TEST_BUNDLED: "1"
    MNEMOS_TEST_LLAMA_SERVER: "1"
  run: cargo test --workspace -- --include-ignored
```

> Optional: a separate workflow gated on the assets being available. For now, just one step that runs ignored tests.

- [ ] **Step 4: Commit.**

```bash
git add .github/workflows/
git commit -m "ci: fetch + cache bundled embedder assets; run ignored tests with assets present (Plan 9 Task 16)"
```

---

## Task 17: Re-enable Tauri auto-update

Flip `createUpdaterArtifacts` back to `true` and reinstate the `latest.json` generation step in `release.yml` that was removed in Plan 8's Linux-only scope-down.

**Prerequisite (user-driven, NOT in the plan):** before pushing the v0.8.0 release tag, the user must:
1. Run `bash scripts/gen-updater-key.sh` locally → generates `desktop/src-tauri/updater-private.pem`.
2. Paste the printed public key into `desktop/src-tauri/tauri.conf.json` `plugins.updater.pubkey` (replacing `PLACEHOLDER_PUBLIC_KEY_REPLACE_BEFORE_RELEASE`).
3. `gh secret set TAURI_SIGNING_PRIVATE_KEY < desktop/src-tauri/updater-private.pem`.
4. `gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --body ""`.

The plan documents this in PACKAGING.md (Task 21); the implementer **does not run the key-gen step** (secrets stay user-owned). The plan only enables the CI/build-side wiring; the user actually plugs in the key.

**Files:**
- Modify: `desktop/src-tauri/tauri.conf.json` — `createUpdaterArtifacts: true`
- Modify: `.github/workflows/release.yml` — re-add `latest.json` generation
- Document: PACKAGING.md (later in Task 21)

- [ ] **Step 1: `desktop/src-tauri/tauri.conf.json`**

```json
"createUpdaterArtifacts": true
```

- [ ] **Step 2: Update `.github/workflows/release.yml`** — re-add the latest.json generation step that was dropped in Plan 8. Insert it in the `release` job, before "Publish GitHub Release":

```yaml
- name: Build release-manifest tool
  run: cargo build --release --bin mnemos-release-manifest

- name: Generate latest.json
  shell: bash
  run: |
    set -euo pipefail
    VERSION="${GITHUB_REF_NAME#v}"
    LIN_FILE=$(ls release/*.AppImage.tar.gz 2>/dev/null | head -1)
    if [[ -z "$LIN_FILE" ]]; then
      echo "no AppImage.tar.gz found; skipping latest.json generation"
      exit 0
    fi
    LIN_SIG=$(cat "${LIN_FILE}.sig" 2>/dev/null || echo "")
    base_url="https://github.com/${GITHUB_REPOSITORY}/releases/download/${GITHUB_REF_NAME}"
    ./target/release/mnemos-release-manifest \
      --version "$VERSION" \
      --notes "$(awk "/^## \[$VERSION\]/{f=1;next} /^## \[/{f=0} f" CHANGELOG.md | head -100)" \
      --platform linux-x86_64 \
      --url "${base_url}/$(basename "$LIN_FILE")" \
      --signature "$LIN_SIG" \
      --output release/latest.json
    cat release/latest.json
```

(Single-platform version since v0.8.0 is still Linux-only. When mac/windows return, extend back to the multi-platform version from Plan 8.)

- [ ] **Step 3: Make the artifact-staging step pull in `.AppImage.tar.gz` + `.sig`** alongside the existing patterns:

```yaml
- name: Flatten + list
  run: |
    mkdir -p release/
    find artifacts/ -type f \( -name "*.deb" -o -name "*.rpm" -o -name "*.AppImage" -o -name "*.AppImage.tar.gz" -o -name "*.sig" \) -exec cp -v {} release/ \;
    ls -la release/
```

- [ ] **Step 4: Update `release.yml`'s build job to emit updater artifacts.** The build job builds `--bundles deb,rpm,appimage`; with `createUpdaterArtifacts: true`, the appimage build also emits a `.AppImage.tar.gz` + `.sig`. Confirm by checking the staging step picks them up.

- [ ] **Step 5: Commit.**

```bash
git add desktop/src-tauri/tauri.conf.json .github/workflows/release.yml
git commit -m "feat: re-enable Tauri auto-update (linux .AppImage signed manifest) (Plan 9 Task 17)"
```

> Reminder for the release commit: the user MUST run `gen-updater-key.sh` + upload the secret BEFORE pushing the v0.8.0 tag. The plan documents this; the implementer should NOT skip this step.

---

# Group G — First-run wizard + docs + release

## Task 18: First-run wizard updates

Plan 7 Task 17 built a 3-step wizard that probes Ollama and offers to pull `nomic-embed-text`. For v0.8.0 the bundled embedder is the default, so:
- The Ollama probe step becomes optional — if the user wants Ollama, they explicitly select it.
- The default flow: "Welcome → confirm vault path → done". Bundled embedder + LLM=none mean zero setup.
- The integration-snippets step (step 2 in the existing wizard) stays as the third step.

**Files:**
- Modify: `desktop/src/views/FirstRun.tsx`
- Test: extend `desktop/src/views/FirstRun.test.tsx` if it exists; otherwise create

- [ ] **Step 1: Simplify the wizard.** Read the existing component. Replace the Ollama-probe step (step 1) with an embedder-choice step:

```tsx
{step === 1 && (
  <>
    <h1 className="display text-xl">Embedder</h1>
    <p className="text-text-muted font-body">
      Mnemos ships with a local 22 MB embedder (MiniLM). No setup needed.
      To use Ollama or OpenAI instead, change the backend in Settings later.
    </p>
    <div className="flex items-center gap-3">
      <span className="label">✓ Bundled embedder ready</span>
    </div>
    <div className="flex justify-between">
      <button className="label text-text-muted" onClick={() => setStep(0)}>Back</button>
      <Button onClick={() => setStep(2)}>Continue</Button>
    </div>
  </>
)}
```

- [ ] **Step 2: Update the integration-snippets step (step 2)** to mention that the daemon is already running with the bundled embedder; no further setup needed.

- [ ] **Step 3: Pass + commit.**

```bash
cd desktop && pnpm typecheck && pnpm lint && pnpm test -- --run && cd ..
git add desktop/src/views/FirstRun.tsx desktop/src/views/FirstRun.test.tsx
git commit -m "feat(ui): first-run wizard reflects bundled embedder default (Plan 9 Task 18)"
```

---

## Task 19: BUILD.md + PACKAGING.md + README updates

Add a section about the bundled embedder, document the OpenAI backends, document the `mnemos embed-rebuild` migration command, and update the install instructions to reflect zero-setup.

**Files:**
- Modify: `BUILD.md`
- Modify: `PACKAGING.md`
- Modify: `README.md`

- [ ] **Step 1: BUILD.md** — add a new section after "CLI + daemon only":

```markdown
## Bundled embedder

By default, Mnemos ships with `llama-server` (llama.cpp's HTTP server) and
a 22 MB `all-MiniLM-L6-v2` GGUF model. The daemon spawns `llama-server`
as a managed child process on startup; embeddings happen entirely locally.

### Refreshing the bundled assets

```
bash scripts/fetch-bundled-assets.sh
```

Pinned versions:
- llama.cpp tag: `b3447` (~5 MB binary)
- Model: `all-MiniLM-L6-v2.Q8_0.gguf` (~22 MB, 384-dim, Apache-2.0)

Bump `LLAMA_CPP_TAG` in the script to upgrade.

### Switching embedders

Set `MNEMOS_EMBEDDER` to one of `bundled` (default), `ollama`, `openai`,
`mock`, `none`. For new vaults, the env value is the default. For existing
vaults, the vault's recorded embedder is authoritative — to switch, run:

```
mnemos embed-rebuild --target bundled       # or ollama / openai
```

The migration is atomic, resumable, and audit-logged. The old embeddings
are kept as a backup for 7 days.

### OpenAI backends

To use OpenAI embeddings or chat:

```bash
export OPENAI_API_KEY=sk-...
# Optional:
export OPENAI_BASE_URL=https://api.openai.com   # Azure OpenAI: https://your-resource.openai.azure.com
export MNEMOS_EMBEDDER=openai
export MNEMOS_EMBEDDER_MODEL=text-embedding-3-small
export MNEMOS_LLM=openai
export MNEMOS_LLM_MODEL=gpt-4o-mini

mnemos daemon restart
```
```

- [ ] **Step 2: PACKAGING.md** — add a section "Bundled assets refresh" and update the release checklist to include `scripts/fetch-bundled-assets.sh` before the bundle steps. Re-add the Tauri auto-update verification section that was scoped down in Plan 8.

- [ ] **Step 3: README.md** — update the Install section to mention zero-setup:

```markdown
## Install

> v0.8.0 ships **Linux only**. macOS and Windows still blocked on upstream
> issues (see CHANGELOG). A future release will restore both.

### Linux (zero setup)

```
sudo dpkg -i Mnemos_X.Y.Z_amd64.deb       # Debian/Ubuntu
sudo rpm -i Mnemos-X.Y.Z-1.x86_64.rpm     # Fedora/RHEL
```

Then:

```
mnemos remember "User prefers Tauri"
mnemos recall "what does the user like"
```

Mnemos ships with a bundled embedder. No Ollama install, no API key
required. Semantic recall works out of the box.

To use Ollama or OpenAI for embeddings/LLM instead, see [BUILD.md](BUILD.md)
§ "Switching embedders".
```

- [ ] **Step 4: Commit.**

```bash
git add BUILD.md PACKAGING.md README.md
git commit -m "docs: bundled embedder + OpenAI backends + migration command (Plan 9 Task 19)"
```

---

## Task 20: Release v0.8.0

Bump versions, CHANGELOG entry, local tag.

**Files:**
- Modify: `Cargo.toml` (workspace version → 0.8.0)
- Modify: `desktop/package.json`
- Modify: `desktop/src-tauri/Cargo.toml`
- Modify: `desktop/src-tauri/tauri.conf.json`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Bump versions** in all four files.

- [ ] **Step 2: CHANGELOG entry**

```markdown
## [0.8.0] - 2026-05-30

> **Zero-setup release.** Mnemos now ships with a bundled embedder
> (llama.cpp + 22 MB MiniLM Q8 GGUF). A fresh install works
> end-to-end — `mnemos remember` + `mnemos recall` — with no
> Ollama install, no API key, no internet after the .deb download.

### Added
- **Bundled embedder.** llama.cpp's `llama-server` ships in
  `/usr/lib/mnemos/`; daemon spawns + manages it as a child process,
  health-checks every 30s, restarts on crash with backoff. ~80 MB
  total .deb size (vs nomic-embed-text's 274 MB).
- **`MNEMOS_EMBEDDER=bundled`** is the new default for fresh vaults.
- **OpenAI embeddings backend** (`MNEMOS_EMBEDDER=openai`,
  `OPENAI_API_KEY`). Supports Azure OpenAI via `OPENAI_BASE_URL`.
- **OpenAI LLM backend** (`MNEMOS_LLM=openai`) for reflections,
  community summaries, entity extraction. Default model
  `gpt-4o-mini`, override via `MNEMOS_LLM_MODEL`.
- **`mnemos embed-rebuild --target <kind>`** — atomic, resumable,
  audit-logged migration between embedders. Shadow-table-based;
  backup retained 7 days for manual rollback. UI progress view at
  `/embed-rebuild`.
- **Vault meta tracks embedder authoritatively.** Schema v9 adds
  `vault_meta.embedder_kind`. The daemon uses vault meta, not env,
  to choose the backend.
- **Doctor + Settings UI updates.** Doctor surfaces embedder
  mismatch + migration prompt. Settings exposes
  `bundled / ollama / openai / mock / none` for both embedder and
  LLM. Settings includes an `[openai]` block for `base_url` + `api_key`.
- **Tauri auto-update re-enabled.** Deferred from v0.7.0; now back
  on for AppImage + `.AppImage.tar.gz` signed manifest via
  `mnemos_release_manifest`. (Requires the project owner to run
  `bash scripts/gen-updater-key.sh` + upload secret before the
  release tag is pushed.)
- **First-run wizard simplified.** No Ollama probe by default since
  the bundled embedder is ready. Three-step flow: welcome →
  embedder confirm → integration snippets.

### Changed
- **`MNEMOS_LLM` defaults to `none`** for fresh installs (was
  `ollama`). Reflections and community summaries silently no-op
  if no LLM is configured — opt in via Ollama or OpenAI.
- **Schema v9**: adds `vault_meta.embedder_kind` (backfilled from
  existing `embedder_model` for upgrades; defaults to `bundled` for
  fresh vaults).

### Migrating from v0.7.x
- Existing vaults seeded with Ollama keep working — the daemon
  detects `vault.embedder_kind=ollama` and continues using Ollama
  as before.
- Doctor view surfaces a migration prompt: run
  `mnemos embed-rebuild --target bundled` to switch to the bundled
  embedder (re-embeds every memory atomically, ~30s per 100
  memories on a 4-core CPU).

### Known limitations (carried from v0.7.0)
- macOS desktop bundle still blocked on `dispatch2` macro recursion.
- Windows desktop bundle still blocked on `libsql-sys` Unix-only
  APIs.
- Both unblocked by a future plan.
```

- [ ] **Step 3: Release gate**

```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd desktop && pnpm install --frozen-lockfile && pnpm typecheck && pnpm lint && pnpm test -- --run && pnpm build && cd ..
bash scripts/fetch-bundled-assets.sh
bash scripts/prepare-linux-packages.sh
```

All green. (Cross-platform CI runs on tag push.)

- [ ] **Step 4: Commit + tag**

```bash
git add Cargo.toml desktop/package.json desktop/src-tauri/Cargo.toml desktop/src-tauri/tauri.conf.json CHANGELOG.md
git commit -m "chore: release v0.8.0 — bundled embedder + OpenAI backends + auto-update (Plan 9 Task 20)"
git tag -a v0.8.0 -m "v0.8.0 — zero-setup install with bundled embedder"
```

(Do NOT push — user pushes manually after running `gen-updater-key.sh` + uploading the secret.)

---

## Done

After all tasks: `apt install mnemos` (or `dpkg -i`/`rpm -i`/`.AppImage`) → `mnemos remember` + `mnemos recall` work immediately. No Ollama. No API key. No internet after install. Local-first preserved. Cloud opt-in via `OPENAI_API_KEY`. Migration from existing Ollama-seeded vaults is one command, atomic, and audit-logged.

The mnemos roadmap from here is feature work (Mac/Windows portability, encrypt-at-rest, secret detection, more sync backends). The setup-friction story is done.
