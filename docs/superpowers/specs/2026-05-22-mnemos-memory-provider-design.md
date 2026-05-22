# Mnemos — Local-first AI Memory Provider

**Design spec** · 2026-05-22 · Working title: `mnemos`

## Purpose

Build a local-first, MCP-server-based persistent memory and continuous-learning system for AI tools (Claude Code, Gemini CLI, Codex, Antigravity, Hermes Agent, Openclaw, and any MCP-aware or HTTP-capable client). One shared memory layer across tools, with an Obsidian-like desktop UI for inspection, management, and visualization. Local storage primary; cloud sync optional.

## Goals

- **Cross-tool persistence** — single memory store shared across every AI tool the user runs.
- **Semantic recall** — hybrid retrieval (BM25 + dense vectors + entity-graph PPR) with explainable scoring.
- **Continuous learning** — async extraction of durable facts from sessions; importance-triggered reflection; auto-promotion of high-confidence patterns.
- **Knowledge graph** — typed entity relationships with bi-temporal validity, community detection for global queries.
- **Local-first** — runs entirely on a laptop with no cloud dependency; optional file or DB sync.
- **Distinctive UI** — Obsidian-style file/tag/graph navigation plus memory-specific views (bi-temporal timeline, PPR overlay, pipeline status, inspector).
- **Files as source of truth** — every memory is a human-readable markdown file with YAML frontmatter; the DB is a derived, rebuildable index.

## Non-goals (v1)

- Multi-user / multi-tenant operation (single user only).
- Mobile clients (laptop / desktop only).
- Hard real-time replication across machines (file-sync latency is acceptable).
- Per-memory access control (out of scope for personal use).
- Built-in voice / image embeddings (extensible later via providers).

## Research foundation

The design synthesizes the most effective techniques from a survey of existing memory systems and academic work:

- **Pinecone** — cascading retrieval (BM25 + dense in parallel, then rerank).
- **Letta / MemGPT** — CoALA-style tiered memory (working / episodic / semantic / procedural) with explicit promotion.
- **Zep / Graphiti** — bi-temporal knowledge graph; invalidate-don't-delete on contradiction; 94.8% on DMR.
- **mem0** — async ADD/UPDATE/DELETE/NOOP resolver pipeline (but its extraction-only design loses recall vs verbatim — we keep both).
- **HippoRAG** — neuroscience-inspired Personalized PageRank retrieval over an entity graph; multi-hop in one pass; +20% over SOTA RAG on multi-hop QA.
- **Microsoft GraphRAG** — hierarchical Leiden community detection + LLM-written community summaries for global queries; ~70-80% win rate vs naive RAG.
- **Hindsight** — separation of world facts, agent experiences, summaries, and beliefs into distinct stores.
- **OpenViking + Claude Code memory** — filesystem-as-substrate; greppable / git-able / portable.
- **RetainDB / MemPalace** — verbatim-first preservation outperforms extraction-only on conversational benchmarks (88% vs 30-45%).
- **Park et al. Generative Agents** — importance-triggered reflection; ablation collapsed agent coherence in 48 simulated hours.
- **Ebbinghaus forgetting curve** — strength score with exponential decay; resets on retrieval (use-it-or-lose-it).

Storage layer rejected and accepted candidates:

- **libSQL (Turso) + sqlite-vec + FTS5** — chosen. Single embedded DB, microsecond reads, native cloud-sync escape hatch, BM25+vector hybrid in one place. LanceDB sidecar reserved for >2M vectors.
- **Kuzu** — rejected: archived October 2025.
- **Neo4j Community** — rejected: GPLv3 viral license.
- **DuckDB + VSS** — rejected: HNSW persistence flagged experimental, corruption risk.
- **Chroma embedded** — rejected: multi-user OOM at scale, weaker concurrency.
- **Postgres + pgvector + AGE** — rejected: requires running a daemon, violates local-first ergonomic.

## Architecture overview

Three-process model centered on a single Rust daemon:

```
┌──────────────────────────┐      ┌──────────────────────────┐
│  AI Tools (clients)      │      │  Mnemos Desktop (Tauri)  │
│  Claude Code, Gemini,    │      │  React + TS frontend     │
│  Codex, Antigravity,     │      │  Graph view, editor,     │
│  Hermes Agent, Openclaw  │      │  timeline, search        │
└─────────────┬────────────┘      └─────────────┬────────────┘
              │ MCP (HTTP / stdio)              │ HTTP + WebSocket
              │ REST / CLI                      │
              └────────────────┬────────────────┘
                               ▼
                    ┌──────────────────────────┐
                    │ mnemosd (Rust daemon)    │
                    │ ───────────────────────  │
                    │ MCP server               │
                    │ REST + WebSocket         │
                    │ Pipeline workers (tokio) │
                    │ Retrieval engine         │
                    └────────────┬─────────────┘
                                 │
           ┌─────────────────────┼─────────────────────┐
           ▼                     ▼                     ▼
   ┌───────────────┐    ┌───────────────────┐  ┌────────────────┐
   │ Markdown      │    │ libSQL +          │  │ Optional:      │
   │ files (source │    │ sqlite-vec + FTS5 │  │ LanceDB sidecar│
   │ of truth)     │    │ (derived index)   │  │ (>~2M vectors) │
   └───────────────┘    └───────────────────┘  └────────────────┘
                                 │
                                 │ optional sync
                                 ▼
                    ┌────────────────────────────┐
                    │ Turso libSQL cloud,        │
                    │ git remote, Syncthing,     │
                    │ or S3-compatible backend   │
                    └────────────────────────────┘
```

### Core principles

1. **Files are the source of truth.** Every memory is one markdown file with YAML frontmatter. The SQLite/libSQL index is derived and reconstructible from `mnemos rebuild`.
2. **One daemon, four surfaces.** `mnemosd` exposes MCP (HTTP + stdio), REST + WebSocket, and a CLI — all over a single core. No duplicated business logic.
3. **Desktop UI is optional.** Headless installs (server, or "I only use it from Claude Code") never need Tauri. The daemon ships standalone.
4. **Everything async.** Ingestion never blocks the calling agent. Extraction, embedding, entity linking, resolution, reflection, decay, and community detection are all background pipelines.
5. **No single point of failure in retrieval.** BM25, dense vector, and graph PPR are independent — any one can be down and recall still works.
6. **Graceful LLM degradation.** Pipelines prefer MCP sampling (uses the calling client's LLM); fall back to user-configured API; fall back to local Ollama.

### Default paths (XDG-compliant)

| Path | Contents |
|---|---|
| `~/.local/share/mnemos/files/` | Markdown source of truth (tier subdirectories) |
| `~/.local/share/mnemos/index.db` | libSQL/SQLite index |
| `~/.config/mnemos/config.toml` | Settings (LLM providers, decay, sync) |
| `~/.config/mnemos/token` | Daemon auth token (mode 0600) |
| `~/.local/state/mnemos/logs/` | Daemon logs |

Default daemon port: `localhost:7423` (HTTP serving both MCP-HTTP and REST). MCP-stdio is a thin subprocess wrapper that proxies to HTTP.

### Integration surfaces (`mnemosd`)

| Surface | Transport | Used by |
|---|---|---|
| MCP Streamable HTTP | `POST /mcp` | Claude Code, Gemini CLI, Codex, Antigravity, any MCP-native client |
| MCP stdio | subprocess | Legacy MCP clients that prefer stdio |
| REST + WebSocket | `/v1/*`, `/v1/events` | Desktop UI, Hermes Agent, scripts, custom integrations |
| CLI | `mnemos` binary | Openclaw, shell scripts, CI, manual ops |

Each surface is a thin transport over the shared `mnemos_core` crate. Reference adapters for Claude Code, Gemini CLI, Codex, Hermes Agent, Openclaw, and generic-MCP ship under `adapters/`.

## Storage & memory model

### Four-tier file layout (plus reflections and entities)

```
~/.local/share/mnemos/files/
├── working/           # Tier 1: always loaded into agent context (~10 KB cap)
│   ├── identity.md
│   └── projects.md
├── episodic/          # Tier 2: verbatim session log, date-partitioned
│   └── 2026/05/22/session-<ulid>.md
├── semantic/          # Tier 3: extracted atomic facts (one file per fact)
│   └── <entity-slug>/<fact-id>.md
├── procedural/        # Tier 4: rules and preferences (don't decay; edited explicitly)
│   ├── coding-style.md
│   └── git-workflow.md
├── reflections/       # Synthesized summaries (typed: preference|pattern|insight|decision|community)
├── entities/          # Entity profile pages — Obsidian-friendly
└── quarantine/        # Malformed files set aside for user review
```

### Memory file format

```markdown
---
id: mem_01HX...
tier: semantic
type: fact                       # fact|episode|reflection|rule|identity|project|entity|community_summary
title: User prefers Tauri over Electron
tags: [tech-pref, desktop-app]
entities: [tauri, electron]
links: [mem_01HY..., mem_01HZ...]
provenance:
  - session: sess_01HA...
    chunks: [chunk_01HB..., chunk_01HC...]
created_at: 2026-05-22T14:30:00Z
ingested_at: 2026-05-22T14:30:05Z
valid_at: 2026-05-22T14:30:00Z   # when fact became true in the world
invalid_at: null                  # null = still valid
superseded_by: null
strength: 1.0                     # 0..1, Ebbinghaus decay
importance: 0.7                   # 0..1, set at creation, doesn't decay
last_accessed: 2026-05-22T14:30:00Z
access_count: 0
mnemos_version: 1
---

User prefers Tauri over Electron because the install footprint is smaller
and they value the Rust ecosystem.

## Evidence
- Quoted statement: "I want a Rust daemon + Tauri (React/TS) UI"
- Single-binary distribution rationale
```

### Tier semantics

| Tier | In context by default? | Decays? | Created by | User edits? |
|---|---|---|---|---|
| Working | Yes (size-capped) | No | Promotion from semantic, manual edits | Yes — explicit |
| Episodic | No — retrieved | No (deletion-only) | Auto-ingest from sessions | Rare — usually delete only |
| Semantic | No — retrieved | Yes (Ebbinghaus on `strength`) | Extraction pipeline | Yes — review/edit |
| Procedural | No — retrieved when relevant | No | Manual or reflection promotion | Yes — explicit |
| Reflection | No — retrieved | Slower decay than semantic | Reflection pipeline | Rare — view/delete |

### Bi-temporal model

Every memory and every entity edge carries both transaction time (`ingested_at`) and valid time (`valid_at` / `invalid_at`). On contradiction the prior record's `invalid_at` is set to the new record's `valid_at` and `superseded_by` is set — the old record is never deleted. This preserves audit history ("what did I believe last March?") and enables time-travel queries.

### libSQL/SQLite schema (highlights)

```sql
CREATE TABLE memories (
  id TEXT PRIMARY KEY,           -- ULID
  tier TEXT NOT NULL,            -- working|episodic|semantic|procedural|reflection
  type TEXT NOT NULL,
  title TEXT NOT NULL,
  body TEXT NOT NULL,            -- denormalized for retrieval
  file_path TEXT NOT NULL UNIQUE,
  content_hash TEXT NOT NULL,    -- detect external edits
  created_at TIMESTAMP NOT NULL,
  ingested_at TIMESTAMP NOT NULL,
  valid_at TIMESTAMP NOT NULL,
  invalid_at TIMESTAMP,
  superseded_by TEXT,
  strength REAL NOT NULL DEFAULT 1.0,
  importance REAL NOT NULL DEFAULT 0.5,
  last_accessed TIMESTAMP NOT NULL,
  access_count INTEGER NOT NULL DEFAULT 0,
  version INTEGER NOT NULL DEFAULT 1
);

CREATE VIRTUAL TABLE memory_vec USING vec0(
  memory_id TEXT PRIMARY KEY, embedding FLOAT[768]
);
CREATE VIRTUAL TABLE memory_fts USING fts5(
  memory_id UNINDEXED, title, body,
  content=memories, content_rowid=rowid
);

CREATE TABLE chunks (
  id TEXT PRIMARY KEY, session_id TEXT NOT NULL,
  speaker TEXT, ordinal INTEGER NOT NULL, body TEXT NOT NULL,
  created_at TIMESTAMP NOT NULL,
  source_tool TEXT, source_meta TEXT
);
CREATE VIRTUAL TABLE chunk_vec USING vec0(chunk_id TEXT PRIMARY KEY, embedding FLOAT[768]);
CREATE VIRTUAL TABLE chunk_fts USING fts5(chunk_id UNINDEXED, body, content=chunks, content_rowid=rowid);

CREATE TABLE memory_chunks (memory_id TEXT, chunk_id TEXT, PRIMARY KEY(memory_id, chunk_id));

CREATE TABLE entities (
  id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE,
  type TEXT NOT NULL,
  aliases TEXT,                  -- JSON
  description TEXT, file_path TEXT, created_at TIMESTAMP NOT NULL
);
CREATE TABLE entity_mentions (
  memory_id TEXT, entity_id TEXT, PRIMARY KEY(memory_id, entity_id)
);
CREATE TABLE entity_edges (
  id TEXT PRIMARY KEY,
  source_entity_id TEXT NOT NULL, target_entity_id TEXT NOT NULL,
  relation TEXT NOT NULL,
  created_at TIMESTAMP NOT NULL,
  valid_at TIMESTAMP NOT NULL, invalid_at TIMESTAMP,
  weight REAL NOT NULL DEFAULT 1.0,
  source_memory_ids TEXT         -- JSON
);

CREATE TABLE memory_links (
  source_id TEXT, target_id TEXT, type TEXT,
  PRIMARY KEY(source_id, target_id, type)
);
CREATE INDEX idx_links_target ON memory_links(target_id);

CREATE TABLE sessions (
  id TEXT PRIMARY KEY, source_tool TEXT, workspace TEXT,
  started_at TIMESTAMP, ended_at TIMESTAMP, summary TEXT
);

CREATE TABLE audit_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts TIMESTAMP NOT NULL, actor TEXT NOT NULL,
  action TEXT NOT NULL,
  memory_id TEXT, details TEXT
);
-- Audit log is append-only via SQL trigger blocking UPDATE/DELETE.
```

### File ↔ DB synchronization

- **Daemon-initiated writes**: atomic file write (temp + rename) → DB row update in same transaction → content hash recorded.
- **External edits** (user edits in Obsidian/vim): `notify` crate file watcher detects → re-parses frontmatter + body → updates DB row → re-embeds → re-indexes BM25.
- **Drift recovery**: `mnemos rebuild` walks files dir, reconstructs the entire index. DB is treated as a cache.
- **Concurrent edit**: file always wins (source of truth). Audit log records the override.

### Strength + Ebbinghaus decay

On every retrieval that returns a memory: `strength = min(1.0, strength + access_boost)` and `last_accessed = now()`. Hourly background worker:

```
strength = strength * exp(-base_decay_rate * (1 - importance) * hours_elapsed)
```

High-importance memories decay slower. Strength below `strength_threshold` (default 0.1) → soft-invalidate (excluded from default retrieval, audit entry, frontmatter update). Soft-invalidated >30 days → eligible for archival to `archived/` subdirectory. Hard delete only via explicit user action.

## Pipelines

Eight async pipelines run inside `mnemosd`, each its own tokio task with its own dead-letter queue. None of them block the calling agent.

```
client.remember(text)
    │
    ▼  (sync, <10ms)
  Ingest ─► chunks table + audit log ─► return ID
    │
    │  (async tasks enqueued)
    ▼
  Embed ──────────────► chunk_vec
    │
    ▼
  Extract (LLM) ──────► candidate atomic facts
    │
    ▼
  Resolve (LLM) ──────► ADD / UPDATE / DELETE / NOOP
    │                   └─► memories + memory files + provenance
    ▼
  Entity-link (LLM/NER) ► entities + entity_mentions
    │
    ▼
  Graph-update (LLM) ──► entity_edges (bi-temporal)
```

Scheduled:

```
  Decay worker (hourly)             ──► strength decay; soft-invalidate at threshold
  Reflection (salience / daily / manual) ──► typed reflection memories linked to sources
  Community detection (daily)       ──► Leiden over entity graph + LLM community summaries
```

### LLM provider priority

Per pipeline, configurable:

1. **MCP sampling** — if the calling client advertises `sampling/createMessage` (Claude Code does), the daemon asks the client to do the LLM call. Free piggyback on the model the user is already using.
2. **User-configured API** — OpenAI / Anthropic / Google / OpenRouter from `config.toml`.
3. **Local Ollama** — default fallback. Defaults: extractor `qwen2.5:7b` (or `gpt-oss-20b` if RAM allows), embedder `nomic-embed-text` (768d), reranker `bge-reranker-base` via ONNX runtime.

### Per-pipeline detail

- **Ingestion** — Validates input, mints ULID, writes chunk + audit, enqueues async work, returns. Hard p99 latency budget: 10 ms.
- **Embedding** — Batches every 250 ms to amortize model latency. Retries with backoff; DLQ after 5 failures. Retrieval still works on BM25 alone if embedding is broken.
- **Extraction** — Triggered per N chunks OR on session end OR after T seconds of inactivity. LLM prompt: extract atomic facts in subject-predicate-object form with tags, entities, importance 0-1, valid_at.
- **Resolution** — Embed candidate fact → KNN top-K similar existing → LLM decides ADD/UPDATE/DELETE/NOOP with reason. Execute:
  - ADD: new memory file + DB row + provenance to source chunks
  - UPDATE: new file written; old `invalid_at = new.valid_at`, `superseded_by = new.id`; `memory_links(supersedes)` row
  - DELETE: target memory's `invalid_at = now()` (soft); audit entry
  - NOOP: discard candidate; bump target's access counter, reset decay clock
- **Entity linking** — Extract mentions (LLM or distilled NER), fuzzy match against `entities` by name + alias + embedding similarity (threshold 0.85), create entity if no match. Conservative — false-merge worse than false-split (mergeable later in UI).
- **Graph update** — LLM extracts typed relationships from memory body + linked entities; existing edges between the same pair are checked for contradiction and invalidated if needed.
- **Decay** — Hourly. Updates `strength` per the formula above. Soft-invalidates at threshold.
- **Reflection** — Triggers: salience accumulator threshold (default 5), daily at 3am (configurable), manual via `reflect` tool. LLM identifies typed reflections (`preference`, `pattern`, `insight`, `decision`); written as reflection memories with `memory_links(reflects_on)` back to sources. High-confidence preferences may be auto-promoted to procedural tier with UI notification.
- **Community detection** — Daily. Loads `entity_edges`, runs hierarchical Leiden (`petgraph` + `leiden_clustering`). LLM writes community summaries; stored as `community_summary`-typed reflection memories. Used for global-mode retrieval.

### Session boundaries

Three ways the daemon knows a session starts/ends:
- Explicit: `start_session(tool, workspace, meta)` + `end_session(session_id, summary?)` MCP tools
- Implicit: >15-minute gap since last chunk → new session
- `end_session` triggers extraction over the full session and writes the session summary

### Failure isolation

Each pipeline has its own DLQ. Extraction breaking ≠ retrieval breaking. Per-pipeline status surfaced in UI under Pipelines tab: green/yellow/red, queue depth, DLQ inspector, manual replay.

### Backpressure

If extraction queue depth exceeds `max_queue_depth` (default 1000), `remember()` still succeeds but warns in logs. Workers scale parallelism based on queue depth.

## Retrieval engine

Two modes: **local** (specific fact lookup) and **global** (aggregative). Mode is auto-classified by a cheap regex+keyword classifier, with a small LLM fallback if uncertain.

### Local mode — hybrid retrieval

```
                            query
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
        ┌──────────┐    ┌──────────┐   ┌──────────────┐
        │  BM25    │    │  Dense   │   │  Graph PPR   │
        │  (FTS5)  │    │ (vec0)   │   │ (HippoRAG)   │
        │  top 5k  │    │  top 5k  │   │  top 5k      │
        └────┬─────┘    └────┬─────┘   └──────┬───────┘
             └───────────────┼─────────────────┘
                             ▼
                  ┌──────────────────────┐
                  │  RRF fusion (k=60)   │
                  └──────────┬───────────┘
                             ▼
                  ┌──────────────────────┐
                  │  Reweighting:        │
                  │   × recency_decay    │
                  │   × (1 + importance) │
                  │   × strength         │
                  │   × tier_weight      │
                  └──────────┬───────────┘
                             ▼
                  ┌──────────────────────┐
                  │  Bi-temporal +       │
                  │  tier filter         │
                  └──────────┬───────────┘
                             ▼
                  ┌──────────────────────┐
                  │  Cross-encoder       │
                  │  rerank top-50 → k   │
                  └──────────┬───────────┘
                             ▼
                       Hydrate + return
```

**BM25 path** — FTS5 over `memory_fts` ∪ `chunk_fts`. Top 5k by BM25.

**Dense path** — Embed query (same model as ingestion), KNN over `memory_vec` ∪ `chunk_vec` using `vec_distance_cosine`.

**Graph PPR path (HippoRAG)** — The differentiator:
1. Identify seed entities in the query (LLM extraction or substring match against `entities.name` + aliases).
2. Personalized PageRank on `entity_edges`, seeded on those entities (`α=0.85`, ~30 iterations, `petgraph`).
3. Fetch memories that mention top entities, weighted by entity PPR mass.
4. Returns associatively-close memories — multi-hop discovery in a single pass.

### RRF fusion

```
score(m) = Σ over retrievers i:  1 / (60 + rank_i(m))
```

k=60 canonical. RRF discards raw scores so we don't have to normalize between cosine, BM25, and PPR mass.

### Reweighting

```
final_score = rrf_score
            × exp(-recency_decay · age_days)
            × (1 + importance)
            × strength
            × tier_weight[tier]
```

Default tier weights: `working=2.0, procedural=1.5, reflection=1.2, semantic=1.0, episodic=0.8`.

### Filtering

- Bi-temporal: drop `invalid_at IS NOT NULL` unless `include_invalid` or `as_of: <ts>` specified.
- Tier filter: explicit inclusion list.
- Time range: `created_at` BETWEEN.
- Source filter: tool / workspace / session.

### Cross-encoder rerank

Top-50 → bge-reranker-base via ONNX runtime → top-k. On by default; 5-15% nDCG gain consistently in hybrid systems.

### Global mode (GraphRAG-style)

For aggregative queries:
1. Embed query, KNN against `community_summary` reflection memories.
2. Top-k community summaries returned.
3. Optional traversal from matched community down to underlying entities and memories for citations.

### Working memory injection

Working tier is always available to the agent — small enough (<10 KB) that it can be prepended. MCP resource `mnemos://working` lets clients pull it explicitly; for tools that don't read resources, it's auto-prepended to `recall` results when called from a session-aware tool.

### Verbatim vs facts (anti-mem0 design)

Search both `chunk_fts/vec` (verbatim) and `memory_fts/vec` (extracted facts) on the BM25 and Dense paths. Extraction is lossy, so we keep both. The reranker handles dedup — when a fact and its source chunk both score high, the reranker usually prefers the fact; when they're paraphrases, only one wins.

### Explainability

Every returned hit carries an `explain` payload:

```json
{
  "memory_id": "mem_01HX...",
  "ranks": { "bm25": 3, "dense": 1, "graph_ppr": 12 },
  "rrf_score": 0.0429,
  "weights": { "recency": 0.85, "importance": 1.2, "strength": 0.9, "tier": 1.0 },
  "rerank_score": 0.873,
  "final_score": 0.873,
  "matched_entities": ["tauri", "rust"],
  "ppr_path": ["rust", "tauri", "desktop-app"]
}
```

The UI's Memory Inspector renders this fully.

### Side effects (async after recall)

- `last_accessed = now()`, `access_count++` on each returned memory
- `strength = min(1.0, strength + access_boost)`
- Audit entry: actor, query, returned_ids

### Latency budget

- p50: <80 ms · p99: <250 ms

### Configuration

```toml
[retrieval]
default_k          = 10
rrf_k              = 60
recency_decay      = 0.02     # per-day
importance_weight  = 1.0
tier_weights       = { working = 2.0, procedural = 1.5, reflection = 1.2, semantic = 1.0, episodic = 0.8 }
strength_threshold = 0.1
rerank_enabled     = true
rerank_model       = "bge-reranker-base"

[retrieval.graph]
ppr_alpha          = 0.85
ppr_iterations     = 30
seed_entity_max    = 5

[retrieval.modes]
auto_classify      = true
local_default      = true
```

## MCP server + AI tool integration

### MCP tool surface

```
Core memory
  remember(text, *, tier?, type?, tags?, importance?, source?, valid_at?)
  recall(query, *, k?, tiers?, mode?, time_filter?, workspace?, include_invalid?, rerank?)
  forget(memory_id, *, reason?)
  update_memory(memory_id, changes)
  get_memory(memory_id)
  list_memories(filter?)

Session lifecycle
  start_session(*, tool, workspace?, metadata?)
  add_chunk(session_id, *, speaker, body, ordinal?, meta?)
  end_session(session_id, *, summary?)

Reflection
  reflect(*, scope?, lookback?)
  list_reflections(filter?)

Entities & graph
  list_entities(filter?)
  get_entity(entity_id)
  entity_graph(entity_id, *, depth?)
  merge_entities(source_id, target_id)

Inspection
  search_chunks(query, *, k?)
  time_travel(query, as_of)
  audit(memory_id)
```

All tools are MCP-spec-compliant with JSON Schema input/output. Schemas live in `mnemos_core::schema` and are shared by REST handlers.

### MCP resources

```
mnemos://working
mnemos://memory/{id}
mnemos://session/{id}
mnemos://entity/{id}
mnemos://recent
mnemos://reflections/recent
mnemos://context/{workspace}
```

### MCP prompts

```
mnemos.context-for(workspace?)
  → working tier + matched procedural rules + recent reflections for this workspace,
    formatted as an injectable system prompt

mnemos.session-resume(prior_session_id)
  → summary of the prior session + open threads, for continuation
```

### REST API (1:1 mirror)

```
POST   /v1/memories                     remember
GET    /v1/memories/{id}                get_memory
PATCH  /v1/memories/{id}                update_memory
DELETE /v1/memories/{id}                forget (soft)
POST   /v1/memories/search              recall
POST   /v1/memories/time-travel         time_travel
GET    /v1/memories/{id}/audit          audit

POST   /v1/sessions                     start_session
POST   /v1/sessions/{id}/chunks         add_chunk
POST   /v1/sessions/{id}/end            end_session
GET    /v1/sessions/{id}

POST   /v1/reflections                  reflect
GET    /v1/reflections                  list_reflections

GET    /v1/entities
GET    /v1/entities/{id}
GET    /v1/entities/{id}/graph
POST   /v1/entities/merge

GET    /v1/working

WS     /v1/events
GET    /openapi.json
```

### CLI

```
mnemos remember "User prefers Tauri" [--tier semantic --tag tech-pref]
mnemos recall "what desktop framework" [--k 5 --workspace ~/code/foo]
mnemos sessions list | show <id> | end <id>
mnemos forget <id> [--reason "..."]
mnemos reflect [--lookback 24h]
mnemos entity list | show <id> | graph <id>
mnemos sync push | pull | status
mnemos rebuild
mnemos doctor
mnemos status
mnemos daemon start | stop | restart | logs
```

### Multi-tool coordination

Multiple clients running concurrently each open their own `session_id`; the daemon serializes writes (single SQLite writer), reads are fully concurrent. Working-tier updates propagate via WebSocket; clients without notification support re-read working at the start of each `recall`.

### Workspace scoping

Every memory has an optional `workspace` (absolute path or label). Recall returns workspace-tagged + globally-scoped memories. Global identity facts surface everywhere; project-scoped facts only inside their workspace.

### Authentication

Daemon listens on `127.0.0.1:7423`. All endpoints require `Authorization: Bearer <token>`; token stored in `~/.config/mnemos/token` mode 0600, written at install, rotatable from the UI. MCP stdio exempt (subprocess launched by same user).

### Reference adapters

```
adapters/
  claude-code/        # CLAUDE.md fragment + mnemos://context wiring
  gemini-cli/         # equivalent for Gemini
  codex/              # OpenAI Codex adapter
  hermes-agent/       # REST client + glue (Hermes Agent is not MCP-native)
  openclaw/           # CLI wrapper script
  generic-mcp/        # minimal MCP client example
  openai-functions/   # function-calling schema for OpenAI tool use
```

Each is <200 lines and serves as both a working integration and a copy-paste template.

## Desktop UI

### Tech stack

| Layer | Choice |
|---|---|
| Framework | Tauri 2.x |
| UI | React 18 + TypeScript (types codegen via `ts-rs` from Rust) |
| State | Zustand |
| Routing | TanStack Router |
| Styling | Tailwind CSS + custom design tokens |
| Markdown | CodeMirror 6 |
| Graph | Sigma.js (main) + react-force-graph (neighborhood widget) |
| Charts | Visx |
| Icons | Lucide (customized — no defaults) |
| WS | Native WebSocket |

### Layout (three-column Obsidian-style)

Top bar: command palette / global search / sync status / quick-add. Left sidebar: tier / tag / entity / recent / reflection browser. Center: view router (editor / graph / timeline / search / pipelines / audit / entity profile). Right sidebar: Memory Inspector (always visible).

### Ten core views

1. **Tier browser** — left sidebar; hierarchical tree by tier; tabs for tags / entities / recent.
2. **Memory editor** — CodeMirror 6; form-style frontmatter; wiki-link autocomplete on `[[mem_...`; auto-save; diff modal on external concurrent edit.
3. **Graph view** — Sigma.js; entity / memory / mixed modes; PPR overlay (animated mass propagation), community overlay (Leiden hulls with summaries), time overlay (bi-temporal fade in/out via slider).
4. **Timeline view** — Visx; horizontal zoomable; bi-temporal bars (`valid_at` → `invalid_at`) with `ingested_at` ticks; draggable "now" cursor for time-travel queries.
5. **Search view** — hybrid search UI; per-retriever rank bars; full filter set; results with explainability scores.
6. **Pipeline status** — per-pipeline cards (queue, throughput, errors, DLQ inspector with manual replay); 24h throughput sparkline.
7. **Memory inspector** — right sidebar; frontmatter; strength curve forecast; provenance chain; backlinks; outgoing links; per-event audit log; "why this matched" trace.
8. **Reflection viewer** — recent reflections grouped by trigger; "Promote to procedural" action for high-confidence preferences.
9. **Entity profile** — description, aliases, memories, edges; embedded neighborhood graph; merge action.
10. **Audit log view** — filterable timeline; export to CSV.

### Command palette (⌘K)

New memory, search memories, open graph, open entity / memory, reflect now, time-travel, run pipeline, rebuild, toggle inspector / nav, export view.

### Real-time updates

Single WebSocket to `ws://localhost:7423/v1/events`. Events: `memory.created/updated/invalidated`, `pipeline.*`, `reflection.completed`, `decay.tick`, `sync.*`. Optimistic UI on user edits; reconcile on event.

### Distinctive design (anti-slop discipline)

- **Typography**: Display = Fraunces; body = Source Serif 4; mono = JetBrains Mono. No Inter / Roboto / system-ui.
- **Color**: Warm off-white `#FAF9F6` (light) / deep blue-black `#0F1218` (dark). No purple/indigo accents. Tier-coded palette: working = warm amber, episodic = muted graphite, semantic = deep teal, procedural = brick red, reflection = sage.
- **Layout**: Asymmetric where it earns it — timeline and graph go full-bleed.
- **Bi-temporal as visual primitive** — invalidated memories shown dashed/faded with strikethrough title; "as-of" mode reskins UI subtly with cooler accent + "viewing 2026-03-15" pill.
- **Strength as ambient motion** — memories near soft-invalidation pulse subtly in the browser sidebar (sub-300 ms ease-out, opacity-only, honors `prefers-reduced-motion`).

### Settings

LLM providers (per-pipeline assignment), embedder + reranker config, decay parameters, reflection trigger config, sync backend, file paths, auth token rotation.

### First-run quality-of-life

First-run wizard detects installed AI tools and drops integration fragments; detects Ollama and offers to pull `nomic-embed-text`. Doctor view runs `mnemos doctor` from inside the UI. Full vault export/import as zip.

## Operations

### Cloud sync (optional, three file-sync backends + optional DB layer)

Files are the durable record. Sync the files; rebuild the DB from files on each machine.

| Backend | Best for | Mechanism |
|---|---|---|
| **Git remote** | Power users; auditability; collaboration | Periodic commits, push/pull; custom merge driver for memory frontmatter |
| **Filesystem sync** (Syncthing / Dropbox / iCloud / OneDrive) | Lowest friction | Daemon points files dir at synced location; detects sync-conflict files |
| **S3-compatible** | Server installs, NAS, B2/MinIO | Rclone-style periodic upload + pull-on-start |

**Optional DB-layer sync via Turso libSQL embedded replicas** layered on top — sub-second cross-machine propagation. Files remain durable; Turso is fresh-read acceleration.

**New machine bootstrap**:

```
mnemos init --files-dir ~/Sync/mnemos-vault
mnemos rebuild
mnemos daemon start
```

### Error handling — graceful degradation as a design property

No single point of failure in retrieval. Any one retriever (BM25, dense, PPR) can be down and recall still works.

| Failure | Effect | Recovery |
|---|---|---|
| LLM provider down | Extract / resolve queue; recall unaffected (LLM-free retrieval path) | Retry with backoff; fall to next configured provider |
| Embedder down | New memories miss vectors; BM25 + PPR continue | Backfill on recovery |
| Reranker down | Skip rerank, use post-RRF + reweight | Transparent |
| Vector index corrupt | BM25 + PPR continue | `mnemos rebuild --vec` |
| DB corruption | `mnemos rebuild` reconstructs from files | <1 min per 10k memories |
| FS error | Audit captures; quarantine | UI surfaces; user resolves |
| Malformed external edit | Quarantine to `files/quarantine/<id>.md`; UI prompt | User fixes in-app |

Every durability-affecting failure writes to audit log and surfaces in pipeline tab.

### Security

- Token auth on all daemon endpoints (Section 5); MCP stdio exempt.
- File modes: files `0600`, DB `0600`, config `0600`, credentials `0700/0600`.
- **Secret detection at ingestion** — pattern scan for AWS keys, JWT, SSH private keys, generic high-entropy strings, OpenAI/Anthropic key prefixes. Hit → quarantine + prompt user via UI. Configurable allowlist.
- **Audit log append-only** at SQL trigger level. Purge requires explicit CLI with confirmation.
- **Optional encrypt-at-rest** — opt-in: libSQL native encryption + `age` for files. Breaks Obsidian-direct editing; off by default.
- **No network egress by default.** All LLM/embedding calls go through configured providers (local Ollama default). Cloud sync is opt-in.

### Observability

- Structured logs via `tracing` crate, JSON to `~/.local/state/mnemos/logs/`.
- `GET /v1/metrics` Prometheus endpoint, off by default.
- Trace IDs threaded through pipelines.
- No upstream telemetry. Opt-in anonymous usage metrics only.

### Testing strategy

| Layer | Scope | Coverage target |
|---|---|---|
| Unit (`mnemos_core`) | Schema migrations, RRF math, bi-temporal logic, decay math, resolver decision tree | >80% |
| Integration | Full pipeline with recorded LLM transcripts; concurrent writes; file watcher; `rebuild` parity; sync round-trip | All happy paths + 2+ failure paths each |
| E2E (Tauri WebDriver) | First-run, CRUD via UI, graph rendering, time-travel, conflict modal | All ten views smoke-tested |
| LLM eval suite (nightly) | Extraction P/R, resolution F1, retrieval on LongMemEval + custom personal set, mode classifier accuracy | Regression gate: >5% drop fails build |

### Performance targets (100k memories + 1M chunks)

| Operation | p50 | p99 |
|---|---|---|
| Recall (full pipeline) | 80 ms | 250 ms |
| Recall (no rerank) | 50 ms | 150 ms |
| Ingest ack (sync) | 3 ms | 10 ms |
| Async pipeline per chunk | 1.5 s | 5 s |
| Decay sweep (full) | <5 s | <15 s |
| Index rebuild | ~30 s per 10k memories | n/a |
| DB size | ~500 MB | n/a |
| Files size | ~1 GB | n/a |
| Embedding storage | ~300 MB | n/a |

Above ~2M vectors total → enable LanceDB sidecar for `chunk_vec`.

### Project structure

```
mnemos/
├── Cargo.toml                       # workspace root
├── LICENSE                          # Apache-2.0
├── README.md
├── crates/
│   ├── mnemos_core/                 # schema, types, retrieval, pipelines, traits
│   ├── mnemos_daemon/               # long-running daemon binary (MCP+REST+WS+workers)
│   ├── mnemos_cli/                  # CLI binary
│   ├── mnemos_sync/                 # turso, git, s3 backends
│   └── mnemos_providers/            # ollama, openai, anthropic, mcp-sampling, onnx-reranker
├── ui/                              # Tauri 2.x desktop app
│   ├── src-tauri/
│   ├── src/                         # React + TS (components, views, stores, api, design-tokens)
│   └── tauri.conf.json
├── adapters/                        # reference integrations
│   ├── claude-code/
│   ├── gemini-cli/
│   ├── codex/
│   ├── hermes-agent/
│   ├── openclaw/
│   └── generic-mcp/
├── docs/                            # architecture, ADRs, design docs (this file)
├── eval/                            # LLM eval suite (datasets, runners)
├── examples/                        # sample vaults, workflows
└── scripts/                         # dev tooling
```

### Packaging and distribution

| Target | Mechanism | Contains |
|---|---|---|
| macOS / Windows / Linux desktop | Tauri build (`.dmg` / `.msi` / `.deb` / `.AppImage`) | UI + daemon + CLI |
| Headless server | `cargo install`, `brew`, deb/rpm | Daemon + CLI |
| Docker | Official image `mnemos/mnemos:latest` | Daemon |
| systemd | Shipped unit file | Daemon |
| macOS / Windows autostart | `launchctl` plist; Windows service via Tauri sidecar | Daemon |

GitHub Releases publishes signed binaries for every target per tag.

### Versioning and migrations

- Semver on `mnemos_core::schema`. Breaking SQL change → new migration step + per-version migrator.
- Frontmatter `mnemos_version: <N>`; daemon migrates files on read after upgrade.
- Roll-forward only. Downgrade: `mnemos export --to-version N` + reinstall.

## Open decisions (for the implementation plan)

These were either deferred or are operational choices that don't affect the architecture:

- Final name (working title: `mnemos`). Rename is mechanical.
- Default extraction model (`qwen2.5:7b` vs `gpt-oss-20b`) — RAM-dependent, decide in first-run wizard.
- Default embedding dimension (768 vs 1024) — locks the vector schema. Default 768 (`nomic-embed-text`); migration available.
- Exact reflection trigger threshold defaults — tune from real usage data after MVP.
- Whether to ship Codex adapter day one or after MVP (depends on Codex's MCP support state).

## Out of scope (v1)

- Multi-user / shared vault collaboration semantics.
- Mobile clients.
- Voice or image embeddings (extensible later via providers crate).
- Per-memory ACL.
- Cross-vault federation.

## Success criteria

The v1 ships when:

1. All four AI tool integration surfaces (MCP-HTTP, MCP-stdio, REST+WS, CLI) work end-to-end against reference adapters for Claude Code, Gemini CLI, and one non-MCP client (Hermes Agent or Openclaw).
2. A user can: open Claude Code, have a conversation, close the session, and recall facts from that conversation in a later Gemini CLI session.
3. The desktop UI renders all ten core views with a 5k-memory fixture vault.
4. The bi-temporal time-travel slider correctly reproduces historical state.
5. The full LLM eval suite passes (extraction P>0.8, resolution F1>0.85, retrieval beats BM25-only baseline by >15% on the personal eval set).
6. `mnemos rebuild` reconstructs the index from files to a state byte-identical to a fresh ingestion.
7. The daemon survives 24 hours of continuous load (1 chunk/second) with stable memory and queue depth.
8. End-to-end performance targets at 100k memories met within 25% tolerance.
