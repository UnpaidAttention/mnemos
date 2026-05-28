# Hermes Agent adapter

Hermes isn't MCP-native, so this adapter is a tiny Python REST client that
wraps mnemos's HTTP API. Drop `hermes_mnemos.py` next to your Hermes config
and import it.

## Install

```bash
cp hermes_mnemos.py ~/.config/hermes/plugins/
export MNEMOS_TOKEN="$(cat ~/.config/mnemos/token)"
```

## Use from a Hermes prompt

```python
from hermes_mnemos import remember, recall
remember("User prefers Tauri")
hits = recall("desktop framework", k=5)
```
