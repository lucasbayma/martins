<p align="center">
  <img src="resources/martins.png" width="1000" alt="João Carlos Martins" />
</p>

# Martins

> *"A música não tem fronteiras."* — João Carlos Martins

Named after the legendary Brazilian maestro **João Carlos Martins** — a man who lost the movement of his hands, twice, and still found a way to conduct an orchestra. Martins is a terminal workspace manager for AI coding agents, built with the same stubbornness: no matter how many tools you juggle, how many projects you run, or how many times you close and reopen your terminal — the show goes on.

Just as Maestro Martins conducts dozens of musicians into a single symphony, this tool orchestrates multiple AI agents — OpenCode, Claude Code, Codex — each running in their own workspace, each on their own branch, all under one baton.

The workspaces carry the spirit of Brazilian music. Names are auto-generated from the tradition of **MPB** — you might get a *caetano*, a *gil*, an *elis*, a *chico*, a *gal*, a *milton*, a *djavan*, a *marisa*. Each workspace is a musician in your orchestra. Each tab is an instrument. Together, they make something worth listening to.

---

## Platform

> **macOS only.** Martins is built and tested exclusively for macOS. Linux and Windows are not supported at this time.

## Features

- **Multi-project** — open and switch between multiple git repositories
- **Workspaces** — named workspaces per project with isolated git worktrees
- **Tabs** — run multiple agents and shells side-by-side within each workspace
- **Persistent sessions** — powered by tmux, conversations survive app restarts
- **Mouse support** — click to navigate projects, workspaces, tabs, and files
- **Folder browser** — visual directory picker to add projects

## Install

### Homebrew (recommended)

```bash
brew tap lucasbayma/martins
brew install martins
```

### From source

Make sure you have Rust, tmux, and Git installed:

```bash
brew install rust tmux git
```

Then build and install:

```bash
git clone https://github.com/lucasbayma/martins.git
cd martins
cargo install --path .
```

The `martins` binary will be available at `~/.cargo/bin/martins`. Make sure `~/.cargo/bin` is in your `PATH`.

## Usage

```bash
martins                    # open with last session
martins /path/to/repo      # open with specific project
```

Once inside:

1. **Add a project** — press `a` or click `+ Add Project` in the sidebar. A folder browser opens at `~/`. Navigate to a git repository and press Enter to open it.
2. **Create a workspace** — press `n`. Type a name or leave blank for auto-generated MPB names (caetano, gil, elis...). The workspace creates an isolated git worktree.
3. **Open a tab** — press `t` or click `[+]` in the tab bar. Choose an agent (OpenCode, Claude Code, Codex) or a plain shell.
4. **Interact with the terminal** — press `i` or click the terminal pane to enter Terminal mode. All keystrokes go to the agent. Press `Esc Esc` to exit back to Normal mode.
5. **Switch between tabs** — press `1`-`9` or click the tab labels.
6. **Close and reopen** — press `q` to quit. All tmux sessions persist. Next time you run `martins`, all your tabs and conversations are restored exactly where you left off.

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate selection |
| `n` | New workspace |
| `t` | New tab (agent/shell) |
| `T` | Close tab |
| `1-9` | Switch tab |
| `i` | Enter terminal mode |
| `Esc Esc` | Exit terminal mode |
| `d` | Delete workspace |
| `a` | Archive workspace |
| `p` | Preview file |
| `e` | Edit file in $EDITOR |
| `[` / `]` | Toggle sidebars |
| `?` | Show help |
| `q` | Quit |

## Requirements

- **macOS** (Apple Silicon or Intel)
- **tmux** — session persistence backend
- **Git** — worktree and repository management
- At least one AI agent installed: [OpenCode](https://github.com/nicholaskoerfer/opencode), [Claude Code](https://docs.anthropic.com/en/docs/claude-code), or [Codex](https://github.com/openai/codex)

## Contributing

Contributions are welcome! Check the [Contributing Guide](CONTRIBUTING.md) for setup instructions, project structure, and how to submit a PR.

## License

[MIT](LICENSE)
