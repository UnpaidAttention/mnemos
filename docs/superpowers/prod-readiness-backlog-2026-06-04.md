# Mnemos Production-Readiness Backlog (2026-06-04)

Overall readiness: **2/5** — NO-GO for production as a shippable "set it and forget it" product, though the engineering foundation is genuinely strong. The core architecture is sound — solid daemon auth, fully parameterized SQL, fail-open hooks, a large green test suite (798 source files / 95 test files), and a well-structured bundled-embedder lifecycle. But the release is currently uninstallable and unupdatable as advertised: the repo is private (every documented download path and the auto-updater 404), the desktop bundle ships llama-server without its .so libraries so the default embedder cannot launch in the GUI, the auto-updater polls a manifest that is never produced, and no systemd unit is shipped or enabled so the daemon never auto-starts. On top of that there are two outright crash/data-loss bugs (an auto_title panic on any non-ASCII memory body, and the embed-rebuild path silently installing stale/wrong-model vectors), a guaranteed-broken `mnemos daemon start` (wrong binary name), and three flagship config knobs that are dead at runtime (capture toggle, retention, recall_budget). Several silent-failure UI paths and an unbatched, WAL-less storage layer that degrades badly at 10k+ memories round out the picture. None of this is unrecoverable — the fixes are concrete and mostly mechanical — but a user cannot currently install it, cannot rely on it to capture or auto-start, and risks a crash on the first non-Latin memory. Ship-blocking until the P0 list below is cleared; the P1 list should be cleared before calling it production-grade.

## Scorecard
- **Packaging, Release & Install** (2/5): The deb/rpm daemon path is solid and CI-tested, but the product as a whole is not installable as advertised: private repo breaks all documented downloads and the auto-updater, desktop bundle ships llama-server without its .so libs, no AppImage/latest.json/signature is ever produced despite docs claiming auto-update is enabled, and no systemd unit is shipped/enabled so the daemon never auto-starts.
- **MCP Protocol Correctness** (2/5): Happy-path stdio framing and dispatch are correct, but the error paths — exactly what an unattended install hits — are broken: the bridge relays raw non-2xx HTTP bodies as malformed frames (a rotated token yields a blank line, not a JSON-RPC error), any transport error kills the whole MCP process, PARSE_ERROR is dead code, and tool failures are returned as protocol errors instead of result.isError.
- **Performance & Resource Use** (2/5): The latency-critical per-prompt recall path is expensive by default (full graph load + 30-iter PageRank + N+1 re-fetch on every message), storage runs with no WAL/busy_timeout/PRAGMA tuning, the hourly decay pass does per-row fsync'd UPDATEs, and embedder rebuild is serial/unbatched. Individually tolerable, collectively unreliable at 10k+ memories.
- **Operability, Observability & Docs** (2/5): Health, doctor, tracing, and PID file exist, but the flagship autonomy layer is undocumented, GET /v1/config leaks secrets, a documented `mnemos daemon restart` command doesn't exist, hooks hardcode port 7423 ignoring config, the capture toggle is a dead knob, and the bundled llama-server log path is never disclosed for diagnosis.
- **Security** (3/5): Core auth (constant-time bearer compare, 0600 token, loopback bind) and the fully-parameterized SQL/FTS layer are solid with no injection found. Held back by GET /v1/config leaking the OpenAI key and Turso token in plaintext, a TOCTOU window on the token file, a redact() guard missing common secret formats, the WS token-in-URL, and a null Tauri CSP over AI-ingested content.
- **Data Integrity & Storage** (3/5): Single-writer mutex, transactional chunk deletion, and dim-mismatch guards work, but migrations run outside transactions (crash leaves an unknown half-migrated schema), file+DB writes are non-atomic (crash creates permanent file-DB divergence), FTS is never cleaned on forget/supersede (ghost rows degrade search), and there is no backup facility for the only durable copy of user memory.
- **Rust Correctness — mnemos_core** (3/5): check/clippy/fmt/tests all clean with good typed-error discipline, but two real bugs block it: a guaranteed panic in auto_title on non-ASCII bodies (taking down the primary write path), and the embed-rebuild shadow table reusing stale vectors on repeated runs, silently installing wrong-model embeddings into the live index.
- **Rust Correctness — mnemos_cli + mnemos_client** (3/5): Hooks are correctly fail-open and the transcript parser is sound, but `mnemos daemon start` looks for the wrong binary name ('mnemosd' vs installed 'mnemos-daemon') so explicit daemon management is broken in packaged builds; secondary issues include blocking full-file log reads in async and the client masking a missing response id as an empty string.
- **Failure Handling, Async & Resilience** (3/5): Hooks fail open, the pipeline runner survives per-session errors, and the embedder watchdog uses backoff, but git/rclone sync subprocesses have no timeout (sync silently wedges and stalls shutdown for minutes), the OpenAI client ignores the configured timeout, and llama-server logs vanish to /dev/null after a watchdog restart.
- **Desktop App** (3/5): TS strict mode, clean typecheck, 68 green tests, and a well-structured Tauri bridge with good guards, but the most common production state — daemon down — produces confusing or invisible UI: Settings shows a permanent loading skeleton and silently drops save failures, the FirstRun wizard gets stuck with no recourse, and several action handlers swallow errors silently.
- **Rust Correctness — mnemos_daemon** (4/5): Largely solid: clean check/clippy/tests, thorough Result-based request error handling, and a well-structured embedder lifecycle. Real but contained issues: llama-server output dropped after restart, an abort/completion status-overwrite race in embed-rebuild, and autonomy.retention being an unvalidated free-form String that silently defaults to keep-raw on a typo.
- **Test Coverage & Quality** (4/5): Substantially larger and better-structured than typical: 95 test files all green, with genuine e2e coverage of the pipeline, migrations, embed-rebuild atomicity, connectors, MCP-over-stdio, and WS events. Gaps: no full `mnemos hook session-end` CLI-against-live-daemon test, the default distill-and-prune branch is never asserted, and capture/recall_budget config knobs have no behavioral tests.

## Top blockers
- Product is not installable or updatable as shipped: private repo 404s every documented download path and the auto-updater; desktop bundle ships llama-server WITHOUT its .so libraries so the default embedder can't launch in the GUI; no AppImage/latest.json/signature is produced despite docs claiming auto-update is on; no systemd unit is shipped or enabled so the daemon never auto-starts. The core 'install it and forget it' promise fails at step one.
- Two crash/data-loss bugs on primary paths: auto_title panics the daemon on any non-ASCII memory body (every remember() without an explicit title), and the embed-rebuild path silently reuses stale vectors on repeated/round-trip runs, installing wrong-model embeddings and making semantic recall effectively random with no error.
- Flagship autonomy knobs are dead at runtime: the autonomy.capture privacy toggle never gates ingestion (users who 'pause capture' are still captured), retention is an unvalidated string that silently defaults to keep-raw on a typo (unbounded disk growth), and hooks hardcode port 7423 ignoring daemon.port so a non-default port silently captures nothing.
- Secrets exposure: GET /v1/config returns the OpenAI api_key and Turso auth_token in plaintext to any bearer-token holder (and the Settings UI), and redact() misses common secret formats (GitHub PAT, Anthropic key, GCP JSON) before storing transcripts in the vault.
- Daemon-down — the most common real state — produces confusing/invisible behavior across the stack: the MCP bridge emits unparseable frames and the whole MCP process dies on a single error; the Settings view shows a permanent skeleton and silently drops saves; the FirstRun wizard traps the user. And `mnemos daemon start` is broken in packaged builds (wrong binary name).

## Backlog
### [P0] P0-1: Repo is private — all documented downloads and the auto-updater 404 for real users
- dimension: Packaging, Release & Install
- file: README.md (lines 16-38, 71-78)
- fix: Make the repo public before release, OR document the authenticated download path (gh release download / token) AND host the updater latest.json on a public endpoint (GitHub Pages/S3/public releases repo). Do not embed a token in the client.

### [P0] P0-2: Desktop bundle ships llama-server without its .so libraries — default embedder cannot launch in the GUI
- dimension: Packaging, Release & Install
- file: desktop/src-tauri/tauri.conf.json (lines 41-44)
- fix: Add the full .so set (same list as crates/mnemos_daemon/Cargo.toml assets) to bundle.resources, and export LD_LIBRARY_PATH=<assets dir> in daemon.rs when starting the sidecar. Add an e2e test that launches the daemon via the desktop sidecar path and asserts /v1/doctor reports the embedder reachable.

### [P0] P0-3: Auto-update documented as enabled but no AppImage/latest.json/signature is ever produced
- dimension: Packaging, Release & Install
- file: .github/workflows/release.yml (lines 84, 189-194, 216-225)
- fix: Either (a) re-add appimage to build bundles, set createUpdaterArtifacts=true, fix the AppImage embedder .so staging, and verify latest.json + .sig publish; or (b) remove the updater plugin/endpoint from tauri.conf.json and correct CHANGELOG/README to state auto-update is deferred. Add a CI guard: fail the release if updater is configured but latest.json is absent.

### [P0] P0-4: auto_title panics the daemon on non-ASCII body text (first line > 80 bytes)
- dimension: Rust Correctness — mnemos_core
- file: crates/mnemos_core/src/vault.rs (lines 621-628)
- fix: Replace `&line[..77]` with the char_indices truncation idiom already used by truncate_title (compute a UTF-8 char boundary <= 76 bytes), so multibyte text never slices mid-codepoint.

### [P0] P0-5: Embed-rebuild shadow table reuses stale vectors, silently installing wrong-model embeddings
- dimension: Rust Correctness — mnemos_core
- file: crates/mnemos_core/src/embedder_rebuild.rs (lines 95-111, 186-195)
- fix: After ensure_shadow_table, compare opts.target_kind/target_model against the embedder_kind/embedder_model already in the shadow table; if they differ, DELETE FROM memory_embeddings_v2 before proceeding (or add WHERE embedder_kind=? AND embedder_model=? to the shadow_has query).

### [P0] P0-6: autonomy.capture toggle is a dead knob — capture cannot be paused (privacy/trust failure)
- dimension: Operability, Observability & Docs
- file: crates/mnemos_daemon/src/config.rs (lines 453-470) + routes/sessions.rs / hook.rs
- fix: Gate ingestion on state.config.autonomy.capture: check it in start_session (return 409/sentinel) and/or in hook.rs::do_session_end before posting the session. Document which path enforces it in the config comment.

### [P0] P0-7: GET /v1/config leaks OpenAI api_key and Turso auth_token in plaintext
- dimension: Security
- file: crates/mnemos_daemon/src/routes/config.rs (lines 19-23)
- fix: Add #[serde(skip_serializing)] to OpenAiConfig.api_key and TursoSyncConfig.auth_token, or return a sanitized ConfigView that masks secrets as "(set)"/"(not set)". PUT still accepts them for writing; GET must not echo them. Apply the same masking to the PrintConfig CLI command. (Reported by both Security and Operability.)

### [P0] P0-8: `mnemos daemon start` looks for 'mnemosd' but installed binary is 'mnemos-daemon'
- dimension: Rust Correctness — mnemos_cli + mnemos_client
- file: crates/mnemos_cli/src/commands/daemon.rs (lines 44-45)
- fix: Change bin_name to 'mnemos-daemon' to match the installed name, sidecar staging, systemd ExecStart, and resolve_daemon_bin(). Optionally add a [[bin]] name='mnemos-daemon' alias for cargo-install dev users, or centralize on a single shared constant.

### [P0] P0-9: Hooks and daemon_ctl hardcode port 7423, ignoring daemon.port — non-default port captures nothing
- dimension: Operability, Observability & Docs
- file: crates/mnemos_cli/src/commands/hook.rs (line 29) + daemon_ctl.rs
- fix: Resolve the daemon URL from config at runtime (read MNEMOS_DAEMON_PORT matching apply_env_overrides, or Config::load_default with a cache). Centralize into a single daemon_url() helper shared by hook.rs and daemon_ctl.rs.

### [P0] P0-10: MCP bridge relays non-2xx HTTP bodies as malformed frames; auth failure yields a blank line, not JSON-RPC
- dimension: MCP Protocol Correctness
- file: crates/mnemos_daemon/src/bin/mnemos_mcp_stdio.rs (lines 49-58)
- fix: After send().await, check resp.status(); on non-success, best-effort parse the request id and emit {"jsonrpc":"2.0","id":<id>,"error":{"code":-32603,"message":"daemon returned HTTP <status>: <snippet>"}}. Map 401/403 to a clear 'authentication to mnemos daemon failed — check token' message.

### [P0] P0-11: Any transport error kills the entire MCP server process instead of returning a per-request error
- dimension: MCP Protocol Correctness
- file: crates/mnemos_daemon/src/bin/mnemos_mcp_stdio.rs (lines 49-59)
- fix: Replace the `?` on the per-request HTTP call with a match; on Err, emit a JSON-RPC error frame (code -32603) keyed to the request id and `continue` the loop. Only break on stdin EOF. Add a short bounded retry for connection-refused to survive daemon restarts.

### [P1] P1-1: Each migration step runs outside a transaction — crash mid-migration leaves an unknown half-migrated schema
- dimension: Data Integrity & Storage
- file: crates/mnemos_core/src/storage/migrations.rs (lines 11-104)
- fix: Wrap each migration_vN (v1-v9) plus its schema_migrations INSERT in its own explicit transaction (DDL-in-transaction is supported by libsql) so each version is atomic and retryable. Also resolves the documented write_lock-bypass inconsistency.

### [P1] P1-2: remember/forget/patch/promote are not atomic across file write and DB write — crash creates permanent file-DB divergence
- dimension: Data Integrity & Storage
- file: crates/mnemos_core/src/vault.rs (lines 193-201, 239-265, 302-316, 349-368)
- fix: Adopt write-file-first / idempotent-DB-upsert: write the file before the DB row, use INSERT OR REPLACE / precondition-matched UPDATE so a retry after crash is safe. Add a `mnemos doctor --repair` that re-indexes FileNotInDb and soft-invalidates DbRowNoFile.

### [P1] P1-3: No backup/snapshot facility — the live SQLite file is the only copy of user memory
- dimension: Data Integrity & Storage
- file: crates/mnemos_cli/src/commands/export.rs (lines 1-58)
- fix: Add `mnemos backup <path>`: PRAGMA wal_checkpoint(FULL), use SQLite online backup API for a consistent DB snapshot, archive the markdown files alongside it. Document prominently; optionally let the sync worker include the snapshot. (export currently drops graph/audit/session data.)

### [P1] P1-4: memory_fts never cleaned on forget/decay/supersede — ghost rows degrade BM25 and grow unbounded
- dimension: Data Integrity & Storage
- file: crates/mnemos_core/src/storage/memory_ops.rs (lines 143-177, 183-195)
- fix: Add DELETE FROM memory_fts WHERE memory_id = ? inside soft_invalidate() and supersede_memory(), in the same transaction as the memories UPDATE. Ship a one-time INSERT INTO memory_fts(memory_fts) VALUES('rebuild') repair migration. Drop or properly maintain chunk_fts.

### [P1] P1-5: ensure_vec_tables_dim silently drops all vectors on a dim mismatch / mid-migration state (TOCTOU)
- dimension: Data Integrity & Storage
- file: crates/mnemos_core/src/storage/vec_ops.rs (lines 85-115)
- fix: Treat a non-zero dim as authoritative even without model_id; re-check dim inside the write-lock transaction before DROP; if memory_vec is non-empty at a different dim, return an error requiring an explicit embed-rebuild rather than silently wiping. (Reported by both mnemos_core and Data Integrity.)

### [P1] P1-6: Graph recall (full graph load + 30-iter PPR + N+1 re-fetch) runs on every user prompt by default
- dimension: Performance & Resource Use
- file: crates/mnemos_daemon/src/routes/memories.rs (lines 223-260); retrieval/hybrid.rs (lines 95-128)
- fix: Make graph recall cheap on the per-prompt path: pass graph:false from the user-prompt hook OR cache the MemoryGraph in AppState and rebuild incrementally OR cap PPR to the seed neighborhood; lower default ppr_iterations / add early convergence. Build a HashMap from already-hydrated bm25+dense hits to kill the N+1 re-fetch.

### [P1] P1-7: SQLite opened with no WAL, busy_timeout, or PRAGMA tuning — readers/writers block, spurious SQLITE_BUSY
- dimension: Performance & Resource Use
- file: crates/mnemos_core/src/storage/mod.rs (lines 61-93)
- fix: On first connection set PRAGMA journal_mode=WAL; busy_timeout=5000; synchronous=NORMAL (consider cache_size/mmap_size). Enables concurrent reads during background writes and removes spurious BUSY errors. Foundational for the decay/rebuild batching fixes below.

### [P1] P1-8: Hourly decay pass does a full-table scan with per-row, non-transactional, fsync'd UPDATEs
- dimension: Performance & Resource Use
- file: crates/mnemos_core/src/pipeline/decay.rs (lines 61-113)
- fix: Acquire write_conn once and wrap all strength UPDATEs in a single transaction (or one batched UPDATE ... CASE). Combined with WAL this turns thousands of fsyncs into one and makes the pass all-or-nothing. (Reported by both Data Integrity and Performance.)

### [P1] P1-9: Embedder rebuild/backfill issues serial, unbatched embed calls
- dimension: Performance & Resource Use
- file: crates/mnemos_core/src/embedder_rebuild.rs (lines 95-112)
- fix: Implement embed_batch on BundledEmbedder and OpenAI (POST input:[..] arrays), embed in batches, and write shadow rows in batched transactions. Converts N round-trips/fsyncs into N/batch — turns minutes of blocking work into seconds on a 10k vault.

### [P1] P1-10: Git/rclone sync subprocesses have no timeout — sync silently wedges and stalls shutdown
- dimension: Failure Handling, Async & Resilience
- file: crates/mnemos_core/src/sync/git.rs (lines 29-44)
- fix: Wrap git and rclone Command::output() in tokio::time::timeout (~5 min) and set .kill_on_drop(true) so a stalled remote returns an error instead of blocking the sync worker indefinitely and hanging graceful shutdown.

### [P1] P1-11: llama-server output dropped to /dev/null after watchdog restart — post-restart crashes undiagnosable
- dimension: Rust Correctness — mnemos_daemon
- file: crates/mnemos_daemon/src/bundled_embedder.rs (lines 230-243)
- fix: Reopen the log file (log_path()) in append mode in the restart branch and reuse the Stdio::from(log) pattern from the initial spawn; extract a shared spawn_child(cfg, log_path) helper. (Reported by mnemos_daemon, Failure Handling, and Operability.)

### [P1] P1-12: embed-rebuild abort vs background-completion race can silently overwrite status and allow concurrent rebuilds
- dimension: Rust Correctness — mnemos_daemon
- file: crates/mnemos_daemon/src/routes/embed_rebuild.rs (lines 92-124 vs 132-143)
- fix: Add an Aborted variant (or a generation counter incremented on each start); the background task only writes its final status if it was not aborted / its generation still matches. Prevents interleaved shadow-table writes from two rebuilds.

### [P1] P1-13: autonomy.retention is an unvalidated free-form String — a typo silently defaults to keep-raw (unbounded disk growth)
- dimension: Rust Correctness — mnemos_daemon
- file: crates/mnemos_daemon/src/config.rs (line 458)
- fix: Replace String with enum RetentionPolicy { DistillAndPrune, KeepRaw } using #[serde(rename_all="kebab-case")]; update maybe_prune_chunks to match the enum. Makes invalid config a hard parse error at startup.

### [P1] P1-14: OpenAI LLM provider ignores user-configurable llm.timeout_secs (hardcoded 60s)
- dimension: Failure Handling, Async & Resilience
- file: crates/mnemos_core/src/providers/openai_llm.rs (lines 64-65)
- fix: Add timeout_secs to OpenAiLlmConfig (default 60), populate from config.llm.timeout_secs in llm.rs, and apply .timeout(Duration::from_secs(cfg.timeout_secs.max(1))) — mirror the working OllamaLlm pattern.

### [P1] P1-15: Settings view: getConfig() failure shows a permanent loading skeleton with no error
- dimension: Desktop App
- file: desktop/src/views/Settings.tsx (lines 153-155)
- fix: Add a loadError state (mirror StorageSettings/AutonomySettings): .then(setCfg).catch(() => setLoadError('Could not reach the daemon')) and render the error instead of the skeleton.

### [P1] P1-16: Settings view: save() error is not surfaced to the user
- dimension: Desktop App
- file: desktop/src/views/Settings.tsx (lines 165-173)
- fix: Add a saveError state; wrap save in try/catch/finally, render the error below the Save button so a failed persist of embedder/LLM/sync config is visible.

### [P1] P1-17: FirstRun wizard finish() and App.tsx getFirstRun() have no error handling — wizard traps the user / onboarding vanishes when daemon is down
- dimension: Desktop App
- file: desktop/src/views/FirstRun.tsx (lines 16-19, 130); desktop/src/App.tsx (lines 17-19)
- fix: Wrap finish in try/catch with a footer error (mirror handleEnableService graceful degrade). In App.tsx add .catch(() => setFirstRunShown(false)) or poll-until-reachable so firstRunShown never stays null forever.

### [P1] P1-18: distill-and-prune retention branch is never asserted in any pipeline integration test
- dimension: Test Coverage & Quality
- file: crates/mnemos_daemon/tests/pipeline_runner.rs
- fix: After PipelineCompleted, assert SELECT COUNT(*) FROM chunks WHERE session_id=? is 0 for the default policy; add a companion keep-raw test asserting chunks are NOT deleted. The default retention path and the claimed privacy property are currently untested.

### [P1] P1-19: No integration test exercises the full `mnemos hook session-end` binary against a live daemon
- dimension: Test Coverage & Quality
- file: crates/mnemos_cli/tests/
- fix: Add an assert_cmd test: spawn a real daemon on a random port, write a minimal JSONL transcript, run `mnemos hook session-end` with a JSON payload over stdin, then assert /v1/memories/search returns the expected content. This is the central user-visible flow and is currently untested end-to-end.

### [P1] P1-20: mnemos doctor (CLI) only reports file/DB drift; hides embedder/LLM/schema health, especially when daemon is down
- dimension: Operability, Observability & Docs
- file: crates/mnemos_cli/src/commands/doctor.rs (lines 5-36)
- fix: Extend `mnemos doctor` to call GET /v1/doctor when daemon is up (merge results) and, when down, explicitly say so with the log path and suggest `mnemos daemon start`. Add embedder/schema checks on the direct-vault path. Surface the bundled llama-server log path in doctor/startup warnings.

### [P2] P2-1: Token file written world-readable before chmod 0600 (TOCTOU window)
- dimension: Security
- file: crates/mnemos_daemon/src/auth.rs (lines 24-31)
- fix: Create the file with mode 0o600 atomically via OpenOptions::new().write(true).create_new(true).mode(0o600).open(path) (unix OpenOptionsExt) and drop the subsequent set_permissions call.

### [P2] P2-2: redact() misses GitHub PATs, Anthropic keys, and GCP service-account JSON
- dimension: Security
- file: crates/mnemos_cli/src/commands/hook.rs (lines 451-511)
- fix: Extend redact() to cover ghp_/gho_/github_pat_, sk-ant-, and generic high-entropy 40+ char tokens (consider detect-secrets / trufflehog regex sets). Document that user-pasted secrets remain the user's responsibility — this is defense-in-depth.

### [P2] P2-3: Bearer token exposed in WebSocket URL query string
- dimension: Security
- file: desktop/src/api/ws.ts (line 33)
- fix: Either document the loopback-only tradeoff, or implement a short-lived upgrade ticket: POST /v1/events/ticket (header-authed) returns a 30s one-time token used in the WS URL, eliminating the long-lived secret from the URL.

### [P2] P2-4: Tauri WebView CSP is null — no Content-Security-Policy over AI-ingested content
- dimension: Security
- file: desktop/src-tauri/tauri.conf.json (line 22)
- fix: Set a restrictive CSP: default-src 'self'; connect-src http://127.0.0.1:7423 ws://127.0.0.1:7423; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:. Verify no memory body is rendered as raw HTML (no dangerouslySetInnerHTML).

### [P2] P2-5: Idempotency state file grows unbounded with O(n) scan on every hook fire
- dimension: Failure Handling, Async & Resilience
- file: crates/mnemos_cli/src/commands/hook.rs (lines 260-298)
- fix: Move idempotency tracking into the indexed SQLite sessions table for O(1) lookup, or cap the file to a rolling window (~10k entries) and truncate on write. Add a size guard so a bloated/corrupt file degrades safely. (Reported by mnemos_cli, Failure Handling, Performance, and Operability.)

### [P2] P2-6: systemd unit has no TimeoutStartSec/StartLimit/resource limits and logs aren't routed to journal
- dimension: Operability, Observability & Docs
- file: packaging/systemd/mnemosd.service (lines 1-12)
- fix: Add TimeoutStartSec=60, StartLimitIntervalSec=300, StartLimitBurst=5, MemoryMax=2G, StandardOutput=journal, StandardError=journal. Document `journalctl --user -u mnemosd -f`.

### [P2] P2-7: Health endpoint reports only daemon liveness, not bundled-embedder status
- dimension: Operability, Observability & Docs
- file: crates/mnemos_daemon/src/routes/health.rs (lines 4-10)
- fix: Add an optional embedder field to /health backed by the existing 30s health task cache (or a 500ms HEAD to the llama-server port) reporting ok/degraded, so monitoring probes get a meaningful signal when embedding is silently broken.

### [P2] P2-8: Autonomy layer (hooks, service setup, capture) and `mnemos daemon restart` are absent/incorrect in docs
- dimension: Operability, Observability & Docs
- file: README.md (lines 1-284, 55); adapters/claude-code/README.md; BUILD.md (line 97)
- fix: Add an 'Autonomy (set it and forget it)' README section (service install/enable, connect wizard, the three hooks, verification, where captured sessions live). Implement DaemonAction::Restart OR replace the documented `mnemos daemon restart` with `stop && start`.

### [P2] P2-9: Client::remember silently returns empty string when daemon response lacks id; blocking full-file log read in async
- dimension: Rust Correctness — mnemos_cli + mnemos_client
- file: crates/mnemos_client/src/lib.rs (line 53); crates/mnemos_cli/src/commands/daemon.rs (lines 139-152)
- fix: Replace unwrap_or_default() with ok_or_else(|| ClientError::Server{...}) so a missing id surfaces as an error. For logs(): use spawn_blocking or tail only the last ~1MB instead of read_to_string on the whole file.

### [P2] P2-10: MCP PARSE_ERROR is dead code (malformed JSON → HTTP 422) and tool failures returned as JSON-RPC errors instead of result.isError
- dimension: MCP Protocol Correctness
- file: crates/mnemos_daemon/src/mcp/mod.rs (lines 25, 92-103)
- fix: Accept the body as Json<Value>/Bytes, manually deserialize, and return a well-formed JSON-RPC PARSE_ERROR(-32700)/INVALID_REQUEST(-32600) on failure (HTTP 200). In tools_call, return result {content:[...], isError:true} for tool execution failures; reserve JSON-RPC errors for bad/unknown tool names.

### [P2] P2-11: Session-start working set loads the entire Reflection tier and filters tags in Rust
- dimension: Performance & Resource Use
- file: crates/mnemos_daemon/src/routes/working.rs (lines 56-69)
- fix: Push the tag filter into SQL (FTS, a tags table, or json_each on tags_json with an index) and apply LIMIT in the query rather than hydrating and sorting the whole tier in memory on every session-start.

### [P2] P2-12: Dense KNN applies validity/workspace/tier filters AFTER the vec0 k-limit, silently shrinking results
- dimension: Performance & Resource Use
- file: crates/mnemos_core/src/retrieval/dense.rs (lines 42-71)
- fix: Either delete vectors from memory_vec when a memory is invalidated/superseded (keep the vec table live-only), or over-fetch k (k*N) before applying validity filters so enough live candidates remain.

### [P2] P2-13: Daemon HTTP listener startup is gated on the bundled embedder becoming healthy (5s) — cold model load can fail startup
- dimension: Performance & Resource Use
- file: crates/mnemos_daemon/src/lib.rs (lines 68-89)
- fix: Start the HTTP listener immediately and bring the embedder up asynchronously (serve BM25-only recall until healthy); raise/inform the ensure_daemon health wait to accommodate realistic cold GGUF load instead of failing startup at 5s.

### [P2] P2-14: Bundled llama.cpp binary fetched without sha256 verification (only the model is checksummed)
- dimension: Packaging, Release & Install
- file: scripts/fetch-bundled-assets.sh (lines 37-61, 73-76)
- fix: Pin and verify sha256 for the llama.cpp tarball (and ideally each .so), bumped alongside LLAMA_CPP_TAG. Record a full asset manifest with hashes for reproducible deb/rpm contents.

### [P2] P2-15: Silent error handlers in Desktop action buttons (promote/abort/trigger) and DaemonEvent union missing embed_rebuild_* variants
- dimension: Desktop App
- file: desktop/src/views/Reflections.tsx (24-28); EmbedRebuild.tsx (60-63); Pipelines.tsx (21-33); desktop/src/api/events.ts
- fix: Add try/catch + local error state surfaced near each button for promote/abort/trigger. Add the four embed_rebuild_* variants to the DaemonEvent union so exhaustive narrowing stays correct.

### [P2] P2-16: set_embedder_meta/set_vault_meta silently succeed when the vault_meta row is missing
- dimension: Rust Correctness — mnemos_core
- file: crates/mnemos_core/src/storage/vault_meta.rs (lines 60-77)
- fix: Check the affected row count from the UPDATE; if 0, return MnemosError::Internal('vault_meta row missing; run Storage::open to initialize') so silent metadata loss (which triggers vector-table drop) surfaces as an error.

### [P2] P2-17: Shadow table memory_embeddings_v2 never cleaned after a successful embed-rebuild
- dimension: Data Integrity & Storage
- file: crates/mnemos_core/src/embedder_rebuild.rs (lines 28-30)
- fix: After swap_memory_vec() succeeds, DELETE FROM memory_embeddings_v2 as a committed step (same transaction as the swap if possible). Resolve the TODO so full vectors from prior models don't accumulate on disk.

### [P2] P2-18: Several CLI/UI hardening nits: service_status .expect() panic, curl JSON via format!, weak App smoke test, EntityProfile MSW unhandled request
- dimension: Test Coverage & Quality
- file: crates/mnemos_cli/src/commands/service.rs (106-109); embed_rebuild.rs (38-41); desktop/src/App.test.tsx; desktop/src/views/EntityProfile.test.tsx
- fix: Replace .expect('tokio runtime') with map_err(...)?; build curl example bodies with serde_json::json!; strengthen App.test.tsx to assert nav links and a rendered route; add the /v1/entities MSW handler (or onUnhandledRequest:'error'). Add unit tests for VaultIO export/import error paths.

### [P2] P2-19: enable_service uses PATH lookup (.command("mnemos")) instead of bundled sidecar; transcript_path read without validation
- dimension: Security
- file: desktop/src-tauri/src/commands.rs (line 30); crates/mnemos_cli/src/commands/hook.rs (lines 176-210)
- fix: Use app.shell().sidecar("mnemos") consistent with daemon.rs (avoids PATH command-hijacking). Canonicalize transcript_path and verify it resolves within expected Claude Code transcript dirs / home before reading.

### [P2] P2-20: PID file acquisition has a TOCTOU window; two concurrent daemon starts can both pass the liveness check
- dimension: Rust Correctness — mnemos_daemon
- file: crates/mnemos_daemon/src/pid.rs (lines 18-27)
- fix: Use an OS-level exclusive file lock (fs2/file-guard try_lock_exclusive or fcntl F_SETLK) for exclusivity, keeping PID content only for diagnostics. Prevents two instances writing the same vault on double-launch.

### [P2] P2-21: Log format configurable in config but no env-var override and undocumented; bundled embedder build panics via .expect()
- dimension: Failure Handling, Async & Resilience
- file: crates/mnemos_daemon/src/config.rs (228-233); crates/mnemos_core/src/providers/bundled.rs (line 32)
- fix: Add MNEMOS_LOG_FORMAT (json/compact) to apply_env_overrides and document MNEMOS_LOG/MNEMOS_LOG_FORMAT. Change BundledEmbedder::new() to return Result and propagate the reqwest build error instead of panicking (mirror OpenAiEmbedder::new).

### [P2] P2-22: Bundled-embedder integration tests ignored on macOS CI; ignore condition inconsistent across crates
- dimension: Test Coverage & Quality
- file: crates/mnemos_core/tests/bundled_embedder.rs (line 18)
- fix: Add a macOS asset-fetch step to CI, or add a wiremock-based mock-server test that exercises the HTTP client path without assets so the default embedder is covered on both platforms with a consistent gating condition.
