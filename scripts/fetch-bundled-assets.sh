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
# P2-14: sha256 of the llama.cpp release tarball.  Bump this alongside
# LLAMA_CPP_TAG by running:
#   curl -fL "$LLAMA_CPP_URL" | sha256sum | awk '{print $1}'
# and pasting the result here.  Using "<FILL_AFTER_FIRST_DOWNLOAD>" keeps the
# verification mechanism active so reviewers know it needs updating on a tag
# bump.  The model uses the same pattern (MODEL_SHA256 below).
LLAMA_CPP_SHA256="<FILL_AFTER_FIRST_DOWNLOAD>"

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
    # Verify the archive checksum before extraction (P2-14).
    # This mirrors the MODEL_SHA256 pattern; update LLAMA_CPP_SHA256 on every tag bump.
    if [[ "$LLAMA_CPP_SHA256" != "<FILL_AFTER_FIRST_DOWNLOAD>" ]]; then
        verify_sha "$tmpdir/llama.tar.gz" "$LLAMA_CPP_SHA256"
    else
        echo "WARNING: LLAMA_CPP_SHA256 is not set — skipping archive checksum." \
             "Run: sha256sum $tmpdir/llama.tar.gz and fill LLAMA_CPP_SHA256 in this script." >&2
    fi
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

# Embedder model file
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

# ── LLM model (Qwen3-0.6B Q4_K_M) ──────────────────────────────────────────
# Used by the bundled LLM server for the learning pipeline (entity extraction,
# reflections, community summaries). ~462 MB quantized — runs well on CPU.
LLM_MODEL_NAME="Qwen3-0.6B-Q4_K_M.gguf"
LLM_MODEL_URL="https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf"
LLM_MODEL_SHA256="<FILL_AFTER_FIRST_DOWNLOAD>"

TARGET_LLM="assets/${LLM_MODEL_NAME}"
if [[ ! -f "$TARGET_LLM" ]]; then
    echo "=== fetching ${LLM_MODEL_NAME} ==="
    curl -fL --retry 3 -o "$TARGET_LLM" "$LLM_MODEL_URL"
    echo "✓ $TARGET_LLM"
else
    echo "✓ $TARGET_LLM (cached)"
fi

if [[ "$LLM_MODEL_SHA256" != "<FILL_AFTER_FIRST_DOWNLOAD>" ]]; then
    verify_sha "$TARGET_LLM" "$LLM_MODEL_SHA256"
fi

# ── Symlink for env-var resolution ───────────────────────────────────────────
# MNEMOS_BUNDLED_BIN_DIR resolves to <dir>/llama-server, but the fetched binary
# is llama-server-linux-x86_64. Create a symlink to bridge the naming gap.
if [[ -f "$TARGET_BINARY" ]] && [[ ! -e "assets/llama-server" ]]; then
    ln -sf "$(basename "$TARGET_BINARY")" "assets/llama-server"
    echo "✓ assets/llama-server → $(basename "$TARGET_BINARY")"
fi

echo
echo "=== summary ==="
ls -la assets/*.gguf assets/llama-server* 2>/dev/null
echo
echo "Total size:"
du -ch assets/llama-server-linux-x86_64 assets/${MODEL_NAME} assets/${LLM_MODEL_NAME} 2>/dev/null | tail -1

