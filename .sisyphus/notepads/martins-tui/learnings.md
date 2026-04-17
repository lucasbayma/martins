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
