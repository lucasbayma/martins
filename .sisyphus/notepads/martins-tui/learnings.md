## [T1] Cargo skeleton created
- All deps pinned as specified
- CI workflow: macos-latest + ubuntu-latest matrix
- .sisyphus/evidence/.gitkeep created for QA
## [T2] Module layout created
- All 23 module files stubbed
- State types defined with serde derives
- InputMode enum in keys.rs
- AppError in error.rs
- cargo build passes with stub modules

## [T3] MPB names implemented
- 50 ARTISTS const, all pass validate()
- generate_name: Fisher-Yates shuffle + suffix fallback
- normalize: NFD + ascii filter + lowercase + space→dash
- All tests pass

## [T4] State persistence implemented
- Atomic write via tmp + rename pattern
- Backup recovery: copy (not rename) existing valid json to .bak before writing
- Version check: UnsupportedVersion error for version != 1
- load() fallback chain: main → .bak → default
- All 5 tests pass

## [T5] Config paths implemented
- repo_state_path: always {root}/.martins/state.json
- is_writable: probes .martins/.write_probe
- hash_repo_path: SHA-256 truncated to 12 hex chars
- ensure_gitignore: create/append/noop with line-by-line check
- All 5 config tests pass

## [T6] Pre-flight tools implemented
- Tool enum: Bat, Opencode, Claude, Codex
- detect(): uses which::which() ambient PATH
- detect_in(): uses which::which_in() for test isolation
- preflight(): returns MissingTools with all absent tools
- install_command(): OS-aware (macos=brew, linux=apt for bat; npm for agents)
- All tests pass
- Added #![allow(dead_code)] since module items aren't wired to main yet
- Fixed clippy: use `if let Some(path)` instead of `is_some()` + `unwrap()`

## [T7] Logging setup implemented
- init_logging(): rolling daily files via tracing-appender
- try_init() instead of init() to handle multiple test calls
- install_panic_hook(): logs panic + best-effort terminal restore
- File-only logging (no stdout/stderr during TUI)
- RUST_LOG env filter respected

## [T8] Input modes and keymap implemented
- Keymap::default_keymap() builds HashMap<KeyEvent, Action>
- EscapeDetector: double-Esc within 300ms → ExitTerminalMode
- Ctrl+B always exits Terminal mode immediately
- Terminal mode: all keys return None (forward to PTY) except Ctrl+B and double-Esc
- Normal mode: 'i' → EnterTerminalMode, Ctrl+Q → Quit
- All 7 tests pass

## [T9] Git repo ops implemented
- discover(): Repository::discover + workdir() for root path
- current_branch(): head().shorthand() or 8-char hash for detached HEAD
- current_branch_async(): spawn_blocking wrapper
- main_repo_root(): repo.commondir().parent() for worktree→main resolution
- All tests pass (worktree test skips gracefully if git CLI unavailable)
- CRITICAL: Always open Repository fresh per operation, never share

## [T11] PTY session lifecycle implemented
- spawn(): native_pty_system + openpty + spawn_command + background read thread
- Read thread: reads into vt100::Parser, sends exit code via oneshot on EOF
- resize(): master.resize() + parser.set_size()
- kill(): best-effort via writer close (Drop closes master → SIGHUP)
- vt100::Parser created with 1000-line scrollback
- Tests use tokio::runtime::Runtime::new().block_on() for async oneshot
- All 3 tests pass

## [T12] File watcher implemented
- new_debouncer(750ms) with noise filter in callback
- NOISE_DIRS: .git/, target/, node_modules/, .martins/, dist/, build/, .next/, .venv/
- FsEvent::Changed / FsEvent::Removed
- next_event(): async recv from unbounded channel
- Tests: detect_change, filter_noise, debounce_rapid all pass
- CRITICAL: notify-debouncer-mini 0.4 depends on notify 6.x internally — must use notify_debouncer_mini::notify re-exports, NOT the top-level notify 8.x crate
- is_noise() needs both contains(/.git/) AND ends_with(/.git) to catch dir creation events

## [T10] Git worktree CRUD implemented
- create(): validates name via mpb::validate, creates sibling dir, new branch, WorktreeAddOptions with reference
- prune(): WorktreePruneOptions::working_tree(true) + remove_dir_all + optional branch delete
- count_unpushed_commits(): revwalk push(head) + hide(base) + count()
- list(): repo.worktrees() + find_worktree() for each
- All 3 tests pass

## [T13] Git diff implemented
- modified_files(): diff_tree_to_workdir_with_index + status API for untracked
- Sort: untracked first, then alphabetical
- DiffError::BaseBranchMissing when branch not found
- All 3 tests pass

## [T14] .gitignore bootstrap verified
- ensure_gitignore() already in config.rs from T5
- GitignoreAction: Created/Appended/NoChange
- All config tests pass

## [T15] Responsive layout implemented
- compute(): breakpoints at 80/100/120 cols
- show_left forced false at <100, show_right forced false at <120
- sidebar_w = clamp(20, 30, 20% of frame)
- status_bar: always 1 row at bottom
- theme.rs: all design tokens as Color::Rgb constants
- All 5 tests pass

## [T16] Left sidebar implemented
- render(): ratatui List widget with ListState for scrolling
- Status icons: ● ○ ◐ ⋯ with correct colors from theme
- Archived section with ▼ header and indented items
- Empty state: "No workspaces. Press 'n' to create one."
- TestBackend used for unit tests (no snapshot files needed for MVP)
- All 2 tests pass

## [T17] Right sidebar implemented
- render(): List widget with status icons M/A/D/R/?
- truncate_path(): prefix "..." for paths > max_width
- Empty state: "No changes."
- All 3 tests pass

## [T19] Modal system implemented
- Modal enum: None/NewWorkspace/ConfirmDelete/InstallMissing
- centered_rect(): percentage-based centering with Clear widget
- render(): dispatches to sub-renderers
- ConfirmDelete: red border + ⚠ warning when unpushed_commits > 0
- All 3 tests pass

## [T20] Fuzzy picker implemented
- Picker::new(): initializes with all items visible
- update_filter(): nucleo Pattern::parse + score, top 20 results
- on_key(): char→append+filter, Backspace→pop+filter, Down/Up→navigate, Enter→Selected, Esc→Cancelled
- render(): 3-section layout (input/list/footer), 60%×50% centered
- All 4 tests pass

## [T18] Terminal pane + PtyManager implemented
- PtyManager: HashMap<(WorkspaceId, TabId), PtySession>, max 5 tabs enforced
- render(): tab bar (1 row) + PseudoTerminal widget from tui-term
- Border color: gold=Normal, sage=Terminal mode
- try_read() on parser to avoid blocking render
- All 4 tests pass (2 manager + 2 terminal)

## [T21] Bat preview + editor spawn implemented
- bat_preview(): runs bat --color=never, falls back to fs::read_to_string
- render_preview(): 80%×80% centered overlay with Clear widget
- open_in_editor(): disable_raw_mode + LeaveAlternateScreen before spawn
- Caller must re-enter raw mode after open_in_editor() returns
- All 4 tests pass

## [T22] Main event loop implemented
- App::new(): discovers repo, loads state, gets base branch
- run(): tokio::select! on EventStream + 5s refresh tick
- draw(): dispatches to all UI panes, overlays modal/picker/preview
- handle_key(): picker → modal → terminal → normal mode priority
- dispatch_action(): Quit, navigation, mode switch, sidebar toggle, fuzzy, archive
- ratatui::init() / ratatui::restore() for terminal setup/teardown
- cargo build succeeds, binary launches

## [T23] Agent detection + workspace creation implemented
- detect_agents(): checks Opencode/Claude/Codex via which::which
- default_agent(): first available or Opencode fallback
- create_workspace_entry(): validates name via mpb::validate, generates if empty
- Workspace status starts as Inactive (not Active — PTY not spawned yet)
- All 5 tests pass

## [T24] README written
- Covers: features, requirements, install (brew/cargo/source), usage, keybindings, state, dev
- Homebrew tap: bayma/martins (placeholder for T26)
