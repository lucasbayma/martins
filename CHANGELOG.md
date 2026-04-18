# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.1] - 2026-04-18

### Changed

- New tabs automatically focus the terminal pane — no manual click or key press needed

## [0.3.0] - 2025-04-18

### Added

- Client-side text selection (amux strategy) with automatic clipboard copy via pbcopy
- Bracketed paste support — Cmd+V pastes entire text at once instead of character by character
- Loading indicator when creating workspaces
- Archive workspace confirmation modal
- F1-F9 to switch tabs from any mode (Terminal or Normal)
- Ctrl+B to switch focus to sidebar from terminal
- Arrow keys in sidebar navigate across all projects and workspaces
- Worktrees stored at `~/.martins/workspaces/<project>/<workspace>`
- State stored at `~/.martins/state.json`
- OpenCode session resume with `opencode -c` on reattach
- Auto-restart agents that fell back to shell on app reopen

### Fixed

- Terminal freezing on workspace/tab switch (non-blocking tmux resize)
- Alternate screen escape leak (tmux config with `alternate-screen off` applied before session start)
- Dead PTY sessions no longer receive input (auto-exit Terminal mode)
- Removed project no longer re-added on restart
- Git diff sidebar now shows uncommitted changes (HEAD diff) instead of full branch diff
- tmux copy-mode no longer traps terminal input
- Sidebar workspace count shows only active workspaces, not archived
- PTY output latency eliminated (removed 16ms frame sleep)
- Default focus on terminal pane instead of sidebar

### Changed

- Workspace creation only asks for name (no agent selection)
- tmux options enforced at runtime on every session, not just config file
- `allow-passthrough` set to off for escape containment
- All tmux management commands null stdout to prevent outer terminal interference

## [0.1.0] - 2025-04-17

### Added

- Multi-project support with folder browser for adding git repositories
- Named workspaces per project with isolated git worktrees at `~/.martins/`
- Tabs for running multiple AI agents (OpenCode, Claude Code, Codex) and shells
- Persistent sessions via tmux that survive app restarts
- Full mouse support: click projects, workspaces, tabs, files, and buttons
- Keyboard shortcuts with `?` help modal
- Bottom menu bar with clickable actions
- Quit confirmation dialog
- Delete workspace and remove project with confirmation
- Workspace info screen when no tabs are open
- Auto-generated workspace names from Brazilian MPB artists
- Homebrew formula for macOS installation
