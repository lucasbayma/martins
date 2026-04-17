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
