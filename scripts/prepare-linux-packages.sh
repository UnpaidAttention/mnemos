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
    cp README.md "crates/$crate/README.md"
    cp CHANGELOG.md "crates/$crate/CHANGELOG.md"
done

trap 'rm -f crates/mnemos_cli/README.md crates/mnemos_cli/CHANGELOG.md crates/mnemos_daemon/README.md crates/mnemos_daemon/CHANGELOG.md' EXIT

echo
echo "=== building .rpm packages ==="
cargo generate-rpm -p crates/mnemos_cli
cargo generate-rpm -p crates/mnemos_daemon

echo
echo "=== artifacts ==="
ls -la target/debian/*.deb target/generate-rpm/*.rpm
