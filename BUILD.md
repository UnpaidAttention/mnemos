# Building Mnemos from source

This document describes how to build Mnemos locally on each supported
platform. CI does the same on tag push; this page is for development.

## Prerequisites

All platforms:
- **Rust** (stable, ≥ 1.78). Install via [rustup](https://rustup.rs).
- **Node.js** 20+ and **pnpm 9+** (for the desktop frontend).

Linux (Debian/Ubuntu):

```
sudo apt-get install -y \
  libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev \
  librsvg2-dev libappindicator3-dev patchelf libssl-dev
```

Linux (Fedora/RHEL):

```
sudo dnf install -y \
  gtk3-devel webkit2gtk4.1-devel libsoup3-devel \
  librsvg2-devel libappindicator-gtk3-devel patchelf openssl-devel
```

macOS:
- Xcode command-line tools: `xcode-select --install`.

Windows:
- Microsoft Visual Studio Build Tools (Desktop development with C++).
- WebView2 Runtime (preinstalled on Windows 11; download from Microsoft for older).

## CLI + daemon only

```
cargo build --release -p mnemos_cli -p mnemos_daemon
./target/release/mnemos --help
./target/release/mnemosd --help
```

> The daemon binary is named `mnemosd` (per `crates/mnemos_daemon/Cargo.toml [[bin]]`). The CLI binary is `mnemos`.

## Bundled embedder

By default, Mnemos ships with `llama-server` (llama.cpp's HTTP server) and
a 22 MB `all-MiniLM-L6-v2` GGUF model. The daemon spawns `llama-server`
as a managed child process on startup; embeddings happen entirely locally.

The bundled embedder is included in the daemon `.deb` and `.rpm` packages
(installed to `/usr/lib/mnemos/`). It is **not yet bundled into the
desktop AppImage** — AppImage users who want the local embedder install
the daemon package separately, or fall back to Ollama / OpenAI.

### Refreshing the bundled assets

```
bash scripts/fetch-bundled-assets.sh
```

Pinned versions (edit the script to upgrade):
- llama.cpp: `b9400` (~17 KB binary + ~25 MB of .so libraries)
- Model: `all-MiniLM-L6-v2.Q8_0.gguf` (~22 MB, 384-dim, Apache-2.0)

The script downloads + extracts to `assets/`. Both `.deb`/`.rpm`
packaging and CI test runs consume the same `assets/` tree.

### Switching embedders

Set `MNEMOS_EMBEDDER` to one of `bundled` (default), `ollama`, `openai`,
`mock`, `none`. For NEW vaults, the env value is the default. For
EXISTING vaults, the vault's recorded embedder is authoritative — to
switch, run:

```
mnemos embed-rebuild --target bundled       # or ollama / openai / mock
```

The migration is atomic, resumable, and audit-logged. The old
embeddings are kept as a `memory_embeddings_v2_backup_<ts>` table for
7 days; drop manually after that, or wait for the future cleanup task.

### OpenAI backends

To use OpenAI embeddings or chat instead of local:

```bash
export OPENAI_API_KEY=sk-...
# Optional:
export OPENAI_BASE_URL=https://api.openai.com   # Azure: https://<resource>.openai.azure.com
export MNEMOS_EMBEDDER=openai
export MNEMOS_EMBEDDER_MODEL=text-embedding-3-small   # or -large for 3072d
export MNEMOS_LLM=openai
export MNEMOS_LLM_MODEL=gpt-4o-mini

mnemos daemon restart
```

For new vaults, the env value seeds `vault.embedder_kind`. To switch
an existing Ollama-seeded vault to OpenAI:

```
mnemos embed-rebuild --target openai
```

### Wrapper script

The bundled `llama-server` binary is dynamically linked against
`libllama.so` + several `libggml*.so` files. The `.deb`/`.rpm`
install a small wrapper at `/usr/bin/mnemos-llama-server` that sets
`LD_LIBRARY_PATH=/usr/lib/mnemos` before exec'ing the real binary.
The Mnemos daemon's `bundled_embedder` module prefers this wrapper
when present.

## Desktop app

From the repo root:

```
cd desktop
pnpm install
pnpm tauri dev          # development mode (hot reload)
pnpm tauri build        # production bundle (all platform targets)
pnpm tauri build --bundles deb,rpm,appimage      # Linux only
pnpm tauri build --bundles dmg,app               # macOS only
pnpm tauri build --bundles msi                   # Windows only
```

The `beforeBuildCommand` in `src-tauri/tauri.conf.json` automatically
stages the daemon + CLI binaries into `src-tauri/binaries/` via the
`build-sidecars.sh` script. The resulting bundles include them as
Tauri sidecars.

## Server-side .deb / .rpm packages

Independent of the desktop bundle, you can produce stand-alone CLI and
daemon packages for Linux servers:

```
cargo install cargo-deb cargo-generate-rpm --locked
bash scripts/prepare-linux-packages.sh
```

Outputs under `target/debian/` and `target/generate-rpm/`.

## Code-signing (not yet enabled)

The CI pipeline currently produces **unsigned** installers. To enable
signed builds:

### macOS

1. Enroll in the Apple Developer Program ($99/year).
2. Generate a "Developer ID Application" certificate and download it.
3. Add the following secrets to the GitHub repo:
   - `APPLE_CERTIFICATE` — base64-encoded `.p12` cert export
   - `APPLE_CERTIFICATE_PASSWORD` — its passphrase
   - `APPLE_SIGNING_IDENTITY` — e.g. `Developer ID Application: Your Name (TEAM123)`
   - `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` — for notarytool
4. Set `tauri.conf.json → bundle.macOS.signingIdentity` to the value of
   `APPLE_SIGNING_IDENTITY`.
5. The `release.yml` workflow already exposes these env vars to the
   macOS job; Tauri picks them up automatically.

### Windows

1. Acquire an Authenticode certificate (OV or EV from a trusted CA).
2. Add the following secrets:
   - `WINDOWS_CERTIFICATE` — base64-encoded `.pfx` cert
   - `WINDOWS_CERTIFICATE_PASSWORD` — its passphrase
3. Set `tauri.conf.json → bundle.windows.certificateThumbprint` to the
   cert's thumbprint.
4. Tauri picks the rest up at bundle time.

### Linux

Linux installers (.deb, .rpm, .AppImage) are typically distributed
unsigned. To GPG-sign for inclusion in an apt PPA or OBS project, see
`PACKAGING.md` § "Linux package repositories".

## Tauri updater signing key

The auto-update flow uses an ed25519 keypair (Tauri-specific, not GPG).
It is project-owned and lives in CI secrets.

To generate (one-time setup, or regenerate after a key compromise):

```
bash scripts/gen-updater-key.sh
# Paste the printed public key into tauri.conf.json → plugins.updater.pubkey
# (replacing the PLACEHOLDER_PUBLIC_KEY_REPLACE_BEFORE_RELEASE sentinel)
gh secret set TAURI_SIGNING_PRIVATE_KEY < desktop/src-tauri/updater-private.pem
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --body ""
# DO NOT COMMIT updater-private.pem (it's in .gitignore)
```

**Warning**: rotating the key invalidates all in-the-wild auto-updates
from older versions. Users on older releases must manually download the
next version. Don't rotate without a strong reason.

## Troubleshooting

**`pnpm tauri build` fails with WebKit/GTK error on Linux** — install
the missing -dev package the error names. WebKit2GTK 4.1 (not 4.0) is
required for Tauri 2.

**`cargo deb` complains about missing LICENSE** — confirm the repo root
has a `LICENSE` file. The `[package.metadata.deb] license-file` field
in each crate points at it.

**The sidecar binaries don't match the host architecture in the
bundle** — `build-sidecars.sh` reads the host triple from `rustc -vV`.
If you're cross-compiling, pass the target explicitly:
`bash desktop/src-tauri/build-sidecars.sh aarch64-apple-darwin`.

**AppImage build fails on newer Fedora / Arch with `strip` errors on
modern ELF section types** — Tauri's vendored linuxdeploy ships an
older binutils that rejects sections like `.relr.dyn`. The Linux .deb
and .rpm targets work fine; AppImage builds reliably on
`ubuntu-22.04` (which is what CI uses). On the developer's local
machine, prefer `pnpm tauri build --bundles deb,rpm` until upstream
linuxdeploy ships a newer binutils.

**CI matrix job times out at the bundle step** — Tauri Linux bundles
include a full WebKit runtime; this is normal. Use the `Swatinem/rust-cache`
hit rate as your indicator of regressions.
