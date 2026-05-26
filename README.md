# mnemos

Local-first, file-as-source-of-truth memory provider for AI tools. Plan 1
ships the CLI foundation — vectors, daemon, MCP, and UI come in later
plans.

## What works today (v0.0.1)

- `mnemos remember "<body>"` — store a memory (markdown file on disk + DB index).
- `mnemos recall "<query>"` — BM25 search via SQLite FTS5.
- `mnemos get <id>` / `mnemos list` / `mnemos forget <id>` — basic CRUD with
  bi-temporal soft invalidation.
- `mnemos rebuild` — reconstruct the DB index from files.
- `mnemos doctor` — detect file / DB drift, orphans, hash mismatches.
- `mnemos status` — vault summary.

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
