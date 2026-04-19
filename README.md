<p align="center">
  <a href="https://github.com/lucasbayma/martins/releases"><img src="https://img.shields.io/github/v/release/lucasbayma/martins?color=gold&label=release" alt="Release" /></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-1.85%2B-orange?logo=rust" alt="Rust" /></a>
  <img src="https://img.shields.io/badge/os-macOS-lightgrey?logo=apple" alt="macOS only" />
  <a href="LICENSE"><img src="https://img.shields.io/github/license/lucasbayma/martins?color=blue" alt="License" /></a>
  <a href="https://github.com/lucasbayma/martins/stargazers"><img src="https://img.shields.io/github/stars/lucasbayma/martins?style=flat&color=yellow" alt="Stars" /></a>
</p>

<p align="center">
  <img src="resources/martins.png" width="1000" alt="João Carlos Martins" />
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#usage">Usage</a> ·
  <a href="#keyboard-shortcuts">Keyboard Shortcuts</a> ·
  <a href="#architecture--stack">Architecture</a> ·
  <a href="#contributing">Contributing</a>
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
| `1-9` | Switch tab (Normal mode) |
| `F1-F9` | Switch tab (any mode) |
| `Ctrl+B` | Switch to sidebar |
| `d` | Delete workspace |
| `a` | Archive workspace |
| `p` | Preview file |
| `[` / `]` | Toggle sidebars (Normal mode) |
| `?` | Show help |
| `q` | Quit |

## Architecture & Stack

Martins is built on a stack chosen for speed, reliability, and zero-compromise terminal rendering.

### Rust

The entire application is written in Rust. No garbage collector, no runtime overhead. The event loop processes input, renders frames, and manages PTY sessions within a single `tokio` async runtime. The result is sub-millisecond input latency and a ~4MB binary with no external runtime dependencies.

### tmux

Every tab runs inside its own [tmux](https://github.com/tmux/tmux) session. When you close Martins, the tmux sessions keep running in the background — your agents continue working. When you reopen, Martins reattaches to the existing sessions and restores the full terminal output, scrollback, and conversation state. No lost context, no restarted agents, no wasted tokens.

### ratatui + crossterm

The TUI is rendered with [ratatui](https://github.com/ratatui/ratatui) and [crossterm](https://github.com/crossterm-rs/crossterm), the standard Rust terminal UI stack. Full mouse support with click-to-navigate projects, workspaces, tabs, and files. Client-side text selection with automatic clipboard copy — no modifier keys needed.

### portable-pty + vt100

Each terminal tab is a real PTY session managed by [portable-pty](https://github.com/wez/wezterm/tree/main/pty). Terminal output is parsed by the [vt100](https://github.com/doy/vt100-rust) crate, which implements a complete VT100/xterm terminal emulator in software. This means full support for colors, cursor positioning, alternate screen buffers, and any TUI application running inside the agents.

### git2 + worktrees

Each workspace gets its own [git worktree](https://git-scm.com/docs/git-worktree) via [libgit2](https://github.com/rust-lang/git2-rs). Worktrees are isolated copies of the repository that share the same `.git` directory — so each agent works on its own branch without interfering with others. No stashing, no branch switching, no merge conflicts between agents. Worktrees are stored at `~/.martins/` to keep your project directories clean.

### Why this stack

| Concern | How it's solved |
|---|---|
| **Performance** | Rust + async tokio — no GC pauses, 60fps rendering, 16KB PTY read buffers |
| **Session persistence** | tmux — agents survive app restarts, terminal crashes, SSH disconnects |
| **Isolation** | git worktrees — each agent gets its own branch and working directory |
| **Terminal fidelity** | vt100 parser — full escape sequence support, alternate screen, colors, cursor |
| **Native feel** | ratatui — renders directly to the terminal, no Electron, no web views, no overhead |
| **Clipboard** | Client-side selection with `pbcopy` — works in any terminal emulator |
| **Binary size** | ~4MB static binary — `brew install` and you're done |

## Requirements

- **macOS** (Apple Silicon or Intel)
- **tmux** — session persistence backend
- **Git** — worktree and repository management
- At least one AI agent installed: [OpenCode](https://github.com/nicholaskoerfer/opencode), [Claude Code](https://docs.anthropic.com/en/docs/claude-code), or [Codex](https://github.com/openai/codex)

## Contributing

Contributions are welcome! Check the [Contributing Guide](CONTRIBUTING.md) for setup instructions, project structure, and how to submit a PR.

## License

[MIT](LICENSE)
