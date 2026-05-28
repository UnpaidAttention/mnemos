# OpenAI function-calling schema

Copy-paste `schema.json` into any OpenAI chat-completions or assistants
request that supports `tools`. The five mnemos MCP tools become callable
functions; your code then proxies each call to `http://localhost:7423/v1/*`
(the daemon's REST API).

## Curl example

```bash
curl -fsS -X POST https://api.openai.com/v1/chat/completions \
  -H "authorization: Bearer $OPENAI_API_KEY" \
  -H "content-type: application/json" \
  -d "$(jq -n --slurpfile tools schema.json '{
    "model": "gpt-4o",
    "messages": [{"role":"user","content":"Remember that I prefer Tauri."}],
    "tools": $tools[0]
  }')"
```
