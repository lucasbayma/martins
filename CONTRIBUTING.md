# Contributing to Martins

Thanks for your interest in contributing! Martins is an open-source project and we welcome contributions of all kinds: bug reports, feature requests, documentation, and code.

## Getting Started

### Prerequisites

- **macOS** (the only supported platform)
- **Rust 1.85+** — `brew install rust`
- **tmux** — `brew install tmux`
- **Git** — `brew install git`

### Setup

```bash
git clone https://github.com/lucasbayma/martins.git
cd martins
cargo build
cargo test
```

### Running locally

```bash
cargo run                      # from a git repo directory
cargo run -- /path/to/repo     # or specify a project
```

## Development Workflow

### Branch naming

- `feat/short-description` — new features
- `fix/short-description` — bug fixes
- `docs/short-description` — documentation only
- `refactor/short-description` — code restructuring

### Before submitting a PR

1. **Build** — `cargo build` must succeed
2. **Tests** — `cargo test` must pass
3. **Lint** — `cargo clippy --all-targets -- -D warnings` must be clean
4. **Format** — `cargo fmt --check` must pass

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

### Commit messages

Use conventional commits:

```
feat: add workspace rename support
fix: terminal not resizing on window change
docs: update keyboard shortcuts table
refactor: extract tmux session management
```

## Project Structure

```
src/
├── main.rs          # Entry point, terminal setup, global state loading
├── app.rs           # App struct, event loop, action dispatch, draw
├── state.rs         # GlobalState, Project, Workspace, persistence
├── config.rs        # Paths (global state, logs, worktrees)
├── keys.rs          # Keymap, actions, input modes
├── agents.rs        # Workspace creation, name generation
├── tmux.rs          # tmux session management (create, attach, resize, kill)
├── tools.rs         # External tool detection
├── watcher.rs       # File system watcher for git changes
├── git/
│   ├── diff.rs      # Git diff parsing
│   ├── repo.rs      # Repository discovery
│   └── worktree.rs  # Git worktree create/prune
├── pty/
│   ├── manager.rs   # PTY session registry, spawn/close/resize
│   └── session.rs   # Individual PTY session (portable-pty + vt100)
└── ui/
    ├── layout.rs    # Pane layout computation
    ├── modal.rs     # Modal dialogs (new workspace, folder browser, help, confirm)
    ├── picker.rs    # Fuzzy picker overlay
    ├── sidebar_left.rs   # Project/workspace tree
    ├── sidebar_right.rs  # Modified files list
    ├── terminal.rs       # Terminal pane with tab bar
    ├── theme.rs          # Color constants
    └── preview.rs        # File preview
```

## Architecture Overview

- **ratatui** + **crossterm** for the TUI rendering and input
- **portable-pty** for pseudo-terminal management
- **vt100** for terminal output parsing
- **tui-term** for rendering parsed terminal output
- **tmux** for persistent sessions that survive app restarts
- **git2** for worktree and diff operations

Each workspace tab runs inside its own tmux session. The PTY spawns `tmux attach-session` to connect to it. When the app closes, tmux sessions keep running. On restart, sessions are reattached.

## What to Contribute

### Good first issues

- Improve error messages for common failures
- Add more auto-generated workspace names (MPB artists)
- Better empty state screens
- Documentation improvements

### Feature ideas

- Linux support
- Configurable keybindings
- Workspace templates
- Session sharing between users
- Git branch visualization in sidebar

### Areas that need help

- Test coverage for UI components
- Accessibility improvements
- Performance profiling for large repositories

## Submitting a Pull Request

1. Fork the repo and create your branch from `main`
2. Make your changes
3. Run the full check suite (build + test + clippy + fmt)
4. Open a PR with a clear description of what changed and why
5. Link any related issues

## Reporting Bugs

Open an issue with:

- macOS version and architecture (Apple Silicon / Intel)
- Martins version (`martins --version` or git commit)
- Steps to reproduce
- Expected vs actual behavior
- Terminal emulator (iTerm2, Terminal.app, Alacritty, etc.)

## Code of Conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you agree to uphold it.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
