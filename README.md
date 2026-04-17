# martins

> A TUI for managing AI agent teams via git worktrees, one terminal, many agents.

`martins` is a terminal UI that lets you create and manage multiple AI coding agent workspaces (opencode, claude, codex) as git worktrees, all from a single terminal window.

## Features

- **3-pane layout**: workspace list, embedded terminal, modified files sidebar
- **Git worktrees**: each workspace is an isolated branch + directory
- **Multiple agents**: opencode, claude, codex support
- **Fuzzy picker**: `/` to search workspaces and files
- **File preview**: `p` to preview any modified file with bat
- **Responsive**: collapses sidebars at narrow widths

## Requirements

- Rust 1.85+
- Git 2.5+ (worktree support)
- One or more AI agents: [opencode](https://opencode.ai), [claude](https://claude.ai/code), [codex](https://github.com/openai/codex)
- Optional: [bat](https://github.com/sharkdp/bat) for syntax-highlighted previews

## Install

### Homebrew (macOS/Linux)

```bash
brew tap bayma/martins
brew install martins
```

### Cargo

```bash
cargo install martins
```

### From source

```bash
git clone https://github.com/bayma/martins
cd martins
cargo build --release
cp target/release/martins ~/.local/bin/
```

## Usage

Navigate to any git repository and run:

```bash
cd /path/to/your/project
martins
```

`martins` will discover the repository root automatically.

## Keybindings

### Normal Mode

| Key | Action |
|-----|--------|
| `j` / `↓` | Next item |
| `k` / `↑` | Previous item |
| `n` | New workspace (named) |
| `N` | New workspace (auto-name) |
| `a` | Archive workspace |
| `u` | Unarchive workspace |
| `d` | Delete workspace |
| `i` / `Tab` | Enter terminal mode |
| `/` | Fuzzy picker |
| `p` | Preview file |
| `e` | Open in $EDITOR |
| `Ctrl+B` | Toggle left sidebar |
| `Ctrl+N` | Toggle right sidebar |
| `q` / `Ctrl+Q` | Quit |

### Terminal Mode

| Key | Action |
|-----|--------|
| `Esc Esc` | Exit terminal mode (double-Esc within 300ms) |
| `Ctrl+B` | Exit terminal mode |
| All other keys | Forwarded to PTY |

## Workspace Names

Workspaces are named after Brazilian MPB artists (caetano, gil, elis, chico, ...). You can also provide a custom name. It must be lowercase alphanumeric with hyphens only.

## State

State is stored in `.martins/state.json` inside your repository (gitignored automatically). Falls back to `~/.local/share/martins/{repo-hash}/state.json` if the repo is read-only.

## Development

```bash
# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run

# Check formatting
cargo fmt --check

# Lint
cargo clippy --all-targets -- -D warnings
```

## License

MIT
