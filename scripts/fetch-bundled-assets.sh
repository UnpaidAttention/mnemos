#!/usr/bin/env bash
# Download the bundled embedder runtime + model.
#
# Outputs:
#   assets/llama-server-linux-x86_64       (~5 MB, llama.cpp upstream binary)
#   assets/all-MiniLM-L6-v2.Q8_0.gguf      (~22 MB, GGUF model)
#
# Idempotent: skips download if files already exist and match the expected
# sha256. Bump LLAMA_CPP_TAG and MODEL_URL to update the pinned versions.
set -euo pipefail

cd "$(dirname "$0")/.."
mkdir -p assets

# Pinned versions. Bump these to upgrade; each bump should be a separate
# commit so reviewers can audit the diff against upstream changelogs.
LLAMA_CPP_TAG="b9400"
LLAMA_CPP_ARCHIVE="llama-${LLAMA_CPP_TAG}-bin-ubuntu-x64.tar.gz"
LLAMA_CPP_URL="https://github.com/ggml-org/llama.cpp/releases/download/${LLAMA_CPP_TAG}/${LLAMA_CPP_ARCHIVE}"

MODEL_NAME="all-MiniLM-L6-v2.Q8_0.gguf"
MODEL_URL="https://huggingface.co/leliuga/all-MiniLM-L6-v2-GGUF/resolve/main/all-MiniLM-L6-v2.Q8_0.gguf"
MODEL_SHA256="e5ec722e8c82dc4ffaf965175ca472f5da3f97b695590b5b0780bdbfa29bcaf3"

verify_sha() {
    local file="$1" expected="$2"
    local actual
    actual=$(sha256sum "$file" | awk '{print $1}')
    if [[ "$actual" != "$expected" ]]; then
        echo "sha256 mismatch on $file" >&2
        echo "  expected: $expected" >&2
        echo "  actual:   $actual" >&2
        exit 1
    fi
}

# llama-server binary
TARGET_BINARY="assets/llama-server-linux-x86_64"
if [[ ! -x "$TARGET_BINARY" ]]; then
    echo "=== fetching llama.cpp ${LLAMA_CPP_TAG} ==="
    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT
    curl -fL --retry 3 -o "$tmpdir/llama.tar.gz" "$LLAMA_CPP_URL"
    mkdir -p "$tmpdir/llama"
    tar -xzf "$tmpdir/llama.tar.gz" -C "$tmpdir/llama"
    # llama.cpp's archive layout: <root>/build/bin/llama-server (or just bin/)
    found=$(find "$tmpdir/llama" -name llama-server -type f -executable | head -1)
    if [[ -z "$found" ]]; then
        echo "llama-server not found in $LLAMA_CPP_ARCHIVE" >&2
        exit 1
    fi
    cp "$found" "$TARGET_BINARY"
    chmod +x "$TARGET_BINARY"
    # Copy shared libraries (libllama.so, libggml*.so) alongside the binary so
    # it can be invoked without LD_LIBRARY_PATH gymnastics.
    libdir=$(dirname "$found")
    find "$libdir" -maxdepth 1 -name '*.so*' -exec cp -P {} assets/ \; 2>/dev/null || true
    echo "✓ $TARGET_BINARY"
else
    echo "✓ $TARGET_BINARY (cached)"
fi

# Model file
TARGET_MODEL="assets/${MODEL_NAME}"
if [[ ! -f "$TARGET_MODEL" ]]; then
    echo "=== fetching ${MODEL_NAME} ==="
    curl -fL --retry 3 -o "$TARGET_MODEL" "$MODEL_URL"
    echo "✓ $TARGET_MODEL"
else
    echo "✓ $TARGET_MODEL (cached)"
fi

# Verify sha256 if pinned (skip if placeholder)
if [[ "$MODEL_SHA256" != "<FILL_AFTER_FIRST_DOWNLOAD>" ]]; then
    verify_sha "$TARGET_MODEL" "$MODEL_SHA256"
fi

echo
echo "=== summary ==="
ls -la assets/
echo
echo "Total size:"
du -ch assets/llama-server-linux-x86_64 assets/${MODEL_NAME} | tail -1
