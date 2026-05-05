# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.0] - 2026-05-05

### Added

- **GSD agent option** — `gsd` is now a first-class agent alongside `opencode`, `claude`, and `codex`. Detected via `which gsd`, surfaced in the agent picker modal, and included in pre-flight tool checks.

## [0.8.0] - 2026-04-27

The Fluidity milestone. Eliminate input lag, render-loop CPU burn, and background-work spikes; ship Ghostty-style text selection on the PTY main pane. Single-contributor work over 4 days, 22 plans, 0 → 145 tests, operator-validated against the qualitative "feels like Ghostty" gate.

### Added

- **Dual-path PTY-pane text selection** — tmux-native delegate path for non-mouse-app sessions (bash/zsh) plus REVERSED-XOR overlay retained for mouse-app sessions (vim mouse=a, htop, opencode). Selection feels indistinguishable from running tmux directly in Ghostty
- **3-tier `cmd+c` precedence:** overlay selection → tmux paste-buffer (`tmux save-buffer | pbcopy`) → SIGINT to active PTY
- **3-tier `Esc` precedence:** overlay clear → forward `\x1b` to delegating tmux (single-press copy-mode exit) → fall-through to PTY
- **Tab/workspace switch cancels outgoing tmux selection** so leftover highlights don't persist across navigation
- **Selection survives streaming PTY output** via anchored coordinate translation (`Arc<AtomicU64> scroll_generation` on `PtySession`, drain-loop sourced)
- **Double-click word selection, triple-click line selection, shift+click extend** in the overlay path
- **Auto-copy on mouse-up** in the delegate path via tmux's `copy-pipe-and-cancel pbcopy`
- `MARTINS_MOUSE_DEBUG` env var for diagnostic mouse-event + selection-render tracing (zero-cost when off, opt-in for future bug investigations)

### Changed

- **`src/app.rs` split** from 2000+ LOC monolith into focused modules: `events.rs` (event routing + `encode_sgr_mouse`), `workspace.rs` (lifecycle), `ui/modal_controller.rs` (modal dispatch), `ui/draw.rs` (top-level render). Final `app.rs` core: 436 LOC
- **Render loop now gated by dirty-flag** — `terminal.draw()` only fires when state changed; idle CPU drops to near-zero
- **`tokio::select!` made biased with input branch first** — keyboard/mouse never starved by PTY output bursts or background timers
- **Diff refresh: 5s polling timer dropped** in favor of event-driven flow via debounced `notify` (~200ms) plus a 30s safety-net fallback
- **Tab/workspace switching: `refresh_diff` is fire-and-forget** via new `refresh_diff_spawn` + `mpsc` drain branch — no blank-frame stutter on the nav hot path
- **State save (`~/.martins/state.json`):** 13 hot-path call sites migrated to async `save_state_spawn`; archive `remove_dir_all` wrapped in `spawn_blocking`
- **Overlay selection highlight:** switched from XOR-toggled `Modifier::REVERSED` to tmux's default `mode-style` (fg=Black, bg=Yellow) for visual parity with native tmux
- **Per-gesture delegation latch** (`tmux_gesture_delegating: Arc<AtomicBool>`) — Drag/Up always honor the latch even if the inner program flips mouse mode mid-gesture, preventing tmux's button state from getting orphaned

### Fixed

- Per-keystroke render lag in the PTY pane (PTY-01..03)
- Sidebar/workspace/tab-switch latency — instant on keyboard or mouse (NAV-01..04)
- Random lag spikes from background work (BG-01..05): no more 5s polling-timer pauses, no more blocking state writes
- Selection highlight no longer flickers, jitters, or disappears under streaming PTY output (SEL-04)
- `set_active_tab` now clears `tmux_in_copy_mode` and `tmux_drag_seen` flags on the outgoing session before mutating `active_tab`, preventing stale state on tab round-trip

## [0.7.0] - 2026-04-21

### Added

- `martins workspaces prune` to remove orphan workspace directories after explicit confirmation
- `martins --version` and `martins -v` to print the current application version
- Archived workspace entries in the left sidebar, collapsed by default per project

### Changed

- Clicking `✕` on an active workspace now archives it instead of deleting it immediately
- Archived workspaces can be permanently deleted from the archived section while preserving their state entry as `Deleted`

### Fixed

- Homebrew formula and tap release metadata now match the published macOS asset URL for releases

## [0.6.0] - 2026-04-20

### Added

- CLI subcommands for workspace management: `martins workspaces list`, `martins workspaces remove <project> <name>`, `martins workspaces archive <project> <name>`, `martins workspaces unarchive <project> <name>`
- `martins keybinds` command to print keyboard shortcuts from the terminal

## [0.5.0] - 2026-04-19

### Added

- Expanded agent picker: aider, gemini, amp, goose, cline now available alongside opencode, claude, codex, and shell
- Custom command args: when selecting an agent, a form lets you add extra arguments (e.g. `claude --model opus`)
- Sidebar workspace-only navigation: mouse scroll and keyboard j/k skip project headers and buttons, landing only on workspaces

## [0.4.0] - 2026-04-19

### Added

- Click a file in the modified files sidebar to open a diff tab in the workspace. The tab runs `git diff --color=always <file> | less -R` in the worktree
- Diff tab also works for untracked (new) files — shows the full content as additions via `git diff --no-index`

### Fixed

- Terminal scroll is now instant and precise — uses native SGR mouse events directly to PTY instead of `tmux send-keys` subprocess (no more lag or jumpy scrolling)
- Clicking a file with scrollback no longer opens the wrong file — click mapping now uses absolute index (offset + visible row)
- Tab close click (the `✕` icon) now lines up correctly with tab labels that differ from command (e.g. `diff:app.rs`)
- Added 200ms delay between tmux session creation and initial command send to avoid shell init race

### Changed

- Sidebar toggle back to `[` / `]` (Normal mode) — simpler than `Ctrl+B/N` which conflicted with other bindings
- Help modal updated to reflect current shortcuts

## [0.3.4] - 2026-04-18

### Added

- Working/done indicator on each workspace in the sidebar: ⚡ when any tab had output in the last 2 seconds, ✓ otherwise
- Bracketed paste forwarded to PTY — agents now detect large pastes and show `[Pasted N lines]` instead of rendering character by character

### Fixed

- Modified files sidebar now shows the diff for the active workspace's worktree, not the main repo

## [0.3.3] - 2026-04-18

### Added

- New workspaces now auto-create a shell tab — no manual tab creation needed for the first terminal

### Changed

- Clicking a workspace with tabs focuses the terminal pane (previously stayed on sidebar)

### Fixed

- Removed `e` keybinding that launched `$EDITOR` in the outer terminal, causing screen corruption when the editor exited. File editing should be done through the agent.

## [0.3.2] - 2026-04-18

### Fixed

- Mouse clicks now work on interactive elements inside the terminal pane (agent buttons, prompts). Click events are forwarded to the PTY as SGR mouse escape sequences. Drag still triggers text selection.

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
