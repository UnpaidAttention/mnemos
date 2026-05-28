#!/usr/bin/env bash
# openclaw wrapper that streams the session through mnemos.
set -u

MNEMOS_URL="${MNEMOS_URL:-http://localhost:7423}"
MNEMOS_TOKEN="${MNEMOS_TOKEN:-$(cat "$HOME/.config/mnemos/token" 2>/dev/null || true)}"

if [[ -z "$MNEMOS_TOKEN" ]]; then
  echo "openclaw-with-mnemos: no token at ~/.config/mnemos/token; running openclaw without capture" >&2
  exec openclaw "$@"
fi

start=$(curl -fsS -X POST "$MNEMOS_URL/v1/sessions" \
  -H "authorization: Bearer $MNEMOS_TOKEN" -H "content-type: application/json" \
  -d '{"source_tool":"openclaw"}' || true)
session_id=$(printf '%s' "$start" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("id",""))' 2>/dev/null || true)

if [[ -z "$session_id" ]]; then
  echo "openclaw-with-mnemos: could not start mnemos session; running openclaw without capture" >&2
  exec openclaw "$@"
fi

cleanup() {
  if [[ -n "$session_id" ]]; then
    curl -fsS -o /dev/null -X POST -H "authorization: Bearer $MNEMOS_TOKEN" \
      -H "content-type: application/json" -d "{}" \
      "$MNEMOS_URL/v1/sessions/$session_id/end" >/dev/null || true
  fi
}
trap cleanup EXIT

openclaw "$@" | while IFS= read -r line; do
  printf '%s\n' "$line"
  body=$(printf '%s' "$line" | python3 -c 'import json,sys; print(json.dumps({"speaker":"openclaw","body":sys.stdin.read()}))')
  curl -fsS -o /dev/null -X POST "$MNEMOS_URL/v1/sessions/$session_id/chunks" \
    -H "authorization: Bearer $MNEMOS_TOKEN" -H "content-type: application/json" -d "$body" || true
done
