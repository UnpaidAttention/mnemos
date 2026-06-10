# Contributing to Mnemos

Mnemos is in early development. We appreciate contributions of all kinds.

## Getting Started

- **Build from source:** See [BUILD.md](BUILD.md) for prerequisites, build steps, and development setup.
- **Packaging:** See [PACKAGING.md](PACKAGING.md) for creating `.deb`, `.rpm`, and `.AppImage` packages.

## Before Opening a PR

1. `cargo fmt --all`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `cd desktop && pnpm test` (if you touched frontend code)

All checks must pass. New features must come with tests — TDD is the rule, not the exception.

## Commit Messages

Follow `<type>: <subject>` form:

- `feat:` — New feature
- `fix:` — Bug fix
- `refactor:` — Code restructuring (no behavior change)
- `docs:` — Documentation only
- `test:` — Adding or updating tests
- `chore:` — Build, CI, tooling changes

Reference the relevant Plan + Task in the body when applicable.

## Code Style

- Rust: follow `rustfmt` defaults and `clippy` lints.
- TypeScript/React: follow the existing ESLint + Prettier config in `desktop/`.
- Keep components under 150 lines. Extract shared logic into hooks or utilities.

## Reporting Issues

Use the [issue templates](https://github.com/UnpaidAttention/mnemos/issues/new/choose) to report bugs or request features.

## Security

See [SECURITY.md](SECURITY.md) for reporting vulnerabilities. **Do not** open a public issue for security bugs.
