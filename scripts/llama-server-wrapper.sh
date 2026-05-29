#!/usr/bin/env sh
# Wrapper that sets LD_LIBRARY_PATH so the dynamically-linked llama-server
# can find its bundled libllama.so / libggml*.so neighbors.
# Installed at /usr/bin/mnemos-llama-server by the packages.
# Real binary lives at /usr/lib/mnemos/llama-server.
set -eu
LIB_DIR="/usr/lib/mnemos"
export LD_LIBRARY_PATH="${LIB_DIR}:${LD_LIBRARY_PATH:-}"
exec "${LIB_DIR}/llama-server" "$@"
