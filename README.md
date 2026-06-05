# mnemos

Local-first, file-as-source-of-truth memory provider for AI tools. Plan 1
ships the CLI foundation — vectors, daemon, MCP, and UI come in later
plans.

## Install

> v0.8.0 ships **Linux only** for the desktop bundle. The daemon
> `.deb`/`.rpm` also Linux-only. macOS and Windows still blocked on
> upstream issues (see CHANGELOG § "Known limitations").

### Linux (zero setup)

> **NOTE: This repository is private. Release assets are not
> anonymously downloadable. The `gh` CLI (authenticated) is required
> for the install commands below. Public / anonymous install requires
> making the repository (or a releases mirror) public — owner's
> decision.**

Download the release package with the `gh` CLI, then install the
local file:

```bash
# Replace X.Y.Z with the version you want (e.g. 0.8.0).
gh release download vX.Y.Z --repo UnpaidAttention/mnemos \
    -p 'mnemos-daemon_*_amd64.deb'          # Debian/Ubuntu
sudo dpkg -i mnemos-daemon_X.Y.Z_amd64.deb

# Fedora/RHEL
gh release download vX.Y.Z --repo UnpaidAttention/mnemos \
    -p 'mnemos-daemon-*.x86_64.rpm'
sudo rpm -i mnemos-daemon-X.Y.Z-1.x86_64.rpm
```

The daemon package includes the bundled embedder (22 MB MiniLM-L6 GGUF
+ llama.cpp's llama-server). Then:

```
mnemos remember "User prefers Tauri"
mnemos recall "what does the user like"
```

Semantic recall works immediately. No Ollama install, no API key
required.

### Desktop GUI (Linux)

```bash
# Debian/Ubuntu
gh release download vX.Y.Z --repo UnpaidAttention/mnemos \
    -p 'Mnemos_*_amd64.deb'
sudo dpkg -i Mnemos_X.Y.Z_amd64.deb

# Fedora/RHEL
gh release download vX.Y.Z --repo UnpaidAttention/mnemos \
    -p 'Mnemos-*.x86_64.rpm'
sudo rpm -i Mnemos-X.Y.Z-1.x86_64.rpm
```

> AppImage is not currently produced (AppImage bundling of the
> bundled embedder `.so` libs is deferred). Use `.deb` or `.rpm`.

> The desktop `.deb`/`.rpm` now includes the bundled embedder
> libraries. The daemon manages llama-server as a child process; no
> separate daemon package install is required for the desktop bundle.

### Switching embedders or LLM

Set `MNEMOS_EMBEDDER` and/or `MNEMOS_LLM` in your env:

```
export MNEMOS_EMBEDDER=ollama       # bundled / ollama / openai / mock / none
export MNEMOS_LLM=openai             # ollama / openai / mock / none (default)
export OPENAI_API_KEY=sk-...         # if using openai for either
mnemos daemon restart
```

For existing vaults seeded with a different embedder, run:

```
mnemos embed-rebuild --target bundled   # or ollama / openai
```

The migration is atomic and audit-logged; see [BUILD.md](BUILD.md)
§ "Switching embedders".

### Build from source

See [BUILD.md](BUILD.md).

[releases]: https://github.com/UnpaidAttention/mnemos/releases/latest

### Auto-update

**DEFERRED.** The Tauri in-app updater is disabled as of v0.8.0.
Shipping the updater requires an AppImage with the bundled embedder
`.so` libs staged correctly; that work is deferred to a future release.
The `UpdateBanner` UI component and `mnemos_release_manifest` tooling
remain in the tree for when AppImage bundling is re-enabled.

Update via your package manager:

```bash
# Debian/Ubuntu — after downloading the new .deb
sudo dpkg -i Mnemos_X.Y.Z_amd64.deb

# Fedora/RHEL
sudo rpm -U Mnemos-X.Y.Z-1.x86_64.rpm
```

The daemon (CLI + bundled embedder) similarly updates via package
manager once a repository is configured (see [PACKAGING.md](PACKAGING.md) §
"Linux package repositories").

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

## What works today (v0.4.0)

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

## Graph intelligence (v0.4.0)

Recall is now graph-aware, and the system distills what it learns.

- **Graph PPR retrieval (HippoRAG)** — a third retriever fused into hybrid recall
  via RRF. It seeds Personalized PageRank from the query's BM25 neighborhood and
  walks the entity graph, surfacing memories connected by relationships even when
  they share no words. On by default; disable per-query with `"graph": false`.
- **Reflection** — when the salience accumulator crosses a threshold (after
  session pipelines add enough new knowledge), the daemon synthesizes recent
  memories into typed reflection-tier memories (preference / pattern / insight /
  decision), linked back to their sources.
- **Community detection (GraphRAG)** — Louvain clustering of the entity graph;
  each cluster gets an LLM-written `community_summary`. Global/thematic queries
  retrieve over these summaries.

### New endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/v1/reflections` | Run a reflection pass now |
| `GET`  | `/v1/reflections` | List reflection-tier memories |
| `POST` | `/v1/maintenance/communities` | Run community detection + summarization |
| `POST` | `/v1/memories/search` (`"graph": bool`, `"global": bool`) | Graph-fused recall; global community-summary recall |

New MCP tools: `reflect`, `list_reflections`, `correct` (capture a wrong→right→why
correction; recurring ones harden into rules surfaced at session start); `recall`
gains `graph` and `global` args. REST: `POST`/`GET /v1/corrections`.

### Config

```toml
[retrieval]
ppr_alpha = 0.85
ppr_iterations = 30

[reflection]
salience_threshold = 5.0
max_sources = 20

[community]
min_community_size = 3
```

> Note: PPR and community detection (Louvain) are hand-rolled and dependency-free.
> Hierarchical Leiden refinement is a future enhancement.

## Automatic learning pipeline (v0.3.0)

When a session ends, the daemon turns its conversation chunks into durable
memories automatically — no manual `remember` calls required:

1. **Extract** — atomic facts are pulled from the session transcript.
2. **Resolve** — each fact is ADDed, used to UPDATE (supersede) an existing
   memory, DELETE (invalidate) a contradicted one, or skipped as a NOOP.
3. **Entity-link** — named entities are upserted and linked to the memory.
4. **Graph-update** — relationship edges between entities are recorded.

A background worker also runs an hourly **Ebbinghaus decay** pass: unused
working/episodic memories lose strength and are eventually invalidated, while
important and semantic memories persist far longer.

### Configuring the LLM

```toml
[llm]
kind = "ollama"        # "ollama" | "mock" | "none"
url = "http://localhost:11434"
model = "llama3.2"
timeout_secs = 120
```

Env overrides: `MNEMOS_LLM`, `MNEMOS_LLM_URL`, `MNEMOS_LLM_MODEL`.
Set `kind = "none"` to disable automatic learning (manual `remember` still works).

### New endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `GET`  | `/v1/pipelines` | Pipeline status: counters, recent runs, configured model |
| `POST` | `/v1/maintenance/decay` | Trigger a decay pass now |
| `PATCH`| `/v1/memories/{id}` | Patch a memory's tags / importance |
| `POST` | `/v1/memories/time-travel` | Recall as of a past timestamp |

### CLI

```bash
mnemos decay        # run a decay pass locally
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
