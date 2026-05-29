# Mnemos Bundled Embedder — Design Spec (v0.8.0)

**Purpose:** Make Mnemos work end-to-end on a fresh install with no external dependencies. Today's default (`MNEMOS_EMBEDDER=ollama`) requires the user to install Ollama and pull `nomic-embed-text` (~500 MB) before semantic recall works. v0.8.0 bundles a tiny embedder + the inference runtime directly in the install so semantic recall works the moment `mnemosd` starts.

**One-line architecture:** Mnemos ships with `llama-server` (llama.cpp's upstream HTTP server) and a 22 MB `all-MiniLM-L6-v2` GGUF model. The daemon spawns `llama-server` as a child process on startup; embeddings hit it over HTTP at `127.0.0.1:7424`. Ollama and OpenAI remain supported backends; users opt in via config.

---

## Goals

1. **Zero-setup semantic recall.** A user installs `mnemos_X.Y.Z_amd64.deb`, runs `mnemos remember "hello"` then `mnemos recall "greeting"`, and gets the result. No `ollama pull`, no API key.
2. **Local-first preserved.** Embeddings never leave the machine by default. The bundled embedder runs in-process (well — in a child process); no network calls during embed.
3. **Low-spec friendly.** Total install footprint ≤ 100 MB (binary + model + Mnemos itself). RAM at idle ~150 MB. Runs on a Raspberry Pi 4 (4 GB).
4. **Backward-compatible.** Existing vaults seeded with Ollama-768 continue to work. Users migrate when they want, via one command.
5. **No corners cut.** Migration is atomic, resumable, and audit-logged. The bundled `llama-server` lifecycle is monitored, crash-isolated, and restarts on failure with backoff.

## Non-goals

- **Bundling a chat LLM.** Only the embedder gets bundled. Plan 4/5's reflection + community-summary features still require an LLM (Ollama or OpenAI); they no-op gracefully if neither is configured.
- **Cross-platform support beyond Linux.** macOS (`dispatch2` macro recursion) and Windows (`libsql-sys` Unix-only APIs) are still blocked from v0.7.0; this plan does not unblock them.
- **Replacing Ollama entirely.** Ollama remains a first-class supported backend. Users who already have it can keep using it indefinitely; the doctor view surfaces an optional migration prompt.
- **Replacing libsql.** Storage layer is unchanged.

---

## Architecture

### Components

```
                                 ┌─────────────────────────────────┐
                                 │  mnemosd  (127.0.0.1:7423)      │
                                 │                                 │
   AI tool (Claude Code, etc.)   │   ┌─────────────────────────┐   │
        │                        │   │  embedder::Bundled       │  │
        │  MCP / REST            │   │                          │  │
        ▼                        │   │  • spawns child on start │  │
   ┌──────────────────┐          │   │  • health-checks         │  │
   │ mnemos-mcp-stdio ├──────────►   │  • auto-restart on crash │  │
   └──────────────────┘          │   │  • kills on shutdown     │  │
                                 │   └────────────┬─────────────┘   │
                                 │                │ HTTP            │
                                 │                ▼                 │
                                 │   ┌─────────────────────────┐    │
                                 │   │  llama-server  (127.0.0.1:7424)│
                                 │   │  bundled binary, ~5 MB  │    │
                                 │   │  + GGUF model, ~22 MB   │    │
                                 │   └─────────────────────────┘    │
                                 └─────────────────────────────────┘
```

### Embedder backends after this plan

| Backend | Setting | Default? | Notes |
|---|---|---|---|
| `bundled` | `MNEMOS_EMBEDDER=bundled` | **yes** (new vaults) | Spawns llama-server, uses MiniLM-L6 Q8 |
| `ollama` | `MNEMOS_EMBEDDER=ollama` | (was default) | Same as today; users who have Ollama keep using it |
| `openai` | `MNEMOS_EMBEDDER=openai` + `OPENAI_API_KEY` | no | New backend; uses `text-embedding-3-small` (1536-dim) or `text-embedding-3-large` (3072-dim) |
| `mock` | `MNEMOS_EMBEDDER=mock` | no | Existing; deterministic vectors for tests |
| `none` | `MNEMOS_EMBEDDER=none` | no | Existing; embedder disabled, BM25-only recall |

### LLM backends after this plan

| Backend | Setting | Notes |
|---|---|---|
| `ollama` | `MNEMOS_LLM=ollama` | (was default) |
| `openai` | `MNEMOS_LLM=openai` + `OPENAI_API_KEY` | **new** — uses `gpt-4o-mini` by default, override with `MNEMOS_LLM_MODEL` |
| `mock` | `MNEMOS_LLM=mock` | Existing |
| `none` | `MNEMOS_LLM=none` | **new default** — reflections/community summaries no-op silently |

Defaults for fresh installs: `MNEMOS_EMBEDDER=bundled`, `MNEMOS_LLM=none`. Users who want reflections opt in to Ollama or OpenAI.

### llama-server child process lifecycle

- **Start:** daemon spawns `llama-server --model <bundled-gguf> --port 7424 --embedding --pooling mean` on its own startup, before the HTTP server binds 7423. Waits ≤ 5s for llama-server's `/health` endpoint to return 200.
- **Health:** every 30s, the embedder probes `http://127.0.0.1:7424/health`. Three consecutive failures → restart.
- **Restart:** exponential backoff capped at 60s. After 5 consecutive restart attempts, mark embedder unhealthy; doctor + WS event surface this.
- **Shutdown:** daemon SIGTERMs llama-server on its own shutdown, waits ≤ 2s, SIGKILLs if still alive.
- **Logs:** llama-server's stderr forwards to `~/.local/state/mnemos/logs/llama-server.log` (rotated daily, kept 7 days).

### Vault metadata

Schema v9 adds `embedder_kind TEXT NOT NULL DEFAULT 'ollama'` to `vault_meta`. Combined with existing `embedder_model` + `embedder_dim`, the vault now records exactly which embedder seeded it:

```
embedder_kind  embedder_model           embedder_dim
─────────────  ──────────────────────  ─────────────
bundled        all-MiniLM-L6-v2         384
ollama         nomic-embed-text         768
openai         text-embedding-3-small   1536
mock           mock                     384  (was 768 in v0.7.0)
```

**Vault meta is authoritative.** The daemon always uses `vault.embedder_kind` to decide which backend to load — not the `MNEMOS_EMBEDDER` env. The env variable is the default for **new** vaults only. If `MNEMOS_EMBEDDER` is set explicitly and disagrees with an existing vault's meta, the daemon logs a warning at startup ("env says `bundled`, vault was seeded with `ollama`, continuing with `ollama`") and uses the vault's setting. To actually switch, the user runs `mnemos embed-rebuild`. This makes the vault impossible to corrupt by mixing embedders accidentally.

### `mnemos embed-rebuild`

The migration command. Re-embeds every memory in the vault with a target embedder, then atomically swaps the vault's embedder metadata.

```
mnemos embed-rebuild --target bundled        # migrate to bundled
mnemos embed-rebuild --target ollama         # migrate to Ollama (must be installed)
mnemos embed-rebuild --target openai         # migrate to OpenAI (must have key)
mnemos embed-rebuild --status                # show progress of an in-flight rebuild
mnemos embed-rebuild --abort                 # cancel an in-flight rebuild
```

**Atomicity:** new embeddings are written to a shadow table `memory_embeddings_v2` keyed by memory_id. Only after every memory is re-embedded successfully does the daemon:
1. Atomically rename `memory_embeddings → memory_embeddings_v1_backup`
2. Atomically rename `memory_embeddings_v2 → memory_embeddings`
3. Update `vault_meta.embedder_kind`, `embedder_model`, `embedder_dim`
4. Write an audit entry: `embedder_migrated: ollama-768 → bundled-384`
5. Drop the backup after a 7-day retention (so a botched migration can be rolled back manually)

**Resumability:** the shadow table is durable. If the daemon dies mid-rebuild, the next `mnemos embed-rebuild` resumes from where it left off (skips memories that already have a row in `memory_embeddings_v2`).

**Progress:** REST `GET /v1/embed-rebuild/status` returns `{ status: "running"|"idle"|"failed", processed: N, total: M, eta_seconds: K }`. The CLI subscribes to a WS event stream `embed_rebuild_progress` for live updates. Settings UI shows a progress bar.

### OpenAI backends

Both `OpenAiEmbedder` and `OpenAiLlm`:
- Read `OPENAI_API_KEY` from env or `~/.config/mnemos/openai-token` (mode 0600)
- Read `OPENAI_BASE_URL` (default `https://api.openai.com`) so users can point at Azure OpenAI or a local OpenAI-compatible server
- Default model: `text-embedding-3-small` (1536-dim) for embeddings, `gpt-4o-mini` for chat. Overridable via `MNEMOS_EMBEDDER_MODEL` and `MNEMOS_LLM_MODEL`
- Use the standard OpenAI HTTP API; ~30 lines of code each

### Tauri auto-update re-enabled

v0.7.0 deferred auto-update with `createUpdaterArtifacts: false` because no signing key was generated. v0.8.0 re-enables:

1. User runs `bash scripts/gen-updater-key.sh` once locally; pastes the public key into `tauri.conf.json` (replacing `PLACEHOLDER_PUBLIC_KEY_REPLACE_BEFORE_RELEASE`).
2. User uploads the private key to GH Secrets as `TAURI_SIGNING_PRIVATE_KEY` (one-time).
3. `createUpdaterArtifacts: true` flips back on.
4. `release.yml` re-adds the `latest.json` generation step (using `mnemos-release-manifest` from Plan 8).
5. Future releases auto-update from v0.8.0 → v0.8.x via the UpdateBanner UI shipped in Plan 8 Task 7.

This is a non-engineering blocker — the plan documents the secret setup, the user actually does it before the v0.8.0 release.

---

## File structure produced by this plan

```
crates/mnemos_core/src/embedder/
  bundled.rs                                 # NEW — spawns + manages llama-server child
  openai.rs                                  # NEW — OpenAI embeddings backend
  mod.rs                                     # MOD — add Bundled + OpenAi to EmbedderKind enum

crates/mnemos_core/src/llm/
  openai.rs                                  # NEW — OpenAI chat completions

crates/mnemos_core/src/embedder_rebuild.rs   # NEW — atomic migration core
crates/mnemos_core/src/storage/migrations.rs # MOD — schema v9 (embedder_kind column)

crates/mnemos_daemon/src/routes/
  embed_rebuild.rs                           # NEW — GET status + POST start/abort
  doctor.rs                                  # MOD — bundle-embedder health check

crates/mnemos_daemon/src/bundled_embedder.rs # NEW — process lifecycle (spawn/health/restart)
crates/mnemos_daemon/src/lib.rs              # MOD — register bundled embedder in build_app_full

crates/mnemos_cli/src/commands/
  embed_rebuild.rs                           # NEW — CLI command

desktop/src/views/Settings.tsx               # MOD — embedder section: bundled/ollama/openai/none
desktop/src/views/Doctor.tsx                 # MOD — surface migration prompt
desktop/src/views/EmbedRebuild.tsx           # NEW — progress UI

scripts/fetch-bundled-assets.sh              # NEW — downloads llama-server + GGUF in CI
.github/workflows/release.yml                # MOD — invoke fetch-bundled-assets + bundle in package
.github/workflows/ci.yml                     # MOD — same, for test runs

desktop/src-tauri/tauri.conf.json            # MOD — bundle assets, re-enable updater
desktop/src-tauri/Cargo.toml                 # MOD — bundle resources path

crates/mnemos_cli/Cargo.toml                 # MOD — cargo-deb asset paths for bundled files
crates/mnemos_daemon/Cargo.toml              # MOD — same

BUILD.md                                     # MOD — bundled embedder section
PACKAGING.md                                 # MOD — release runbook update
README.md                                    # MOD — install section reflects zero-setup
CHANGELOG.md                                 # MOD — 0.8.0 entry
```

---

## User flows

### Fresh Linux install

```
$ sudo dpkg -i Mnemos_0.8.0_amd64.deb
$ mnemos remember "User prefers Tauri"      # daemon auto-spawns, llama-server starts,
                                              # memory written, embedding computed locally
$ mnemos recall "what does the user like"   # works immediately
```

No `ollama pull`, no API key, no setup wizard.

### Upgrade from v0.7.0 (existing Ollama-seeded vault)

```
$ sudo dpkg -i Mnemos_0.8.0_amd64.deb       # replaces v0.7.0
$ mnemosd                                   # daemon starts; detects vault.embedder_kind=ollama
                                              # continues using Ollama as before
$ mnemos doctor
   ⚠ embedder: ollama (vault was seeded with nomic-embed-text)
   ⚠ bundled embedder is available (all-MiniLM-L6-v2, 384-dim)
   →  to migrate: mnemos embed-rebuild --target bundled
$ mnemos embed-rebuild --target bundled
   re-embedding 42 memories with all-MiniLM-L6-v2...
   [████████████░░░░] 27/42  ETA 8s
   ✓ migration complete (vault_meta.embedder_kind = bundled)
   audit: embedder_migrated ollama-768 → bundled-384
```

### Opt-in to OpenAI for synthesis (existing user)

```
$ export OPENAI_API_KEY=sk-...
$ export MNEMOS_LLM=openai
$ mnemos daemon restart
$ mnemos reflect                            # now uses OpenAI gpt-4o-mini for reflection
```

The embedder stays bundled (local). Only synthesis goes to OpenAI. User can also flip `MNEMOS_EMBEDDER=openai` if they want fully-cloud retrieval too.

### Air-gapped / no-Ollama / no-OpenAI

```
$ export MNEMOS_LLM=none                    # reflections + community summaries silently skipped
$ mnemosd
                                              # bundled embedder still works locally
                                              # remember + recall + sync + doctor all work
                                              # reflection/community endpoints return 503
                                              #   with message: "LLM not configured"
```

---

## Error handling

| Failure mode | Behavior |
|---|---|
| llama-server binary missing from install | daemon refuses to start; clear error: "bundled embedder not found at `<path>`; reinstall or set `MNEMOS_EMBEDDER=ollama`" |
| llama-server fails to bind 7424 | daemon retries 3 times with 1s backoff; if still failing, marks embedder unhealthy, daemon continues with embedder disabled (recall degrades to BM25, remember fails) |
| llama-server crashes after startup | exponential-backoff restart (1s → 60s), surfaces via doctor + WS event `embedder_unhealthy` after 5 consecutive failures |
| Embed-rebuild aborted mid-flight | shadow table preserved; next rebuild resumes; partial vault remains queryable via the old embedder |
| Embed-rebuild target backend unavailable | abort with clear error before touching the vault; rollback is a no-op (nothing changed) |
| `MNEMOS_EMBEDDER` env disagrees with vault meta | daemon warns at startup, uses vault meta (authoritative). To switch, run `mnemos embed-rebuild --target <new>` |
| `OPENAI_API_KEY` missing with `MNEMOS_LLM=openai` | daemon refuses to start; doctor shows fail |

---

## Testing strategy

- **Unit tests:**
  - `BundledEmbedder` produces deterministic vectors against a known input fixture (same input → same 384-dim vector across runs)
  - `OpenAiEmbedder` request shape matches OpenAI's schema (mock the HTTP layer with `wiremock`)
  - `embedder_rebuild::run()` is atomic — abort mid-rebuild, restart, verify state is sane
  - Schema v9 migration adds `embedder_kind` without breaking v8 vaults
- **Integration tests:**
  - Daemon test spawns real `llama-server` (must be on `PATH` in CI), runs an embed, asserts dim=384
  - Embed-rebuild migrates a 10-memory Ollama-seeded vault to bundled, asserts all memories have new embeddings + vault_meta updated + audit logged
  - Doctor endpoint correctly reports embedder mismatch
- **CI:**
  - `scripts/fetch-bundled-assets.sh` runs once per CI invocation, downloads + caches `llama-server` + GGUF model
  - Test runs use the cached assets; release runs vendor them into the package
- **Smoke test:**
  - After v0.8.0 .deb installs, `mnemos remember` + `mnemos recall` work end-to-end with NO external setup. This is the acceptance test.

---

## Risks + mitigations

| Risk | Mitigation |
|---|---|
| llama-server binary size bloats the .deb to ~80 MB | Acceptable. Compare: Ollama install is 200+ MB; nomic-embed-text alone is 274 MB. Total v0.8.0 .deb at ~80 MB is competitive. |
| Tracking llama.cpp upstream releases adds maintenance | `scripts/fetch-bundled-assets.sh` pins a specific release tag. Bumping it is one variable + a test run. Quarterly cadence is realistic. |
| Embed-rebuild on a large vault (~10k memories) takes minutes | The atomic-resumable design means it's safe to leave running; progress UI keeps the user informed. Doctor + Settings UI surface ETA. |
| Embedded model quality is worse than nomic-embed-text | Real but small. MiniLM-L6 scores ~58 on MTEB vs nomic-embed-text at ~62. For mnemos's "find related memories" use case, the gap is in the noise. Users who want the bigger model can `MNEMOS_EMBEDDER=ollama` + `MNEMOS_EMBEDDER_MODEL=nomic-embed-text`. |
| llama.cpp licenses: MIT. all-MiniLM-L6-v2: Apache-2.0. Both compatible with Mnemos's Apache-2.0. | Document in `LICENSES.md` for the bundled binaries. |
| Auto-update re-enable requires user to do one-time key gen | Document clearly in CHANGELOG + the release runbook; first-run wizard on upgrade prompts the user to set up the key. |

---

## Open questions / future work

- **macOS / Windows portability** — still blocked on `dispatch2` macro recursion (macOS) and `libsql-sys` Unix-only APIs (Windows). Separate plan after llama.cpp + Mnemos work; estimated v0.9.0+.
- **Bundled chat LLM** — deferred indefinitely. If users want local synthesis, they install Ollama. Bundling a 400+ MB chat model defeats the lightweight goal and most users won't use reflections.
- **OpenAI-compatible local LLM endpoints** — `OPENAI_BASE_URL` already lets users point at LocalAI, Jan, or `llama-server`'s OpenAI-compat mode. Documented but not part of the default flow.
- **Embedder benchmarking suite** — measure recall quality across MiniLM vs Nomic vs OpenAI on a fixed corpus. Not a v0.8.0 blocker; useful for advising users which backend to choose.

---

## Acceptance criteria

- [ ] `mnemos_0.8.0_amd64.deb` installs cleanly on a fresh Ubuntu 22.04 VM with no Ollama, no internet (except for the .deb download itself).
- [ ] First-run `mnemos remember "test"` then `mnemos recall "test"` returns the memory with a real (non-zero) similarity score — proving bundled embedder works.
- [ ] An existing v0.7.0 vault upgrades cleanly: daemon starts, vault is queryable, doctor reports mismatch, `mnemos embed-rebuild --target bundled` completes successfully + atomically.
- [ ] `OPENAI_API_KEY` + `MNEMOS_LLM=openai` makes reflection generation work end-to-end.
- [ ] Tauri auto-update verifies a v0.8.1-test signed release manifest against the embedded public key.
- [ ] CI: full matrix (build + linux-packages + release) passes; `cargo clippy --workspace --all-targets -- -D warnings` clean.
