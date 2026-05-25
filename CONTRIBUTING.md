# Contributing to mnemos

Mnemos is in early development. Before opening a PR:

1. `cargo fmt --all`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`

All three must pass. New features must come with tests — TDD is the rule, not the exception.

Commit messages follow `<type>: <subject>` form (`feat:`, `fix:`, `chore:`,
`docs:`, `test:`, `refactor:`). Reference the relevant Plan + Task in the
body when applicable.
