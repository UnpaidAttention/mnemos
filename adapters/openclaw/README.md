# Openclaw adapter

A shell wrapper that captures Openclaw CLI sessions and feeds them through
mnemos so the daemon's extraction pipeline turns them into semantic
memories.

## Install

```bash
cp openclaw-wrapper.sh ~/.local/bin/openclaw-with-mnemos
chmod +x ~/.local/bin/openclaw-with-mnemos
# Add to ~/.bashrc or ~/.zshrc:
alias openclaw='openclaw-with-mnemos'
```

The wrapper starts a mnemos session, streams the original openclaw output
through to your terminal while POSTing each line as a chunk, and ends the
session on exit. Mnemos's pipeline (Plan 4) then extracts facts
asynchronously.
