# mnemos

Local-first, file-as-source-of-truth memory provider for AI tools. Plan 1
ships the CLI foundation — vectors, daemon, MCP, and UI come in later
plans.

## What works today (v0.1.0)

- `mnemos remember "<body>"` — store a memory (markdown file + DB + vector).
- `mnemos recall "<query>"` — hybrid retrieval (BM25 + dense via sqlite-vec, fused with RRF, re-weighted by recency/importance/strength/tier).
  - `--rerank` enables an optional cross-encoder reranker (requires `--features rerank-onnx` at build time + model files in `~/.local/share/mnemos/models/`).
  - `--explain` emits structured per-hit scoring breakdown.
- `mnemos embed status` — report how many memories are embedded.
- `mnemos embed backfill` — embed every memory missing a vector.
- `mnemos get <id>` / `mnemos list` / `mnemos forget <id>` — basic CRUD with
  bi-temporal soft invalidation.
- `mnemos rebuild` — reconstruct the DB index + vectors from files.
- `mnemos doctor` / `mnemos status` — diagnostics.

### Embedder selection (via env var)

| `MNEMOS_EMBEDDER` | Behavior |
|---|---|
| `ollama` (default) | Use Ollama at `http://localhost:11434`, model `nomic-embed-text` (768d). Override URL with `MNEMOS_OLLAMA_URL`; model with `MNEMOS_OLLAMA_MODEL`. |
| `mock` | Deterministic test embedder (768d by default; set `MNEMOS_EMBEDDER_DIM` to override). |
| `none` | No embedder; falls back to BM25-only retrieval. |

Pull the default Ollama model before first use:

```bash
ollama pull nomic-embed-text
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
