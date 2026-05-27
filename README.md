# mnemos

Local-first, file-as-source-of-truth memory provider for AI tools. Plan 1
ships the CLI foundation — vectors, daemon, MCP, and UI come in later
plans.

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

New MCP tools: `reflect`, `list_reflections`; `recall` gains `graph` and `global` args.

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

## Install (from source)

```bash
git clone https://github.com/UnpaidAttention/mnemos
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
