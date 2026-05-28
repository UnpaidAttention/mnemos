# Mnemos Plan 8 — Packaging, installers, auto-update (v0.7.0)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship installable mnemos for macOS, Linux, and Windows with an auto-updating desktop app and CLI/daemon packages distributable via apt and dnf — without paid signing certs.

**Architecture:** Tauri 2's bundler emits per-platform installers (.dmg + .app for macOS, .deb + .rpm + .AppImage for Linux, .msi for Windows). The desktop installer carries the daemon + CLI binaries as Tauri sidecars so one install gets everything. Independent .deb and .rpm packages for `mnemos_cli` + `mnemos_daemon` (no GUI) are built via `cargo-deb` and `cargo-generate-rpm` so Linux server users can `apt install mnemos`. A GitHub Actions matrix on tag push builds all three platforms in parallel, signs the Tauri updater manifest with a project-owned ed25519 key (kept in CI secrets), and uploads everything to GitHub Releases. Code-signing for the installers themselves is deferred — `BUILD.md` documents the manual notarization / Authenticode steps so a future signed release is a config delta, not a refactor.

**Tech Stack:** Tauri 2 (already in use), `cargo-deb 3`, `cargo-generate-rpm 0.16`, `@tauri-apps/plugin-updater` (Tauri 2 plugin), GitHub Actions, OBS / Launchpad PPA (documented, not auto-published).

---

## Plan sequence context

Plan 8 is the final increment before mnemos is shippable to non-developers.

- Plan 1 (v0.1.0) — core storage + CLI
- Plan 2 (v0.2.0) — daemon + REST + MCP
- Plan 3 (v0.2.5) — embeddings + reranker
- Plan 4 (v0.3.0) — async learning pipelines
- Plan 5 (v0.4.0) — graph intelligence (PPR + reflection + communities)
- Plan 6 (v0.5.0) — Tauri desktop app
- Plan 7 (v0.6.0) — cloud sync + settings + doctor + adapters
- **Plan 8 (v0.7.0) — packaging, installers, auto-update**

After Plan 8, mnemos is **shippable**: a user on macOS / Linux / Windows can download a single installer, get the desktop app + the daemon + the CLI, and the app keeps itself current.

---

## What this plan deliberately defers

| Item | Why deferred | Where it lands |
|---|---|---|
| Apple Developer notarization | Requires paid cert ($99/year) + provisioning profile | `BUILD.md` documents the manual steps; a future v0.7.x release re-runs CI with `APPLE_*` secrets set |
| Microsoft Authenticode signing | Requires paid cert ($200-700/year EV or OV) | Same — documented in `BUILD.md`, future delta |
| Linux package repo hosting (apt PPA / OBS rpm) | Requires Launchpad/OBS accounts the framework can't create itself | `PACKAGING.md` documents the submission flow; .deb + .rpm are still built and uploaded to GH Releases for direct `dpkg -i` / `rpm -i` |
| Homebrew tap | User explicitly opted out in Plan 8 scoping | Skip |
| Cargo crates.io publish | User opted out (the desktop app is the primary distribution) | Skip |
| Encrypt-at-rest, secret detection | Deferred from Plan 7 | Future |
| Turso embedded replicas wire-up | Deferred from Plan 7 (no test target) | Future |
| In-app update from inside the daemon (CLI/server users) | Out of scope — apt/dnf handles it | Document in `PACKAGING.md` |

---

## Hard prerequisites

The plan assumes:
- Plan 7 landed (`v0.6.0` tag local on master).
- Linux dev machine (matches the user's environment). `pnpm build` works.
- A GitHub repo exists (origin) with workflows enabled. CI minutes available.
- `cargo`, `pnpm`, `rustup` installed. The Tauri Linux prereqs (`libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libsoup-3.0-dev`, `librsvg2-dev`) are documented but not auto-installed by the plan.

The plan does NOT assume:
- Apple Developer account, Windows code-signing cert.
- A Launchpad PPA or OBS project (documented as a follow-up).
- macOS or Windows local dev — those builds happen entirely in GitHub Actions.

---

## File structure produced by this plan

```
.github/workflows/
  release.yml                       # NEW — tag-triggered build matrix
.github/
  RELEASE_TEMPLATE.md               # NEW — release notes template
crates/mnemos_cli/Cargo.toml        # MOD — add [package.metadata.deb] + [package.metadata.generate-rpm]
crates/mnemos_daemon/Cargo.toml     # MOD — same
desktop/src-tauri/
  tauri.conf.json                   # MOD — full bundle + updater config
  Cargo.toml                        # MOD — add tauri-plugin-updater
  src/lib.rs                        # MOD — register updater plugin
  capabilities/default.json         # MOD — allow updater
  icons/                            # NEW — icon set (icns / ico / png / svg source)
  resources/                        # NEW — sidecar binaries staged here at build time
  build-sidecars.sh                 # NEW — copies daemon + CLI binaries into resources/
desktop/src/components/
  UpdateBanner.tsx                  # NEW — auto-update notification UI
  UpdateBanner.test.tsx             # NEW
desktop/src/App.tsx                 # MOD — mount <UpdateBanner />
desktop/package.json                # MOD — add @tauri-apps/plugin-updater
scripts/
  gen-updater-key.sh                # NEW — local key-gen helper
  prepare-linux-packages.sh         # NEW — builds .deb + .rpm for CLI + daemon
  package-linux.sh                  # NEW — local-build .AppImage for desktop
BUILD.md                            # NEW — cross-platform build instructions
PACKAGING.md                        # NEW — release + distribution runbook
README.md                           # MOD — add "Install" section near the top
CHANGELOG.md                        # MOD — 0.7.0 entry
Cargo.toml                          # MOD — workspace version → 0.7.0
desktop/package.json                # MOD — version → 0.7.0
desktop/src-tauri/Cargo.toml        # MOD — version → 0.7.0
desktop/src-tauri/tauri.conf.json   # MOD — version → 0.7.0
```

---

## Conventions (same as Plans 1-7)

- Every task is one commit. Commit message format: `feat: <description> (Plan 8 Task N)` or `chore: ...` for release bumps.
- Workspace dependencies preferred; check root `Cargo.toml` before adding inline versions.
- No `dbg!` / `println!` (debug) in production code. Use the existing `tracing` setup.
- No emoji, no AI-slop language (no "Empower", "Seamless", "Unlock", etc.) in any documentation file.
- Documentation files are written for a competent dev who has never touched the repo. Each command is verbatim runnable; each section answers "what / why / how" in that order.
- Updater signing private key is NEVER committed. The .gitignore must list `desktop/src-tauri/updater-private.pem` and any `.tauri-key` files.

---

# Group A — Bundler config + icons + sidecars

## Task 1: Tauri bundler config + icon set

Replace the minimal `tauri.conf.json` with a full bundle spec: identifier, category, copyright, per-platform bundle settings (deb/rpm dependencies, MSI WiX template, macOS minimum version, AppImage args). Generate a placeholder icon set with `pnpm tauri icon` from a single source SVG so every required size exists.

**Files:**
- Modify: `desktop/src-tauri/tauri.conf.json`
- Create: `desktop/src-tauri/icons/mnemos.svg` (source)
- Create: `desktop/src-tauri/icons/32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico` (generated)

- [ ] **Step 1: Create source SVG** — `desktop/src-tauri/icons/mnemos.svg`. Brand-aligned (warm off-white background `#FAF9F6`, deep blue-black foreground `#0F1218`, geometric mark — a stylized "M" made of three vertical strokes of varying weight, evoking the tier hierarchy). Keep it simple and recognizable at 16×16.

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
  <rect width="256" height="256" rx="48" fill="#0F1218"/>
  <g fill="#FAF9F6">
    <!-- Three vertical strokes of decreasing weight, evoking tier hierarchy. -->
    <rect x="56"  y="56" width="28" height="144" rx="3"/>
    <rect x="114" y="74" width="20" height="126" rx="3"/>
    <rect x="164" y="92" width="14" height="108" rx="3" opacity="0.7"/>
    <!-- Connecting cross-bar near the top -->
    <rect x="56" y="56" width="122" height="14" rx="3" opacity="0.85"/>
  </g>
</svg>
```

- [ ] **Step 2: Generate the platform icons.** From `desktop/src-tauri/`:

```bash
cd desktop
pnpm tauri icon icons/mnemos.svg
```

This populates `icons/icon.ico`, `icons/icon.icns`, and the various PNG sizes. Commit ONLY the generated icons — `.svg` stays as the source.

- [ ] **Step 3: Rewrite `desktop/src-tauri/tauri.conf.json`** to the full bundle spec:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Mnemos",
  "version": "0.6.0",
  "identifier": "dev.mnemos.desktop",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "pnpm dev",
    "beforeBuildCommand": "pnpm build"
  },
  "app": {
    "windows": [
      {
        "title": "Mnemos",
        "width": 1440,
        "height": 900,
        "minWidth": 960,
        "minHeight": 600
      }
    ],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "targets": ["deb", "rpm", "appimage", "dmg", "app", "msi"],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "category": "DeveloperTool",
    "copyright": "© 2026 Mnemos contributors",
    "shortDescription": "Local-first AI memory provider",
    "longDescription": "Mnemos is a local-first AI memory provider for Claude Code and other AI tools. It stores memories as markdown files in your vault and indexes them with hybrid BM25 + dense + graph PPR search.",
    "homepage": "https://github.com/mnemos/mnemos",
    "externalBin": ["binaries/mnemos-daemon", "binaries/mnemos"],
    "resources": [],
    "linux": {
      "deb": {
        "depends": ["libwebkit2gtk-4.1-0", "libgtk-3-0"],
        "section": "utils",
        "priority": "optional"
      },
      "rpm": {
        "depends": ["webkit2gtk4.1", "gtk3"],
        "epoch": 0,
        "release": "1"
      },
      "appimage": {
        "bundleMediaFramework": false
      }
    },
    "macOS": {
      "frameworks": [],
      "minimumSystemVersion": "11.0",
      "exceptionDomain": "",
      "signingIdentity": null,
      "providerShortName": null,
      "entitlements": null
    },
    "windows": {
      "certificateThumbprint": null,
      "digestAlgorithm": "sha256",
      "timestampUrl": "",
      "wix": {
        "language": "en-US",
        "template": null
      },
      "nsis": null
    }
  }
}
```

The `version` stays at `0.6.0` for now — Task 15 bumps to `0.7.0` at the release commit.

- [ ] **Step 4: Verify** — run `pnpm tauri info` and confirm it parses without warnings. Then `pnpm tauri build --debug --no-bundle` to confirm the Rust side compiles (skip bundling for speed).

```bash
cd desktop
pnpm tauri info
pnpm tauri build --debug --no-bundle
```

- [ ] **Step 5: Commit.**

```bash
git add desktop/src-tauri/tauri.conf.json desktop/src-tauri/icons/
git commit -m "feat: full Tauri bundler config + icon set (Plan 8 Task 1)"
```

---

## Task 2: Sidecar staging — bundle daemon + CLI inside the desktop app

The `tauri.conf.json` from Task 1 declares `externalBin: ["binaries/mnemos-daemon", "binaries/mnemos"]`. Tauri expects these binaries staged at `desktop/src-tauri/binaries/{name}-{triple}` at bundle time. Add a small shell script + a Cargo build-script that handles cross-platform staging.

**Files:**
- Create: `desktop/src-tauri/build-sidecars.sh`
- Modify: `desktop/src-tauri/build.rs` (or create if absent)

- [ ] **Step 1: Create `desktop/src-tauri/build-sidecars.sh`**

```bash
#!/usr/bin/env bash
# Stages mnemos-daemon and mnemos (CLI) binaries into
# desktop/src-tauri/binaries/ with the triple-suffix Tauri expects.
#
# Usage:
#   build-sidecars.sh [target-triple]
# If no triple is passed, uses `rustc -vV | awk '/host/ {print $2}'`.
set -euo pipefail

cd "$(dirname "$0")/../.."  # workspace root

TARGET="${1:-$(rustc -vV | awk '/host/ {print $2}')}"
SUFFIX=""
[[ "$TARGET" == *windows* ]] && SUFFIX=".exe"

OUT="desktop/src-tauri/binaries"
mkdir -p "$OUT"

# Build the daemon + CLI in release mode for the target.
if [[ "$TARGET" == "$(rustc -vV | awk '/host/ {print $2}')" ]]; then
  cargo build --release -p mnemos_daemon -p mnemos_cli
  SRC="target/release"
else
  cargo build --release --target "$TARGET" -p mnemos_daemon -p mnemos_cli
  SRC="target/$TARGET/release"
fi

cp "$SRC/mnemos_daemon$SUFFIX" "$OUT/mnemos-daemon-$TARGET$SUFFIX"
cp "$SRC/mnemos$SUFFIX"        "$OUT/mnemos-$TARGET$SUFFIX"
echo "staged sidecars for $TARGET → $OUT/"
```

Make it executable: `chmod +x desktop/src-tauri/build-sidecars.sh`.

- [ ] **Step 2: Wire into `tauri.conf.json`** — modify the `beforeBuildCommand` to also stage sidecars before `pnpm build`:

In `tauri.conf.json`, change:
```json
"beforeBuildCommand": "pnpm build"
```
to:
```json
"beforeBuildCommand": "sh ../src-tauri/build-sidecars.sh && pnpm build"
```

> The relative path is `../src-tauri/` because `beforeBuildCommand` runs from the frontend dir (`desktop/`), not from `src-tauri/`.

- [ ] **Step 3: Update `.gitignore`** to ignore the staged sidecars (they're build artifacts):

```
desktop/src-tauri/binaries/
```

- [ ] **Step 4: Verify locally** — run a Linux bundle:

```bash
cd desktop
pnpm tauri build --bundles deb
```

Expected output: `desktop/src-tauri/target/release/bundle/deb/Mnemos_0.6.0_amd64.deb` exists. Open the .deb's contents with `dpkg-deb --contents` and confirm `usr/lib/Mnemos/binaries/mnemos-daemon-<triple>` and `mnemos-<triple>` are present.

- [ ] **Step 5: Commit.**

```bash
git add desktop/src-tauri/tauri.conf.json desktop/src-tauri/build-sidecars.sh .gitignore
git commit -m "feat: sidecar staging script bundles daemon + CLI in desktop installer (Plan 8 Task 2)"
```

---

## Task 3: Smoke-test the local Linux .deb install

Before adding CI, confirm the .deb produced by Task 2 actually installs and runs on the dev machine (Fedora user can use `--force-architecture` + `alien`, OR build the `rpm` target directly).

**Files:**
- Create: `scripts/package-linux.sh` (convenience wrapper for local Linux build)

- [ ] **Step 1: Create `scripts/package-linux.sh`**

```bash
#!/usr/bin/env bash
# Build the Linux bundles locally for smoke-testing.
#
# Outputs:
#   desktop/src-tauri/target/release/bundle/deb/*.deb
#   desktop/src-tauri/target/release/bundle/rpm/*.rpm
#   desktop/src-tauri/target/release/bundle/appimage/*.AppImage
set -euo pipefail

cd "$(dirname "$0")/.."

# Build the frontend + Rust + bundle.
cd desktop
pnpm install --frozen-lockfile
pnpm tauri build --bundles deb,rpm,appimage

cd ..
echo
echo "=== bundles produced ==="
find desktop/src-tauri/target/release/bundle -maxdepth 3 -type f \( -name "*.deb" -o -name "*.rpm" -o -name "*.AppImage" \) -print
```

Make it executable.

- [ ] **Step 2: Verify** — run it:

```bash
bash scripts/package-linux.sh
```

Expected: at least one .deb, one .rpm, and one .AppImage in `desktop/src-tauri/target/release/bundle/`. The .AppImage should be executable and `--appimage-extract-and-run` should launch (a quick `./Mnemos_0.6.0_amd64.AppImage --appimage-extract` to verify structure is enough — actual GUI launch needs a display).

- [ ] **Step 3: Commit.**

```bash
git add scripts/package-linux.sh
git commit -m "feat: local Linux bundle build script (Plan 8 Task 3)"
```

---

# Group B — Standalone Linux packages for CLI + daemon

## Task 4: `cargo-deb` config for `mnemos_cli` and `mnemos_daemon`

The desktop installer carries daemon + CLI inside it, but headless Linux users (servers, CI runners) want lightweight packages without the GUI. Add `[package.metadata.deb]` to both crates so `cargo deb -p mnemos_cli` and `cargo deb -p mnemos_daemon` produce installable `.deb` files.

**Files:**
- Modify: `crates/mnemos_cli/Cargo.toml`
- Modify: `crates/mnemos_daemon/Cargo.toml`

- [ ] **Step 1: Add to `crates/mnemos_cli/Cargo.toml`**:

```toml
[package.metadata.deb]
maintainer = "Mnemos contributors"
copyright = "2026, Mnemos contributors"
license-file = ["../../LICENSE", "0"]
extended-description = """\
Mnemos CLI — local-first AI memory provider. The `mnemos` binary lets you
remember, recall, list, forget, rebuild, sync, and diagnose without the
desktop app or daemon. For the full GUI, install mnemos-desktop instead.
"""
section = "utils"
priority = "optional"
depends = ""
assets = [
    ["target/release/mnemos", "usr/bin/", "755"],
    ["../../README.md", "usr/share/doc/mnemos/README", "644"],
    ["../../CHANGELOG.md", "usr/share/doc/mnemos/CHANGELOG", "644"],
]
```

(If `../../LICENSE` doesn't yet exist in the repo root, drop the `license-file` line and add a `license = "MIT OR Apache-2.0"` to the `[package]` block. Verify by `ls LICENSE*` first.)

- [ ] **Step 2: Add to `crates/mnemos_daemon/Cargo.toml`**:

```toml
[package.metadata.deb]
name = "mnemos-daemon"
maintainer = "Mnemos contributors"
copyright = "2026, Mnemos contributors"
license-file = ["../../LICENSE", "0"]
extended-description = """\
Mnemos daemon — the local HTTP + MCP server for the Mnemos memory
provider. Listens on 127.0.0.1:7423 by default and exposes the REST + WS
API plus the MCP streamable HTTP endpoint used by Claude Code, Codex,
and other AI clients.
"""
section = "utils"
priority = "optional"
depends = "$auto"
assets = [
    ["target/release/mnemos_daemon", "usr/bin/mnemos-daemon", "755"],
    ["../../README.md", "usr/share/doc/mnemos-daemon/README", "644"],
    ["../../CHANGELOG.md", "usr/share/doc/mnemos-daemon/CHANGELOG", "644"],
]
```

> Note the `name = "mnemos-daemon"` override: the Rust crate is `mnemos_daemon` (underscore), but the .deb package is `mnemos-daemon` (hyphen) by Debian convention.

- [ ] **Step 3: Verify locally.**

```bash
cargo install cargo-deb --locked  # one-time
cargo deb -p mnemos_cli --no-build  # build + package the existing release binary
cargo deb -p mnemos_daemon --no-build
ls -la target/debian/
dpkg-deb --info target/debian/mnemos_*.deb
dpkg-deb --info target/debian/mnemos-daemon_*.deb
```

Expected: two `.deb` files under `target/debian/`, each <20MB.

- [ ] **Step 4: Commit.**

```bash
git add crates/mnemos_cli/Cargo.toml crates/mnemos_daemon/Cargo.toml
git commit -m "feat: cargo-deb config for mnemos and mnemos-daemon packages (Plan 8 Task 4)"
```

---

## Task 5: `cargo-generate-rpm` config + Linux server-side packaging script

Same idea for RPM: add `[package.metadata.generate-rpm]` to both crates so Fedora/RHEL users get .rpm packages.

**Files:**
- Modify: `crates/mnemos_cli/Cargo.toml`
- Modify: `crates/mnemos_daemon/Cargo.toml`
- Create: `scripts/prepare-linux-packages.sh`

- [ ] **Step 1: Add `[package.metadata.generate-rpm]` to `crates/mnemos_cli/Cargo.toml`** (below the `[package.metadata.deb]` block):

```toml
[package.metadata.generate-rpm]
assets = [
    { source = "target/release/mnemos", dest = "/usr/bin/mnemos", mode = "755" },
    { source = "README.md", dest = "/usr/share/doc/mnemos/README", mode = "644", doc = true },
    { source = "CHANGELOG.md", dest = "/usr/share/doc/mnemos/CHANGELOG", mode = "644", doc = true },
]
```

- [ ] **Step 2: Add to `crates/mnemos_daemon/Cargo.toml`**:

```toml
[package.metadata.generate-rpm]
name = "mnemos-daemon"
assets = [
    { source = "target/release/mnemos_daemon", dest = "/usr/bin/mnemos-daemon", mode = "755" },
    { source = "README.md", dest = "/usr/share/doc/mnemos-daemon/README", mode = "644", doc = true },
    { source = "CHANGELOG.md", dest = "/usr/share/doc/mnemos-daemon/CHANGELOG", mode = "644", doc = true },
]
```

> `cargo-generate-rpm` looks for `README.md` and `CHANGELOG.md` relative to the **crate directory** by default, but the workspace root contains the real files. The `source` field treats relative paths from the crate dir, so for those `doc` assets we need workspace-relative paths. To keep it portable, the `prepare-linux-packages.sh` script will `cp` the workspace files into each crate dir before running `cargo generate-rpm`, then remove them.

- [ ] **Step 3: Create `scripts/prepare-linux-packages.sh`**:

```bash
#!/usr/bin/env bash
# Build .deb and .rpm packages for mnemos_cli and mnemos_daemon.
# Outputs under target/debian/ and target/generate-rpm/.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "=== building release binaries ==="
cargo build --release -p mnemos_cli -p mnemos_daemon

echo
echo "=== building .deb packages ==="
cargo deb -p mnemos_cli --no-build
cargo deb -p mnemos_daemon --no-build

echo
echo "=== staging doc assets for rpm builds ==="
for crate in mnemos_cli mnemos_daemon; do
    cp README.md "crates/$crate/README.md.tmp"
    cp CHANGELOG.md "crates/$crate/CHANGELOG.md.tmp"
    mv "crates/$crate/README.md.tmp" "crates/$crate/README.md"
    mv "crates/$crate/CHANGELOG.md.tmp" "crates/$crate/CHANGELOG.md"
done

trap 'rm -f crates/mnemos_cli/README.md crates/mnemos_cli/CHANGELOG.md crates/mnemos_daemon/README.md crates/mnemos_daemon/CHANGELOG.md' EXIT

echo
echo "=== building .rpm packages ==="
cargo generate-rpm -p crates/mnemos_cli
cargo generate-rpm -p crates/mnemos_daemon

echo
echo "=== artifacts ==="
ls -la target/debian/*.deb target/generate-rpm/*.rpm
```

Make it executable. Add to `.gitignore`:
```
crates/*/README.md
crates/*/CHANGELOG.md
```

> Caveat: this glob also matches any intentional per-crate README — which we don't have today. If a future plan adds a real `crates/mnemos_core/README.md`, this gitignore line will hide it; update accordingly.

- [ ] **Step 4: Verify locally.**

```bash
cargo install cargo-generate-rpm --locked  # one-time
bash scripts/prepare-linux-packages.sh
```

Expected: two .deb and two .rpm files. Confirm:
```bash
rpm -qpi target/generate-rpm/mnemos-*.rpm
dpkg-deb --info target/debian/mnemos*.deb
```

- [ ] **Step 5: Commit.**

```bash
git add crates/mnemos_cli/Cargo.toml crates/mnemos_daemon/Cargo.toml scripts/prepare-linux-packages.sh .gitignore
git commit -m "feat: cargo-generate-rpm config + Linux packaging script (Plan 8 Task 5)"
```

---

# Group C — Tauri auto-update

## Task 6: Generate updater signing key + add `tauri-plugin-updater`

The Tauri updater verifies update manifests against a public key embedded in the bundled app. Generate an ed25519 keypair locally; the public key goes in `tauri.conf.json`, the private key is committed only as `.env.example` (placeholder) and kept in CI secrets as `TAURI_SIGNING_PRIVATE_KEY`.

**Files:**
- Create: `scripts/gen-updater-key.sh`
- Modify: `desktop/src-tauri/Cargo.toml` (add plugin)
- Modify: `desktop/src-tauri/src/lib.rs` (register plugin)
- Modify: `desktop/src-tauri/capabilities/default.json` (allow updater calls)
- Modify: `.gitignore` (ignore private key file)

- [ ] **Step 1: `scripts/gen-updater-key.sh`**

```bash
#!/usr/bin/env bash
# Generate a Tauri updater signing keypair.
#
# Run once per project:
#   bash scripts/gen-updater-key.sh
#
# Outputs:
#   desktop/src-tauri/updater-private.pem   (NEVER COMMIT — add to CI as TAURI_SIGNING_PRIVATE_KEY)
#   prints public key to stdout — paste into tauri.conf.json's updater.pubkey
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ -f desktop/src-tauri/updater-private.pem ]]; then
    echo "Refusing to overwrite existing desktop/src-tauri/updater-private.pem" >&2
    echo "Move or delete it first if you really want a fresh key." >&2
    exit 1
fi

cd desktop
pnpm tauri signer generate --write-keys -p "" --force \
    --write-keys-folder ../desktop/src-tauri/

echo
echo "=== PUBLIC KEY (paste into desktop/src-tauri/tauri.conf.json → bundle.createUpdaterArtifacts then plugins.updater.pubkey) ==="
cat ../desktop/src-tauri/updater-private.pem.pub
echo
echo "Add the private key file to CI:"
echo "  gh secret set TAURI_SIGNING_PRIVATE_KEY < desktop/src-tauri/updater-private.pem"
echo "  gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --body ''"
echo
echo "DO NOT COMMIT desktop/src-tauri/updater-private.pem"
```

Make executable. Add to `.gitignore`:
```
desktop/src-tauri/updater-private.pem
desktop/src-tauri/updater-private.pem.pub
```

> The plan does NOT actually invoke this script in CI — the user runs it once locally, captures the public key into `tauri.conf.json`, and uploads the private key to GitHub Secrets. The script is the documented one-time setup.

- [ ] **Step 2: Add plugin to `desktop/src-tauri/Cargo.toml`**:

```toml
[dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
directories = "5"
tauri-plugin-updater = "2"
```

- [ ] **Step 3: Register the plugin in `desktop/src-tauri/src/lib.rs`** (or `main.rs` — whichever is the Tauri builder entry point). Existing code likely looks like:

```rust
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())   // example existing plugin
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Add the updater plugin:

```rust
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        // ... existing plugins ...
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

> Read the existing file first; the actual structure may differ slightly. Just slot the updater plugin alongside any existing `.plugin(...)` calls.

- [ ] **Step 4: Capability** — Tauri 2 uses capabilities to gate IPC. Add updater permissions to `desktop/src-tauri/capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capabilities for the desktop app.",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "updater:default"
  ]
}
```

> Read the existing `capabilities/default.json` and merge the `updater:default` permission into the existing `permissions` array. Don't drop pre-existing permissions.

- [ ] **Step 5: Generate the key + paste pubkey** — run the script and paste the public key. Add the `updater` block to `tauri.conf.json`:

```bash
bash scripts/gen-updater-key.sh
# copy the printed public key
```

In `tauri.conf.json`, add to the `bundle` block:

```json
"createUpdaterArtifacts": true
```

And at the top level, add a `plugins` block:

```json
"plugins": {
  "updater": {
    "pubkey": "<PASTE PUBLIC KEY HERE>",
    "endpoints": [
      "https://github.com/mnemos/mnemos/releases/latest/download/latest.json"
    ]
  }
}
```

> The pubkey is the contents of `updater-private.pem.pub` — a single-line base64-ish string starting with `dW50cnVz...` (the Tauri prefix).

- [ ] **Step 6: Add npm plugin** to `desktop/package.json`:

```bash
cd desktop
pnpm add @tauri-apps/plugin-updater
cd ..
```

This updates `package.json` and `pnpm-lock.yaml`. Commit both.

- [ ] **Step 7: Verify locally** — `cargo build -p mnemos-desktop` should compile with the new dep. `pnpm typecheck` should pass.

- [ ] **Step 8: Commit.**

```bash
git add scripts/gen-updater-key.sh .gitignore desktop/src-tauri/Cargo.toml desktop/src-tauri/src/lib.rs desktop/src-tauri/capabilities/default.json desktop/src-tauri/tauri.conf.json desktop/package.json desktop/pnpm-lock.yaml
git commit -m "feat: Tauri updater plugin + signing key generation script (Plan 8 Task 6)"
```

---

## Task 7: `UpdateBanner.tsx` — frontend update notification UI

When a new release is published, the Tauri updater plugin returns "available" on the next launch. Render a non-intrusive banner across the top of the shell with a deferred-install action and a manifest signature note. No modal — the user keeps working until they click Install.

**Files:**
- Create: `desktop/src/components/UpdateBanner.tsx`
- Create: `desktop/src/components/UpdateBanner.test.tsx`
- Modify: `desktop/src/App.tsx` (mount the banner)

- [ ] **Step 1: Failing test** — `UpdateBanner.test.tsx`:

```tsx
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { UpdateBanner } from "./UpdateBanner";

// Mock the @tauri-apps/plugin-updater API.
const mockDownloadAndInstall = vi.fn(async () => {});
vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn(async () => ({
    version: "0.7.1",
    currentVersion: "0.7.0",
    downloadAndInstall: mockDownloadAndInstall,
  })),
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: vi.fn(async () => {}),
}));

beforeEach(() => {
  mockDownloadAndInstall.mockClear();
});

test("shows the banner with the available version", async () => {
  render(<UpdateBanner />);
  expect(await screen.findByText(/0\.7\.1/i)).toBeInTheDocument();
});

test("clicking Install kicks off downloadAndInstall + relaunch", async () => {
  render(<UpdateBanner />);
  const btn = await screen.findByRole("button", { name: /install/i });
  await userEvent.click(btn);
  await waitFor(() => expect(mockDownloadAndInstall).toHaveBeenCalledOnce());
});

test("clicking Later dismisses the banner", async () => {
  render(<UpdateBanner />);
  const laterBtn = await screen.findByRole("button", { name: /later/i });
  await userEvent.click(laterBtn);
  expect(screen.queryByText(/0\.7\.1/i)).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Verify fail.**

- [ ] **Step 3: `desktop/src/components/UpdateBanner.tsx`**

```tsx
import { useEffect, useState } from "react";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { Button } from "../design/primitives";

type State =
  | { kind: "idle" }
  | { kind: "available"; version: string; download: () => Promise<void> }
  | { kind: "downloading" }
  | { kind: "ready_to_relaunch" }
  | { kind: "error"; message: string };

export function UpdateBanner() {
  const [state, setState] = useState<State>({ kind: "idle" });

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const update = await check();
        if (cancelled || !update) return;
        setState({
          kind: "available",
          version: update.version,
          download: async () => {
            setState({ kind: "downloading" });
            try {
              await update.downloadAndInstall();
              setState({ kind: "ready_to_relaunch" });
            } catch (e) {
              setState({
                kind: "error",
                message: e instanceof Error ? e.message : "update failed",
              });
            }
          },
        });
      } catch {
        // Not running under Tauri (e.g., vitest jsdom), or no network —
        // leave the banner idle.
      }
    })();
    return () => { cancelled = true; };
  }, []);

  if (state.kind === "idle") return null;

  if (state.kind === "available") {
    return (
      <div
        role="status"
        className="flex items-center justify-between gap-3 border-b border-border bg-surface-raised px-4 py-2 text-sm"
      >
        <span className="font-body">
          A new version is available — <span className="mono">{state.version}</span>
        </span>
        <div className="flex items-center gap-2">
          <Button variant="ghost" onClick={() => setState({ kind: "idle" })}>
            Later
          </Button>
          <Button onClick={() => void state.download()}>Install</Button>
        </div>
      </div>
    );
  }

  if (state.kind === "downloading") {
    return (
      <div
        role="status"
        className="border-b border-border bg-surface-raised px-4 py-2 text-sm text-text-muted"
      >
        Downloading update…
      </div>
    );
  }

  if (state.kind === "ready_to_relaunch") {
    return (
      <div
        role="status"
        className="flex items-center justify-between gap-3 border-b border-border bg-surface-raised px-4 py-2 text-sm"
      >
        <span className="font-body">Update installed. Relaunch to apply.</span>
        <Button onClick={() => void relaunch()}>Relaunch now</Button>
      </div>
    );
  }

  // error
  return (
    <div
      role="alert"
      className="border-b border-border bg-surface-raised px-4 py-2 text-sm text-tier-procedural"
      title={state.message}
    >
      Update failed: {state.message}
    </div>
  );
}
```

- [ ] **Step 4: Add `@tauri-apps/plugin-process` to deps** (for `relaunch`):

```bash
cd desktop
pnpm add @tauri-apps/plugin-process
```

And add the `process:default` permission to `capabilities/default.json` alongside `updater:default`.

- [ ] **Step 5: Mount in `App.tsx`** — above the existing shell, below any other top-level overlay:

```tsx
import { UpdateBanner } from "./components/UpdateBanner";
// ...
return (
  <QueryClientProvider client={queryClient}>
    <UpdateBanner />
    <RouterProvider router={router} />
    {firstRunShown && <FirstRun onClose={() => setFirstRunShown(false)} />}
  </QueryClientProvider>
);
```

> Read the existing `App.tsx` first; the order matters (the banner goes ABOVE the router so it's always visible).

- [ ] **Step 6: Pass + commit.**

```bash
cd desktop && pnpm typecheck && pnpm lint && pnpm test -- --run && cd ..
git add desktop/src/components/UpdateBanner.tsx desktop/src/components/UpdateBanner.test.tsx desktop/src/App.tsx desktop/package.json desktop/pnpm-lock.yaml desktop/src-tauri/capabilities/default.json
git commit -m "feat(ui): UpdateBanner — Tauri updater notification (Plan 8 Task 7)"
```

---

## Task 8: `latest.json` manifest generator + integration test

The Tauri updater polls a JSON manifest hosted on GitHub Releases (`endpoints: [".../latest.json"]`). CI uploads a `latest.json` per release, listing the per-platform installer URLs + signatures. Write a small Rust binary that generates this manifest from a set of installer paths + signatures, and a unit test for it.

**Files:**
- Create: `crates/mnemos_release_manifest/Cargo.toml`
- Create: `crates/mnemos_release_manifest/src/main.rs`
- Modify: workspace `Cargo.toml` (add member)

- [ ] **Step 1: Workspace member.** In root `Cargo.toml [workspace] members`, append `"crates/mnemos_release_manifest"`.

- [ ] **Step 2: `crates/mnemos_release_manifest/Cargo.toml`**:

```toml
[package]
name = "mnemos_release_manifest"
version.workspace = true
edition = "2021"

[[bin]]
name = "mnemos-release-manifest"
path = "src/main.rs"

[dependencies]
clap = { workspace = true, features = ["derive"] }
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true, features = ["serde"] }
anyhow = { workspace = true }
```

- [ ] **Step 3: `crates/mnemos_release_manifest/src/main.rs`**:

```rust
//! Generates the `latest.json` manifest Tauri's updater polls.
//!
//! Invoked from CI after the bundle matrix produces signed artifacts:
//!   mnemos-release-manifest \
//!     --version 0.7.0 \
//!     --notes "See CHANGELOG.md" \
//!     --pub-date 2026-05-28T13:00:00Z \
//!     --platform darwin-x86_64 \
//!     --url https://github.com/.../Mnemos_0.7.0_x64.app.tar.gz \
//!     --signature "dW50cnVz..." \
//!     [--platform linux-x86_64 --url ... --signature ...]... \
//!     --output latest.json

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Parser, Debug)]
struct Args {
    /// Semver of this release.
    #[arg(long)]
    version: String,

    /// Release notes (markdown).
    #[arg(long, default_value = "")]
    notes: String,

    /// Publication timestamp (RFC 3339). Defaults to now.
    #[arg(long)]
    pub_date: Option<DateTime<Utc>>,

    /// Platform triple, e.g. darwin-x86_64, linux-x86_64, windows-x86_64.
    /// Repeat with paired --url and --signature.
    #[arg(long = "platform", num_args = 1.., value_delimiter = ',')]
    platforms: Vec<String>,

    /// Download URLs in the same order as --platform.
    #[arg(long = "url", num_args = 1..)]
    urls: Vec<String>,

    /// Tauri signatures in the same order as --platform.
    #[arg(long = "signature", num_args = 1..)]
    signatures: Vec<String>,

    /// Output path (writes JSON).
    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Serialize)]
struct Platform {
    signature: String,
    url: String,
}

#[derive(Serialize)]
struct Manifest {
    version: String,
    notes: String,
    pub_date: String,
    platforms: BTreeMap<String, Platform>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    anyhow::ensure!(
        args.platforms.len() == args.urls.len() && args.urls.len() == args.signatures.len(),
        "--platform / --url / --signature counts must match: got {} / {} / {}",
        args.platforms.len(),
        args.urls.len(),
        args.signatures.len()
    );

    let mut platforms = BTreeMap::new();
    for ((p, u), s) in args
        .platforms
        .into_iter()
        .zip(args.urls.into_iter())
        .zip(args.signatures.into_iter())
    {
        platforms.insert(p, Platform { signature: s, url: u });
    }

    let manifest = Manifest {
        version: args.version,
        notes: args.notes,
        pub_date: args.pub_date.unwrap_or_else(Utc::now).to_rfc3339(),
        platforms,
    };

    let text = serde_json::to_string_pretty(&manifest)
        .context("serialize manifest")?;
    std::fs::write(&args.output, text).context("write manifest")?;
    println!("wrote {}", args.output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    #[test]
    fn round_trip_three_platforms() {
        // Build first so the binary exists.
        let status = Command::new(env!("CARGO"))
            .args(["build", "--bin", "mnemos-release-manifest"])
            .status()
            .unwrap();
        assert!(status.success());

        let tmp = TempDir::new().unwrap();
        let out = tmp.path().join("latest.json");
        let bin = env!("CARGO_BIN_EXE_mnemos-release-manifest");

        let status = Command::new(bin)
            .args([
                "--version", "0.7.0",
                "--notes", "test",
                "--pub-date", "2026-05-28T13:00:00Z",
                "--platform", "darwin-x86_64",
                "--platform", "linux-x86_64",
                "--platform", "windows-x86_64",
                "--url", "https://example/mac.tar.gz",
                "--url", "https://example/linux.tar.gz",
                "--url", "https://example/win.tar.gz",
                "--signature", "sig-mac",
                "--signature", "sig-linux",
                "--signature", "sig-win",
                "--output",
            ])
            .arg(&out)
            .status()
            .unwrap();
        assert!(status.success());

        let text = std::fs::read_to_string(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["version"], "0.7.0");
        assert_eq!(v["platforms"]["darwin-x86_64"]["signature"], "sig-mac");
        assert_eq!(v["platforms"]["linux-x86_64"]["url"], "https://example/linux.tar.gz");
        assert_eq!(v["platforms"]["windows-x86_64"]["signature"], "sig-win");
    }
}
```

- [ ] **Step 4: Test + commit.**

```bash
cargo test -p mnemos_release_manifest
cargo clippy -p mnemos_release_manifest --all-targets -- -D warnings
git add Cargo.toml crates/mnemos_release_manifest/
git commit -m "feat: mnemos-release-manifest binary generates Tauri updater latest.json (Plan 8 Task 8)"
```

---

# Group D — GitHub Actions release workflow

## Task 9: `.github/workflows/release.yml` — cross-platform build matrix

A tag-triggered (`v*.*.*`) matrix workflow that builds on `macos-latest`, `ubuntu-latest`, and `windows-latest`. Each job runs `pnpm tauri build`, uploads its artifacts to the release, and emits the per-platform Tauri signature for use by the manifest job.

**Files:**
- Create: `.github/workflows/release.yml`
- Create: `.github/RELEASE_TEMPLATE.md`

- [ ] **Step 1: `.github/workflows/release.yml`**

```yaml
name: Release

on:
  push:
    tags:
      - "v*.*.*"
  workflow_dispatch:
    inputs:
      tag:
        description: "Tag to release (e.g. v0.7.0). Must already exist."
        required: true

permissions:
  contents: write

jobs:
  build:
    name: Build (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: macos-latest
            platform: darwin-aarch64
            bundles: "dmg,app"
          - os: ubuntu-22.04
            platform: linux-x86_64
            bundles: "deb,rpm,appimage"
          - os: windows-latest
            platform: windows-x86_64
            bundles: "msi"

    env:
      TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}

    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.event.inputs.tag || github.ref }}

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2

      - name: Install Linux deps
        if: matrix.os == 'ubuntu-22.04'
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev \
            librsvg2-dev libappindicator3-dev patchelf libssl-dev

      - name: Install pnpm
        uses: pnpm/action-setup@v4
        with:
          version: 9

      - name: Install Node
        uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: "pnpm"
          cache-dependency-path: desktop/pnpm-lock.yaml

      - name: Install desktop dependencies
        working-directory: desktop
        run: pnpm install --frozen-lockfile

      - name: Build desktop bundle
        working-directory: desktop
        run: pnpm tauri build --bundles ${{ matrix.bundles }}
        # `beforeBuildCommand` in tauri.conf.json runs build-sidecars.sh first.

      - name: Stage artifacts
        shell: bash
        run: |
          mkdir -p staged
          # Copy every bundle output into a flat staging directory.
          shopt -s globstar nullglob
          for f in desktop/src-tauri/target/release/bundle/**/*.{dmg,app.tar.gz,deb,rpm,AppImage,msi,nsis}; do
            cp "$f" staged/
          done
          # The Tauri updater also produces a .sig file next to the
          # bundle it's signed against; preserve those.
          for f in desktop/src-tauri/target/release/bundle/**/*.sig; do
            cp "$f" staged/
          done
          ls -la staged/

      - name: Upload bundle artifacts
        uses: actions/upload-artifact@v4
        with:
          name: bundle-${{ matrix.platform }}
          path: staged/
          retention-days: 7

  linux-packages:
    name: Linux server-side packages
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install cargo-deb + cargo-generate-rpm
        run: |
          cargo install cargo-deb --locked
          cargo install cargo-generate-rpm --locked
      - name: Build server packages
        run: bash scripts/prepare-linux-packages.sh
      - name: Upload server-side artifacts
        uses: actions/upload-artifact@v4
        with:
          name: linux-server-packages
          path: |
            target/debian/*.deb
            target/generate-rpm/*.rpm
          retention-days: 7

  release:
    name: Publish release
    needs: [build, linux-packages]
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4

      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts/

      - name: Flatten + list
        run: |
          mkdir -p release/
          find artifacts/ -type f \( -name "*.dmg" -o -name "*.tar.gz" -o -name "*.deb" -o -name "*.rpm" -o -name "*.AppImage" -o -name "*.msi" -o -name "*.sig" \) -exec cp -v {} release/ \;
          ls -la release/

      - name: Build release-manifest tool
        run: cargo build --release --bin mnemos-release-manifest

      - name: Generate latest.json
        shell: bash
        run: |
          set -euo pipefail
          VERSION="${GITHUB_REF_NAME#v}"

          read_sig() {
            local pattern="$1"
            local sig_file
            sig_file=$(ls release/$pattern 2>/dev/null | head -1)
            if [[ -z "$sig_file" ]]; then echo ""; else cat "$sig_file"; fi
          }

          first_match() {
            ls release/$1 2>/dev/null | head -1
          }

          MAC_FILE=$(first_match "*.app.tar.gz")
          LIN_FILE=$(first_match "*.AppImage.tar.gz")
          WIN_FILE=$(first_match "*.msi.zip")

          # Tauri's updater signatures live next to the .tar.gz / .zip — same name + .sig
          MAC_SIG=$(cat "${MAC_FILE}.sig" 2>/dev/null || echo "")
          LIN_SIG=$(cat "${LIN_FILE}.sig" 2>/dev/null || echo "")
          WIN_SIG=$(cat "${WIN_FILE}.sig" 2>/dev/null || echo "")

          base_url="https://github.com/${GITHUB_REPOSITORY}/releases/download/${GITHUB_REF_NAME}"

          ./target/release/mnemos-release-manifest \
            --version "$VERSION" \
            --notes "$(awk "/^## \[$VERSION\]/{f=1;next} /^## \[/{f=0} f" CHANGELOG.md | head -100)" \
            --platform darwin-aarch64 \
            --platform linux-x86_64 \
            --platform windows-x86_64 \
            --url  "${base_url}/$(basename "$MAC_FILE")" \
            --url  "${base_url}/$(basename "$LIN_FILE")" \
            --url  "${base_url}/$(basename "$WIN_FILE")" \
            --signature "$MAC_SIG" \
            --signature "$LIN_SIG" \
            --signature "$WIN_SIG" \
            --output release/latest.json

          cat release/latest.json

      - name: Publish GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ github.event.inputs.tag || github.ref_name }}
          name: ${{ github.event.inputs.tag || github.ref_name }}
          generate_release_notes: false
          body_path: CHANGELOG.md
          files: release/*
          make_latest: true
          fail_on_unmatched_files: false
```

> Two important details:
> 1. The "latest.json" generation step assumes Tauri produced `.app.tar.gz` (macOS), `.AppImage.tar.gz` (Linux), and `.msi.zip` (Windows) — these are the Tauri-updater specific artifacts that ship alongside the user-facing bundles (`.dmg` / `.AppImage` / `.msi`). They're auto-emitted when `createUpdaterArtifacts: true` is set in `tauri.conf.json`.
> 2. The `read_sig` helper is unused in the final script (left as a comment for clarity); signatures are read directly via the `${FILE}.sig` convention.

- [ ] **Step 2: `.github/RELEASE_TEMPLATE.md`** — for `softprops/action-gh-release`'s body fallback:

```markdown
## Mnemos $VERSION

See [CHANGELOG.md](https://github.com/mnemos/mnemos/blob/master/CHANGELOG.md) for what's new.

### Downloads

| Platform | File |
|---|---|
| macOS (Apple Silicon) | `Mnemos_${VERSION}_aarch64.dmg` |
| Linux (x86_64) | `Mnemos_${VERSION}_amd64.AppImage` / `mnemos_${VERSION}_amd64.deb` / `mnemos-${VERSION}-1.x86_64.rpm` |
| Windows (x86_64) | `Mnemos_${VERSION}_x64_en-US.msi` |

The macOS and Windows builds are **unsigned**. macOS may warn about an unidentified developer (right-click → Open the first time). Windows will SmartScreen-warn (More info → Run anyway).

Linux server / CLI-only packages: `mnemos_${VERSION}_amd64.deb` (CLI) and `mnemos-daemon_${VERSION}_amd64.deb` (daemon). RPM equivalents under `target/generate-rpm/`.

### Auto-update

The desktop app polls `https://github.com/mnemos/mnemos/releases/latest/download/latest.json` on launch. Update manifests are signed with the project's ed25519 key.
```

- [ ] **Step 3: Commit.**

```bash
git add .github/workflows/release.yml .github/RELEASE_TEMPLATE.md
git commit -m "feat: GitHub Actions release workflow with cross-platform matrix (Plan 8 Task 9)"
```

> The workflow won't actually run until a tag is pushed. CI validates the YAML syntax via the existing `ci.yml` linter (if present). Otherwise the next `v*.*.*` push exercises it.

---

## Task 10: Add `LICENSE` if missing + finalize repo metadata

Both `cargo-deb` configs reference `../../LICENSE`. If the repo doesn't yet have one, add it now so the .deb packages are well-formed.

**Files:**
- Create: `LICENSE` (MIT-by-default for an open-source memory provider)
- Modify: workspace `Cargo.toml` (`license` field on `[workspace.package]`)

- [ ] **Step 1: Check.** `ls LICENSE*` — if exists, skip Steps 2-3 entirely.

- [ ] **Step 2: `LICENSE`** — verbatim MIT (substitute the year):

```
MIT License

Copyright (c) 2026 Mnemos contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 3: Workspace metadata** — root `Cargo.toml [workspace.package]`:

```toml
license = "MIT"
repository = "https://github.com/mnemos/mnemos"
homepage = "https://github.com/mnemos/mnemos"
description = "Local-first AI memory provider"
authors = ["Mnemos contributors"]
```

Each member crate either inherits these via `license.workspace = true` (etc.) or has the values already.

- [ ] **Step 4: Commit.**

```bash
git add LICENSE Cargo.toml
git commit -m "chore: add LICENSE (MIT) + workspace package metadata (Plan 8 Task 10)"
```

---

## Task 11: Validate the release workflow without an actual tag

GitHub Actions has a `workflow_dispatch` mode — we can trigger `release.yml` manually with a pre-existing tag input to confirm the workflow runs end-to-end before pushing a real tag.

**Files:** (none — verification only)

- [ ] **Step 1: Document the dry-run process** — append to `PACKAGING.md` (created in Task 13) the manual-test command. For this task, just verify the YAML is syntactically valid:

```bash
# YAML syntax check
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))" && echo ok
# or, if act is installed:
act -l --workflows .github/workflows/release.yml 2>&1 | head -20
```

- [ ] **Step 2: GitHub Actions schema validation** — push the branch and let GH's UI surface any YAML errors. Or use the `gh workflow view` command:

```bash
gh workflow view "Release" 2>&1 | head -20
```

If everything passes syntax, proceed to Task 12. This task produces no commit — it's a checkpoint before the documentation tasks.

---

# Group E — Documentation

## Task 12: `BUILD.md` — cross-platform build instructions

A new dev's first-week reference. Covers Linux/macOS/Windows local builds, the daemon + CLI in isolation, the desktop app, signing prerequisites (deferred), and CI debugging tips.

**Files:**
- Create: `BUILD.md`

- [ ] **Step 1: `BUILD.md`** — at the repo root:

```markdown
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
./target/release/mnemos_daemon --help
```

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

To regenerate (if ever lost):

```
bash scripts/gen-updater-key.sh
# Paste the printed public key into tauri.conf.json → plugins.updater.pubkey
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

**CI matrix job times out at the bundle step** — Tauri Linux bundles
include a full WebKit runtime; this is normal. Use the `Swatinem/rust-cache`
hit rate as your indicator of regressions.
```

- [ ] **Step 2: Commit.**

```bash
git add BUILD.md
git commit -m "docs: BUILD.md cross-platform build guide (Plan 8 Task 12)"
```

---

## Task 13: `PACKAGING.md` — release runbook

The companion to BUILD.md, this one is for the person cutting a release. Covers the tag-and-push flow, what to verify in artifacts, how to write release notes, how to push the Linux server-side .deb to a Launchpad PPA and the .rpm to an OBS project, and how to rotate the updater key (linking back to BUILD.md).

**Files:**
- Create: `PACKAGING.md`

- [ ] **Step 1: `PACKAGING.md`** — repo root:

```markdown
# Release + distribution runbook

This document describes how to cut a Mnemos release.

## Release flow (summary)

1. Land all PRs targeting the release. Run the full test suite locally:

   ```
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   (cd desktop && pnpm typecheck && pnpm lint && pnpm test -- --run && pnpm build)
   ```

2. Update `CHANGELOG.md`: add a `## [X.Y.Z] - YYYY-MM-DD` block at the
   top describing what's new.

3. Bump versions in:
   - `Cargo.toml` `[workspace.package] version`
   - `desktop/package.json` `version`
   - `desktop/src-tauri/Cargo.toml` `version`
   - `desktop/src-tauri/tauri.conf.json` `version`

4. Commit the bump:

   ```
   git add Cargo.toml desktop/package.json desktop/src-tauri/Cargo.toml \
           desktop/src-tauri/tauri.conf.json CHANGELOG.md
   git commit -m "chore: release vX.Y.Z"
   ```

5. Tag:

   ```
   git tag -a vX.Y.Z -m "vX.Y.Z — short summary"
   ```

6. Push:

   ```
   git push origin master --tags
   ```

7. Watch the GitHub Actions "Release" workflow. On success, the release
   appears at `https://github.com/<org>/mnemos/releases/tag/vX.Y.Z` with:
   - `Mnemos_X.Y.Z_aarch64.dmg`, `.app.tar.gz`, `.app.tar.gz.sig`
   - `Mnemos_X.Y.Z_amd64.AppImage`, `.AppImage.tar.gz`, `.AppImage.tar.gz.sig`
   - `Mnemos_X.Y.Z_amd64.deb`, `mnemos-X.Y.Z-1.x86_64.rpm`
   - `Mnemos_X.Y.Z_x64_en-US.msi`, `.msi.zip`, `.msi.zip.sig`
   - `mnemos_X.Y.Z_amd64.deb` (server-side CLI)
   - `mnemos-daemon_X.Y.Z_amd64.deb`
   - `.rpm` equivalents
   - `latest.json` — Tauri updater manifest

8. Verify a download:
   - **macOS**: open the `.dmg`, drag to Applications, launch, confirm the
     `Mnemos` window opens, run a quick `mnemos --version` from a terminal
     pointed at the bundled binary, smoke-test the Doctor view.
   - **Linux**: `sudo dpkg -i Mnemos_*.deb` AND `sudo dpkg -i mnemos_*.deb` (the
     CLI-only one). Run `mnemos --version`, run the bundled daemon via the
     `.desktop` entry, smoke-test the GUI.
   - **Windows**: install the `.msi`, launch from the Start Menu, smoke-test.

## Dry-running the release workflow

The `release.yml` workflow exposes a `workflow_dispatch` trigger with a
`tag` input. To validate without cutting a real release:

```
gh workflow run "Release" -f tag=v0.7.0-rc1
gh run watch
```

(Create a `v0.7.0-rc1` tag locally first, push it, then dispatch.) After
the run, delete the `v0.7.0-rc1` release + tag.

## Auto-update verification (post-release)

After a release publishes:

1. Take a known-older Mnemos build (e.g., the previous version's `.dmg`)
   and install it.
2. Launch it. The UpdateBanner should appear within ~5 seconds with the
   new version's number.
3. Click Install. Confirm the download progresses and the app prompts
   to relaunch.
4. Relaunch and verify the version in `About` matches.

If the banner never appears:

- Check the release's `latest.json` exists and is reachable:
  `curl -sSf https://github.com/<org>/mnemos/releases/latest/download/latest.json`.
- Check the older build's `tauri.conf.json` updater endpoint matches.
- Check the older build's public key matches the current signing key
  (if you rotated, older builds are stranded — they need a manual
  download).

## Linux package repositories

`.deb` and `.rpm` artifacts on GitHub Releases are user-installable via
`dpkg -i` / `rpm -i`, but for `apt`/`dnf` to find them automatically,
they need to live in a repository.

### apt: Launchpad PPA

1. [Create a Launchpad account](https://launchpad.net) and a PPA, e.g.
   `~mnemos/+archive/ubuntu/mnemos`.
2. Generate a GPG key tied to that account.
3. Sign each `.deb` with the key:

   ```
   debsign -k <KEYID> *.deb
   ```

4. Upload:

   ```
   dput ppa:mnemos/mnemos *.changes
   ```

Launchpad rebuilds the source package and signs the resulting binary.
Users then add the PPA:

```
sudo add-apt-repository ppa:mnemos/mnemos
sudo apt update
sudo apt install mnemos mnemos-daemon
```

### dnf: openSUSE Build Service (OBS)

OBS is the most universal RPM hosting layer (Fedora COPR is the
alternative — choose based on your audience).

1. Create an OBS account at <https://build.opensuse.org>.
2. Create a project (e.g. `home:mnemos:mnemos`).
3. Per release, upload the `.rpm` (or the source `.spec` + tarball).
4. OBS rebuilds against Fedora, openSUSE, RHEL etc. and serves a repo.
5. Users add the repo and install:

   ```
   sudo dnf config-manager --add-repo \
     https://download.opensuse.org/repositories/home:mnemos:mnemos/Fedora_38/home:mnemos:mnemos.repo
   sudo dnf install mnemos mnemos-daemon
   ```

> Both Launchpad and OBS require accounts the framework cannot
> auto-create. Treat this section as the manual follow-up after the
> first release lands.

### AppImage updates

AppImages don't have a package repo concept. The Tauri updater
serves the new AppImage via `latest.json` and replaces the existing
file in place.

## Code-signing

See [BUILD.md](BUILD.md) § "Code-signing" for the macOS and Windows
secret setup. Once the secrets are added, the next release runs
through CI and produces signed installers without code changes.

## Rotating the updater key

See [BUILD.md](BUILD.md) § "Tauri updater signing key" — only do this
after a key compromise. Users on the prior key won't auto-update.

## Common release-day issues

- **`actions/upload-artifact` fails: artifact too large** — Tauri
  bundles are ~80 MB compressed; if you're seeing >2 GB, check that
  `target/release/bundle/` isn't being globbed (only `staged/` should
  be uploaded). The workflow already scopes this.

- **`pnpm tauri build` runs but no `.app.tar.gz` is emitted** —
  confirm `tauri.conf.json` has `bundle.createUpdaterArtifacts: true`.
  Without it, Tauri skips the updater-specific tarballs.

- **Signature mismatch on update** — the public key in
  `tauri.conf.json` doesn't match the private key in CI. Confirm
  via `pnpm tauri signer sign --help` and a manual signature test
  before re-cutting.
```

- [ ] **Step 2: Commit.**

```bash
git add PACKAGING.md
git commit -m "docs: PACKAGING.md release + distribution runbook (Plan 8 Task 13)"
```

---

## Task 14: README install section

The repo's `README.md` already has feature sections. Add a prominent "Install" section near the top so users landing on the repo see download instructions before architecture details.

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Insert** — after the project intro (top of README) and before any "Features" or version sections, add:

```markdown
## Install

### macOS

Download `Mnemos_X.Y.Z_aarch64.dmg` from [the latest release][releases]
and drag Mnemos to your Applications folder. The first launch will
prompt "unidentified developer" — right-click the icon and choose Open.

### Linux

Direct install (Debian/Ubuntu/Fedora):

```
# Desktop app (includes daemon + CLI as sidecars):
sudo dpkg -i Mnemos_X.Y.Z_amd64.deb       # Debian/Ubuntu
sudo rpm -i Mnemos-X.Y.Z-1.x86_64.rpm     # Fedora/RHEL

# Or run the AppImage without installing:
chmod +x Mnemos_X.Y.Z_amd64.AppImage
./Mnemos_X.Y.Z_amd64.AppImage

# Server-only (no GUI):
sudo dpkg -i mnemos_X.Y.Z_amd64.deb mnemos-daemon_X.Y.Z_amd64.deb
```

Add an apt PPA or dnf repo (see [PACKAGING.md](PACKAGING.md)) for
`apt update` / `dnf upgrade` integration.

### Windows

Download `Mnemos_X.Y.Z_x64_en-US.msi` from [the latest release][releases]
and double-click. SmartScreen may warn — choose More info → Run anyway
(the installer is currently unsigned; see [BUILD.md](BUILD.md) for the
notarization roadmap).

### Build from source

See [BUILD.md](BUILD.md).

[releases]: https://github.com/mnemos/mnemos/releases/latest

### Auto-update

The desktop app polls for updates on launch. When a new version is
available, an UpdateBanner appears at the top of the window with
Install / Later actions. Update manifests are ed25519-signed.

The CLI + daemon (`apt install mnemos` / `dnf install mnemos` /
`dpkg -i mnemos_*.deb`) update via your package manager.
```

> Read the existing `README.md` first; the insertion point depends on the existing structure. Place the Install section directly after the top-of-file project description.

- [ ] **Step 2: Commit.**

```bash
git add README.md
git commit -m "docs: README install section (Plan 8 Task 14)"
```

---

# Group F — Release v0.7.0

## Task 15: Bump to v0.7.0, CHANGELOG, tag

Same shape as Plan 7 Task 19. Local tag only.

**Files:**
- Modify: `Cargo.toml` (workspace version → 0.7.0)
- Modify: `desktop/package.json` → 0.7.0
- Modify: `desktop/src-tauri/Cargo.toml` → 0.7.0
- Modify: `desktop/src-tauri/tauri.conf.json` → 0.7.0
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Bump versions** in all four files.

- [ ] **Step 2: CHANGELOG entry** at the top:

```markdown
## [0.7.0] - 2026-05-28

### Added
- Cross-platform installers via Tauri bundler (`.dmg` + `.app` macOS,
  `.deb` + `.rpm` + `.AppImage` Linux, `.msi` Windows). Desktop installer
  bundles the daemon + CLI as Tauri sidecars.
- Stand-alone `.deb` + `.rpm` packages for the CLI (`mnemos`) and daemon
  (`mnemos-daemon`) via `cargo-deb` and `cargo-generate-rpm`.
- Tauri-built-in auto-update: ed25519-signed `latest.json` manifest on
  GitHub Releases, `UpdateBanner` UI in the desktop app, defer-or-install
  flow with progress.
- `.github/workflows/release.yml` — tag-triggered build matrix on macOS,
  Linux, and Windows runners + a release-publish job that uploads all
  artifacts and generates the updater manifest.
- `mnemos_release_manifest` workspace member — small binary that
  generates the Tauri updater `latest.json` from a tagged set of
  platform / URL / signature triples.
- Icon set (SVG source + generated PNG/ICO/ICNS) under
  `desktop/src-tauri/icons/`.
- Documentation: `BUILD.md` (cross-platform build steps), `PACKAGING.md`
  (release + distribution runbook), `LICENSE` (MIT), README "Install"
  section.

### Deferred
- Apple Developer notarization, Microsoft Authenticode signing — both
  documented in BUILD.md; future v0.7.x release re-runs CI with secrets.
- Launchpad PPA / OBS RPM repository submission — documented in
  PACKAGING.md; requires accounts the framework cannot create.
- Homebrew tap, crates.io publish — explicitly opted out in plan scoping.
- Turso libSQL embedded replicas wire-up, encrypt-at-rest, secret
  detection at ingest — carried forward from Plan 7.

### Notes
- The Tauri updater public key is committed in `tauri.conf.json`. The
  private key lives in `TAURI_SIGNING_PRIVATE_KEY` (CI secret).
- macOS and Windows installers are **unsigned** for v0.7.0. SmartScreen
  / Gatekeeper warnings on first launch are expected; see [PACKAGING.md].
```

- [ ] **Step 3: Release gate** (same as Plan 7 Task 19):

```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd desktop && pnpm install --frozen-lockfile && pnpm typecheck && pnpm lint && pnpm test -- --run && pnpm build && cd ..
bash scripts/package-linux.sh   # local smoke-build all 3 Linux bundles
```

All green. (The actual cross-platform CI matrix runs on tag push.)

- [ ] **Step 4: Commit + tag** (local only).

```bash
git add Cargo.toml desktop/package.json desktop/src-tauri/Cargo.toml desktop/src-tauri/tauri.conf.json CHANGELOG.md
git commit -m "chore: release v0.7.0 — packaging, installers, auto-update (Plan 8 Task 15)"
git tag -a v0.7.0 -m "v0.7.0 — packaging + auto-update + cross-platform installers"
```

(Do NOT push — user reviews, then pushes manually. The tag push is what triggers the CI release pipeline.)

---

## Done

After all tasks: mnemos is **shippable to non-developers**.

- Desktop app installable on macOS (.dmg), Linux (.deb/.rpm/.AppImage), Windows (.msi)
- Sidecar daemon + CLI included in every desktop install
- Stand-alone CLI + daemon .deb/.rpm packages for headless Linux
- Auto-update via Tauri's signed-manifest pipeline (defer-or-install flow)
- CI tag-trigger publishes everything to GitHub Releases
- Documentation covers building, releasing, signing (when ready), and PPA/OBS submission

What's deferred is purely **transactional** (paid certs, account creation on third-party services) — not architectural. The next release can flip notarization on by adding secrets; no code or workflow changes required.

The mnemos roadmap from here is feature work (encrypt-at-rest, Turso wire-up, secret detection) and ecosystem (more adapters, more sync backends, telemetry). The shipping pipeline is done.
