# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.9.x   | ✅         |
| 0.8.x   | ✅         |
| < 0.8   | ❌         |

## Reporting a Vulnerability

If you discover a security vulnerability in Mnemos, please report it responsibly:

1. **Do NOT** create a public GitHub issue.
2. Email your report to **security@mnemos.dev** (or the project maintainers) with:
   - A description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

We aim to acknowledge reports within **48 hours** and provide a fix or mitigation within **7 days** for critical issues.

## Security Model

Mnemos is designed as a **localhost-only** daemon:

- The daemon binds to `127.0.0.1:7423` (loopback only, never `0.0.0.0`).
- All API endpoints (except `/health`) require a **bearer token**.
- The token is a 32-byte random value stored at `~/.config/mnemos/token` with `0600` permissions.
- Token comparison uses **constant-time equality** to prevent timing attacks.
- WebSocket connections authenticate via query parameter (accepted risk: localhost-only traffic).
- CORS is restricted to Tauri webview origins and the local dev server.
- The config endpoint **masks secrets** (API keys, sync tokens) in GET responses.
- Request body size is globally capped at **10 MB** to prevent memory exhaustion.

## Dependencies

We recommend periodically running `cargo audit` to check for known vulnerabilities in Rust dependencies. Frontend dependencies should be checked with `npm audit`.
