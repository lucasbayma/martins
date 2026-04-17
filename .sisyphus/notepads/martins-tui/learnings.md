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
