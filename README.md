# Mnemos

**Local-first memory for AI tools.** Mnemos gives your AI assistants persistent memory — they learn from every session, recall context automatically, and get smarter over time. All data stays on your machine.

## Install the Desktop App

> **Linux only** in this release. macOS and Windows support is planned.

### Option A: Download the installer (fastest)

Download the latest package from the [Releases page](https://github.com/UnpaidAttention/mnemos/releases/latest):

**Debian / Ubuntu:**
```bash
# Download from the releases page, then:
sudo dpkg -i Mnemos_0.8.0_amd64.deb
```

**Fedora / RHEL:**
```bash
sudo rpm -i Mnemos-0.8.0-1.x86_64.rpm
```

Launch **Mnemos** from your application launcher. Done.

---

### Option B: Build from source

<details>
<summary>Click to expand build instructions</summary>

#### Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| **Rust** | stable ≥ 1.78 | [rustup.rs](https://rustup.rs) |
| **Node.js** | 20+ | [nodejs.org](https://nodejs.org) |
| **pnpm** | 9+ | `npm install -g pnpm` |
| **System libs** | — | See below |

**Debian / Ubuntu:**
```bash
sudo apt-get install -y libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev \
  librsvg2-dev libappindicator3-dev patchelf libssl-dev
```

**Fedora / RHEL:**
```bash
sudo dnf install -y gtk3-devel webkit2gtk4.1-devel libsoup3-devel \
  librsvg2-devel libappindicator-gtk3-devel patchelf openssl-devel
```

#### Setup (one-time)

```bash
git clone https://github.com/UnpaidAttention/mnemos.git
cd mnemos

# 1. Build the daemon + CLI
cargo build --release -p mnemos_cli -p mnemos_daemon

# 2. Install to your PATH
cp target/release/mnemos target/release/mnemosd ~/.cargo/bin/

# 3. Start the daemon
mnemosd &

# 4. Install frontend dependencies
cd desktop
pnpm install
```

#### Launch the Desktop App

```bash
cd desktop
pnpm tauri dev
```

#### Build a standalone installer (optional)

```bash
pnpm tauri build
# Outputs .deb/.rpm in desktop/src-tauri/target/release/bundle/
```

</details>


## What You Get

| Feature | Description |
|---------|-------------|
| **Desktop GUI** | Browse, search, edit, and manage all memories in a native app |
| **Knowledge Graph** | Interactive graph of entities and relationships, powered by Sigma.js |
| **Hybrid Search** | BM25 + dense vectors + graph-aware PageRank, fused with RRF |
| **Learning Pipeline** | Auto-extracts facts from AI sessions — no manual `remember` calls |
| **Reflections** | The system synthesizes insights from your memories over time |
| **Timeline** | Bi-temporal view with time-travel cursor |
| **MCP Integration** | Connects to Claude Code, Gemini CLI, and any MCP-aware client |
| **Bundled Embedder** | Local embeddings via llama.cpp — no API keys needed |
| **Audit Log** | Full provenance trail for every memory operation |

## Connect Your AI Tools

After launching the desktop app, go to **Settings → Connections** to connect your AI tools:

### Claude Code (automatic)

```bash
# Copy the MCP config snippet into your Claude Code config:
cat adapters/claude-code/claude_mcp_config.json
# Add it to ~/.config/claude-code/mcp_servers.json
```

### Run as a System Service (optional)

Keep the daemon running in the background so memories are always available:

```bash
mnemos service install     # install systemd unit
mnemos service enable      # enable and start
mnemos daemon status       # verify
```

## CLI Quick Reference

```bash
mnemos remember "User prefers Tauri"       # store a memory
mnemos recall "what does the user like"    # semantic search
mnemos list --tier semantic --limit 10     # browse by tier
mnemos get <id>                            # fetch a specific memory
mnemos forget <id>                         # soft-delete a memory
mnemos doctor                              # health check
mnemos daemon start|stop|status|logs       # daemon management
```

## Configuration

Settings are managed through the desktop app (**Settings** tab), or manually in `~/.config/mnemos/config.toml`.

### Switching the Embedder or LLM

```bash
export MNEMOS_EMBEDDER=ollama       # bundled / ollama / openai / mock / none
export MNEMOS_LLM=openai             # ollama / openai / mock / none
export OPENAI_API_KEY=sk-...         # if using openai
mnemos daemon restart
```

## Data Location

| Path | Contents |
|------|----------|
| `~/.local/share/mnemos/files/` | Markdown memory files |
| `~/.local/share/mnemos/index.db` | SQLite index + vector table |
| `~/.config/mnemos/config.toml` | Configuration |
| `~/.config/mnemos/token` | Daemon auth token (mode 0600) |

Override the vault root: `--vault <path>` or `MNEMOS_VAULT=<path>`.

## Advanced

- **[BUILD.md](BUILD.md)** — Full build guide, cross-compilation, code-signing, packaging
- **[SECURITY.md](SECURITY.md)** — Security model and threat analysis

## License

Apache-2.0

[releases]: https://github.com/UnpaidAttention/mnemos/releases/latest
