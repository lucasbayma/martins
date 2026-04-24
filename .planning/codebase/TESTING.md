# Testing

**Analysis Date:** 2026-04-24

## Framework

**Primary:** Built-in Cargo test runner (`cargo test`), with `#[test]` and `#[tokio::test]` attributes.

**Dev-dependencies declared in `Cargo.toml`:**
```toml
insta = "1.40"
tempfile = "3.10"
assert_cmd = "2.0"
predicates = "3"
```

- `insta` — snapshot testing (limited usage observed)
- `tempfile` — temporary directories for filesystem tests
- `assert_cmd` — spawn the compiled binary for integration-style tests
- `predicates` — assertion helpers used with `assert_cmd`

## Test Layout

**All tests are co-located inline with source modules.** There is **no top-level `tests/` directory** in this repo.

**Pattern:**
```rust
// src/foo.rs
pub fn parse(...) -> Result<...> { ... }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_input() { ... }
}
```

**Test-bearing files (count of `#[test]` / `#[tokio::test]` attributes):**

| File | Approx. tests | What is tested |
|---|---|---|
| `src/keys.rs` | ~7 | Keymap parsing, mode transitions, action dispatch |
| `src/ui/layout.rs` | ~6 | Layout math for 3-pane responsive layout |
| `src/ui/terminal.rs` | ~2 | Terminal pane rendering helpers |
| `src/ui/preview.rs` | ~3 | File preview rendering |
| `src/ui/picker.rs` | ~4 | Picker navigation and filtering |
| `src/ui/sidebar_right.rs` | ~2 | Modified files list rendering |
| `src/ui/sidebar_left.rs` | (present) | Sidebar navigation |
| `src/mpb.rs` | (present) | MPB name generation |
| `src/state.rs` | (present) | State serialization/migration |
| `src/config.rs` | (present) | Path resolution and write probes |

## Running Tests

```bash
cargo test                  # all tests
cargo test <pattern>        # filter by name
cargo test --all-targets    # include unit + bench + examples
```

**CI command (`.github/workflows/ci.yml`):**
```yaml
- name: Test
  run: cargo test
```

Runs on both `macos-latest` and `ubuntu-latest` matrix entries, despite macOS-only runtime — Linux builds guard against platform-specific compile regressions.

## Test Style

### Unit tests
- Pure-function tests for layout math, key parsing, name generation
- Assertions via `assert_eq!`, `assert!`, `matches!`
- No heavy test harness; standard `#[test]` fn pattern

Example pattern (from `src/ui/layout.rs`):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_pane_layout_fits_small_terminal() {
        let rects = compute_layout(/* ... */);
        assert_eq!(rects.sidebar_left.width, /* ... */);
    }
}
```

### Filesystem tests
- Use `tempfile::TempDir` for isolation
- Construct fake `~/.martins/` hierarchies in a temp dir, point config at it, assert state read/write behavior

### CLI tests
- `assert_cmd::Command::cargo_bin("martins")` spawns the built binary
- `predicates` used for stdout/stderr assertions
- Useful for CLI subcommands like `martins workspaces list`, `prune`

## What Is NOT Tested

**Observed gaps (also surfaced in `CONCERNS.md`):**

- **`src/app.rs`** — the 2000+ line main event loop has no inline tests. Event routing, modal state machine, and workspace creation flow are not unit-tested.
- **PTY / tmux integration** — no tests around `src/pty/` or `src/tmux.rs`. These depend on live subprocesses.
- **Git worktree operations** — `src/git/worktree.rs` shells out to `git`; no tests with a scaffolded repo observed.
- **State migration** — no explicit migration-from-v1-to-v2 test found in `src/state.rs`.
- **State corruption / backup recovery** — fallback from `state.json` → `state.json.bak` → default not exercised by tests.
- **File watcher debouncing** — `src/watcher.rs` flow is untested.
- **Crossterm/input handling** — event dispatch not unit-tested.

## Mocking Strategy

**No mocking framework** (e.g., `mockall`, `wiremock`). Observed approaches:
- Dependency injection via explicit function parameters (pass paths, configs)
- Fake filesystems via `TempDir`
- No abstractions around tmux/git subprocesses — those paths are effectively untested

## Snapshot Testing (`insta`)

Declared as a dev-dep but usage is minimal. If present, snapshots live next to the module they test (`snapshots/` subdir next to source files). No standing `snapshots/` tree was observed in the top-level listing.

## Coverage

- **No coverage tool configured** (no `cargo-tarpaulin`, `cargo-llvm-cov`, or grcov setup)
- **No coverage threshold enforced** in CI
- Practical coverage is concentrated in pure-logic modules (layout, keys, mpb, preview)

## Test Dependencies on Environment

Inline unit tests should be hermetic, but integration-style tests via `assert_cmd` may require:
- No pre-existing `~/.martins/` (or use a temp home override)
- `git` binary present
- `tmux` binary present if exercising PTY/session paths (mostly skipped per above)

## How to Add Tests

**Unit tests:**
1. Open the source file for the module
2. Add or extend `#[cfg(test)] mod tests { ... }` at the bottom
3. Use `use super::*` to pull in module items
4. Run `cargo test <module>::<fn>`

**CLI-level tests:**
1. Since there's no `tests/` directory yet, create `tests/cli.rs` (outside `src/`)
2. Use `assert_cmd::Command::cargo_bin("martins")` + `predicates`
3. Scaffold filesystem state with `tempfile::TempDir`

**Snapshot tests:**
1. Use `insta::assert_snapshot!(value)` or `assert_yaml_snapshot!`
2. Run `cargo insta review` to accept new snapshots

## CI Test Matrix

From `.github/workflows/ci.yml`:
```yaml
strategy:
  matrix:
    os: [macos-latest, ubuntu-latest]
```

- Both platforms run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
- Release binary only built on macOS (`.github/workflows/release.yml`)
