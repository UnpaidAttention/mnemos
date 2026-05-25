# Changelog

All notable changes to this project are recorded here.

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
