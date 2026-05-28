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
