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
# Note: mnemos_daemon crate produces a binary named `mnemosd`; mnemos_cli produces `mnemos`.
if [[ "$TARGET" == "$(rustc -vV | awk '/host/ {print $2}')" ]]; then
  cargo build --release -p mnemos_daemon -p mnemos_cli
  SRC="target/release"
else
  cargo build --release --target "$TARGET" -p mnemos_daemon -p mnemos_cli
  SRC="target/$TARGET/release"
fi

cp "$SRC/mnemosd$SUFFIX" "$OUT/mnemos-daemon-$TARGET$SUFFIX"
cp "$SRC/mnemos$SUFFIX"  "$OUT/mnemos-$TARGET$SUFFIX"
echo "staged sidecars for $TARGET → $OUT/"
