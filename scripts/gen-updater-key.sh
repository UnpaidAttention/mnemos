#!/usr/bin/env bash
# Generate a Tauri updater signing keypair.
#
# Run once per project:
#   bash scripts/gen-updater-key.sh
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ -f desktop/src-tauri/updater-private.pem ]]; then
    echo "Refusing to overwrite existing desktop/src-tauri/updater-private.pem" >&2
    echo "Move or delete it first if you really want a fresh key." >&2
    exit 1
fi

cd desktop
# Tauri 2.x signer: writes private key to -w path and public key to <path>.pub
pnpm tauri signer generate -w ../desktop/src-tauri/updater-private.pem -p "" --ci

echo
echo "=== PUBLIC KEY (paste into desktop/src-tauri/tauri.conf.json → plugins.updater.pubkey) ==="
cat ../desktop/src-tauri/updater-private.pem.pub
echo
echo "Add the private key file to CI:"
echo "  gh secret set TAURI_SIGNING_PRIVATE_KEY < desktop/src-tauri/updater-private.pem"
echo "  gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --body ''"
echo
echo "DO NOT COMMIT desktop/src-tauri/updater-private.pem"
