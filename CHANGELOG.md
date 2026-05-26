# Changelog

All notable changes to this project are recorded here.

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
