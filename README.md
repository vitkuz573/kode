# kode

**kode** — universal AI coding CLI built in Rust. Multi-model routing, streaming, agent mode with tools, beautiful TUI, sessions, cost tracking.

## Features

- **Multi-model routing** — switch between any OpenAI-compatible provider/model on the fly
- **Agent mode** — autonomous loop with tools: `read_file`, `write_file`, `bash`, `list_dir`, `glob`
- **Streaming** — real-time token streaming with reasoning/thinking block support (DeepSeek, QwQ, o1)
- **TUI** — beautiful terminal UI built with ratatui + Catppuccin/Nord/Dracula/Gruvbox/Tokyo Night themes
- **Sessions** — persistent conversation history in `~/.local/share/kode/sessions/`
- **Context management** — sliding window to stay within token budget
- **Cost tracking** — real-time token count and USD estimate
- **Pipe-friendly** — works as a unix filter: `echo "..." | kode`
- **Command palette** — `Ctrl+P` for all commands
- **Mouse support** — scroll with mouse wheel
- **Auto model discovery** — fetches available models from provider's `/models` endpoint

## Install

```bash
git clone https://github.com/vitkuz573/kode
cd kode
cargo build --release
cp target/release/kode ~/.local/bin/
```

## Usage

```bash
# Interactive TUI
kode

# One-shot prompt
kode --prompt "explain this code"

# Pipe mode
echo "what is Rust?" | kode

# Agent mode (with tools)
kode --prompt "list files and summarize the project" --agent

# Subcommands
kode models          # list configured models
kode sessions        # list saved sessions
kode config          # show config path
```

## TUI Keybindings

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+P` | Command palette |
| `Ctrl+T` | Theme picker |
| `Ctrl+M` | Model picker |
| `Ctrl+B` | Toggle sidebar |
| `Ctrl+N` | New session |
| `Ctrl+L` | Clear chat |
| `Ctrl+R` | Refresh models from provider |
| `Tab` | Session list |
| `↑↓ / PgUp/PgDn` | Scroll messages |
| `Ctrl+C` | Quit |

## Configuration

Config is auto-created at `~/.config/kode/config.toml` on first run.

```toml
model = "omniroute/kr/auto"

[providers.omniroute]
base_url = "http://127.0.0.1:20128/v1"
api_key = "sk-..."
name = "OmniRoute"
models = ["kr/auto"]

[agent]
max_steps = 32
temperature = 0.1

[context]
max_tokens = 128000
strategy = "sliding"

[cost]
show = true
```

## Architecture

```
kode/
├── src/
│   ├── main.rs          # CLI entry point (clap), pipe mode, subcommands
│   └── tui_runner.rs    # TUI event loop, mouse, agent spawning
└── crates/
    ├── kode-core/       # Config, Session, Context manager, Cost tracker, Types
    ├── kode-llm/        # OpenAI-compatible client, SSE streaming, model router
    ├── kode-agent/      # Agent loop, tool registry, built-in tools
    └── kode-tui/        # ratatui UI, themes, markdown renderer, input handler
```

## License

MIT
