# mnemos

Local-first, file-as-source-of-truth memory provider for AI tools. Plan 1
ships the CLI foundation — vectors, daemon, MCP, and UI come in later
plans.

## What works today (v0.2.0)

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
