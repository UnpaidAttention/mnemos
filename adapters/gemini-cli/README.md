# Gemini CLI adapter

Use mnemos as Gemini CLI's persistent memory.

## Install

Add the snippet from `gemini-mcp.json` into your Gemini CLI MCP config
(`~/.config/gemini-cli/mcp.json` or platform equivalent). After restart,
Gemini CLI exposes mnemos's `remember`, `recall`, `forget`, `get_memory`,
and `list_memories` tools.

## Verify

```
gemini -t mnemos.remember --body "Use Tauri 2"
gemini -t mnemos.recall --query "desktop framework"
```
