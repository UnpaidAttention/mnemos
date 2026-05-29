# Changelog

All notable changes to this project are recorded here.

## [0.7.0] - 2026-05-29

### Added
- Cross-platform installers via Tauri bundler (`.dmg` + `.app` macOS,
  `.deb` + `.rpm` + `.AppImage` Linux, `.msi` Windows). Desktop installer
  bundles the daemon + CLI as Tauri sidecars.
- Stand-alone `.deb` + `.rpm` packages for the CLI (`mnemos`) and daemon
  (`mnemos-daemon`) via `cargo-deb` and `cargo-generate-rpm`.
- Tauri-built-in auto-update: ed25519-signed `latest.json` manifest on
  GitHub Releases, `UpdateBanner` UI in the desktop app, defer-or-install
  flow with progress.
- `.github/workflows/release.yml` — tag-triggered build matrix on macOS,
  Linux, and Windows runners + a release-publish job that uploads all
  artifacts and generates the updater manifest.
- `mnemos_release_manifest` workspace member — small binary that
  generates the Tauri updater `latest.json` from a tagged set of
  platform / URL / signature triples.
- Icon set (SVG source + generated PNG/ICO/ICNS) under
  `desktop/src-tauri/icons/`.
- Documentation: `BUILD.md` (cross-platform build steps), `PACKAGING.md`
  (release + distribution runbook), README "Install" section.
- Workspace package metadata (license, repository, homepage,
  description, authors).

### Deferred
- Apple Developer notarization, Microsoft Authenticode signing — both
  documented in BUILD.md; future v0.7.x release re-runs CI with secrets.
- Launchpad PPA / OBS RPM repository submission — documented in
  PACKAGING.md; requires accounts the framework cannot create.
- Homebrew tap, crates.io publish — explicitly opted out in plan scoping.
- Turso libSQL embedded replicas wire-up, encrypt-at-rest, secret
  detection at ingest — carried forward from Plan 7.

### Notes
- The Tauri updater public key is `PLACEHOLDER_PUBLIC_KEY_REPLACE_BEFORE_RELEASE`
  in `tauri.conf.json` — run `bash scripts/gen-updater-key.sh` and replace
  before cutting the first signed release. The private key lives in
  `TAURI_SIGNING_PRIVATE_KEY` (CI secret).
- macOS and Windows installers are **unsigned** for v0.7.0. SmartScreen
  / Gatekeeper warnings on first launch are expected; see PACKAGING.md.
- AppImage builds may fail locally on Fedora 43 (linuxdeploy/binutils
  version drift on `.relr.dyn` ELF sections). CI on ubuntu-22.04 builds
  AppImages cleanly. See BUILD.md § Troubleshooting.

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

## [0.4.0] - 2026-05-27

### Added
- Graph PPR retriever (HippoRAG-style): dependency-free `MemoryGraph` +
  Personalized PageRank, fused into hybrid recall as a third RRF list. `RecallHit`
  gains `ppr_rank`; `RecallOpts` gains `graph`/`ppr_alpha`/`ppr_iterations`.
- Reflection: salience-triggered `reflect()` pipeline writing typed reflection
  memories with `reflects_on` links; `POST/GET /v1/reflections`; MCP `reflect` +
  `list_reflections`; `[reflection]` config.
- Community detection: dependency-free Louvain + LLM `community_summary` memories;
  `POST /v1/maintenance/communities`; global-mode recall (`"global": true`);
  `[community]` config.
- Schema v5 (salience accumulator + `memories.reflected_at`) and v6
  (`entity_communities`).
- Events `ReflectionCompleted`, `CommunityDetected`.

### Changed
- `hybrid_recall` / `hybrid_recall_with_rerank` are now thin wrappers over a
  unified `hybrid_recall_full` that includes the optional graph retriever.

### Notes
- PPR and Louvain are hand-rolled (no `petgraph` / `leiden_clustering`) for
  determinism and zero dependency risk. Hierarchical Leiden refinement deferred.

## [0.3.0] - 2026-05-27

### Added
- `LlmProvider` trait with `OllamaLlm` (default) and deterministic `MockLlm` for CI.
- Async learning pipeline triggered on `SessionEnded`: extract → resolve
  (ADD/UPDATE/DELETE/NOOP) → entity-link → graph-update.
- Hourly Ebbinghaus decay worker + `POST /v1/maintenance/decay` + `mnemos decay`.
- `GET /v1/pipelines` status endpoint (counters, recent runs, configured model).
- `PATCH /v1/memories/{id}` (tags/importance) and `POST /v1/memories/time-travel`
  (replacing the Plan 3 `501` stubs).
- `[llm]` config section with `MNEMOS_LLM*` env overrides.
- Schema v4: `sessions.processed_at` for idempotent pipeline processing.

### Fixed
- Reject chunks posted to a nonexistent session (was silently creating orphans).
- Daemon graceful shutdown now joins the background pipeline + decay workers.

### Changed
- Extracted a shared recall helper used by both the REST search endpoint and the
  MCP recall tool (removing duplicated logic).

### Deferred
- MCP `sampling/createMessage` (extraction via the calling client's LLM): async
  pipelines run after the triggering request returns, so there is no
  request-scoped connection to sample from. Revisit with a streaming transport.

## [0.2.0] - 2026-05-26

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

## [0.1.0] - 2026-05-26

### Added
- Dense vector retrieval via `sqlite-vec` (`vec0` virtual tables; 768d).
- Pluggable `Embedder` trait with three implementations:
  - `OllamaEmbedder` (default, calls Ollama HTTP API)
  - `MockEmbedder` (deterministic, for tests)
  - ONNX `OnnxReranker` (feature-gated under `rerank-onnx`)
- Hybrid retrieval: BM25 ∪ Dense → RRF fusion → recency·importance·strength·tier reweighting → optional cross-encoder rerank.
- New `RecallOpts` fields: `rrf_k`, `reweight: ReweightConfig`, `explain`, `rerank`.
- New `RecallHit` fields: `dense_rank`, `dense_distance`, `explain: Option<Explain>`.
- CLI: `mnemos recall --rerank --explain`; new `mnemos embed status` / `mnemos embed backfill` subcommands.
- `Vault::backfill_embeddings` to embed pre-existing memories.
- Schema migration v2 — adds `memory_vec` and `chunk_vec` virtual tables.

### Changed
- `Vault::open` is now sugar for `Vault::open_with_embedder(paths, None)`.
- `rebuild_index` is now sugar for `rebuild_index_with_embedder(paths, None)`.
- `forget` deletes the corresponding vector in addition to soft-invalidating the memory.

### Notes
- ONNX reranker is feature-gated. Build with `cargo build --features rerank-onnx` to enable; expects `bge-reranker-base.onnx` and matching tokenizer in `~/.local/share/mnemos/models/`.
- All CLI integration tests set `MNEMOS_EMBEDDER=mock` so CI doesn't need Ollama running.
- `--rerank` CLI flag emits a stderr warning when no reranker is configured (will be wired by the daemon in Plan 3).

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
