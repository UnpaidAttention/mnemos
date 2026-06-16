# Mnemos

**Local-first memory for AI tools.** Mnemos gives your AI assistants persistent memory — they learn from every session, recall context automatically, and get smarter over time. All data stays on your machine.

## Install the Desktop App

> **Linux only** in this release. macOS and Windows support is planned.

### Option A: Download the installer (fastest)

Grab the latest desktop bundle from the [Releases page](https://github.com/UnpaidAttention/mnemos/releases/latest):

**Debian / Ubuntu (.deb):**
```bash
# Download and install (or upgrade from a previous version):
wget https://github.com/UnpaidAttention/mnemos/releases/latest/download/Mnemos_0.9.4_amd64.deb
sudo dpkg -i Mnemos_0.9.4_amd64.deb
```

**Fedora / RHEL (.rpm):**
```bash
wget https://github.com/UnpaidAttention/mnemos/releases/latest/download/Mnemos-0.9.4-1.x86_64.rpm
sudo rpm -U Mnemos-0.9.4-1.x86_64.rpm
```

**AppImage (any distro):**
```bash
wget https://github.com/UnpaidAttention/mnemos/releases/latest/download/Mnemos_0.9.4_amd64.AppImage
chmod +x Mnemos_0.9.4_amd64.AppImage
./Mnemos_0.9.4_amd64.AppImage
```

Launch **Mnemos** from your application launcher. The daemon, CLI, and bundled embedder are all included — no separate installs needed.

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
cp target/release/mnemos target/release/mnemosd target/release/mnemos-mcp-stdio ~/.cargo/bin/

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
# Outputs .deb/.rpm/.AppImage in desktop/src-tauri/target/release/bundle/
```

</details>

---

## Updating from a Previous Version

### Desktop app users (`.deb` / `.rpm` / `.AppImage`)

Re-download and install the latest package — it upgrades in place:

**Debian / Ubuntu:**
```bash
wget https://github.com/UnpaidAttention/mnemos/releases/latest/download/Mnemos_0.9.4_amd64.deb
sudo dpkg -i Mnemos_0.9.4_amd64.deb
```

**Fedora / RHEL:**
```bash
wget https://github.com/UnpaidAttention/mnemos/releases/latest/download/Mnemos-0.9.4-1.x86_64.rpm
sudo rpm -U Mnemos-0.9.4-1.x86_64.rpm
```

### Source build users

```bash
cd mnemos
git pull origin master
cargo build --release -p mnemos_cli -p mnemos_daemon
cp target/release/mnemos target/release/mnemosd target/release/mnemos-mcp-stdio ~/.cargo/bin/
mnemos daemon restart
```

### After updating

Your existing vault and memories are preserved — schema migrations run automatically on daemon start. Check the [CHANGELOG.md](CHANGELOG.md) for what's new in each version.

If you previously connected AI tools, reconnect them to pick up updated MCP tool descriptions:

```bash
# In the Mnemos desktop app: Settings → Connections → Reconnect
# Or manually restart your AI tool (Claude Code, Antigravity IDE, etc.)
```


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

### Learning Pipeline (LLM)

The learning pipeline extracts facts from your AI sessions automatically. It needs a chat model. Three options:

| Option | Model | Size | Setup |
|--------|-------|------|-------|
| **Bundled** (default) | Qwen3-0.6B | 462 MB, runs on CPU | Works out of the box — no setup needed |
| **Ollama + Gemma 4** (recommended) | gemma4:12b | ~8 GB | Better quality; needs Ollama installed |
| **OpenAI** | gpt-4o-mini | Cloud | Needs API key |

**To switch to Ollama + Gemma 4 (recommended for best results):**

```bash
# 1. Install Ollama (https://ollama.ai)
curl -fsSL https://ollama.com/install.sh | sh

# 2. Pull the Gemma 4 model
ollama pull gemma4:12b

# 3. Switch Mnemos to use Ollama
export MNEMOS_LLM=ollama
mnemos daemon restart
```

Mnemos will automatically use `gemma4:12b` as the default model when Ollama is selected.

### Switching the Embedder

```bash
export MNEMOS_EMBEDDER=ollama       # bundled / ollama / openai / mock / none
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

## Troubleshooting

### "Embedder failed" / "Bundled LLM server not reachable" in Doctor tab

The desktop app bundles the embedder assets, but the daemon needs to find them. If the daemon was started outside the desktop app (e.g. from a terminal), it won't know where the bundled files are.

**Fix — symlink the bundled assets to the XDG location:**

```bash
# Find where the .deb installed the assets:
ASSETS_DIR="/usr/lib/Mnemos/_up_/_up_/assets"

# Create the XDG symlink so the daemon can find them:
mkdir -p ~/.local/share/mnemos
ln -sf "$ASSETS_DIR" ~/.local/share/mnemos/assets

# Restart the daemon:
mnemos daemon restart
# Or from the desktop app: Settings → Restart Daemon
```

**Alternative — use the desktop app to manage the daemon:**

The desktop app automatically sets the correct paths when it starts the daemon. If you always launch Mnemos from the app menu (not manually from terminal), the embedder should work automatically.

**Alternative — use Ollama instead of the bundled embedder:**

```bash
# Install Ollama (https://ollama.ai)
ollama pull nomic-embed-text

# Configure Mnemos to use Ollama:
export MNEMOS_EMBEDDER=ollama
mnemos daemon restart
```

### Memories not being captured during sessions

Check that:
1. The MCP server is connected — verify in **Settings → Connections**
2. The daemon is running — check the **Doctor** tab
3. An LLM is configured for the learning pipeline — check **Settings → LLM**

The learning pipeline requires an LLM (Ollama or OpenAI) to extract facts from sessions. The bundled embedder handles *search*, but *learning* needs a chat model:

```bash
# Option A: Use Ollama (free, local)
ollama pull llama3.2
export MNEMOS_LLM=ollama

# Option B: Use OpenAI
export MNEMOS_LLM=openai
export OPENAI_API_KEY=sk-...

mnemos daemon restart
```

## Advanced

- **[BUILD.md](BUILD.md)** — Full build guide, cross-compilation, code-signing, packaging
- **[SECURITY.md](SECURITY.md)** — Security model and threat analysis

## License

Apache-2.0

[releases]: https://github.com/UnpaidAttention/mnemos/releases/latest
