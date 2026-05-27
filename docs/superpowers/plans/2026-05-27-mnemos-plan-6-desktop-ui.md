# Mnemos Plan 6 — Desktop UI (Tauri + React)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A distinctive, Obsidian-style desktop app for the mnemos memory vault. Three-column shell (tier/tag/entity browser · view router · always-on memory inspector) over the daemon's REST + WebSocket API, with all ten core views — tier browser, markdown editor, **Sigma.js graph view** (entity/memory/mixed modes, animated PPR mass overlay, Louvain community hulls, bi-temporal time slider), **bi-temporal timeline**, hybrid search with per-retriever explainability bars, pipeline status, reflection viewer, entity profile, and audit log — plus a ⌘K command palette and live updates. End state: a person can browse, search, edit, visualize, and reflect on their memory graph from a real desktop window.

**Architecture:** A Tauri 2 desktop shell (Rust) hosts a React 18 + TypeScript frontend (Vite). The frontend talks to the already-running `mnemosd` daemon over HTTP + WebSocket via a typed client; a single Tauri command reads the bearer token from `~/.config/mnemos/token` so the secret never lives in renderer code or env. State: TanStack Query for server cache + Zustand for UI/event state; TanStack Router for views. Styling: Tailwind driven entirely by a custom tier-coded design-token layer (the default palette is never used). Graph: Sigma.js (main canvas) + react-force-graph (entity neighborhood widget). Editor: CodeMirror 6. Charts/timeline: Visx. This plan also fills three daemon API gaps the UI needs: a whole-graph endpoint, a real entity-neighborhood + enriched entity-detail endpoint, and a communities endpoint.

**Tech Stack:** Tauri 2, React 18, TypeScript, Vite, Tailwind CSS (custom tokens), TanStack Router + Query, Zustand, CodeMirror 6, Sigma.js + graphology, react-force-graph, Visx, Lucide (customized), Vitest + Testing Library + MSW, Playwright. Rust side (daemon gaps): axum 0.8, libsql.

---

## Plan sequence context

Plan 6 of 8 (the UI was kept as a single plan; sync/packaging moves to Plan 8), producing **v0.5.0**. Built on v0.4.0 (Plan 5: PPR retrieval, reflection, communities). The daemon already exposes memories, search (with `explain` + `ppr_rank`/`bm25_rank`/`dense_rank`), sessions, reflections, pipelines, working tier, audit, and partial entity endpoints. Subsequent:
- Plan 7: sync backends (file-sync + optional DB layer), settings view, first-run wizard, additional adapters.
- Plan 8: packaging, installers, auto-update, signing.

The Tauri app lives in a new top-level `desktop/` directory, **outside** the Cargo workspace (its own `desktop/src-tauri/Cargo.toml`) so the heavy Tauri/GUI dependency tree never touches the library/daemon build or CI matrix.

---

## What this plan defers

| Capability | Why | Target |
|---|---|---|
| Settings view, first-run wizard, Ollama-pull UX | Tie to config + install flow; large on their own | Plan 7 |
| Sync status in the top bar | Sync backends don't exist until Plan 8 | Plan 8 (top bar shows daemon connection status for now) |
| `POST /v1/entities/merge` + UI merge action | Entity merge is a destructive graph edit; needs its own design | Plan 7 |
| "Promote to procedural" persistence | Re-tiering needs a daemon endpoint; the button is wired but shows a "coming soon" toast | Plan 7 (button present, disabled with tooltip) |
| `ts-rs` type codegen from Rust | Hand-written TS types matching the Rust structs are sufficient for v0.5.0 and avoid a build-time codegen dependency | Later |
| `GET /openapi.json` | The typed client is hand-written; no consumer needs OpenAPI yet | Later |
| Vault export/import zip, doctor view | QoL; not core to "view and manage memory" | Plan 7 |
| App packaging / installers / signing | Distinct concern | Plan 8 |

---

## Hard prerequisites

- Plan 5 (`v0.4.0`) shipped; daemon builds and CI green.
- Node.js >= 20 and a package manager (`pnpm` assumed; `npm` works — adjust commands).
- Rust toolchain (for `src-tauri`) and the OS Tauri prerequisites (webkit2gtk on Linux, etc.).
- A running `mnemosd` for manual/E2E testing (`MNEMOS_EMBEDDER=mock MNEMOS_LLM=mock cargo run -p mnemos_daemon -- serve`). Automated tests mock the daemon with MSW — no live daemon required for CI.

---

## Design language (anti-slop, from the design spec)

Locked before any view is built (Task 4 implements it). **Every view must consume these tokens — no ad-hoc colors, fonts, or spacing.**

- **Typography**: Display = **Fraunces**; body = **Source Serif 4**; mono = **JetBrains Mono**. Never Inter/Roboto/system-ui. Weight extremes (300 body / 800 display), tight tracking on headings (-0.02em), wide tracking on labels (0.08em).
- **Color**: warm off-white `#FAF9F6` (light bg) / deep blue-black `#0F1218` (dark bg). Off-white text on dark `#E8E8F0`. **No purple/indigo/violet anywhere.** Primary accent = deep teal `#1F6F6B`.
- **Tier palette** (semantic, used for tier chips, graph nodes, timeline bars):
  - working = warm amber `#C77D33`
  - episodic = muted graphite `#5B6168`
  - semantic = deep teal `#1F6F6B`
  - procedural = brick red `#A6432E`
  - reflection = sage `#6E8B6A`
- **Depth**: layered shadows (flush → subtle → raised → floating), colored to the surface, not flat gray. Mixed corner radii (sharp panels, soft cards). No uniform 8px-radius-everywhere.
- **Motion**: micro 120ms, state 240ms, custom cubic-bezier; honor `prefers-reduced-motion`. Strength-near-invalidation pulse is opacity-only.
- **Bi-temporal as a visual primitive**: invalid memories render dashed + faded + strikethrough title; "as-of" mode tints the UI with a cooler accent and shows a "viewing <date>" pill.
- **Required states everywhere**: skeleton loaders, error UI, designed empty states, disabled states, visible focus rings, WCAG AA contrast, color never the sole signal.

---

## File structure produced by this plan

```
crates/mnemos_daemon/src/routes/
├── graph.rs            # NEW: GET /v1/graph (whole entity graph)
├── communities.rs      # NEW: GET /v1/communities
├── entities.rs         # MODIFIED: real /{id}/graph neighborhood + enriched /{id}
└── mod.rs              # MODIFIED: mount graph + communities routers

desktop/                                  # NEW — Tauri app (outside the cargo workspace)
├── package.json  vite.config.ts  tsconfig.json  tailwind.config.ts  index.html
├── src-tauri/
│   ├── Cargo.toml  tauri.conf.json  build.rs
│   └── src/main.rs        # window + `read_token` command + locate daemon
├── src/
│   ├── main.tsx  App.tsx  router.tsx
│   ├── design/            # tokens.css, theme.ts, ThemeProvider.tsx, primitives (Chip, Card, Button, Skeleton…)
│   ├── api/               # client.ts (typed REST), types.ts, ws.ts, token.ts, queries.ts (TanStack Query hooks)
│   ├── store/             # ui.ts, events.ts (Zustand)
│   ├── layout/            # Shell.tsx, TopBar.tsx, LeftSidebar.tsx, Inspector.tsx
│   ├── views/             # Browser, Editor, Search, Graph, Timeline, Pipelines, Reflections, EntityProfile, Audit
│   ├── components/        # CommandPalette, QuickAdd, RankBars, TierChip, GraphCanvas, …
│   └── test/              # msw handlers, setup, fixtures
└── tests-e2e/             # Playwright specs

README.md / CHANGELOG.md   # MODIFIED: v0.5.0
```

---

## Conventions

- **Backend tasks** (Group BK): Rust TDD as in Plans 1-5 — failing test → implement → `cargo fmt`/`clippy -D warnings`/`cargo test` green → commit. Daemon endpoints behind bearer auth (mounted in the `authed` router).
- **Frontend tasks**: TDD with **Vitest + Testing Library + MSW** (mock the daemon). Each component/hook task: write the failing test (render/interaction against MSW), implement, `pnpm test` green, `pnpm lint` (eslint) + `pnpm typecheck` (`tsc --noEmit`) clean, commit. E2E tasks use **Playwright** against `vite dev` + MSW.
- **No placeholders / no AI-slop**: every view handles loading/error/empty/disabled states; all styling via design tokens; components < 150 lines (split otherwise); no `any`.
- Commit `<type>: <subject>` referencing Plan 6 / Task N.
- Frontend commands run from `desktop/`. Daemon commands from repo root. All paths relative to `/home/jons/AntiGravityProjects/mnemos/`.
- `desktop/` is NOT in the Cargo workspace; the daemon CI is unaffected. A separate `desktop` CI job (added in the release task) runs `pnpm typecheck && pnpm lint && pnpm test`.

---

# Group BK — Daemon API gaps (Rust)

These three endpoints give the UI the graph + community data it needs. All behind bearer auth.

## Task 1: `GET /v1/graph` — whole entity graph

**Files:**
- Create: `crates/mnemos_daemon/src/routes/graph.rs`
- Modify: `crates/mnemos_daemon/src/routes/mod.rs` (declare + mount)
- Test: `crates/mnemos_daemon/tests/graph_endpoint.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_daemon/tests/graph_endpoint.rs`:

```rust
use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::storage::entity_ops::{upsert_edge, upsert_entity};
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

#[tokio::test]
async fn graph_endpoint_returns_nodes_and_edges() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let a = upsert_entity(vault.storage(), "Rust", "tool").await.unwrap();
    let b = upsert_entity(vault.storage(), "Tauri", "tool").await.unwrap();
    upsert_edge(vault.storage(), &a, &b, "uses", "mem_1", chrono::Utc::now()).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();

    let (s, b_) = call(app, "GET", "/v1/graph", Some(&state.token), "").await;
    assert_eq!(s, StatusCode::OK, "{b_}");
    let v: serde_json::Value = serde_json::from_str(&b_).unwrap();
    assert_eq!(v["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(v["edges"].as_array().unwrap().len(), 1);
    assert_eq!(v["edges"][0]["relation"], "uses");
    // community_id defaults to -1 when no community detection has run
    assert_eq!(v["nodes"][0]["community_id"], -1);
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

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test graph_endpoint`
Expected: FAIL — 404 (route not mounted).

- [ ] **Step 3: Create `crates/mnemos_daemon/src/routes/graph.rs`**

```rust
//! `GET /v1/graph` — the whole entity graph (nodes + active edges) for the UI
//! graph view. Node `community_id` is -1 when community detection hasn't run.

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/graph", get(get_graph))
}

async fn get_graph(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    use mnemos_core::error::MnemosError;
    let conn = state.vault.storage().conn()?;

    let mut nrows = conn
        .query(
            "SELECT e.id, e.name, e.kind,
                    COALESCE(ec.community_id, -1) AS community_id,
                    (SELECT COUNT(*) FROM entity_mentions m WHERE m.entity_id = e.id) AS mentions
               FROM entities e
               LEFT JOIN entity_communities ec ON ec.entity_id = e.id
              ORDER BY e.created_at",
            (),
        )
        .await
        .map_err(MnemosError::from)?;
    let mut nodes: Vec<Value> = Vec::new();
    while let Some(r) = nrows.next().await.map_err(MnemosError::from)? {
        nodes.push(json!({
            "id": r.get::<String>(0).map_err(MnemosError::from)?,
            "name": r.get::<String>(1).map_err(MnemosError::from)?,
            "kind": r.get::<String>(2).map_err(MnemosError::from)?,
            "community_id": r.get::<i64>(3).map_err(MnemosError::from)?,
            "mentions": r.get::<i64>(4).map_err(MnemosError::from)?,
        }));
    }
    drop(nrows);

    let mut erows = conn
        .query(
            "SELECT id, source_entity_id, target_entity_id, relation, weight
               FROM entity_edges WHERE invalid_at IS NULL",
            (),
        )
        .await
        .map_err(MnemosError::from)?;
    let mut edges: Vec<Value> = Vec::new();
    while let Some(r) = erows.next().await.map_err(MnemosError::from)? {
        edges.push(json!({
            "id": r.get::<String>(0).map_err(MnemosError::from)?,
            "source": r.get::<String>(1).map_err(MnemosError::from)?,
            "target": r.get::<String>(2).map_err(MnemosError::from)?,
            "relation": r.get::<String>(3).map_err(MnemosError::from)?,
            "weight": r.get::<f64>(4).map_err(MnemosError::from)?,
        }));
    }

    Ok(Json(json!({ "nodes": nodes, "edges": edges })))
}
```

- [ ] **Step 4: Mount it** in `routes/mod.rs` — add `pub mod graph;` and `.merge(graph::router())` to the `authed` chain.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test graph_endpoint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/graph.rs crates/mnemos_daemon/src/routes/mod.rs crates/mnemos_daemon/tests/graph_endpoint.rs
git commit -m "feat: GET /v1/graph whole entity graph endpoint (Plan 6 Task 1)"
```

---

## Task 2: Enriched entity detail + real neighborhood graph

Replace the thin `get_entity` and the `entity_graph_stub` in `entities.rs` with full data: entity detail (aliases, description, mention count, mentioned memory ids, incident edges) and a real `/{id}/graph` neighborhood (the entity + its direct neighbors + connecting edges).

**Files:**
- Modify: `crates/mnemos_daemon/src/routes/entities.rs`
- Test: `crates/mnemos_daemon/tests/entities.rs` (extend, or create if absent)

- [ ] **Step 1: Write the failing test** — create/extend `crates/mnemos_daemon/tests/entities.rs`:

```rust
use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::storage::entity_ops::{link_entity_mention, upsert_edge, upsert_entity};
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

async fn fixture() -> (axum::Router, String, String, String) {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let mem = vault.remember("rust note", RememberOpts::default()).await.unwrap();
    let a = upsert_entity(vault.storage(), "Rust", "tool").await.unwrap();
    let b = upsert_entity(vault.storage(), "Tauri", "tool").await.unwrap();
    upsert_edge(vault.storage(), &a, &b, "uses", &mem, chrono::Utc::now()).await.unwrap();
    link_entity_mention(vault.storage(), &mem, &a).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    (app, state.token, a, mem)
}

#[tokio::test]
async fn entity_detail_is_enriched() {
    let (app, token, a, mem) = fixture().await;
    let (s, b) = call(app, "GET", &format!("/v1/entities/{a}"), Some(&token), "").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["name"], "Rust");
    assert_eq!(v["mention_count"], 1);
    assert!(v["memory_ids"].as_array().unwrap().iter().any(|m| m == &mem));
    assert_eq!(v["edges"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn entity_neighborhood_graph() {
    let (app, token, a, _mem) = fixture().await;
    let (s, b) = call(app, "GET", &format!("/v1/entities/{a}/graph"), Some(&token), "").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    // self + 1 neighbor
    assert_eq!(v["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(v["edges"].as_array().unwrap().len(), 1);
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

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test entities`
Expected: FAIL — current `get_entity` returns only id/name/kind; `/{id}/graph` returns empty.

- [ ] **Step 3: Replace `get_entity` and `entity_graph_stub` in `entities.rs`** with the enriched versions (keep `list_entities` and the imports; add `MnemosError` use inside the fns):

```rust
async fn get_entity(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    use mnemos_core::error::MnemosError;
    let conn = state.vault.storage().conn()?;

    let mut rows = conn
        .query(
            "SELECT id, name, kind, aliases, description FROM entities WHERE id = ?",
            params![id.clone()],
        )
        .await
        .map_err(MnemosError::from)?;
    let row = rows
        .next()
        .await
        .map_err(MnemosError::from)?
        .ok_or_else(|| ApiError::not_found(format!("entity {id}")))?;
    let aliases: Vec<String> =
        serde_json::from_str(&row.get::<String>(3).map_err(MnemosError::from)?).unwrap_or_default();
    let detail = serde_json::json!({
        "id": row.get::<String>(0).map_err(MnemosError::from)?,
        "name": row.get::<String>(1).map_err(MnemosError::from)?,
        "kind": row.get::<String>(2).map_err(MnemosError::from)?,
        "aliases": aliases,
        "description": row.get::<Option<String>>(4).map_err(MnemosError::from)?,
    });
    drop(rows);

    // memory ids that mention this entity
    let mut mrows = conn
        .query(
            "SELECT memory_id FROM entity_mentions WHERE entity_id = ?",
            params![id.clone()],
        )
        .await
        .map_err(MnemosError::from)?;
    let mut memory_ids: Vec<String> = Vec::new();
    while let Some(r) = mrows.next().await.map_err(MnemosError::from)? {
        memory_ids.push(r.get::<String>(0).map_err(MnemosError::from)?);
    }
    drop(mrows);

    // incident active edges
    let mut erows = conn
        .query(
            "SELECT id, source_entity_id, target_entity_id, relation, weight
               FROM entity_edges
              WHERE (source_entity_id = ?1 OR target_entity_id = ?1) AND invalid_at IS NULL",
            params![id.clone()],
        )
        .await
        .map_err(MnemosError::from)?;
    let mut edges: Vec<serde_json::Value> = Vec::new();
    while let Some(r) = erows.next().await.map_err(MnemosError::from)? {
        edges.push(serde_json::json!({
            "id": r.get::<String>(0).map_err(MnemosError::from)?,
            "source": r.get::<String>(1).map_err(MnemosError::from)?,
            "target": r.get::<String>(2).map_err(MnemosError::from)?,
            "relation": r.get::<String>(3).map_err(MnemosError::from)?,
            "weight": r.get::<f64>(4).map_err(MnemosError::from)?,
        }));
    }

    let mut detail = detail;
    detail["mention_count"] = serde_json::json!(memory_ids.len());
    detail["memory_ids"] = serde_json::json!(memory_ids);
    detail["edges"] = serde_json::json!(edges);
    Ok(Json(detail))
}

async fn entity_graph(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    use mnemos_core::error::MnemosError;
    use std::collections::BTreeSet;
    let conn = state.vault.storage().conn()?;

    // incident edges → neighbor ids
    let mut erows = conn
        .query(
            "SELECT id, source_entity_id, target_entity_id, relation, weight
               FROM entity_edges
              WHERE (source_entity_id = ?1 OR target_entity_id = ?1) AND invalid_at IS NULL",
            params![id.clone()],
        )
        .await
        .map_err(MnemosError::from)?;
    let mut edges: Vec<serde_json::Value> = Vec::new();
    let mut ids: BTreeSet<String> = BTreeSet::new();
    ids.insert(id.clone());
    while let Some(r) = erows.next().await.map_err(MnemosError::from)? {
        let src: String = r.get(1).map_err(MnemosError::from)?;
        let tgt: String = r.get(2).map_err(MnemosError::from)?;
        ids.insert(src.clone());
        ids.insert(tgt.clone());
        edges.push(serde_json::json!({
            "id": r.get::<String>(0).map_err(MnemosError::from)?,
            "source": src, "target": tgt,
            "relation": r.get::<String>(3).map_err(MnemosError::from)?,
            "weight": r.get::<f64>(4).map_err(MnemosError::from)?,
        }));
    }
    drop(erows);

    // node detail for self + neighbors
    let mut nodes: Vec<serde_json::Value> = Vec::new();
    for nid in &ids {
        let mut nr = conn
            .query("SELECT id, name, kind FROM entities WHERE id = ?", params![nid.clone()])
            .await
            .map_err(MnemosError::from)?;
        if let Some(r) = nr.next().await.map_err(MnemosError::from)? {
            nodes.push(serde_json::json!({
                "id": r.get::<String>(0).map_err(MnemosError::from)?,
                "name": r.get::<String>(1).map_err(MnemosError::from)?,
                "kind": r.get::<String>(2).map_err(MnemosError::from)?,
            }));
        }
    }

    Ok(Json(serde_json::json!({ "nodes": nodes, "edges": edges })))
}
```

Update the router line `.route("/v1/entities/{id}/graph", get(entity_graph_stub))` → `get(entity_graph)`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test entities`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/entities.rs crates/mnemos_daemon/tests/entities.rs
git commit -m "feat: enriched entity detail + real neighborhood graph (Plan 6 Task 2)"
```

---

## Task 3: `GET /v1/communities`

Returns each detected community (id + member entities) plus the `community_summary` memories. The UI colors graph nodes by community and shows summaries; it correlates loosely (no strict community→summary FK in v0.4.0).

**Files:**
- Create: `crates/mnemos_daemon/src/routes/communities.rs`
- Modify: `crates/mnemos_daemon/src/routes/mod.rs` (declare + mount)
- Test: `crates/mnemos_daemon/tests/communities_endpoint.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_daemon/tests/communities_endpoint.rs`:

```rust
use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::storage::community_ops::store_communities;
use mnemos_core::storage::entity_ops::upsert_entity;
use mnemos_daemon::{build_app, config::Config};
use mnemos_core::vault::Vault;
use tempfile::TempDir;

#[tokio::test]
async fn communities_endpoint_lists_members() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let a = upsert_entity(vault.storage(), "Rust", "tool").await.unwrap();
    let b = upsert_entity(vault.storage(), "Tauri", "tool").await.unwrap();
    store_communities(vault.storage(), &[(a.clone(), 0), (b.clone(), 0)], chrono::Utc::now()).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();

    let (s, body) = call(app, "GET", "/v1/communities", Some(&state.token), "").await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let comms = v["communities"].as_array().unwrap();
    assert_eq!(comms.len(), 1);
    assert_eq!(comms[0]["community_id"], 0);
    assert_eq!(comms[0]["members"].as_array().unwrap().len(), 2);
    assert!(v["summaries"].is_array());
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

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test communities_endpoint`
Expected: FAIL — 404.

- [ ] **Step 3: Create `crates/mnemos_daemon/src/routes/communities.rs`**

```rust
//! `GET /v1/communities` — detected communities (id + member entities) plus the
//! `community_summary` memories. The UI correlates summaries to communities
//! loosely (no strict FK yet).

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};
use std::collections::BTreeMap;

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/communities", get(get_communities))
}

async fn get_communities(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    use mnemos_core::error::MnemosError;
    let conn = state.vault.storage().conn()?;

    let mut rows = conn
        .query(
            "SELECT ec.community_id, e.id, e.name
               FROM entity_communities ec
               JOIN entities e ON e.id = ec.entity_id
              ORDER BY ec.community_id, e.name",
            (),
        )
        .await
        .map_err(MnemosError::from)?;
    let mut grouped: BTreeMap<i64, Vec<Value>> = BTreeMap::new();
    while let Some(r) = rows.next().await.map_err(MnemosError::from)? {
        let cid: i64 = r.get(0).map_err(MnemosError::from)?;
        let id: String = r.get(1).map_err(MnemosError::from)?;
        let name: String = r.get(2).map_err(MnemosError::from)?;
        grouped.entry(cid).or_default().push(json!({ "id": id, "name": name }));
    }
    drop(rows);

    let communities: Vec<Value> = grouped
        .into_iter()
        .map(|(community_id, members)| json!({ "community_id": community_id, "members": members }))
        .collect();

    let summaries = mnemos_core::storage::memory_ops::list_by_kind(
        state.vault.storage(),
        mnemos_core::types::MemoryType::CommunitySummary,
        100,
    )
    .await?;

    Ok(Json(json!({ "communities": communities, "summaries": summaries })))
}
```

- [ ] **Step 4: Mount it** in `routes/mod.rs` — add `pub mod communities;` and `.merge(communities::router())` to the `authed` chain.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test communities_endpoint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/communities.rs crates/mnemos_daemon/src/routes/mod.rs crates/mnemos_daemon/tests/communities_endpoint.rs
git commit -m "feat: GET /v1/communities endpoint (Plan 6 Task 3)"
```

---

# Group A — Foundation (Tauri shell, design system, client, shell)

## Task 4: Scaffold the Tauri 2 + React + Vite app

Create the `desktop/` project: Vite + React + TS frontend, Tauri 2 `src-tauri` shell with a `read_token` command (reads `~/.config/mnemos/token`), Vitest + Testing Library wiring. A smoke test renders the app.

**Files (all new, under `desktop/`):** `package.json`, `tsconfig.json`, `tsconfig.node.json`, `vite.config.ts`, `index.html`, `.eslintrc.cjs`, `src/main.tsx`, `src/App.tsx`, `src/vite-env.d.ts`, `src/test/setup.ts`, `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`, `src/App.test.tsx`.

- [ ] **Step 1: Create `desktop/package.json`**

```json
{
  "name": "mnemos-desktop",
  "private": true,
  "version": "0.5.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "preview": "vite preview",
    "tauri": "tauri",
    "test": "vitest run",
    "test:watch": "vitest",
    "typecheck": "tsc --noEmit",
    "lint": "eslint src --ext .ts,.tsx",
    "e2e": "playwright test"
  },
  "dependencies": {
    "@tanstack/react-query": "^5.51.0",
    "@tanstack/react-router": "^1.45.0",
    "@codemirror/lang-markdown": "^6.2.5",
    "@codemirror/state": "^6.4.1",
    "@codemirror/view": "^6.28.0",
    "@visx/axis": "^3.10.1",
    "@visx/scale": "^3.5.0",
    "@visx/shape": "^3.5.0",
    "graphology": "^0.25.4",
    "graphology-layout-forceatlas2": "^0.10.1",
    "lucide-react": "^0.408.0",
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "react-force-graph-2d": "^1.25.5",
    "sigma": "^3.0.0",
    "zustand": "^4.5.4",
    "@tauri-apps/api": "^2.0.0"
  },
  "devDependencies": {
    "@playwright/test": "^1.45.0",
    "@tanstack/router-devtools": "^1.45.0",
    "@testing-library/jest-dom": "^6.4.6",
    "@testing-library/react": "^16.0.0",
    "@testing-library/user-event": "^14.5.2",
    "@types/react": "^18.3.3",
    "@types/react-dom": "^18.3.0",
    "@typescript-eslint/eslint-plugin": "^7.16.0",
    "@typescript-eslint/parser": "^7.16.0",
    "@vitejs/plugin-react": "^4.3.1",
    "@tauri-apps/cli": "^2.0.0",
    "autoprefixer": "^10.4.19",
    "eslint": "^8.57.0",
    "eslint-plugin-react-hooks": "^4.6.2",
    "jsdom": "^24.1.0",
    "msw": "^2.3.1",
    "postcss": "^8.4.39",
    "tailwindcss": "^3.4.6",
    "typescript": "^5.5.3",
    "vite": "^5.3.3",
    "vitest": "^2.0.2"
  }
}
```

- [ ] **Step 2: Config files**

`desktop/tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ES2022", "useDefineForClassFields": true, "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext", "skipLibCheck": true, "moduleResolution": "bundler",
    "allowImportingTsExtensions": true, "resolveJsonModule": true, "isolatedModules": true,
    "noEmit": true, "jsx": "react-jsx", "strict": true, "noUnusedLocals": true,
    "noUnusedParameters": true, "noFallthroughCasesInSwitch": true,
    "types": ["vitest/globals", "@testing-library/jest-dom"],
    "baseUrl": ".", "paths": { "@/*": ["src/*"] }
  },
  "include": ["src", "tests-e2e"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

`desktop/tsconfig.node.json`:
```json
{ "compilerOptions": { "composite": true, "skipLibCheck": true, "module": "ESNext", "moduleResolution": "bundler", "allowSyntheticDefaultImports": true }, "include": ["vite.config.ts"] }
```

`desktop/vite.config.ts`:
```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

export default defineConfig({
  plugins: [react()],
  resolve: { alias: { "@": path.resolve(__dirname, "src") } },
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    css: true,
  },
});
```

`desktop/index.html`:
```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Mnemos</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`desktop/.eslintrc.cjs`:
```js
module.exports = {
  root: true,
  parser: "@typescript-eslint/parser",
  plugins: ["@typescript-eslint", "react-hooks"],
  extends: ["eslint:recommended", "plugin:@typescript-eslint/recommended", "plugin:react-hooks/recommended"],
  parserOptions: { ecmaVersion: 2022, sourceType: "module" },
  env: { browser: true, es2022: true },
  rules: { "@typescript-eslint/no-explicit-any": "error" },
  ignorePatterns: ["dist", "src-tauri", "node_modules"],
};
```

- [ ] **Step 3: Frontend entry + smoke**

`desktop/src/vite-env.d.ts`: `/// <reference types="vite/client" />`

`desktop/src/test/setup.ts`:
```ts
import "@testing-library/jest-dom/vitest";
```

`desktop/src/App.tsx`:
```tsx
export default function App() {
  return (
    <main>
      <h1>mnemos</h1>
    </main>
  );
}
```

`desktop/src/main.tsx`:
```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

- [ ] **Step 4: Tauri shell**

`desktop/src-tauri/Cargo.toml`:
```toml
[package]
name = "mnemos-desktop"
version = "0.5.0"
edition = "2021"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
directories = "5"

[features]
custom-protocol = ["tauri/custom-protocol"]
```

`desktop/src-tauri/build.rs`: `fn main() { tauri_build::build() }`

`desktop/src-tauri/tauri.conf.json`:
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Mnemos",
  "version": "0.5.0",
  "identifier": "dev.mnemos.desktop",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "pnpm dev",
    "beforeBuildCommand": "pnpm build"
  },
  "app": {
    "windows": [{ "title": "Mnemos", "width": 1440, "height": 900, "minWidth": 960, "minHeight": 600 }],
    "security": { "csp": null }
  },
  "bundle": { "active": true, "targets": "all" }
}
```

`desktop/src-tauri/src/main.rs`:
```rust
// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Read the daemon bearer token from `~/.config/mnemos/token`. Kept in the Rust
/// shell so the secret never lives in renderer-accessible env or storage.
#[tauri::command]
fn read_token() -> Result<String, String> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .ok_or_else(|| "could not resolve config dir".to_string())?;
    let path = dirs.config_dir().join("token");
    std::fs::read_to_string(&path)
        .map(|s| s.trim().to_string())
        .map_err(|e| format!("read token {}: {e}", path.display()))
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![read_token])
        .run(tauri::generate_context!())
        .expect("error while running mnemos desktop");
}
```

- [ ] **Step 5: Write the smoke test** — `desktop/src/App.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import App from "./App";

test("renders the app name", () => {
  render(<App />);
  expect(screen.getByRole("heading", { name: /mnemos/i })).toBeInTheDocument();
});
```

- [ ] **Step 6: Install + verify**

```bash
cd desktop && pnpm install
pnpm typecheck && pnpm test
```
Expected: typecheck clean; 1 test passes. (Tauri Rust build is verified during the release task; it requires OS GUI libs.)

- [ ] **Step 7: Commit**

```bash
cd /home/jons/AntiGravityProjects/mnemos
printf 'dist\nnode_modules\nsrc-tauri/target\n' > desktop/.gitignore
git add desktop/package.json desktop/tsconfig.json desktop/tsconfig.node.json desktop/vite.config.ts desktop/index.html desktop/.eslintrc.cjs desktop/.gitignore desktop/src/ desktop/src-tauri/Cargo.toml desktop/src-tauri/build.rs desktop/src-tauri/tauri.conf.json desktop/src-tauri/src/main.rs
git commit -m "feat: scaffold Tauri 2 + React + Vite desktop app (Plan 6 Task 4)"
```

(Do not commit `desktop/pnpm-lock.yaml`? Commit it for reproducibility: `git add desktop/pnpm-lock.yaml` if present.)

---

## Task 5: Design system — tokens, Tailwind, theme, primitives

Implement the locked design language as CSS custom properties + a Tailwind config that consumes ONLY those tokens, a `ThemeProvider` (light/dark, `prefers-reduced-motion`/`prefers-color-scheme` aware), and the base primitives every view reuses: `TierChip`, `Button`, `Card`, `Skeleton`.

**Files:** `desktop/postcss.config.cjs`, `desktop/tailwind.config.ts`, `desktop/src/design/tokens.css`, `desktop/src/design/theme.ts`, `desktop/src/design/ThemeProvider.tsx`, `desktop/src/design/primitives.tsx`, `desktop/src/design/primitives.test.tsx`, plus import tokens in `src/main.tsx`.

- [ ] **Step 1: Write the failing test** — `desktop/src/design/primitives.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { TierChip, Button } from "./primitives";

test("TierChip shows the tier label and carries the tier data attribute", () => {
  render(<TierChip tier="semantic" />);
  const chip = screen.getByText(/semantic/i);
  expect(chip).toBeInTheDocument();
  expect(chip.closest("[data-tier]")).toHaveAttribute("data-tier", "semantic");
});

test("Button renders children and fires onClick", async () => {
  const { default: userEvent } = await import("@testing-library/user-event");
  const onClick = vi.fn();
  render(<Button onClick={onClick}>Save</Button>);
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(onClick).toHaveBeenCalledOnce();
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd desktop && pnpm test src/design`
Expected: FAIL — module not found.

- [ ] **Step 3: `desktop/postcss.config.cjs`**

```js
module.exports = { plugins: { tailwindcss: {}, autoprefixer: {} } };
```

- [ ] **Step 4: `desktop/src/design/tokens.css`** — the single source of truth for color/type/space/shadow/motion. Tailwind maps to these; nothing else defines color.

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

:root {
  /* surfaces — warm off-white light theme */
  --bg: #faf9f6;
  --surface: #fffdf9;
  --surface-raised: #ffffff;
  --text: #1c1b18;
  --text-muted: #5b5750;
  --border: #e7e2d8;
  --accent: #1f6f6b;        /* deep teal — never purple/indigo */
  --accent-contrast: #ffffff;
  /* tier palette */
  --tier-working: #c77d33;
  --tier-episodic: #5b6168;
  --tier-semantic: #1f6f6b;
  --tier-procedural: #a6432e;
  --tier-reflection: #6e8b6a;
  /* shadows — colored, layered */
  --shadow-subtle: 0 1px 2px rgba(28, 27, 24, 0.06);
  --shadow-raised: 0 4px 12px rgba(28, 27, 24, 0.10);
  --shadow-floating: 0 12px 32px rgba(28, 27, 24, 0.16);
  /* motion */
  --ease: cubic-bezier(0.22, 1, 0.36, 1);
  --dur-micro: 120ms;
  --dur-state: 240ms;
}

:root[data-theme="dark"] {
  --bg: #0f1218;          /* deep blue-black */
  --surface: #161b23;
  --surface-raised: #1d2430;
  --text: #e8e8f0;
  --text-muted: #9aa3b2;
  --border: #2a313d;
  --accent: #3a9d97;
  --accent-contrast: #06110f;
  --shadow-subtle: 0 1px 2px rgba(0, 0, 0, 0.4);
  --shadow-raised: 0 4px 14px rgba(0, 0, 0, 0.5);
  --shadow-floating: 0 12px 36px rgba(0, 0, 0, 0.6);
}

html, body, #root { height: 100%; }
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: "Source Serif 4", Georgia, serif;
  font-weight: 300;
  -webkit-font-smoothing: antialiased;
}
h1, h2, h3, .display { font-family: "Fraunces", Georgia, serif; font-weight: 800; letter-spacing: -0.02em; }
.label { letter-spacing: 0.08em; text-transform: uppercase; font-size: 0.72rem; color: var(--text-muted); }
code, pre, .mono { font-family: "JetBrains Mono", ui-monospace, monospace; }

@media (prefers-reduced-motion: reduce) {
  * { animation: none !important; transition: none !important; }
}
```

- [ ] **Step 5: `desktop/tailwind.config.ts`** — maps Tailwind utilities to the tokens (no default palette as primary):

```ts
import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: ["selector", '[data-theme="dark"]'],
  theme: {
    colors: {
      transparent: "transparent",
      current: "currentColor",
      bg: "var(--bg)",
      surface: "var(--surface)",
      "surface-raised": "var(--surface-raised)",
      text: "var(--text)",
      "text-muted": "var(--text-muted)",
      border: "var(--border)",
      accent: "var(--accent)",
      "accent-contrast": "var(--accent-contrast)",
      tier: {
        working: "var(--tier-working)",
        episodic: "var(--tier-episodic)",
        semantic: "var(--tier-semantic)",
        procedural: "var(--tier-procedural)",
        reflection: "var(--tier-reflection)",
      },
    },
    extend: {
      fontFamily: {
        display: ['"Fraunces"', "Georgia", "serif"],
        body: ['"Source Serif 4"', "Georgia", "serif"],
        mono: ['"JetBrains Mono"', "ui-monospace", "monospace"],
      },
      boxShadow: {
        subtle: "var(--shadow-subtle)",
        raised: "var(--shadow-raised)",
        floating: "var(--shadow-floating)",
      },
      transitionTimingFunction: { brand: "var(--ease)" },
    },
  },
  plugins: [],
} satisfies Config;
```

- [ ] **Step 6: `desktop/src/design/theme.ts`** — tier color lookup + types:

```ts
export const TIERS = ["working", "episodic", "semantic", "procedural", "reflection"] as const;
export type Tier = (typeof TIERS)[number];

export const TIER_COLOR_VAR: Record<Tier, string> = {
  working: "var(--tier-working)",
  episodic: "var(--tier-episodic)",
  semantic: "var(--tier-semantic)",
  procedural: "var(--tier-procedural)",
  reflection: "var(--tier-reflection)",
};

export type ThemeMode = "light" | "dark";
```

- [ ] **Step 7: `desktop/src/design/ThemeProvider.tsx`**

```tsx
import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import type { ThemeMode } from "./theme";

const ThemeCtx = createContext<{ mode: ThemeMode; toggle: () => void }>({
  mode: "light",
  toggle: () => {},
});

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [mode, setMode] = useState<ThemeMode>(() =>
    window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light",
  );
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", mode);
  }, [mode]);
  return (
    <ThemeCtx.Provider value={{ mode, toggle: () => setMode((m) => (m === "light" ? "dark" : "light")) }}>
      {children}
    </ThemeCtx.Provider>
  );
}

export const useTheme = () => useContext(ThemeCtx);
```

- [ ] **Step 8: `desktop/src/design/primitives.tsx`**

```tsx
import type { ButtonHTMLAttributes, ReactNode } from "react";
import { TIER_COLOR_VAR, type Tier } from "./theme";

export function TierChip({ tier }: { tier: Tier }) {
  return (
    <span
      data-tier={tier}
      className="label inline-flex items-center gap-1.5 rounded-sm px-1.5 py-0.5"
      style={{ color: TIER_COLOR_VAR[tier] }}
    >
      <span aria-hidden className="h-2 w-2 rounded-full" style={{ background: TIER_COLOR_VAR[tier] }} />
      {tier}
    </span>
  );
}

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "ghost" };
export function Button({ variant = "primary", className = "", children, ...rest }: ButtonProps) {
  const base =
    "font-body text-sm rounded-md px-3 py-1.5 transition-[transform,box-shadow,background] duration-[120ms] ease-brand focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent active:scale-[0.97] disabled:opacity-50 disabled:pointer-events-none";
  const styles =
    variant === "primary"
      ? "bg-accent text-accent-contrast shadow-subtle hover:shadow-raised"
      : "bg-transparent text-text hover:bg-surface-raised";
  return (
    <button className={`${base} ${styles} ${className}`} {...rest}>
      {children}
    </button>
  );
}

export function Card({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <div className={`bg-surface border border-border rounded-lg shadow-subtle ${className}`}>{children}</div>;
}

export function Skeleton({ className = "" }: { className?: string }) {
  return <div aria-hidden className={`animate-pulse bg-border/60 rounded ${className}`} />;
}
```

- [ ] **Step 9: Import tokens** — add to the top of `desktop/src/main.tsx`: `import "./design/tokens.css";`, and wrap `<App />` in `<ThemeProvider>`.

- [ ] **Step 10: Run tests to verify they pass**

Run: `cd desktop && pnpm test src/design && pnpm typecheck`
Expected: PASS (2 tests), typecheck clean.

- [ ] **Step 11: Commit**

```bash
cd /home/jons/AntiGravityProjects/mnemos
git add desktop/postcss.config.cjs desktop/tailwind.config.ts desktop/src/design/ desktop/src/main.tsx
git commit -m "feat: design system — tier-coded tokens, Tailwind, theme, primitives (Plan 6 Task 5)"
```

---

## Task 6: Typed daemon client + types + token + query hooks + MSW

The typed HTTP client over the daemon, TS types mirroring the Rust structs, secure token retrieval (Tauri `read_token` with a dev fallback), TanStack Query hooks, and MSW handlers/fixtures for tests.

**Files:** `desktop/src/api/types.ts`, `desktop/src/api/token.ts`, `desktop/src/api/client.ts`, `desktop/src/api/queries.ts`, `desktop/src/test/fixtures.ts`, `desktop/src/test/handlers.ts`, `desktop/src/api/client.test.ts`.

- [ ] **Step 1: Write the failing test** — `desktop/src/api/client.test.ts`:

```ts
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { MnemosClient } from "./client";

const server = setupServer(...handlers);
beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const client = new MnemosClient("http://localhost:7423", async () => "test-token");

test("listMemories returns the memories array", async () => {
  const mems = await client.listMemories({ limit: 10 });
  expect(mems.length).toBeGreaterThan(0);
  expect(mems[0]).toHaveProperty("title");
});

test("search returns hits with explain ranks", async () => {
  const hits = await client.search({ query: "rust", k: 5, explain: true });
  expect(hits[0].memory).toHaveProperty("id");
  expect(hits[0].explain).toHaveProperty("rrf_score");
});

test("a non-OK response throws ApiError with status", async () => {
  await expect(client.getMemory("missing")).rejects.toMatchObject({ status: 404 });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd desktop && pnpm test src/api/client`
Expected: FAIL — modules not found.

- [ ] **Step 3: `desktop/src/api/types.ts`** (mirror the Rust structs; `kind` is JSON key `type`)

```ts
export type Tier = "working" | "episodic" | "semantic" | "procedural" | "reflection";
export type MemoryType =
  | "fact" | "episode" | "reflection" | "rule" | "identity" | "project" | "entity" | "community-summary";

export interface Provenance { session?: string | null; chunks: string[]; }

export interface Memory {
  id: string;
  tier: Tier;
  type: MemoryType;
  title: string;
  body: string;
  tags: string[];
  entities: string[];
  links: string[];
  provenance: Provenance[];
  created_at: string;
  ingested_at: string;
  valid_at: string;
  invalid_at: string | null;
  superseded_by: string | null;
  strength: number;
  importance: number;
  last_accessed: string;
  access_count: number;
  workspace: string | null;
  source_tool: string | null;
  mnemos_version: number;
}

export interface Explain {
  bm25_rank: number | null;
  dense_rank: number | null;
  dense_distance: number | null;
  ppr_rank: number | null;
  rrf_score: number;
  weight_recency: number;
  weight_importance: number;
  weight_strength: number;
  weight_tier: number;
  rerank_score: number | null;
  final_score: number;
}

export interface RecallHit {
  memory: Memory;
  score: number;
  bm25_rank: number | null;
  dense_rank: number | null;
  dense_distance: number | null;
  ppr_rank: number | null;
  explain: Explain | null;
}

export interface Entity {
  id: string;
  name: string;
  type?: string;
  kind?: string;
  aliases?: string[];
  description?: string | null;
}
export interface EntityDetail extends Entity {
  mention_count: number;
  memory_ids: string[];
  edges: GraphEdge[];
}
export interface GraphNode { id: string; name: string; kind: string; community_id?: number; mentions?: number; }
export interface GraphEdge { id: string; source: string; target: string; relation: string; weight: number; }
export interface Graph { nodes: GraphNode[]; edges: GraphEdge[]; }

export interface PipelineStatus {
  enabled: boolean;
  llm_model: string | null;
  counters: { completed: number; failed: number; facts_added: number };
  recent: { session_id: string; facts_added: number; ok: boolean; at: string }[];
}

export interface AuditEntry { id: number; ts: string; actor: string; action: string; memory_id: string | null; details: string | null; }

export interface SearchReq {
  query: string;
  k?: number;
  tier?: Tier[];
  workspace?: string;
  include_invalid?: boolean;
  explain?: boolean;
  rerank?: boolean;
  graph?: boolean;
  global?: boolean;
}
```

- [ ] **Step 4: `desktop/src/api/token.ts`**

```ts
// Reads the bearer token via the Tauri command (secret stays in the Rust shell).
// In a plain browser (vitest / `vite dev` without Tauri) falls back to a dev token.
export async function getToken(): Promise<string> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<string>("read_token");
  } catch {
    return import.meta.env.VITE_MNEMOS_TOKEN ?? "dev-token";
  }
}
```

- [ ] **Step 5: `desktop/src/api/client.ts`**

```ts
import type {
  AuditEntry, Entity, EntityDetail, Graph, Memory, PipelineStatus, RecallHit, SearchReq, Tier,
} from "./types";

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
    this.name = "ApiError";
  }
}

export class MnemosClient {
  constructor(
    private baseUrl = "http://localhost:7423",
    private tokenFn: () => Promise<string> = async () => "dev-token",
  ) {}

  private async req<T>(method: string, path: string, body?: unknown): Promise<T> {
    const token = await this.tokenFn();
    const res = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers: {
        authorization: `Bearer ${token}`,
        ...(body !== undefined ? { "content-type": "application/json" } : {}),
      },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
    if (!res.ok) {
      let msg = res.statusText;
      try {
        const j = await res.json();
        msg = (j as { error?: string }).error ?? msg;
      } catch { /* ignore */ }
      throw new ApiError(res.status, msg);
    }
    return (await res.json()) as T;
  }

  async listMemories(q: { tier?: Tier[]; workspace?: string; include_invalid?: boolean; limit?: number } = {}): Promise<Memory[]> {
    const p = new URLSearchParams();
    q.tier?.forEach((t) => p.append("tier", t));
    if (q.workspace) p.set("workspace", q.workspace);
    if (q.include_invalid) p.set("include_invalid", "true");
    p.set("limit", String(q.limit ?? 50));
    return (await this.req<{ memories: Memory[] }>("GET", `/v1/memories?${p}`)).memories;
  }
  getMemory(id: string) { return this.req<Memory>("GET", `/v1/memories/${id}`); }
  createMemory(m: { body: string; title?: string; tier?: Tier; kind?: string; tags?: string[]; importance?: number; workspace?: string }) {
    return this.req<{ id: string }>("POST", "/v1/memories", m);
  }
  patchMemory(id: string, patch: { tags?: string[]; importance?: number }) {
    return this.req<Memory>("PATCH", `/v1/memories/${id}`, patch);
  }
  forgetMemory(id: string, reason?: string) {
    return this.req<{ id: string; status: string }>("DELETE", `/v1/memories/${id}${reason ? `?reason=${encodeURIComponent(reason)}` : ""}`);
  }
  async search(req: SearchReq): Promise<RecallHit[]> {
    return (await this.req<{ hits: RecallHit[] }>("POST", "/v1/memories/search", req)).hits;
  }
  async timeTravel(query: string, as_of: string, k = 10): Promise<Memory[]> {
    return (await this.req<{ memories: Memory[] }>("POST", "/v1/memories/time-travel", { query, as_of, k })).memories;
  }
  async audit(id: string): Promise<AuditEntry[]> {
    return (await this.req<{ entries: AuditEntry[] }>("GET", `/v1/memories/${id}/audit`)).entries;
  }
  async listReflections(limit = 50): Promise<Memory[]> {
    return (await this.req<{ reflections: Memory[] }>("GET", `/v1/reflections?limit=${limit}`)).reflections;
  }
  async reflect(): Promise<string[]> {
    return (await this.req<{ created: string[] }>("POST", "/v1/reflections", {})).created;
  }
  pipelines() { return this.req<PipelineStatus>("GET", "/v1/pipelines"); }
  runDecay() { return this.req<{ scanned: number; decayed: number; invalidated: number }>("POST", "/v1/maintenance/decay", {}); }
  runCommunities() { return this.req<{ summaries: string[] }>("POST", "/v1/maintenance/communities", {}); }
  async listEntities(limit = 100): Promise<Entity[]> {
    return (await this.req<{ entities: Entity[] }>("GET", `/v1/entities?limit=${limit}`)).entities;
  }
  getEntity(id: string) { return this.req<EntityDetail>("GET", `/v1/entities/${id}`); }
  entityGraph(id: string) { return this.req<Graph>("GET", `/v1/entities/${id}/graph`); }
  graph() { return this.req<Graph>("GET", "/v1/graph"); }
  communities() { return this.req<{ communities: { community_id: number; members: Entity[] }[]; summaries: Memory[] }>("GET", "/v1/communities"); }
  async working(): Promise<Memory[]> {
    return (await this.req<{ memories: Memory[] }>("GET", "/v1/working")).memories;
  }
}

export const client = new MnemosClient(import.meta.env.VITE_MNEMOS_URL ?? "http://localhost:7423");
```

- [ ] **Step 6: `desktop/src/api/token.ts` wiring into the singleton** — update the `client` export to pass `getToken`:

In `client.ts`, import `getToken` and change the singleton to `new MnemosClient(import.meta.env.VITE_MNEMOS_URL ?? "http://localhost:7423", getToken)`. (Add `import { getToken } from "./token";` at the top.)

- [ ] **Step 7: `desktop/src/test/fixtures.ts` + `desktop/src/test/handlers.ts`**

```ts
// fixtures.ts
import type { Memory, RecallHit } from "../api/types";
export const memFixture = (over: Partial<Memory> = {}): Memory => ({
  id: "mem_1", tier: "semantic", type: "fact", title: "Rust note", body: "Shaun prefers Rust",
  tags: [], entities: [], links: [], provenance: [], created_at: "2026-05-01T00:00:00+00:00",
  ingested_at: "2026-05-01T00:00:00+00:00", valid_at: "2026-05-01T00:00:00+00:00", invalid_at: null,
  superseded_by: null, strength: 1, importance: 0.5, last_accessed: "2026-05-01T00:00:00+00:00",
  access_count: 0, workspace: null, source_tool: null, mnemos_version: 1, ...over,
});
export const hitFixture = (): RecallHit => ({
  memory: memFixture(), score: 1.2, bm25_rank: 1, dense_rank: 2, dense_distance: 0.1, ppr_rank: 3,
  explain: { bm25_rank: 1, dense_rank: 2, dense_distance: 0.1, ppr_rank: 3, rrf_score: 0.05,
    weight_recency: 0.9, weight_importance: 1.5, weight_strength: 1, weight_tier: 1, rerank_score: null, final_score: 1.2 },
});
```

```ts
// handlers.ts
import { http, HttpResponse } from "msw";
import { hitFixture, memFixture } from "./fixtures";
const base = "http://localhost:7423";
export const handlers = [
  http.get(`${base}/v1/memories`, () => HttpResponse.json({ memories: [memFixture()] })),
  http.get(`${base}/v1/memories/missing`, () => HttpResponse.json({ error: "memory not found" }, { status: 404 })),
  http.get(`${base}/v1/memories/:id`, ({ params }) => HttpResponse.json(memFixture({ id: String(params.id) }))),
  http.post(`${base}/v1/memories/search`, () => HttpResponse.json({ hits: [hitFixture()] })),
  http.get(`${base}/v1/pipelines`, () => HttpResponse.json({ enabled: true, llm_model: "mock-llm", counters: { completed: 1, failed: 0, facts_added: 3 }, recent: [] })),
  http.get(`${base}/v1/reflections`, () => HttpResponse.json({ reflections: [memFixture({ id: "mem_r", tier: "reflection", type: "reflection", title: "Reflection (insight)" })] })),
  http.get(`${base}/v1/graph`, () => HttpResponse.json({ nodes: [{ id: "ent_a", name: "Rust", kind: "tool", community_id: 0, mentions: 2 }, { id: "ent_b", name: "Tauri", kind: "tool", community_id: 0, mentions: 1 }], edges: [{ id: "edge_1", source: "ent_a", target: "ent_b", relation: "uses", weight: 2 }] })),
  http.get(`${base}/v1/communities`, () => HttpResponse.json({ communities: [{ community_id: 0, members: [{ id: "ent_a", name: "Rust" }] }], summaries: [] })),
  http.get(`${base}/v1/entities`, () => HttpResponse.json({ entities: [{ id: "ent_a", name: "Rust", kind: "tool" }] })),
];
```

- [ ] **Step 8: `desktop/src/api/queries.ts`** (TanStack Query hooks the views use)

```ts
import { useQuery } from "@tanstack/react-query";
import { client } from "./client";
import type { SearchReq, Tier } from "./types";

export const useMemories = (tier?: Tier[]) =>
  useQuery({ queryKey: ["memories", tier], queryFn: () => client.listMemories({ tier, limit: 100 }) });
export const useMemory = (id: string | null) =>
  useQuery({ queryKey: ["memory", id], queryFn: () => client.getMemory(id!), enabled: !!id });
export const useSearch = (req: SearchReq | null) =>
  useQuery({ queryKey: ["search", req], queryFn: () => client.search(req!), enabled: !!req && !!req.query });
export const useGraph = () => useQuery({ queryKey: ["graph"], queryFn: () => client.graph() });
export const useCommunities = () => useQuery({ queryKey: ["communities"], queryFn: () => client.communities() });
export const usePipelines = () => useQuery({ queryKey: ["pipelines"], queryFn: () => client.pipelines(), refetchInterval: 5000 });
export const useReflections = () => useQuery({ queryKey: ["reflections"], queryFn: () => client.listReflections() });
export const useEntity = (id: string | null) =>
  useQuery({ queryKey: ["entity", id], queryFn: () => client.getEntity(id!), enabled: !!id });
export const useAudit = (id: string | null) =>
  useQuery({ queryKey: ["audit", id], queryFn: () => client.audit(id!), enabled: !!id });
```

- [ ] **Step 9: Run tests to verify they pass**

Run: `cd desktop && pnpm test src/api/client && pnpm typecheck`
Expected: PASS (3 tests), typecheck clean.

- [ ] **Step 10: Commit**

```bash
cd /home/jons/AntiGravityProjects/mnemos
git add desktop/src/api/ desktop/src/test/
git commit -m "feat: typed daemon client, types, token, query hooks, MSW (Plan 6 Task 6)"
```

---

## Task 7: WebSocket live events + Zustand event store

Connect to `ws://localhost:7423/v1/events?token=…`, parse `Event`s, and invalidate the relevant TanStack Query caches so the UI updates live. A small Zustand store tracks connection status + recent events (for a top-bar indicator + pipeline view).

**Files:** `desktop/src/store/events.ts`, `desktop/src/api/ws.ts`, `desktop/src/store/events.test.ts`.

- [ ] **Step 1: Write the failing test** — `desktop/src/store/events.test.ts`:

```ts
import { useEventStore } from "./events";

test("ingesting an event updates status and recent list", () => {
  useEventStore.getState().setStatus("open");
  useEventStore.getState().push({ type: "memory_created", id: "mem_9", title: "X", tier: "semantic" });
  const s = useEventStore.getState();
  expect(s.status).toBe("open");
  expect(s.recent[0]).toMatchObject({ type: "memory_created", id: "mem_9" });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd desktop && pnpm test src/store/events`
Expected: FAIL — module not found.

- [ ] **Step 3: `desktop/src/store/events.ts`**

```ts
import { create } from "zustand";

export type DaemonEvent =
  | { type: "memory_created"; id: string; title: string; tier: string }
  | { type: "memory_updated"; id: string }
  | { type: "memory_invalidated"; id: string; reason: string | null }
  | { type: "session_started"; id: string }
  | { type: "session_ended"; id: string }
  | { type: "pipeline_completed"; session_id: string; facts_added: number }
  | { type: "pipeline_failed"; session_id: string; error: string }
  | { type: "reflection_completed"; reflections_created: number }
  | { type: "community_detected"; communities: number };

type Status = "connecting" | "open" | "closed";

interface EventState {
  status: Status;
  recent: DaemonEvent[];
  setStatus: (s: Status) => void;
  push: (e: DaemonEvent) => void;
}

export const useEventStore = create<EventState>((set) => ({
  status: "connecting",
  recent: [],
  setStatus: (status) => set({ status }),
  push: (e) => set((st) => ({ recent: [e, ...st.recent].slice(0, 50) })),
}));
```

- [ ] **Step 4: `desktop/src/api/ws.ts`** (connect + cache invalidation; reconnect with backoff)

```ts
import type { QueryClient } from "@tanstack/react-query";
import { getToken } from "./token";
import { useEventStore, type DaemonEvent } from "../store/events";

const INVALIDATE: Record<string, string[][]> = {
  memory_created: [["memories"], ["graph"]],
  memory_updated: [["memories"], ["memory"]],
  memory_invalidated: [["memories"], ["memory"]],
  pipeline_completed: [["pipelines"], ["memories"], ["graph"]],
  pipeline_failed: [["pipelines"]],
  reflection_completed: [["reflections"], ["memories"]],
  community_detected: [["communities"], ["graph"]],
  session_started: [], session_ended: [],
};

export function connectEvents(queryClient: QueryClient, baseUrl = "localhost:7423"): () => void {
  let ws: WebSocket | null = null;
  let closed = false;
  let backoff = 500;

  const open = async () => {
    if (closed) return;
    const token = await getToken();
    useEventStore.getState().setStatus("connecting");
    ws = new WebSocket(`ws://${baseUrl}/v1/events?token=${encodeURIComponent(token)}`);
    ws.onopen = () => { backoff = 500; useEventStore.getState().setStatus("open"); };
    ws.onmessage = (msg) => {
      try {
        const e = JSON.parse(msg.data) as DaemonEvent;
        useEventStore.getState().push(e);
        for (const key of INVALIDATE[e.type] ?? []) queryClient.invalidateQueries({ queryKey: key });
      } catch { /* ignore malformed */ }
    };
    ws.onclose = () => {
      useEventStore.getState().setStatus("closed");
      if (!closed) { setTimeout(open, backoff); backoff = Math.min(backoff * 2, 8000); }
    };
  };
  void open();
  return () => { closed = true; ws?.close(); };
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd desktop && pnpm test src/store/events && pnpm typecheck`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cd /home/jons/AntiGravityProjects/mnemos
git add desktop/src/store/events.ts desktop/src/api/ws.ts desktop/src/store/events.test.ts
git commit -m "feat: WebSocket live events + Zustand event store + cache invalidation (Plan 6 Task 7)"
```

---

## Task 8: App shell — router, three-column layout, UI store

TanStack Router with routes for every view, the three-column `Shell` (top bar · left sidebar · center `Outlet` · right inspector), and a Zustand `ui` store (selected memory, inspector open, as-of date). `App` wires QueryClientProvider + ThemeProvider + RouterProvider + `connectEvents`.

**Files:** `desktop/src/store/ui.ts`, `desktop/src/layout/{Shell,TopBar,LeftSidebar,Inspector}.tsx`, `desktop/src/router.tsx`, `desktop/src/App.tsx` (replace), `desktop/src/layout/Shell.test.tsx`.

- [ ] **Step 1: Write the failing test** — `desktop/src/layout/Shell.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { Shell } from "./Shell";

test("shell renders the three regions and brand", () => {
  render(<Shell><div>center content</div></Shell>);
  expect(screen.getByText(/center content/)).toBeInTheDocument();
  expect(screen.getByRole("banner")).toBeInTheDocument();        // top bar
  expect(screen.getByRole("navigation")).toBeInTheDocument();    // left sidebar
  expect(screen.getByRole("complementary")).toBeInTheDocument(); // inspector
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd desktop && pnpm test src/layout/Shell`
Expected: FAIL — module not found.

- [ ] **Step 3: `desktop/src/store/ui.ts`**

```ts
import { create } from "zustand";

interface UiState {
  selectedMemoryId: string | null;
  inspectorOpen: boolean;
  asOf: string | null; // ISO date for time-travel mode; null = present
  select: (id: string | null) => void;
  toggleInspector: () => void;
  setAsOf: (d: string | null) => void;
}

export const useUiStore = create<UiState>((set) => ({
  selectedMemoryId: null,
  inspectorOpen: true,
  asOf: null,
  select: (selectedMemoryId) => set({ selectedMemoryId, inspectorOpen: true }),
  toggleInspector: () => set((s) => ({ inspectorOpen: !s.inspectorOpen })),
  setAsOf: (asOf) => set({ asOf }),
}));
```

- [ ] **Step 4: Layout components**

`desktop/src/layout/TopBar.tsx`:
```tsx
import { useEventStore } from "../store/events";
import { useUiStore } from "../store/ui";

export function TopBar({ onCommand }: { onCommand: () => void }) {
  const status = useEventStore((s) => s.status);
  const asOf = useUiStore((s) => s.asOf);
  const dot = status === "open" ? "var(--accent)" : status === "connecting" ? "var(--tier-working)" : "var(--tier-procedural)";
  return (
    <header role="banner" className="flex items-center gap-3 border-b border-border bg-surface px-4 h-12 shrink-0">
      <span className="display text-lg">mnemos</span>
      <button onClick={onCommand} className="label ml-2 rounded-md border border-border px-2 py-1 hover:bg-surface-raised">
        ⌘K  Search / commands
      </button>
      {asOf && (
        <span className="ml-2 rounded-full px-2 py-0.5 text-xs" style={{ background: "var(--tier-episodic)", color: "#fff" }}>
          viewing {asOf.slice(0, 10)}
        </span>
      )}
      <span className="ml-auto flex items-center gap-1.5 label" title={`daemon ${status}`}>
        <span className="h-2 w-2 rounded-full" style={{ background: dot }} /> {status}
      </span>
    </header>
  );
}
```

`desktop/src/layout/LeftSidebar.tsx`:
```tsx
import { Link } from "@tanstack/react-router";
import { TIERS } from "../design/theme";
import { TierChip } from "../design/primitives";

const NAV = [
  ["/", "Browser"], ["/search", "Search"], ["/graph", "Graph"], ["/timeline", "Timeline"],
  ["/pipelines", "Pipelines"], ["/reflections", "Reflections"], ["/audit", "Audit"],
] as const;

export function LeftSidebar() {
  return (
    <nav role="navigation" className="w-56 shrink-0 border-r border-border bg-surface p-3 overflow-y-auto">
      <div className="label mb-1">Views</div>
      <ul className="space-y-0.5">
        {NAV.map(([to, label]) => (
          <li key={to}>
            <Link to={to} className="block rounded-md px-2 py-1 text-sm hover:bg-surface-raised [&.active]:bg-surface-raised [&.active]:text-accent">
              {label}
            </Link>
          </li>
        ))}
      </ul>
      <div className="label mt-4 mb-1">Tiers</div>
      <ul className="space-y-0.5">
        {TIERS.map((t) => (
          <li key={t}>
            <Link to="/" search={{ tier: t }} className="block rounded-md px-2 py-1 hover:bg-surface-raised">
              <TierChip tier={t} />
            </Link>
          </li>
        ))}
      </ul>
    </nav>
  );
}
```

`desktop/src/layout/Inspector.tsx`:
```tsx
import { useUiStore } from "../store/ui";
import { useMemory, useAudit } from "../api/queries";
import { TierChip } from "../design/primitives";

export function Inspector() {
  const { selectedMemoryId, inspectorOpen } = useUiStore();
  const { data: mem } = useMemory(selectedMemoryId);
  const { data: audit } = useAudit(selectedMemoryId);
  if (!inspectorOpen) return null;
  return (
    <aside role="complementary" className="w-80 shrink-0 border-l border-border bg-surface p-4 overflow-y-auto">
      <div className="label mb-2">Inspector</div>
      {!selectedMemoryId && <p className="text-sm text-text-muted">Select a memory to inspect.</p>}
      {mem && (
        <div className="space-y-3">
          <h2 className={`display text-base ${mem.invalid_at ? "line-through opacity-60" : ""}`}>{mem.title}</h2>
          <TierChip tier={mem.tier} />
          <dl className="text-sm space-y-1">
            <div className="flex justify-between"><dt className="text-text-muted">strength</dt><dd className="mono">{mem.strength.toFixed(2)}</dd></div>
            <div className="flex justify-between"><dt className="text-text-muted">importance</dt><dd className="mono">{mem.importance.toFixed(2)}</dd></div>
            <div className="flex justify-between"><dt className="text-text-muted">valid</dt><dd className="mono">{mem.valid_at.slice(0, 10)}</dd></div>
          </dl>
          {!!mem.provenance.length && (
            <div><div className="label">provenance</div><ul className="text-xs mono">{mem.provenance.map((p, i) => <li key={i}>{p.session ?? "—"} · {p.chunks.length} chunks</li>)}</ul></div>
          )}
          <div>
            <div className="label">audit</div>
            <ul className="text-xs mono space-y-0.5">{(audit ?? []).map((a) => <li key={a.id}>{a.ts.slice(0, 16)} · {a.action}</li>)}</ul>
          </div>
        </div>
      )}
    </aside>
  );
}
```

`desktop/src/layout/Shell.tsx`:
```tsx
import { useState, type ReactNode } from "react";
import { TopBar } from "./TopBar";
import { LeftSidebar } from "./LeftSidebar";
import { Inspector } from "./Inspector";
import { CommandPalette } from "../components/CommandPalette";

export function Shell({ children }: { children: ReactNode }) {
  const [paletteOpen, setPaletteOpen] = useState(false);
  return (
    <div className="flex h-full flex-col">
      <TopBar onCommand={() => setPaletteOpen(true)} />
      <div className="flex min-h-0 flex-1">
        <LeftSidebar />
        <main className="min-w-0 flex-1 overflow-y-auto">{children}</main>
        <Inspector />
      </div>
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
    </div>
  );
}
```

> `CommandPalette` is created in Task 21; for THIS task add a temporary stub `desktop/src/components/CommandPalette.tsx` exporting `export function CommandPalette(_: { open: boolean; onClose: () => void }) { return null; }` so the shell compiles. Task 21 replaces it.

- [ ] **Step 5: Router + App**

`desktop/src/router.tsx` — define a root route rendering `<Shell><Outlet/></Shell>` and child routes for each view. Use placeholder view components that Tasks 9-20 replace; for now import the real ones if they exist, else a stub. To avoid forward-dependency churn, create `desktop/src/views/index.tsx` exporting stubs:

```tsx
// desktop/src/views/index.tsx — stubs replaced by Tasks 9-20.
export const Browser = () => <div className="p-6">Browser</div>;
export const Search = () => <div className="p-6">Search</div>;
export const Graph = () => <div className="p-6">Graph</div>;
export const Timeline = () => <div className="p-6">Timeline</div>;
export const Pipelines = () => <div className="p-6">Pipelines</div>;
export const Reflections = () => <div className="p-6">Reflections</div>;
export const Audit = () => <div className="p-6">Audit</div>;
export const EntityProfile = () => <div className="p-6">Entity</div>;
export const Editor = () => <div className="p-6">Editor</div>;
```

`desktop/src/router.tsx`:
```tsx
import { createRootRoute, createRoute, createRouter, Outlet } from "@tanstack/react-router";
import { Shell } from "./layout/Shell";
import * as V from "./views";

const rootRoute = createRootRoute({ component: () => (<Shell><Outlet /></Shell>) });
const r = (path: string, component: () => JSX.Element) => createRoute({ getParentRoute: () => rootRoute, path, component });

const routes = [
  r("/", V.Browser), r("/search", V.Search), r("/graph", V.Graph), r("/timeline", V.Timeline),
  r("/pipelines", V.Pipelines), r("/reflections", V.Reflections), r("/audit", V.Audit),
  r("/editor/$id", V.Editor), r("/entity/$id", V.EntityProfile),
];
const routeTree = rootRoute.addChildren(routes);
export const router = createRouter({ routeTree });
declare module "@tanstack/react-router" { interface Register { router: typeof router; } }
```

`desktop/src/App.tsx` (replace):
```tsx
import { useEffect } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { ThemeProvider } from "./design/ThemeProvider";
import { router } from "./router";
import { connectEvents } from "./api/ws";

const queryClient = new QueryClient({ defaultOptions: { queries: { staleTime: 10_000, retry: 1 } } });

export default function App() {
  useEffect(() => connectEvents(queryClient), []);
  return (
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </ThemeProvider>
  );
}
```

> The `App.test.tsx` smoke test from Task 4 asserted an `<h1>mnemos</h1>`; the brand now lives in `TopBar` (still text "mnemos"). Update that test to assert `screen.getByText(/mnemos/i)` (the TopBar brand) rendered within a `RouterProvider`, OR keep Shell's test as the coverage and delete the brittle Task-4 smoke test. Prefer: replace `App.test.tsx` with a render that wraps in the providers and asserts the TopBar brand is present.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd desktop && pnpm test src/layout && pnpm typecheck`
Expected: PASS; typecheck clean.

- [ ] **Step 7: Commit**

```bash
cd /home/jons/AntiGravityProjects/mnemos
git add desktop/src/store/ui.ts desktop/src/layout/ desktop/src/router.tsx desktop/src/App.tsx desktop/src/views/index.tsx desktop/src/components/CommandPalette.tsx desktop/src/App.test.tsx
git commit -m "feat: app shell — router, three-column layout, UI store (Plan 6 Task 8)"
```

---

# Group V — Views

Each view task: replace the stub in `src/views/index.tsx` with a real component file, wire it into the router import, write one Testing-Library test (render against MSW → content shows), handle loading (`Skeleton`) / error / empty states, commit. Every view consumes design tokens only.

> Convention for this group: move each view into its own file `src/views/<Name>.tsx` and re-export from `src/views/index.tsx` (replace the matching stub line with `export { Browser } from "./Browser";` etc.). Tests live beside the view (`src/views/<Name>.test.tsx`). Tests wrap the component in a `QueryClientProvider` (+ a `RouterProvider` memory history when the view uses router hooks). Add a tiny `src/test/renderWithQuery.tsx` helper in Task 9 and reuse it.

## Task 9: Tier browser view

**Files:** `desktop/src/test/renderWithQuery.tsx` (new helper), `desktop/src/views/Browser.tsx`, `desktop/src/views/index.tsx` (re-export), `desktop/src/views/Browser.test.tsx`.

- [ ] **Step 1: Helper + failing test**

`desktop/src/test/renderWithQuery.tsx`:
```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import type { ReactElement } from "react";

export function renderWithQuery(ui: ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}
```

`desktop/src/views/Browser.test.tsx`:
```tsx
import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Browser } from "./Browser";

const server = setupServer(...handlers);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("lists memories with their tier", async () => {
  renderWithQuery(<Browser />);
  expect(await screen.findByText("Rust note")).toBeInTheDocument();
  expect(screen.getByText(/semantic/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails** — `cd desktop && pnpm test src/views/Browser` → FAIL (module missing).

- [ ] **Step 3: `desktop/src/views/Browser.tsx`**

```tsx
import { useMemories } from "../api/queries";
import { useUiStore } from "../store/ui";
import { TierChip, Skeleton, Card } from "../design/primitives";
import { Link } from "@tanstack/react-router";

export function Browser() {
  const { data, isLoading, isError } = useMemories();
  const select = useUiStore((s) => s.select);
  if (isLoading) return <div className="p-6 space-y-2">{Array.from({ length: 6 }).map((_, i) => <Skeleton key={i} className="h-10 w-full" />)}</div>;
  if (isError) return <div className="p-6 text-tier-procedural">Could not load memories. Is the daemon running?</div>;
  if (!data?.length) return <div className="p-6 text-text-muted">No memories yet. Press ⌘K → New memory to add one.</div>;
  return (
    <div className="p-6 space-y-2">
      <h1 className="display text-xl mb-3">Memories</h1>
      {data.map((m) => (
        <Card key={m.id} className="p-3 hover:shadow-raised transition-shadow duration-[120ms]">
          <button onClick={() => select(m.id)} className="block w-full text-left">
            <div className="flex items-center justify-between gap-2">
              <span className={`font-body ${m.invalid_at ? "line-through opacity-60" : ""}`}>{m.title}</span>
              <TierChip tier={m.tier} />
            </div>
          </button>
          <Link to="/editor/$id" params={{ id: m.id }} className="label text-accent">edit</Link>
        </Card>
      ))}
    </div>
  );
}
```

- [ ] **Step 4: Re-export** — in `src/views/index.tsx` replace `export const Browser = ...` with `export { Browser } from "./Browser";`.

- [ ] **Step 5: Pass + commit**
```bash
cd desktop && pnpm test src/views/Browser && pnpm typecheck
cd /home/jons/AntiGravityProjects/mnemos
git add desktop/src/test/renderWithQuery.tsx desktop/src/views/Browser.tsx desktop/src/views/index.tsx desktop/src/views/Browser.test.tsx
git commit -m "feat: tier browser view (Plan 6 Task 9)"
```

---

## Task 10: Memory editor view

CodeMirror 6 shows the body (read-only — bodies are files, edited on disk and reindexed by the watcher); the form edits **tags + importance** via `PATCH` (the only mutable metadata the daemon exposes). A note explains body edits go through the `.md` file.

**Files:** `desktop/src/views/Editor.tsx`, `desktop/src/components/CodeMirror.tsx`, index re-export, `desktop/src/views/Editor.test.tsx`.

- [ ] **Step 1: Failing test** — `desktop/src/views/Editor.test.tsx`:

```tsx
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Editor } from "./Editor";

const server = setupServer(...handlers);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("loads a memory and saves importance via PATCH", async () => {
  let patched: unknown = null;
  server.use(http.patch("http://localhost:7423/v1/memories/:id", async ({ request }) => {
    patched = await request.json();
    return HttpResponse.json({ ...(await import("../test/fixtures")).memFixture(), importance: 0.9 });
  }));
  renderWithQuery(<Editor id="mem_1" />);
  expect(await screen.findByDisplayValue(/Rust note/i)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: /save/i }));
  expect(patched).toHaveProperty("importance");
});
```

> The route component reads `id` from params; the test renders `<Editor id="mem_1" />` directly. Make `Editor` accept an optional `id` prop and fall back to the route param so it's testable in isolation.

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: `desktop/src/components/CodeMirror.tsx`** (thin read-only viewer)

```tsx
import { useEffect, useRef } from "react";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { markdown } from "@codemirror/lang-markdown";

export function CodeMirrorView({ value }: { value: string }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!ref.current) return;
    const view = new EditorView({
      state: EditorState.create({ doc: value, extensions: [markdown(), EditorView.editable.of(false), EditorView.lineWrapping] }),
      parent: ref.current,
    });
    return () => view.destroy();
  }, [value]);
  return <div ref={ref} className="mono text-sm border border-border rounded-md max-h-80 overflow-auto" />;
}
```

- [ ] **Step 4: `desktop/src/views/Editor.tsx`**

```tsx
import { useEffect, useState } from "react";
import { useParams } from "@tanstack/react-router";
import { useQueryClient } from "@tanstack/react-query";
import { useMemory } from "../api/queries";
import { client } from "../api/client";
import { CodeMirrorView } from "../components/CodeMirror";
import { Button, Skeleton, TierChip } from "../design/primitives";

export function Editor({ id: idProp }: { id?: string }) {
  const params = useParams({ strict: false }) as { id?: string };
  const id = idProp ?? params.id ?? null;
  const { data: mem, isLoading } = useMemory(id);
  const qc = useQueryClient();
  const [tags, setTags] = useState("");
  const [importance, setImportance] = useState(0.5);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (mem) { setTags(mem.tags.join(", ")); setImportance(mem.importance); }
  }, [mem]);

  if (isLoading || !mem) return <div className="p-6"><Skeleton className="h-64 w-full" /></div>;

  const save = async () => {
    setSaving(true);
    try {
      await client.patchMemory(mem.id, { tags: tags.split(",").map((t) => t.trim()).filter(Boolean), importance });
      await qc.invalidateQueries({ queryKey: ["memory", mem.id] });
      await qc.invalidateQueries({ queryKey: ["memories"] });
    } finally { setSaving(false); }
  };

  return (
    <div className="p-6 space-y-4 max-w-3xl">
      <div className="flex items-center gap-2">
        <input className="display text-xl bg-transparent border-b border-border flex-1" defaultValue={mem.title} readOnly />
        <TierChip tier={mem.tier} />
      </div>
      <label className="block">
        <span className="label">tags (comma-separated)</span>
        <input className="mono w-full bg-surface border border-border rounded-md px-2 py-1" value={tags} onChange={(e) => setTags(e.target.value)} />
      </label>
      <label className="block">
        <span className="label">importance: {importance.toFixed(2)}</span>
        <input type="range" min={0} max={1} step={0.05} value={importance} onChange={(e) => setImportance(Number(e.target.value))} className="w-full accent-accent" />
      </label>
      <div>
        <span className="label">body (read-only — edit the .md file to change)</span>
        <CodeMirrorView value={mem.body} />
      </div>
      <Button onClick={save} disabled={saving}>{saving ? "Saving…" : "Save"}</Button>
    </div>
  );
}
```

- [ ] **Step 5: Re-export + pass + commit** (replace the `Editor` stub line; `pnpm test src/views/Editor && pnpm typecheck`).
```bash
git add desktop/src/views/Editor.tsx desktop/src/components/CodeMirror.tsx desktop/src/views/index.tsx desktop/src/views/Editor.test.tsx
git commit -m "feat: memory editor view — metadata patch + body viewer (Plan 6 Task 10)"
```

---

## Task 11: Search view with explainability rank bars

Hybrid search UI: query input, filters (tier multi-select, include-invalid, rerank), `graph`/`global` toggles, and results showing a `RankBars` strip (BM25 / dense / PPR rank + final score) from `explain`.

**Files:** `desktop/src/components/RankBars.tsx`, `desktop/src/views/Search.tsx`, index re-export, `desktop/src/views/Search.test.tsx`.

- [ ] **Step 1: Failing test** — `desktop/src/views/Search.test.tsx`:

```tsx
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Search } from "./Search";

const server = setupServer(...handlers);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("runs a search and shows a result with rank bars", async () => {
  renderWithQuery(<Search />);
  await userEvent.type(screen.getByPlaceholderText(/search/i), "rust");
  await userEvent.click(screen.getByRole("button", { name: /search/i }));
  expect(await screen.findByText("Rust note")).toBeInTheDocument();
  expect(screen.getByText(/PPR/i)).toBeInTheDocument(); // rank bar label
});
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: `desktop/src/components/RankBars.tsx`**

```tsx
import type { Explain } from "../api/types";

function Bar({ label, rank }: { label: string; rank: number | null }) {
  const present = rank != null;
  return (
    <span className="label flex items-center gap-1" title={present ? `rank ${rank}` : "not matched by this retriever"}>
      <span className="h-2 w-2 rounded-full" style={{ background: present ? "var(--accent)" : "var(--border)" }} />
      {label}{present ? ` #${rank}` : ""}
    </span>
  );
}

export function RankBars({ explain }: { explain: Explain | null }) {
  if (!explain) return null;
  return (
    <div className="flex flex-wrap gap-3">
      <Bar label="BM25" rank={explain.bm25_rank} />
      <Bar label="Dense" rank={explain.dense_rank} />
      <Bar label="PPR" rank={explain.ppr_rank} />
      <span className="label mono">score {explain.final_score.toFixed(3)}</span>
    </div>
  );
}
```

- [ ] **Step 4: `desktop/src/views/Search.tsx`**

```tsx
import { useState } from "react";
import { useSearch } from "../api/queries";
import { useUiStore } from "../store/ui";
import { RankBars } from "../components/RankBars";
import { Button, Card, Skeleton, TierChip } from "../design/primitives";
import type { SearchReq } from "../api/types";

export function Search() {
  const [draft, setDraft] = useState("");
  const [req, setReq] = useState<SearchReq | null>(null);
  const [graph, setGraph] = useState(true);
  const [global, setGlobal] = useState(false);
  const { data: hits, isLoading } = useSearch(req);
  const select = useUiStore((s) => s.select);

  const run = () => setReq({ query: draft, k: 20, explain: true, graph, global });

  return (
    <div className="p-6 space-y-4">
      <h1 className="display text-xl">Search</h1>
      <div className="flex gap-2">
        <input className="flex-1 bg-surface border border-border rounded-md px-3 py-2 font-body"
          placeholder="Search memories…" value={draft}
          onChange={(e) => setDraft(e.target.value)} onKeyDown={(e) => e.key === "Enter" && run()} />
        <Button onClick={run}>Search</Button>
      </div>
      <div className="flex gap-4 label">
        <label className="flex items-center gap-1"><input type="checkbox" checked={graph} onChange={(e) => setGraph(e.target.checked)} /> graph (PPR)</label>
        <label className="flex items-center gap-1"><input type="checkbox" checked={global} onChange={(e) => setGlobal(e.target.checked)} /> global (communities)</label>
      </div>
      {isLoading && <Skeleton className="h-24 w-full" />}
      {req && !isLoading && !hits?.length && <p className="text-text-muted">No matches.</p>}
      <div className="space-y-2">
        {hits?.map((h) => (
          <Card key={h.memory.id} className="p-3 space-y-2">
            <button onClick={() => select(h.memory.id)} className="flex w-full items-center justify-between text-left">
              <span className={h.memory.invalid_at ? "line-through opacity-60" : ""}>{h.memory.title}</span>
              <TierChip tier={h.memory.tier} />
            </button>
            <RankBars explain={h.explain} />
          </Card>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 5: Re-export + pass + commit.**
```bash
git add desktop/src/components/RankBars.tsx desktop/src/views/Search.tsx desktop/src/views/index.tsx desktop/src/views/Search.test.tsx
git commit -m "feat: search view with explainability rank bars (Plan 6 Task 11)"
```

---

## Task 12: Pipeline status view

Per-pipeline status from `GET /v1/pipelines` (live via the 5s refetch + WS invalidation), recent runs, and maintenance triggers (`runDecay`, `runCommunities`).

**Files:** `desktop/src/views/Pipelines.tsx`, index re-export, `desktop/src/views/Pipelines.test.tsx`.

- [ ] **Step 1: Failing test** — `desktop/src/views/Pipelines.test.tsx`:

```tsx
import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Pipelines } from "./Pipelines";

const server = setupServer(...handlers);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("shows pipeline counters and model", async () => {
  renderWithQuery(<Pipelines />);
  expect(await screen.findByText(/mock-llm/i)).toBeInTheDocument();
  expect(screen.getByText(/facts added/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: `desktop/src/views/Pipelines.tsx`**

```tsx
import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { usePipelines } from "../api/queries";
import { client } from "../api/client";
import { Button, Card, Skeleton } from "../design/primitives";

function Stat({ label, value }: { label: string; value: number | string }) {
  return <div className="flex flex-col"><span className="label">{label}</span><span className="display text-2xl">{value}</span></div>;
}

export function Pipelines() {
  const { data, isLoading } = usePipelines();
  const qc = useQueryClient();
  const [busy, setBusy] = useState<string | null>(null);
  const trigger = async (which: "decay" | "communities") => {
    setBusy(which);
    try { which === "decay" ? await client.runDecay() : await client.runCommunities(); await qc.invalidateQueries({ queryKey: ["pipelines"] }); }
    finally { setBusy(null); }
  };
  if (isLoading || !data) return <div className="p-6"><Skeleton className="h-40 w-full" /></div>;
  return (
    <div className="p-6 space-y-4">
      <h1 className="display text-xl">Pipelines</h1>
      <Card className="p-4">
        <div className="label mb-2">learning · {data.enabled ? data.llm_model ?? "unknown" : "disabled (no LLM)"}</div>
        <div className="flex gap-8">
          <Stat label="completed" value={data.counters.completed} />
          <Stat label="failed" value={data.counters.failed} />
          <Stat label="facts added" value={data.counters.facts_added} />
        </div>
      </Card>
      <div className="flex gap-2">
        <Button variant="ghost" onClick={() => trigger("decay")} disabled={busy === "decay"}>Run decay</Button>
        <Button variant="ghost" onClick={() => trigger("communities")} disabled={busy === "communities" || !data.enabled}>Detect communities</Button>
      </div>
      <div>
        <div className="label mb-1">recent runs</div>
        {!data.recent.length && <p className="text-text-muted text-sm">No runs yet.</p>}
        <ul className="text-sm mono space-y-0.5">
          {data.recent.map((r, i) => (
            <li key={i} className={r.ok ? "" : "text-tier-procedural"}>{r.at.slice(0, 16)} · {r.session_id.slice(0, 12)} · +{r.facts_added}</li>
          ))}
        </ul>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Re-export + pass + commit.**
```bash
git add desktop/src/views/Pipelines.tsx desktop/src/views/index.tsx desktop/src/views/Pipelines.test.tsx
git commit -m "feat: pipeline status view (Plan 6 Task 12)"
```

---

## Task 13: Reflection viewer

Lists reflection-tier memories grouped by their typed kind (from the `tags`), with a "Reflect now" trigger and a disabled "Promote to procedural" action (re-tiering endpoint lands in Plan 7).

**Files:** `desktop/src/views/Reflections.tsx`, index re-export, `desktop/src/views/Reflections.test.tsx`.

- [ ] **Step 1: Failing test** — render against MSW, assert the reflection title shows and a "Reflect now" button exists.

```tsx
import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Reflections } from "./Reflections";

const server = setupServer(...handlers);
beforeAll(() => server.listen()); afterEach(() => server.resetHandlers()); afterAll(() => server.close());

test("lists reflections and offers Reflect now", async () => {
  renderWithQuery(<Reflections />);
  expect(await screen.findByText(/Reflection \(insight\)/i)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /reflect now/i })).toBeInTheDocument();
});
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: `desktop/src/views/Reflections.tsx`**

```tsx
import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useReflections } from "../api/queries";
import { client } from "../api/client";
import { useUiStore } from "../store/ui";
import { Button, Card, Skeleton } from "../design/primitives";

export function Reflections() {
  const { data, isLoading } = useReflections();
  const qc = useQueryClient();
  const select = useUiStore((s) => s.select);
  const [busy, setBusy] = useState(false);

  const reflectNow = async () => {
    setBusy(true);
    try { await client.reflect(); await qc.invalidateQueries({ queryKey: ["reflections"] }); }
    finally { setBusy(false); }
  };

  if (isLoading) return <div className="p-6"><Skeleton className="h-40 w-full" /></div>;

  return (
    <div className="p-6 space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="display text-xl">Reflections</h1>
        <Button onClick={reflectNow} disabled={busy}>{busy ? "Reflecting…" : "Reflect now"}</Button>
      </div>
      {!data?.length && <p className="text-text-muted">No reflections yet. They form automatically as the system learns, or trigger one now.</p>}
      <div className="space-y-2">
        {data?.map((r) => (
          <Card key={r.id} className="p-3 space-y-1">
            <button onClick={() => select(r.id)} className="block w-full text-left font-body">{r.body}</button>
            <div className="flex items-center justify-between">
              <span className="label">{r.tags.join(" · ") || r.title}</span>
              <button className="label text-text-muted cursor-not-allowed" title="Re-tiering lands in Plan 7" disabled>
                Promote to procedural
              </button>
            </div>
          </Card>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Re-export + pass + commit.**
```bash
git add desktop/src/views/Reflections.tsx desktop/src/views/index.tsx desktop/src/views/Reflections.test.tsx
git commit -m "feat: reflection viewer (Plan 6 Task 13)"
```

---

## Task 14: Entity profile view

Entity detail (name, aliases, description, mention count, mentioned memories, edges) plus a neighborhood force graph (`react-force-graph-2d`) from `/v1/entities/{id}/graph`.

**Files:** `desktop/src/views/EntityProfile.tsx`, `desktop/src/components/EntityNeighborhood.tsx`, index re-export, `desktop/src/views/EntityProfile.test.tsx`.

- [ ] **Step 1: Failing test** (mock the force-graph module — it needs a canvas):

```tsx
import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { renderWithQuery } from "../test/renderWithQuery";
import { EntityProfile } from "./EntityProfile";

vi.mock("react-force-graph-2d", () => ({ default: () => <div data-testid="fg" /> }));

const base = "http://localhost:7423";
const server = setupServer(
  http.get(`${base}/v1/entities/ent_a`, () => HttpResponse.json({ id: "ent_a", name: "Rust", kind: "tool", aliases: [], description: "a language", mention_count: 2, memory_ids: ["mem_1"], edges: [{ id: "e1", source: "ent_a", target: "ent_b", relation: "uses", weight: 2 }] })),
  http.get(`${base}/v1/entities/ent_a/graph`, () => HttpResponse.json({ nodes: [{ id: "ent_a", name: "Rust", kind: "tool" }], edges: [] })),
);
beforeAll(() => server.listen()); afterEach(() => server.resetHandlers()); afterAll(() => server.close());

test("shows entity detail and edges", async () => {
  renderWithQuery(<EntityProfile id="ent_a" />);
  expect(await screen.findByRole("heading", { name: "Rust" })).toBeInTheDocument();
  expect(screen.getByText(/uses/i)).toBeInTheDocument();
  expect(screen.getByText(/2 mentions/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: `desktop/src/components/EntityNeighborhood.tsx`**

```tsx
import { useQuery } from "@tanstack/react-query";
import ForceGraph2D from "react-force-graph-2d";
import { client } from "../api/client";

export function EntityNeighborhood({ id }: { id: string }) {
  const { data } = useQuery({ queryKey: ["entity-graph", id], queryFn: () => client.entityGraph(id) });
  if (!data) return null;
  const graphData = {
    nodes: data.nodes.map((n) => ({ id: n.id, name: n.name })),
    links: data.edges.map((e) => ({ source: e.source, target: e.target })),
  };
  return (
    <div className="border border-border rounded-lg overflow-hidden" style={{ height: 280 }}>
      <ForceGraph2D graphData={graphData} nodeLabel="name" height={280} width={520}
        nodeColor={() => "#1F6F6B"} linkColor={() => "#5B6168"} />
    </div>
  );
}
```

- [ ] **Step 4: `desktop/src/views/EntityProfile.tsx`**

```tsx
import { useParams } from "@tanstack/react-router";
import { useUiStore } from "../store/ui";
import { useEntity } from "../api/queries";
import { EntityNeighborhood } from "../components/EntityNeighborhood";
import { Card, Skeleton } from "../design/primitives";

export function EntityProfile({ id: idProp }: { id?: string }) {
  const params = useParams({ strict: false }) as { id?: string };
  const id = idProp ?? params.id ?? null;
  const { data, isLoading } = useEntity(id);
  const select = useUiStore((s) => s.select);
  if (isLoading || !data || !id) return <div className="p-6"><Skeleton className="h-64 w-full" /></div>;
  return (
    <div className="p-6 space-y-4 max-w-3xl">
      <h1 className="display text-2xl">{data.name}</h1>
      {data.description && <p className="text-text-muted">{data.description}</p>}
      {!!data.aliases?.length && <p className="label">aka {data.aliases.join(", ")}</p>}
      <p className="label">{data.mention_count} mentions</p>
      <EntityNeighborhood id={id} />
      <Card className="p-3">
        <div className="label mb-1">relationships</div>
        <ul className="text-sm mono space-y-0.5">
          {data.edges.map((e) => <li key={e.id}>{e.source === id ? "→" : "←"} {e.relation} (w{e.weight.toFixed(0)})</li>)}
          {!data.edges.length && <li className="text-text-muted">none</li>}
        </ul>
      </Card>
      <Card className="p-3">
        <div className="label mb-1">mentioned in</div>
        <ul className="text-sm space-y-0.5">
          {data.memory_ids.map((m) => <li key={m}><button className="text-accent" onClick={() => select(m)}>{m}</button></li>)}
        </ul>
      </Card>
    </div>
  );
}
```

- [ ] **Step 5: Re-export + pass + commit.**
```bash
git add desktop/src/views/EntityProfile.tsx desktop/src/components/EntityNeighborhood.tsx desktop/src/views/index.tsx desktop/src/views/EntityProfile.test.tsx
git commit -m "feat: entity profile view + neighborhood graph (Plan 6 Task 14)"
```

---

## Task 15: Audit log view (+ global `GET /v1/audit`)

Adds a global audit feed to the daemon (`list_audit(storage, None)`), then a filterable Audit view with CSV export.

**Files (daemon):** `crates/mnemos_daemon/src/routes/memories.rs` (add `GET /v1/audit`), `crates/mnemos_daemon/tests/memories.rs` (test). **Files (UI):** `desktop/src/views/Audit.tsx`, `desktop/src/api/client.ts` (+`auditAll`), `desktop/src/api/queries.ts` (+`useAuditAll`), index re-export, `desktop/src/views/Audit.test.tsx`, MSW handler.

- [ ] **Step 1: Daemon — failing test** in `crates/mnemos_daemon/tests/memories.rs`:

```rust
#[tokio::test]
async fn global_audit_lists_recent_entries() {
    let (app, token) = fixture().await;
    // create a memory → produces a "create" audit entry
    call(app.clone(), "POST", "/v1/memories", Some(&token), r#"{"body":"x","tier":"semantic"}"#).await;
    let (s, b) = call(app, "GET", "/v1/audit?limit=10", Some(&token), "").await;
    assert_eq!(s, axum::http::StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert!(v["entries"].as_array().unwrap().iter().any(|e| e["action"] == "create"));
}
```

- [ ] **Step 2: Implement** — in `memories.rs`, add the route `.route("/v1/audit", get(audit_all))` to the router and the handler (reusing the existing `list_audit` import):

```rust
#[derive(Debug, Deserialize)]
struct AuditAllQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}

async fn audit_all(
    State(state): State<AppState>,
    Query(q): Query<AuditAllQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut entries = list_audit(state.vault.storage(), None).await?;
    entries.truncate(q.limit);
    Ok(Json(serde_json::json!({ "entries": entries })))
}
```

(`default_limit` already exists in the file; if `list_audit` returns oldest-first, reverse before truncate so "recent" means newest — add `entries.reverse();` before `truncate` if needed to match the test's intent.) Run `cargo test -p mnemos_daemon --test memories` → PASS. Commit the daemon change:
```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/memories.rs crates/mnemos_daemon/tests/memories.rs
git commit -m "feat: GET /v1/audit global audit feed (Plan 6 Task 15a)"
```

- [ ] **Step 3: UI** — add to `client.ts`: `async auditAll(limit = 200) { return (await this.req<{ entries: AuditEntry[] }>("GET", \`/v1/audit?limit=${limit}\`)).entries; }`; add `useAuditAll` to `queries.ts`; add the MSW handler:
```ts
http.get(`${base}/v1/audit`, () => HttpResponse.json({ entries: [{ id: 1, ts: "2026-05-01T00:00:00+00:00", actor: "mnemos-cli", action: "create", memory_id: "mem_1", details: null }] })),
```

- [ ] **Step 4: Failing test** — `desktop/src/views/Audit.test.tsx`: render, assert the "create" action row shows and an "Export CSV" button exists.

- [ ] **Step 5: `desktop/src/views/Audit.tsx`**

```tsx
import { useMemo, useState } from "react";
import { useAuditAll } from "../api/queries";
import { Button, Skeleton } from "../design/primitives";

export function Audit() {
  const { data, isLoading } = useAuditAll();
  const [filter, setFilter] = useState("");
  const rows = useMemo(() => (data ?? []).filter((e) => !filter || e.action.includes(filter) || (e.memory_id ?? "").includes(filter)), [data, filter]);

  const exportCsv = () => {
    const header = "ts,actor,action,memory_id\n";
    const body = rows.map((e) => `${e.ts},${e.actor},${e.action},${e.memory_id ?? ""}`).join("\n");
    const url = URL.createObjectURL(new Blob([header + body], { type: "text/csv" }));
    const a = document.createElement("a");
    a.href = url; a.download = "mnemos-audit.csv"; a.click();
    URL.revokeObjectURL(url);
  };

  if (isLoading) return <div className="p-6"><Skeleton className="h-64 w-full" /></div>;
  return (
    <div className="p-6 space-y-3">
      <div className="flex items-center justify-between">
        <h1 className="display text-xl">Audit log</h1>
        <Button variant="ghost" onClick={exportCsv}>Export CSV</Button>
      </div>
      <input className="bg-surface border border-border rounded-md px-2 py-1 mono text-sm w-64"
        placeholder="filter action / memory…" value={filter} onChange={(e) => setFilter(e.target.value)} />
      <table className="w-full text-sm mono">
        <thead><tr className="label text-left"><th>ts</th><th>action</th><th>memory</th><th>actor</th></tr></thead>
        <tbody>
          {rows.map((e) => <tr key={e.id} className="border-t border-border"><td>{e.ts.slice(0, 16)}</td><td>{e.action}</td><td>{e.memory_id ?? "—"}</td><td>{e.actor}</td></tr>)}
        </tbody>
      </table>
      {!rows.length && <p className="text-text-muted">No audit entries.</p>}
    </div>
  );
}
```

- [ ] **Step 6: Re-export + pass + commit.**
```bash
cd desktop && pnpm test src/views/Audit && pnpm typecheck
cd /home/jons/AntiGravityProjects/mnemos
git add desktop/src/views/Audit.tsx desktop/src/api/client.ts desktop/src/api/queries.ts desktop/src/views/index.tsx desktop/src/views/Audit.test.tsx desktop/src/test/handlers.ts
git commit -m "feat: audit log view with CSV export (Plan 6 Task 15b)"
```

---

# Group G — Graph & timeline

## Task 16: `POST /v1/graph/ppr` — per-entity PPR scores

The graph view's PPR overlay needs per-entity mass for a query (not just memory ranks). This endpoint runs the same seed-selection + PPR as recall and returns an `entity_id → score` map.

**Files:** `crates/mnemos_daemon/src/routes/graph.rs` (add route+handler), `crates/mnemos_daemon/tests/graph_endpoint.rs` (add test).

- [ ] **Step 1: Failing test** — append to `crates/mnemos_daemon/tests/graph_endpoint.rs`:

```rust
#[tokio::test]
async fn graph_ppr_returns_entity_scores() {
    use mnemos_core::storage::entity_ops::link_entity_mention;
    use mnemos_core::vault::RememberOpts;
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let mem = vault.remember("rust topic", RememberOpts::default()).await.unwrap();
    let a = upsert_entity(vault.storage(), "Rust", "tool").await.unwrap();
    let b = upsert_entity(vault.storage(), "Tauri", "tool").await.unwrap();
    upsert_edge(vault.storage(), &a, &b, "uses", &mem, chrono::Utc::now()).await.unwrap();
    link_entity_mention(vault.storage(), &mem, &a).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();

    let (s, body) = call(app, "POST", "/v1/graph/ppr", Some(&state.token), r#"{"query":"rust"}"#).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    // seed entity Rust gets mass; reachable Tauri too
    assert!(v["scores"].as_object().unwrap().contains_key(&a));
}
```

- [ ] **Step 2: Verify fail** (404).

- [ ] **Step 3: Add route + handler** in `graph.rs`:

```rust
use axum::routing::post;
use serde::Deserialize;

// in router(): .route("/v1/graph/ppr", post(graph_ppr))

#[derive(Deserialize)]
struct PprReq { query: String }

async fn graph_ppr(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<PprReq>,
) -> Result<Json<Value>, ApiError> {
    use mnemos_core::graph::ppr::personalized_pagerank;
    use mnemos_core::graph::MemoryGraph;
    use mnemos_core::retrieval::graph_recall::select_seeds;

    let g = MemoryGraph::load(state.vault.storage()).await?;
    if g.is_empty() {
        return Ok(Json(json!({ "scores": {} })));
    }
    let seeds = select_seeds(state.vault.storage(), &g, &req.query, 5).await?;
    let scores = personalized_pagerank(
        &g, &seeds, state.config.retrieval.ppr_alpha, state.config.retrieval.ppr_iterations,
    );
    let mut map = serde_json::Map::new();
    for (i, s) in scores.iter().enumerate() {
        if *s > 0.0 {
            map.insert(g.entity_id(i).to_string(), json!(s));
        }
    }
    Ok(Json(json!({ "scores": Value::Object(map) })))
}
```

(`select_seeds` and `personalized_pagerank` are `pub` from Plan 5; `entity_id(i)` is `pub`.)

- [ ] **Step 4: Pass + commit.**
```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/graph.rs crates/mnemos_daemon/tests/graph_endpoint.rs
git commit -m "feat: POST /v1/graph/ppr per-entity PPR scores (Plan 6 Task 16)"
```

Also add the UI client method + handler: in `client.ts` `async graphPpr(query: string) { return (await this.req<{ scores: Record<string, number> }>("POST", "/v1/graph/ppr", { query })).scores; }`, and an MSW handler returning `{ scores: { ent_a: 0.4, ent_b: 0.1 } }`. (Committed with Task 17.)

---

## Task 17: Graph view (Sigma.js) — community coloring + PPR overlay

Sigma.js canvas of the entity graph: node size by mentions, color by community; a query box drives the PPR overlay (node glow/size by PPR mass via `/v1/graph/ppr`); click a node → open its entity profile. Sigma needs WebGL, so the test mocks `sigma`/`graphology`.

**Files:** `desktop/src/components/GraphCanvas.tsx`, `desktop/src/views/Graph.tsx`, `client.ts`/`queries.ts`/`handlers.ts` additions from Task 16, index re-export, `desktop/src/views/Graph.test.tsx`.

- [ ] **Step 1: Failing test** (mock sigma + graphology + layout so it mounts in jsdom):

```tsx
import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Graph } from "./Graph";

vi.mock("sigma", () => ({ default: class { on() {} kill() {} } }));
vi.mock("graphology", () => ({ default: class { addNode() {} addEdge() {} hasNode() { return true; } } }));
vi.mock("graphology-layout-forceatlas2", () => ({ default: { assign: () => {}, inferSettings: () => ({}) } }));

const server = setupServer(...handlers);
beforeAll(() => server.listen()); afterEach(() => server.resetHandlers()); afterAll(() => server.close());

test("renders graph controls and the canvas container", async () => {
  renderWithQuery(<Graph />);
  expect(await screen.findByPlaceholderText(/highlight by query/i)).toBeInTheDocument();
  expect(screen.getByLabelText(/community colors/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: `desktop/src/components/GraphCanvas.tsx`** (see the implementation in the architecture note — full code):

```tsx
import { useEffect, useRef } from "react";
import Graph from "graphology";
import Sigma from "sigma";
import forceAtlas2 from "graphology-layout-forceatlas2";
import type { Graph as GraphData } from "../api/types";

const COMMUNITY_COLORS = ["#1F6F6B", "#C77D33", "#A6432E", "#6E8B6A", "#5B6168"];
const KIND_COLOR = "#5B6168";

export function GraphCanvas({
  data, pprScores, colorByCommunity, onSelect,
}: {
  data: GraphData;
  pprScores?: Record<string, number>;
  colorByCommunity: boolean;
  onSelect?: (id: string) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!ref.current) return;
    const g = new Graph();
    const maxPpr = Math.max(0.0001, ...Object.values(pprScores ?? {}));
    data.nodes.forEach((n, i) => {
      const ppr = pprScores?.[n.id] ?? 0;
      const color = colorByCommunity && n.community_id != null && n.community_id >= 0
        ? COMMUNITY_COLORS[n.community_id % COMMUNITY_COLORS.length]
        : KIND_COLOR;
      g.addNode(n.id, {
        label: n.name,
        x: Math.cos((i / Math.max(1, data.nodes.length)) * 2 * Math.PI),
        y: Math.sin((i / Math.max(1, data.nodes.length)) * 2 * Math.PI),
        size: 4 + (n.mentions ?? 0) * 1.5 + (ppr / maxPpr) * 14,
        color: pprScores && ppr > 0 ? "#1F6F6B" : color,
      });
    });
    data.edges.forEach((e) => {
      if (g.hasNode(e.source) && g.hasNode(e.target)) g.addEdge(e.source, e.target, { size: Math.max(1, e.weight) });
    });
    forceAtlas2.assign(g, { iterations: 120, settings: forceAtlas2.inferSettings(g) });
    const sigma = new Sigma(g, ref.current, { renderEdgeLabels: false });
    if (onSelect) sigma.on("clickNode", ({ node }: { node: string }) => onSelect(node));
    return () => sigma.kill();
  }, [data, pprScores, colorByCommunity, onSelect]);
  return <div ref={ref} className="h-full w-full" data-testid="graph-canvas" />;
}
```

- [ ] **Step 4: `desktop/src/views/Graph.tsx`**

```tsx
import { useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useGraph } from "../api/queries";
import { client } from "../api/client";
import { GraphCanvas } from "../components/GraphCanvas";
import { Skeleton } from "../design/primitives";

export function Graph() {
  const { data, isLoading, isError } = useGraph();
  const [q, setQ] = useState("");
  const [activeQuery, setActiveQuery] = useState<string | null>(null);
  const [byCommunity, setByCommunity] = useState(true);
  const navigate = useNavigate();
  const { data: pprScores } = useQuery({
    queryKey: ["graph-ppr", activeQuery], queryFn: () => client.graphPpr(activeQuery!), enabled: !!activeQuery,
  });

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-border p-3">
        <input className="bg-surface border border-border rounded-md px-3 py-1.5 text-sm w-72"
          placeholder="Highlight by query (PPR)…" value={q}
          onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => e.key === "Enter" && setActiveQuery(q)} />
        <label className="label flex items-center gap-1"><input type="checkbox" checked={byCommunity} onChange={(e) => setByCommunity(e.target.checked)} aria-label="community colors" /> community colors</label>
        {activeQuery && <button className="label text-accent" onClick={() => { setActiveQuery(null); setQ(""); }}>clear overlay</button>}
      </div>
      <div className="relative min-h-0 flex-1">
        {isLoading && <div className="p-6"><Skeleton className="h-full w-full" /></div>}
        {isError && <div className="p-6 text-tier-procedural">Could not load the graph.</div>}
        {data && !data.nodes.length && <div className="p-6 text-text-muted">No entities yet — they form as the learning pipeline links memories.</div>}
        {data && data.nodes.length > 0 && (
          <GraphCanvas data={data} pprScores={pprScores} colorByCommunity={byCommunity}
            onSelect={(id) => navigate({ to: "/entity/$id", params: { id } })} />
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 5: Add the Task 16 client/query/handler bits** (graphPpr method, MSW handler) if not already, re-export Graph, run `pnpm test src/views/Graph && pnpm typecheck`.

- [ ] **Step 6: Commit.**
```bash
git add desktop/src/components/GraphCanvas.tsx desktop/src/views/Graph.tsx desktop/src/api/client.ts desktop/src/test/handlers.ts desktop/src/views/index.tsx desktop/src/views/Graph.test.tsx
git commit -m "feat: Sigma.js graph view with community coloring + PPR overlay (Plan 6 Task 17)"
```

> Deferred within the graph view (note in code comment): full memory-node + mixed modes and the animated time-slider overlay. v0.5.0 ships entity mode with community coloring + query-driven PPR highlight; bi-temporal exploration lives in the Timeline view (Task 18).

---

## Task 18: Bi-temporal timeline view (Visx)

Horizontal timeline of memories: a bar per memory from `valid_at` → `invalid_at` (open-ended if still valid), colored by tier, with a draggable "now" cursor that sets `ui.asOf` (time-travel mode — the top bar shows the "viewing <date>" pill).

**Files:** `desktop/src/views/Timeline.tsx`, index re-export, `desktop/src/views/Timeline.test.tsx`.

- [ ] **Step 1: Failing test** — render against MSW (listMemories), assert a memory title/label appears and an SVG renders.

```tsx
import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Timeline } from "./Timeline";

const server = setupServer(...handlers);
beforeAll(() => server.listen()); afterEach(() => server.resetHandlers()); afterAll(() => server.close());

test("renders a timeline bar for a memory", async () => {
  renderWithQuery(<Timeline />);
  expect(await screen.findByText("Rust note")).toBeInTheDocument();
});
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: `desktop/src/views/Timeline.tsx`** (Visx scale + SVG bars; bodies of memories via `listMemories(include_invalid)`)

```tsx
import { useMemo, useState } from "react";
import { scaleTime } from "@visx/scale";
import { useQuery } from "@tanstack/react-query";
import { client } from "../api/client";
import { useUiStore } from "../store/ui";
import { TIER_COLOR_VAR } from "../design/theme";
import { Skeleton } from "../design/primitives";

const W = 900, ROW = 26, PAD = 120;

export function Timeline() {
  const { data, isLoading } = useQuery({ queryKey: ["timeline"], queryFn: () => client.listMemories({ include_invalid: true, limit: 200 }) });
  const setAsOf = useUiStore((s) => s.setAsOf);
  const select = useUiStore((s) => s.select);
  const [cursor, setCursor] = useState<number>(Date.now());

  const { scale, height } = useMemo(() => {
    const mems = data ?? [];
    const times = mems.flatMap((m) => [new Date(m.valid_at).getTime(), m.invalid_at ? new Date(m.invalid_at).getTime() : Date.now()]);
    const min = times.length ? Math.min(...times) : Date.now() - 86400000;
    const max = times.length ? Math.max(...times, Date.now()) : Date.now();
    return { scale: scaleTime({ domain: [new Date(min), new Date(max)], range: [PAD, W - 20] }), height: Math.max(120, mems.length * ROW + 40) };
  }, [data]);

  if (isLoading) return <div className="p-6"><Skeleton className="h-64 w-full" /></div>;
  const mems = data ?? [];
  const cursorX = scale(new Date(cursor));

  return (
    <div className="p-6 space-y-3">
      <h1 className="display text-xl">Timeline</h1>
      <p className="label">drag the cursor to time-travel</p>
      <svg width={W} height={height} role="img" aria-label="bi-temporal timeline">
        {mems.map((m, i) => {
          const x1 = scale(new Date(m.valid_at));
          const x2 = scale(m.invalid_at ? new Date(m.invalid_at) : new Date());
          const y = 20 + i * ROW;
          return (
            <g key={m.id} onClick={() => select(m.id)} style={{ cursor: "pointer" }}>
              <text x={4} y={y + 10} fontSize={11} fill="var(--text-muted)" className="mono">{m.title.slice(0, 16)}</text>
              <rect x={x1} y={y} width={Math.max(2, x2 - x1)} height={ROW - 8} rx={3}
                fill={TIER_COLOR_VAR[m.tier]} opacity={m.invalid_at ? 0.4 : 0.85}
                strokeDasharray={m.invalid_at ? "3 2" : undefined} stroke={m.invalid_at ? "var(--text-muted)" : "none"} />
            </g>
          );
        })}
        <line x1={cursorX} x2={cursorX} y1={0} y2={height} stroke="var(--accent)" strokeWidth={2} />
      </svg>
      <input type="range" min={scale.domain()[0].getTime()} max={scale.domain()[1].getTime()} value={cursor}
        onChange={(e) => { const t = Number(e.target.value); setCursor(t); setAsOf(new Date(t).toISOString()); }}
        className="w-full accent-accent" aria-label="time-travel cursor" />
    </div>
  );
}
```

- [ ] **Step 4: Re-export + pass + commit.**
```bash
git add desktop/src/views/Timeline.tsx desktop/src/views/index.tsx desktop/src/views/Timeline.test.tsx
git commit -m "feat: bi-temporal timeline view with time-travel cursor (Plan 6 Task 18)"
```

---

# Group P — Command palette, quick-add, polish

## Task 19: Command palette (⌘K) + global search

Replace the Task-8 stub with a real palette: ⌘K opens a centered modal; typing filters commands; a free-text query routes to `/search`. Commands: New memory, Open Graph/Timeline/Pipelines/Reflections/Audit, Reflect now, Toggle inspector.

**Files:** `desktop/src/components/CommandPalette.tsx` (replace stub), `desktop/src/layout/Shell.tsx` (add ⌘K listener), `desktop/src/components/CommandPalette.test.tsx`.

- [ ] **Step 1: Failing test** — `CommandPalette.test.tsx` (render open, type, see a command; Enter on a query navigates — assert the onClose/route effect via a spy):

```tsx
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithQuery } from "../test/renderWithQuery";
import { CommandPalette } from "./CommandPalette";

vi.mock("@tanstack/react-router", () => ({ useNavigate: () => vi.fn() }));

test("filters commands by typed text", async () => {
  renderWithQuery(<CommandPalette open onClose={() => {}} />);
  await userEvent.type(screen.getByPlaceholderText(/type a command/i), "graph");
  expect(screen.getByText(/open graph/i)).toBeInTheDocument();
  expect(screen.queryByText(/open audit/i)).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Verify fail** (stub returns null).

- [ ] **Step 3: `desktop/src/components/CommandPalette.tsx`**

```tsx
import { useMemo, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { useUiStore } from "../store/ui";
import { client } from "../api/client";

interface Cmd { label: string; run: () => void; }

export function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [q, setQ] = useState("");
  const navigate = useNavigate();
  const toggleInspector = useUiStore((s) => s.toggleInspector);

  const commands = useMemo<Cmd[]>(() => {
    const go = (to: string) => () => { navigate({ to }); onClose(); };
    return [
      { label: "New memory", run: () => { useUiStore.setState({}); onClose(); document.dispatchEvent(new CustomEvent("mnemos:quick-add")); } },
      { label: "Open Graph", run: go("/graph") },
      { label: "Open Timeline", run: go("/timeline") },
      { label: "Open Pipelines", run: go("/pipelines") },
      { label: "Open Reflections", run: go("/reflections") },
      { label: "Open Audit", run: go("/audit") },
      { label: "Reflect now", run: () => { void client.reflect(); onClose(); } },
      { label: "Toggle inspector", run: () => { toggleInspector(); onClose(); } },
    ];
  }, [navigate, onClose, toggleInspector]);

  const filtered = commands.filter((c) => c.label.toLowerCase().includes(q.toLowerCase()));
  if (!open) return null;

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") onClose();
    if (e.key === "Enter") {
      if (filtered.length) filtered[0].run();
      else if (q.trim()) { navigate({ to: "/search" }); onClose(); }
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/30 pt-32" onClick={onClose}>
      <div role="dialog" aria-label="Command palette" className="w-[32rem] rounded-lg bg-surface-raised shadow-floating border border-border"
        onClick={(e) => e.stopPropagation()}>
        <input autoFocus className="w-full bg-transparent px-4 py-3 font-body outline-none"
          placeholder="Type a command or search…" value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={onKey} />
        <ul className="max-h-72 overflow-y-auto border-t border-border">
          {filtered.map((c) => (
            <li key={c.label}>
              <button className="w-full px-4 py-2 text-left text-sm hover:bg-surface focus-visible:bg-surface" onClick={c.run}>{c.label}</button>
            </li>
          ))}
          {!filtered.length && q.trim() && <li className="px-4 py-2 text-sm text-text-muted">↵ Search memories for “{q}”</li>}
        </ul>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: ⌘K listener in `Shell.tsx`** — add inside `Shell`:

```tsx
import { useEffect } from "react";
// …
useEffect(() => {
  const h = (e: KeyboardEvent) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") { e.preventDefault(); setPaletteOpen(true); }
  };
  window.addEventListener("keydown", h);
  return () => window.removeEventListener("keydown", h);
}, []);
```

- [ ] **Step 5: Pass + commit.**
```bash
cd desktop && pnpm test src/components/CommandPalette && pnpm typecheck
cd /home/jons/AntiGravityProjects/mnemos
git add desktop/src/components/CommandPalette.tsx desktop/src/layout/Shell.tsx desktop/src/components/CommandPalette.test.tsx
git commit -m "feat: command palette (⌘K) + global search routing (Plan 6 Task 19)"
```

---

## Task 20: Quick-add + strength ambient motion

A Quick-Add modal (POST a new memory: body, tier, tags) opened from the command palette's `mnemos:quick-add` event or a top-bar "+" button; plus the strength-near-invalidation pulse in the Browser (opacity-only, `prefers-reduced-motion`-aware).

**Files:** `desktop/src/components/QuickAdd.tsx`, `desktop/src/layout/Shell.tsx` (mount QuickAdd + "+" in TopBar), `desktop/src/design/tokens.css` (pulse keyframes), `desktop/src/views/Browser.tsx` (apply pulse), `desktop/src/components/QuickAdd.test.tsx`.

- [ ] **Step 1: Failing test** — render QuickAdd open, fill body, submit → POST called.

```tsx
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { renderWithQuery } from "../test/renderWithQuery";
import { QuickAdd } from "./QuickAdd";

let created: unknown = null;
const server = setupServer(http.post("http://localhost:7423/v1/memories", async ({ request }) => {
  created = await request.json(); return HttpResponse.json({ id: "mem_new" });
}));
beforeAll(() => server.listen()); afterEach(() => server.resetHandlers()); afterAll(() => server.close());

test("creates a memory", async () => {
  renderWithQuery(<QuickAdd open onClose={() => {}} />);
  await userEvent.type(screen.getByPlaceholderText(/what should mnemos remember/i), "Use Tauri 2");
  await userEvent.click(screen.getByRole("button", { name: /add memory/i }));
  expect(created).toMatchObject({ body: "Use Tauri 2" });
});
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: `desktop/src/components/QuickAdd.tsx`**

```tsx
import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { client } from "../api/client";
import { Button } from "../design/primitives";
import { TIERS, type Tier } from "../design/theme";

export function QuickAdd({ open, onClose }: { open: boolean; onClose: () => void }) {
  const qc = useQueryClient();
  const [body, setBody] = useState("");
  const [tier, setTier] = useState<Tier>("semantic");
  const [tags, setTags] = useState("");
  const [busy, setBusy] = useState(false);
  if (!open) return null;

  const submit = async () => {
    if (!body.trim()) return;
    setBusy(true);
    try {
      await client.createMemory({ body, tier, tags: tags.split(",").map((t) => t.trim()).filter(Boolean) });
      await qc.invalidateQueries({ queryKey: ["memories"] });
      setBody(""); setTags(""); onClose();
    } finally { setBusy(false); }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/30 pt-28" onClick={onClose}>
      <div role="dialog" aria-label="Quick add" className="w-[34rem] rounded-lg bg-surface-raised shadow-floating border border-border p-4 space-y-3" onClick={(e) => e.stopPropagation()}>
        <textarea autoFocus className="w-full h-28 bg-surface border border-border rounded-md p-2 font-body"
          placeholder="What should mnemos remember?" value={body} onChange={(e) => setBody(e.target.value)} />
        <div className="flex items-center gap-2">
          <select className="bg-surface border border-border rounded-md px-2 py-1 text-sm" value={tier} onChange={(e) => setTier(e.target.value as Tier)}>
            {TIERS.map((t) => <option key={t} value={t}>{t}</option>)}
          </select>
          <input className="flex-1 bg-surface border border-border rounded-md px-2 py-1 mono text-sm" placeholder="tags (comma)" value={tags} onChange={(e) => setTags(e.target.value)} />
          <Button onClick={submit} disabled={busy || !body.trim()}>Add memory</Button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Mount + wire** — in `Shell`, add `const [addOpen, setAddOpen] = useState(false)`, listen for the `mnemos:quick-add` event to open it, render `<QuickAdd open={addOpen} onClose={() => setAddOpen(false)} />`, and pass an `onAdd` to `TopBar` rendering a "+" button. In `tokens.css` add:

```css
@keyframes mnemos-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.55; } }
.pulse-weak { animation: mnemos-pulse 2.4s var(--ease) infinite; }
@media (prefers-reduced-motion: reduce) { .pulse-weak { animation: none; } }
```

In `Browser.tsx`, add `className={... (m.strength < 0.2 && !m.invalid_at ? "pulse-weak" : "")}` to the memory row, conveying near-invalidation.

- [ ] **Step 5: Pass + commit.**
```bash
git add desktop/src/components/QuickAdd.tsx desktop/src/layout/Shell.tsx desktop/src/layout/TopBar.tsx desktop/src/design/tokens.css desktop/src/views/Browser.tsx desktop/src/components/QuickAdd.test.tsx
git commit -m "feat: quick-add modal + strength ambient pulse (Plan 6 Task 20)"
```

---

## Task 21: Error boundary, required-states sweep, a11y, theme toggle

Cross-cutting polish: a route-level `ErrorBoundary`, a theme toggle in the top bar, focus-visible rings + ARIA verified, and a sweep confirming every view has loading/error/empty states.

**Files:** `desktop/src/components/ErrorBoundary.tsx`, `desktop/src/router.tsx` (wrap Outlet), `desktop/src/layout/TopBar.tsx` (theme toggle), `desktop/src/components/ErrorBoundary.test.tsx`.

- [ ] **Step 1: Failing test** — a child that throws renders the fallback:

```tsx
import { render, screen } from "@testing-library/react";
import { ErrorBoundary } from "./ErrorBoundary";

function Boom(): JSX.Element { throw new Error("boom"); }

test("renders fallback when a child throws", () => {
  const spy = vi.spyOn(console, "error").mockImplementation(() => {});
  render(<ErrorBoundary><Boom /></ErrorBoundary>);
  expect(screen.getByRole("alert")).toHaveTextContent(/something went wrong/i);
  spy.mockRestore();
});
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: `desktop/src/components/ErrorBoundary.tsx`**

```tsx
import { Component, type ReactNode } from "react";

export class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };
  static getDerivedStateFromError(error: Error) { return { error }; }
  render() {
    if (this.state.error) {
      return (
        <div role="alert" className="m-6 rounded-lg border border-tier-procedural/40 bg-surface p-4">
          <h2 className="display text-lg text-tier-procedural">Something went wrong</h2>
          <pre className="mono text-xs mt-2 whitespace-pre-wrap text-text-muted">{this.state.error.message}</pre>
          <button className="label mt-3 text-accent" onClick={() => this.setState({ error: null })}>Try again</button>
        </div>
      );
    }
    return this.props.children;
  }
}
```

- [ ] **Step 4: Wrap the router Outlet** — in `router.tsx`, the root route component becomes `() => (<Shell><ErrorBoundary><Outlet /></ErrorBoundary></Shell>)`.

- [ ] **Step 5: Theme toggle in TopBar** — add a `Sun`/`Moon` (Lucide) button calling `useTheme().toggle()` with `aria-label="Toggle theme"`. Verify focus-visible rings exist (the `Button` primitive already has `focus-visible:outline`); ensure all interactive elements are reachable by keyboard and have accessible names.

- [ ] **Step 6: Required-states checklist** (verify, fix any gaps — no new test, this is a review step):
  - Browser/Search/Pipelines/Reflections/Entity/Audit/Timeline: each has `isLoading` (Skeleton), `isError` (message), and empty state. Graph: loading/error/empty. Confirm each renders an empty state with guidance, not a blank panel.

- [ ] **Step 7: Pass + commit.**
```bash
cd desktop && pnpm test src/components/ErrorBoundary && pnpm typecheck && pnpm lint
cd /home/jons/AntiGravityProjects/mnemos
git add desktop/src/components/ErrorBoundary.tsx desktop/src/router.tsx desktop/src/layout/TopBar.tsx desktop/src/components/ErrorBoundary.test.tsx
git commit -m "feat: error boundary, theme toggle, a11y + required-states sweep (Plan 6 Task 21)"
```

---

# Group T — E2E & release

## Task 22: Playwright end-to-end (golden path) with in-browser MSW

Run the real app via `vite dev` with MSW intercepting in the browser (no daemon needed), and drive a golden path: app loads → Browser shows a memory → navigate to Search → query → see a result.

**Files:** `desktop/src/test/browser.ts`, `desktop/src/main.tsx` (conditional worker start), `desktop/playwright.config.ts`, `desktop/tests-e2e/golden.spec.ts`, `desktop/public/mockServiceWorker.js` (generated).

- [ ] **Step 1: Generate the MSW worker + browser setup**

```bash
cd desktop && pnpm msw init public --save
```

`desktop/src/test/browser.ts`:
```ts
import { setupWorker } from "msw/browser";
import { handlers } from "./handlers";
export const worker = setupWorker(...handlers);
```

In `desktop/src/main.tsx`, before `ReactDOM.createRoot(...).render(...)`:
```ts
if (import.meta.env.VITE_MSW === "1") {
  const { worker } = await import("./test/browser");
  await worker.start({ onUnhandledRequest: "bypass" });
}
```
(Wrap the render in an `async` IIFE so the `await` is valid.)

- [ ] **Step 2: `desktop/playwright.config.ts`**

```ts
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests-e2e",
  timeout: 30_000,
  use: { baseURL: "http://localhost:1420" },
  webServer: {
    command: "VITE_MSW=1 pnpm dev",
    url: "http://localhost:1420",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
```

- [ ] **Step 3: Write the spec** — `desktop/tests-e2e/golden.spec.ts`:

```ts
import { test, expect } from "@playwright/test";

test("browse → search golden path", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Rust note")).toBeVisible();

  await page.getByRole("link", { name: "Search" }).click();
  await page.getByPlaceholder(/search memories/i).fill("rust");
  await page.getByRole("button", { name: /search/i }).click();
  await expect(page.getByText("Rust note")).toBeVisible();
  await expect(page.getByText(/PPR/i)).toBeVisible();
});

test("command palette opens with ⌘K and lists commands", async ({ page }) => {
  await page.goto("/");
  await page.keyboard.press("Control+k");
  await expect(page.getByRole("dialog", { name: /command palette/i })).toBeVisible();
  await page.getByPlaceholder(/type a command/i).fill("graph");
  await expect(page.getByText(/open graph/i)).toBeVisible();
});
```

- [ ] **Step 4: Install browsers + run**

```bash
cd desktop && pnpm exec playwright install --with-deps chromium
pnpm e2e
```
Expected: 2 specs pass. (If the environment lacks a display, run with the default headless mode — Playwright is headless by default.)

- [ ] **Step 5: Commit**

```bash
cd /home/jons/AntiGravityProjects/mnemos
git add desktop/src/test/browser.ts desktop/src/main.tsx desktop/playwright.config.ts desktop/tests-e2e/ desktop/public/mockServiceWorker.js
git commit -m "test: Playwright golden-path E2E with in-browser MSW (Plan 6 Task 22)"
```

---

## Task 23: Release v0.5.0 — version, docs, CI, build verification, tag

**Files:** `Cargo.toml` (workspace version 0.5.0), `README.md`, `CHANGELOG.md`, `.github/workflows/desktop.yml` (new), and a frontend build check.

- [ ] **Step 1: Bump the daemon workspace version** in `Cargo.toml`:

```toml
version = "0.5.0"
```
(`desktop/package.json` and `desktop/src-tauri` are already `0.5.0`.)

- [ ] **Step 2: Add the desktop CI workflow** — `.github/workflows/desktop.yml`:

```yaml
name: desktop
on:
  push: { paths: ["desktop/**", ".github/workflows/desktop.yml"] }
  pull_request: { paths: ["desktop/**"] }
jobs:
  frontend:
    runs-on: ubuntu-latest
    defaults: { run: { working-directory: desktop } }
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with: { version: 9 }
      - uses: actions/setup-node@v4
        with: { node-version: 20, cache: pnpm, cache-dependency-path: desktop/pnpm-lock.yaml }
      - run: pnpm install --frozen-lockfile
      - run: pnpm typecheck
      - run: pnpm lint
      - run: pnpm test
      - run: pnpm exec playwright install --with-deps chromium
      - run: pnpm e2e
```

(Tauri Rust bundling is intentionally NOT in CI — it requires platform GUI toolchains; covered by manual/release builds. The existing daemon CI workflow is unchanged since `desktop/` is outside the cargo workspace.)

- [ ] **Step 3: README** — add a "Desktop UI (v0.5.0)" section:

```markdown
## Desktop UI (v0.5.0)

A Tauri 2 + React desktop app (`desktop/`) over the daemon. Three-column
Obsidian-style shell with ten views — tier browser, markdown editor, hybrid
search (with per-retriever explainability bars), **Sigma.js graph** (community
coloring + query-driven PPR overlay), **bi-temporal timeline** (time-travel
cursor), pipeline status, reflection viewer, entity profile, audit log — a ⌘K
command palette, quick-add, and live WebSocket updates. Distinctive tier-coded
design (Fraunces/Source Serif 4/JetBrains Mono, warm off-white / deep blue-black,
no purple).

```bash
# run the daemon, then:
cd desktop && pnpm install
pnpm tauri dev          # desktop window
# or browser dev with mocked daemon:
VITE_MSW=1 pnpm dev
```

The app reads the daemon bearer token from `~/.config/mnemos/token` via a Tauri
command (the secret never enters renderer code).

New daemon endpoints in this release: `GET /v1/graph`, `POST /v1/graph/ppr`,
`GET /v1/communities`, `GET /v1/audit`, enriched `GET /v1/entities/{id}` +
`/{id}/graph`.
```

- [ ] **Step 4: CHANGELOG** — add at the top:

```markdown
## [0.5.0] - 2026-05-27

### Added
- **Desktop UI** (`desktop/`): Tauri 2 + React 18 + TypeScript app over the
  daemon. Ten views (browser, editor, search w/ explainability, Sigma.js graph
  with community + PPR overlays, bi-temporal timeline, pipelines, reflections,
  entity profile, audit), ⌘K command palette, quick-add, live WS updates,
  tier-coded anti-slop design system, light/dark themes.
- Daemon endpoints for the UI: `GET /v1/graph`, `POST /v1/graph/ppr`,
  `GET /v1/communities`, `GET /v1/audit`; enriched `GET /v1/entities/{id}` and a
  real `GET /v1/entities/{id}/graph` neighborhood.
- Frontend test stack: Vitest + Testing Library + MSW (unit/component) and
  Playwright (golden-path E2E); a `desktop` CI workflow.

### Notes
- The Tauri app lives outside the Cargo workspace; daemon CI is unaffected.
- Full memory/mixed graph modes, the graph time-slider, settings, first-run
  wizard, and entity merge are deferred to Plan 7.
```

- [ ] **Step 5: Release gate**

```bash
# daemon (Group BK added endpoints):
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
# frontend:
cd desktop && pnpm install && pnpm typecheck && pnpm lint && pnpm test && pnpm build && cd ..
```
Expected: all green; `pnpm build` produces `desktop/dist`. (`pnpm tauri build` — full native bundle — is a manual/Plan 8 step requiring GUI toolchains.)

- [ ] **Step 6: Commit + tag** (do NOT push — the user reviews and pushes)

```bash
git add Cargo.toml README.md CHANGELOG.md .github/workflows/desktop.yml
git commit -m "chore: release v0.5.0 — Tauri desktop UI (Plan 6 Task 23)"
git tag -a v0.5.0 -m "v0.5.0 — Tauri + React desktop UI"
```

---

## Done

After all tasks: a real desktop window where a person browses memories by tier, edits metadata, runs explainable hybrid search, visualizes the entity graph with community coloring and a query-driven PPR overlay, scrubs a bi-temporal timeline to time-travel, watches pipelines, reads reflections, inspects entities, and audits changes — live-updating over WebSocket, in a distinctive tier-coded interface. All component logic is unit-tested (Vitest + MSW), the golden path is E2E-tested (Playwright), and the daemon gained the four endpoints the UI needs.

**Next:** Plan 7 (sync backends + settings + first-run wizard + entity merge + more adapters), then Plan 8 (packaging, installers, signing, auto-update).
