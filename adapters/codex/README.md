# OpenAI Codex CLI adapter

Wire mnemos into the Codex CLI so it can `remember` and `recall` across
sessions.

## Install

Merge `codex.config.json` into `~/.codex/config.json` (combine the `mcp`
object if you already have one).

## Verify

In a Codex session, `/tools` should list `mnemos.remember` and
`mnemos.recall`.
