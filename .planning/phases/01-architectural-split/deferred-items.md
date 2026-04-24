# Deferred Items — Phase 01 Architectural Split

Out-of-scope findings discovered during execution but not fixed in this plan.

## 01-01 — Extract draw orchestration

### Pre-existing `cargo fmt` violations across the codebase

- **Discovered:** 2026-04-24, during Task 2 verification of plan 01-01
- **Scope:** 94 diffs reported by `cargo fmt --check` before any changes on this branch
- **Files affected:** `src/agents.rs`, `src/app.rs` (pre-existing, unrelated to my diff), `src/cli.rs`, `src/git/diff.rs`, `src/git/worktree.rs`, `src/main.rs`, `src/pty/manager.rs`, `src/state.rs`, `src/tmux.rs`, `src/ui/modal.rs`, `src/ui/sidebar_left.rs`, `src/ui/terminal.rs`
- **Why deferred:** Plan 01-01's acceptance criterion `cargo fmt --check exits 0` is unachievable without reformatting the entire codebase, which falls outside the draw-extraction scope. Verified on the base commit (pre-01-01) that `cargo fmt --check` was already producing 94 diffs, none caused by my changes. My own additions (`src/ui/draw.rs`) and the delegator in `src/app.rs` are now fmt-clean.
- **Suggested fix:** A dedicated chore commit `style: cargo fmt` at the start of phase 1 or as a one-off pass. Not urgent because clippy gate passes and fmt does not block CI.
