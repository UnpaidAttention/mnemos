# Changelog

All notable changes to this project are recorded here.

## [0.9.4] - 2026-06-16

> **Pipeline reliability & CLI config fix.** Resolves the primary cause
> of zero new memories: the CLI used a hardcoded bundled embedder instead
> of reading `config.toml`, abandoned sessions were never ended, and the
> LLM timeout was too short for extraction workloads.

### Fixed
- **CLI reads `config.toml`.** The `mnemos` CLI now reads
  `~/.config/mnemos/config.toml` via `Config::load_default()`, matching
  the daemon's embedder kind/model/URL exactly. Previously the CLI
  hardcoded `bundled` (384d), causing a dimension mismatch when the
  daemon was configured for `ollama` (768d). (`crates/mnemos_cli`)
- **DB-level stale session sweep.** On startup, the daemon now finds
  sessions stuck OPEN in the database (where `ended_at IS NULL` but the
  most recent chunk is >10 minutes old) and ends them automatically.
  This catches hook-originated sessions (Claude Code) that the in-memory
  `SessionManager` sweep never tracked. (`pipeline_runner`)
- **LLM timeout increased.** Default `timeout_secs` raised from 120 to
  300 seconds. Extraction prompts send the full session context to the
  LLM; small models with thinking enabled (qwen3:4b) routinely exceeded
  120s on sessions with many chunks. (`config.rs`)
- **Catch-up logging.** `catch_up()` now logs an `info!` message when
  it finds zero unprocessed sessions, making it diagnosable from logs
  whether the function ran vs was never called. (`pipeline_runner`)

## [0.9.3] - 2026-06-16

> **Memory quality & extraction mode.** Schema-enforced JSON extraction
> via GBNF grammar constraints, configurable extraction modes (local /
> MCP piggyback / disabled), expanded model catalog, and enriched MCP
> tool descriptions for better autonomous memory capture.

### Added
- **Extraction mode setting.** New `extraction_mode` field in Autonomy
  config with three modes: `local` (default — Ollama model extracts),
  `mcp-piggyback` (conversation LLM manages via MCP tools), `none`
  (manual only). Dropdown selector in Autonomy Settings with contextual
  help text for each mode.
- **Expanded model catalog.** Added SmolLM3 3B, Ministral 3B Instruct,
  and Llama 3.2 3B to the CPU-friendly tier. Moved recommended badge
  from Phi-4 Mini to Qwen3 4B (consensus leader for structured JSON).
- **Pipeline extraction guardrails.** `pipeline_runner` checks
  `extraction_mode` before running LLM extraction — skips entirely
  when mode is `mcp-piggyback` or `none`.

### Changed
- **Schema-enforced JSON extraction.** `CompletionRequest` gains
  `format_schema` field. Ollama provider passes the full JSON Schema
  to the `format` field (GBNF grammar-based token masking). OpenAI
  provider uses `json_schema` response format. Extraction pipeline
  defines and passes the extraction schema for all calls.
- **Simplified extraction prompt.** Reduced from ~150 lines / 5
  examples to ~80 lines / 2 examples (~40% shorter). Flattened
  instructions for reliable small-model compliance.
- **Enriched MCP tool descriptions.** `remember` and `recall` tools
  now include detailed usage guidance, field descriptions, importance
  guidelines, and examples — improving LLM proactive tool use in
  MCP piggyback mode.

### Fixed
- Extraction pipeline no longer runs when `extraction_mode` is set to
  `mcp-piggyback` or `none`, preventing duplicate memory creation.

## [0.9.2] - 2026-06-12

> **Graph node visual redesign.** Memory nodes in the knowledge graph
> are now rendered as faceted crystalline obsidian shards with
> directional lighting, specular highlights, and glowing energy cracks
> — replacing the previous simple asteroid shapes.

### Changed
- **Crystalline obsidian node renderer.** Graph nodes now use a
  procedurally generated faceted-shard shape with per-facet linear
  gradients, a global specular shine sweep from a top-left light
  source, and crisp lit-edge highlights. Larger nodes gain neon-glowing
  energy cracks with a soft outer bloom and hot white inner core.
- **Shape generation** uses a seeded PRNG for deterministic,
  highly-irregular crystal silhouettes (6–9 vertices with 0.55–1.1×
  radius variation) and an offset interior hub for asymmetric facet
  geometry.

## [0.9.1] - 2026-06-11

> **Smarter injection path + pipeline prompt refinements.** The Claude Code
> hook now filters recall hits by relevance score, deduplicates memories
> across a session, skips trivial prompts, and prioritises identity/rule
> memories at session start. Pipeline extraction and reflection prompts
> are freed from rigid numerical constraints that limited LLM output
> quality.

### Added
- **Recall score cutoff.** Hits below a 0.25 aggregate score are filtered
  out before injection, preventing weakly-related noise from consuming
  the token budget.
- **Session-level memory dedup.** `ActiveSessionState` now tracks
  `injected_ids` — memories already injected in the current session are
  never re-injected on subsequent prompts.
- **Trivial prompt detection.** Short affirmations and single-word
  responses (`"yes"`, `"looks good"`, `"lgtm"`, etc.) skip recall
  entirely, saving latency and budget.
- **Session-start memory prioritisation.** Identity, rule, and correction
  type memories are rendered first in the working set; total capped at
  15 entries to prevent unbounded growth.
- **10 new unit tests** covering all four injection improvements.

### Changed
- **Extraction prompt** no longer forces "exactly one claim per entry" —
  the LLM is free to include as many claims as a fact naturally warrants.
- **Reflection prompt** removes hard numerical caps (e.g., "1–2
  sentences") on rule descriptions, allowing richer reflections.
- **Entity extraction** instructions clarified to avoid artificial
  constraints on output shape.

### Fixed
- `render_working_set` no longer renders all working-tier memories
  without limit, which could bloat session-start context as the vault
  grew.
- Query-matched recall hits were missing `seen_ids` tracking, allowing
  the same memory to appear in both `[Recovered Context]` and
  `[Relevant Memories]` sections.

## [0.9.0] - 2026-06-10

> **Desktop redesign + pipeline intelligence.** The desktop app gets a
> military-inspired dark dashboard UI, a curated LLM model catalog with
> hardware-tier recommendations, and real-time pipeline progress tracking.
> The memory pipeline gains smarter duplicate detection, entity taxonomy
> constraints, and safer UTF-8 handling. GitHub repo now follows best
> practices with issue templates, dependabot, and CI concurrency.

### Added
- **Military-dashboard UI redesign.** Complete visual overhaul of the
  desktop app with a dark, data-dense aesthetic.
- **Curated LLM model catalog.** Hardware-tier–aware model picker with
  recommendations for low/mid/high-end machines.
- **Real-time pipeline backfill progress.** Live progress bar in the
  Pipelines view showing active backfill status, memories processed,
  and ETA.
- **Enhanced audit logging.** Detailed audit entries with structured
  action descriptions and expanded Audit view.
- **Knowledge page pagination.** Fetches up to 500 memories, displays
  in batches of 50 with "Load more" and count indicators.
- **Graph zoom toggle.** Click a node to zoom in; click again to zoom
  back out and clear the inspector panel.
- **GitHub issue templates** (bug report + feature request forms).
- **PR template** with pre-merge checklist.
- **Dependabot** for Cargo, npm, and GitHub Actions dependency updates.

### Changed
- **Resolve pipeline** now includes truncated memory body previews
  (200 chars) in the LLM context, dramatically improving duplicate
  and contradiction detection.
- **Entity extraction** constrained to a fixed 7-kind taxonomy
  (person, project, organization, tool, concept, place, event) to
  eliminate inconsistent entity types.
- **Lint pipeline** capped at 50 memories per batch to prevent LLM
  context window overflow on large knowledge bases.
- **Fact processing** extracted into a shared `process_facts()` helper,
  ensuring session-end and incremental pipelines stay consistent.
- **CI workflows** now use concurrency groups (cancel superseded runs)
  and least-privilege permissions.
- `Cargo.lock` is now tracked in version control for reproducible
  binary builds.
- Expanded `CONTRIBUTING.md` with build/packaging links, frontend test
  instructions, and code style guidance.

### Fixed
- **UTF-8 truncation safety.** Replaced `floor_char_boundary` (requires
  Rust 1.91) with a MSRV-compatible `truncate_chars()` helper using
  `.chars().take(n)`. Prevents panics on multi-byte characters (emoji,
  CJK, accented text).
- **Ollama lifecycle management.** Auto-unload previous model when
  switching LLM/embedder; session-aware 30-min `keep_alive` timeout;
  daemon is always-on with models loaded on demand.
- **Pipeline session tracking.** Failed pipeline sessions now correctly
  marked as processed, preventing infinite retry loops.
- **Systemd service tests.** Updated assertions to match actual binary
  name (`mnemosd` vs old `mnemos-daemon`).
- **Graph zoom-out** now properly clears selection state and inspector
  breadcrumbs.

### Removed
- Stale feature branches (`feat/autonomy-layer`, `feat/ai-tool-connect-wizard`,
  `feat/correction-learning`, `feat/storage-location-picker`) — all were
  fully merged and have been deleted from the remote.

## [0.8.1] - 2026-06-09

> **Contextual recall + CI stabilization.** Mnemos now proactively
> re-injects project context, entity-linked memories, and recovered
> topics during live AI tool sessions. All CI workflows are green.

### Added
- **3-layer contextual recall** for real-time session injection:
  - **Layer 1 — Project Pinning:** `GET /v1/memories/project-context`
    returns Project + Entity type memories for the current workspace.
    Injected as `[Project Context]` on every prompt (800 char budget).
  - **Layer 2 — Entity Expansion:** `entity_expand` option on
    `/v1/memories/search` follows entity links via `json_each` to
    surface related memories. Expanded hits get discounted scores.
  - **Layer 3 — Session-Aware Context Recovery:** Tracks keyword
    first-seen ordinals per session. When keywords reappear after ≥4
    prompts, augments recall query and injects `[Recovered Context]`.
- **`kinds` filter on `ListFilter`** — query memories by type
  (Project, Entity, Episodic, etc.).
- **Real-time chunk streaming pipeline** — processes session chunks
  immediately as they arrive, not just at session end.
- **Schema v11** — adds `strength` column to memories table.

### Fixed
- **CI: `cargo fmt --check` failures** — all Rust code formatted.
- **CI: `cargo clippy -D warnings` failures** — fixed `push_str`
  for single chars, unnecessary `unsafe` blocks, dead code, useless
  `as_ref()`, and `Error::other()` modernization.
- **CI: desktop workflow `pnpm install` failure** — added missing
  `packages` field to `pnpm-workspace.yaml`.
- **CI: service tests** — updated assertions to match actual systemd
  unit file (`%h/.cargo/bin/mnemos-daemon`, `Restart=on-failure`).
- **CI: migration tests** — updated expected schema version to v11.

## [0.8.0] - 2026-05-29

> **Zero-setup release.** Mnemos now ships with a bundled embedder
> (llama.cpp + 22 MB MiniLM Q8 GGUF) in the daemon `.deb` / `.rpm`
> packages. A fresh install — `apt install ./mnemos-daemon_X.Y.Z_amd64.deb`
> — gives you `mnemos remember` + `mnemos recall` end-to-end with no
> Ollama install, no API key, no internet after the download.

### Added — desktop app & learning (landed before first publish)
- **UI storage-location picker.** Move your vault to a new directory from
  Settings → Storage; the desktop app supervises the daemon and restarts it
  at the new path (atomic, backed-up, with rollback).
- **AI-tool auto-connect wizard.** Detects installed tools (Claude Code,
  Codex, Antigravity CLI; Gemini CLI marked deprecated) and writes their MCP
  config + a session-start hint with a preview-diff + one click.
- **Correction-learning.** New `correct` MCP tool + `/v1/corrections`: capture
  wrong→right→why; recurring corrections harden into rules surfaced at session
  start; session-end mining extracts corrections automatically.

### Added
- **Bundled embedder.** llama.cpp's `llama-server` ships in
  `/usr/lib/mnemos/`; daemon spawns + manages it as a child process,
  health-checks every 30s, restarts on crash with backoff. A wrapper at
  `/usr/bin/mnemos-llama-server` sets `LD_LIBRARY_PATH` so the
  dynamically-linked binary finds its bundled `.so` neighbors. Total
  daemon `.deb` size ~39 MB (vs Ollama's ~200 MB + nomic-embed-text's
  274 MB).
- **`MNEMOS_EMBEDDER=bundled`** is the new default for fresh vaults.
- **OpenAI embeddings backend** (`MNEMOS_EMBEDDER=openai`,
  `OPENAI_API_KEY`). Supports Azure OpenAI via `OPENAI_BASE_URL`.
  Defaults to `text-embedding-3-small` (1536-dim); override via
  `MNEMOS_EMBEDDER_MODEL`.
- **OpenAI LLM backend** (`MNEMOS_LLM=openai`) for reflections,
  community summaries, entity extraction. Default model
  `gpt-4o-mini`, override via `MNEMOS_LLM_MODEL`.
- **`mnemos embed-rebuild --target <kind>`** — atomic, resumable,
  audit-logged migration between embedders. Shadow-table-based;
  handles both same-dim and different-dim migrations via DELETE+INSERT
  or DROP+CREATE+INSERT against the sqlite-vec `memory_vec` virtual
  table. UI progress view at `/embed-rebuild`.
- **Vault meta tracks embedder authoritatively.** Schema v9 adds
  `vault_meta.embedder_kind`. The vault's recorded kind is the source
  of truth; the daemon uses it to choose the backend at startup, env
  vars are only the default for new vaults.
- **Doctor + Settings UI updates.** Doctor surfaces embedder mismatch
  + migration prompt (linking to `/embed-rebuild`). Settings exposes
  `bundled / ollama / openai / mock / none` for both embedder and
  LLM. Settings includes an `[openai]` config block for `base_url` +
  `api_key`.
- **Tauri auto-update — DEFERRED.** The updater plugin and its endpoint
  have been removed from `tauri.conf.json`. Shipping the AppImage
  requires staging the bundled embedder `.so` libs into the AppDir
  (a non-trivial linuxdeploy integration); until that is done no
  AppImage is produced and therefore no `latest.json` update manifest
  can be signed and published. The `mnemos_release_manifest` binary and
  signing key infrastructure remain in the tree for when AppImage
  bundling is re-enabled. The `.deb` and `.rpm` desktop packages
  (the supported install paths) update via the system package manager.
- **First-run wizard simplified.** No Ollama probe by default since
  the bundled embedder is ready. Three-step flow: welcome →
  bundled-embedder confirm → integration snippets.

### Changed
- **`MNEMOS_LLM` defaults to `none`** for fresh installs (was
  `ollama`). Reflections and community summaries silently no-op if no
  LLM is configured — opt in via Ollama or OpenAI.
- **Schema v9**: adds `vault_meta.embedder_kind` (backfilled from
  existing `embedder_model_id` for upgrades; defaults to `bundled` for
  fresh vaults).
- **Modules reorganized** under `crates/mnemos_core/src/providers/`
  (was scattered across `embedder/` + `llm/`). External API
  unchanged.

### Fixed
- **Fresh vault now creates its `vec0` tables at the embedder's
  dimension.** The static schema migration declared `memory_vec` /
  `chunk_vec` as `FLOAT[768]` (nomic-embed-text's dim); on a fresh
  vault using the 384-dim bundled embedder every `remember` failed
  with "Dimension mismatch ... Expected 768 ... received 384".
  `Vault::open_with_embedder` now aligns the tables to the configured
  embedder's dim on first seed (idempotent; never wipes vectors a
  same-dim index rebuild already inserted). Without this the zero-setup
  bundled flow could not store a single memory.
- **CLI `remember` / `recall` now honor the bundled embedder.** The
  CLI embedder factory still defaulted to Ollama and lacked
  `bundled` / `openai` arms, so `mnemos remember` on a fresh install
  tried `localhost:11434` (or errored on `MNEMOS_EMBEDDER=bundled`).
  It now mirrors the daemon: all five kinds, default `bundled`.
- Added an end-to-end regression test exercising remember→store→recall
  at 384-dim (every prior test used a 768-dim mock, masking both bugs).

### Migrating from v0.7.x
- Existing vaults seeded with Ollama keep working — the daemon
  detects `vault.embedder_kind=ollama` (backfilled at schema v9
  upgrade) and continues using Ollama as before.
- Doctor view surfaces a migration prompt: run
  `mnemos embed-rebuild --target bundled` to switch to the bundled
  embedder. The rebuild re-embeds every memory atomically (~30s per
  100 memories on a 4-core CPU) and updates the vault meta.

### Known limitations (carried from v0.7.0)
- macOS desktop bundle still blocked on `dispatch2` macro recursion.
- Windows desktop bundle still blocked on `libsql-sys` Unix-only
  APIs.
- The desktop `.deb`/`.rpm` bundles now include the bundled embedder
  `.so` libraries (fix for P0-2). AppImage bundling of the embedder
  `.so` libs remains deferred; AppImage users who want the local
  embedder should install the daemon `.deb`/`.rpm` separately, or
  fall back to Ollama / OpenAI via Settings.
- Auto-update via the Tauri updater is DEFERRED. The updater plugin
  has been disabled; the `.deb`/`.rpm` packages update via the system
  package manager. Re-enabling the updater requires AppImage `.so`
  staging first.
- Both desktop-portability and AppImage-bundled-embedder gaps will
  be addressed in a future plan.
- The standalone CLI (`mnemos remember` / `recall`) opens the vault
  directly and does not spawn the bundled `llama-server`; with
  `MNEMOS_EMBEDDER=bundled` it requires `mnemosd` to be running (which
  owns the embedder on `:7424`). MCP clients already go through the
  daemon, so they are unaffected. CLI auto-start of the daemon is a
  future-plan candidate.

## [0.7.0] - 2026-05-29

> **Linux-only release.** macOS and Windows desktop bundles surfaced
> platform issues during CI (see "Known limitations" below); v0.7.0
> ships `.deb` + `.rpm` + `.AppImage` for Linux only. The bundler
> configuration, sidecar staging, updater UI, and signing
> infrastructure for all three platforms remain in the tree so the
> follow-up release can re-enable them after the upstream fixes land.

### Added
- Linux installers via Tauri bundler: `.deb` + `.rpm` + `.AppImage`.
  Desktop installer bundles the daemon + CLI as Tauri sidecars.
- Stand-alone `.deb` + `.rpm` packages for the CLI (`mnemos`) and daemon
  (`mnemos-daemon`) via `cargo-deb` and `cargo-generate-rpm`.
- Tauri auto-update plumbing: `tauri-plugin-updater`, `UpdateBanner`
  React component, `mnemos_release_manifest` binary that generates the
  Tauri updater `latest.json`. **Disabled in v0.7.0** —
  `createUpdaterArtifacts: false` until the signing key is generated
  and uploaded to CI secrets (see `BUILD.md` § "Tauri updater signing
  key"). Re-enable in v0.7.1 once the secret is configured.
- `.github/workflows/release.yml` — tag-triggered Linux build job +
  Linux server-side packages job + release-publish job that uploads
  artifacts to a GitHub Release. macOS and Windows jobs removed; will
  be restored after the upstream issues are fixed.
- Icon set (SVG source + generated PNG/ICO/ICNS) under
  `desktop/src-tauri/icons/`.
- Documentation: `BUILD.md` (cross-platform build steps + known
  limitations), `PACKAGING.md` (release + distribution runbook),
  README "Install" section.
- Workspace package metadata (license, repository, homepage,
  description, authors).

### Known limitations
- **macOS desktop bundle**: `dispatch2` (a Tauri transitive dep on
  macOS) hits a `bitflags::bitflags` macro recursion limit on current
  stable Rust. Tracked upstream; will revisit after a `dispatch2`
  release or via a `RUSTFLAGS=-Z macro-backtrace`-style workaround.
- **Windows desktop bundle**: `libsql-sys` uses Unix-only
  `OsStr::as_bytes()` and doesn't compile on Windows. The mnemos
  daemon's storage layer depends on libsql; full Windows support
  requires libsql to gain Windows compatibility or mnemos to swap
  storage backends on Windows.
- **Auto-update**: deferred to v0.7.1 (key generation step is a
  one-time manual setup the user must perform).

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
