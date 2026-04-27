# Concerns & Technical Debt

**Analysis Date:** 2026-04-24

## Executive Summary

Martins is a focused TUI app (~12k Rust LOC) with a small, clean module boundary in most places but significant centralization in `src/app.rs`. The main risks are:
- Monolithic `src/app.rs` coordinating event loop, state, and UI (2000+ lines)
- Wide `.unwrap()` usage (~150 sites) in paths that are not guaranteed infallible
- Missing test coverage on subprocess integrations (tmux, git CLI, PTY)
- No migration tests for state schema (v1 → v2)

None are blockers; all are tractable with targeted refactors.

## Tech Debt

### 1. Monolithic `src/app.rs` (~2000 lines)

**Where:** `src/app.rs`

**Problem:**
- Holds the `App` struct, main event loop, input routing, modal dispatch, workspace creation, PTY orchestration, and draw coordination in a single file
- Makes it hard to reason about state machines (modal stack, focus, input mode) without loading the whole file
- Changes in one area (e.g., modal handling) risk unrelated regressions

**Candidates for extraction:**
- Event routing / `handle_event` into `src/events.rs`
- Modal dispatch into `src/ui/modal_controller.rs`
- Workspace lifecycle (create/archive/delete) into `src/workspace.rs`
- Draw orchestration into `src/ui/draw.rs`

**Effort:** Medium — careful with borrow-checker across extracted state.

### 2. Wide `.unwrap()` usage

**Where:** Across the crate (~150 occurrences). Hot files:

| File | `.unwrap()` count |
|---|---|
| `src/git/repo.rs` | ~27 |
| `src/git/worktree.rs` | ~26 |
| `src/state.rs` | ~21 |
| `src/app.rs` | ~18 |
| `src/config.rs` | ~12 |
| `src/watcher.rs` | ~16 |
| `src/pty/session.rs` | ~12 |

**Problem:**
- Some are fine (infallible conversions, tested invariants), but many are in I/O paths (git, state, watcher) where failure is realistic and a panic would kill the app mid-session — losing unsaved state.

**Action:** Audit per file, replace with `?`-propagation + `anyhow::Context` or logged warn + graceful fallback. Prioritize `state.rs` and `watcher.rs` (user-facing data paths).

### 3. Large UI modal module

**Where:** `src/ui/modal.rs` (~734 lines)

**Problem:** Every modal form type (AddProject, NewWorkspace, ConfirmDelete, CommandArgs) lives in one file. Adding a new modal mixes unrelated validation/render code.

**Action:** Split into `src/ui/modal/{add_project,new_workspace,confirm_delete,command_args}.rs` with a `mod.rs` aggregator. Low risk.

### 4. `#![allow(dead_code)]` at top of `src/state.rs`

**Where:** `src/state.rs` line 3

**Problem:** Blanket allow masks genuinely dead code drift. Some fields/variants may be holdovers from v1 schema that no longer need to exist.

**Action:** Remove the allow, audit warnings, delete unused items or gate with `#[cfg(feature = "...")]` if intentional.

### 5. `#[allow(dead_code)]` on `AppError` variants

**Where:** `src/error.rs`

**Problem:** Some `AppError` variants may never be constructed. If so, they're noise in the error surface.

**Action:** Check each variant usage with `grep`; remove unused ones.

## Fragile Areas

### 1. State persistence race

**Where:** `src/state.rs` save path + `src/app.rs` shutdown

**Risk:**
- State is saved on quit and on some state-change boundaries, but a crash (panic from `.unwrap()`) between mutation and save loses the in-memory change
- Backup `state.json.bak` is updated only after a successful write; a crash after mutation but before save → next boot loads stale state

**Action:** Save state after every meaningful mutation (workspace add/remove/archive, project add/remove). Small write overhead, high durability win.

### 2. Workspace creation is distributed across state + subprocess

**Where:** `src/app.rs::create_workspace`, `src/git/worktree.rs`, tmux spawn

**Risk:**
- Steps: state mutation → `git worktree add` → tmux session → PTY attach → state save
- Partial failure (e.g., git worktree add fails after state update) leaves inconsistent state
- No transactional rollback

**Action:** Make the state mutation the *last* step, only after subprocess ops succeed. Or implement explicit rollback on failure (remove workspace from state if subsequent step fails).

### 3. tmux session lifecycle tied to process, not app

**Where:** `src/tmux.rs`, `src/pty/manager.rs`

**Risk:**
- tmux server keeps running after Martins exits (by design — that's how persistence works)
- But orphaned sessions accumulate if tmux session names drift from stored state (e.g., after a crash mid-create)
- No reaper for unknown sessions

**Action:** On boot, diff tmux session list against stored state; log (or prompt user to clean up) unknown sessions matching `martins:*` pattern.

### 4. PTY cleanup on panic

**Where:** `src/pty/session.rs`

**Risk:** PTY master FDs and child processes may leak on app panic; cleanup relies on Drop ordering and panic-safety of wrapped types.

**Action:** Confirm `Drop` implementations close FDs; consider registering a panic hook that attempts graceful PTY/tmux shutdown before unwind.

## Performance Concerns

### 1. Periodic full diff refresh (5s)

**Where:** `src/app.rs` event loop `refresh_tick`

**Problem:** Every 5 seconds, `refresh_diff()` runs a full git diff for the active workspace even if nothing changed. With `notify` watching the same path, this may be redundant.

**Action:** Keep the timer as a safety net but raise to ~30s; rely primarily on file-watcher signals. Or diff only on change events.

### 2. Render-on-every-tick

**Where:** `src/app.rs::run` top of loop

**Problem:** `terminal.draw(...)` runs every iteration of the select loop. While ratatui is fast, idle redraws burn CPU on battery.

**Action:** Only redraw when a state change or event demands it (dirty flag pattern). Fallback periodic redraw for the clock/status bar at a low cadence.

### 3. PTY read buffer sizing

**Where:** `src/pty/session.rs`

**README claims:** "16KB PTY read buffers"

**Consideration:** Large output bursts (agent `--verbose` logs) may block the read loop under mutex contention if many sessions are active. Profile before changing.

## Security

### 1. Workspace name → path construction

**Where:** `src/app.rs::create_workspace`, `src/state.rs`

**Risk:** Workspace names become path components (`~/.martins/workspaces/{project_hash}/{workspace_name}/`). If not sanitized, `../` or absolute paths in names could escape the sandbox.

**Action:** Audit the name validator. Reject `/`, `..`, null bytes, control chars; normalize unicode (already using `unicode-normalization`).

### 2. `$PATH`-based subprocess resolution

**Where:** `src/tmux.rs`, `src/editor.rs`, agent launch in `src/state.rs`

**Risk:** A compromised `$PATH` (e.g., `.` prepended) could redirect `tmux`, `git`, or the agent binary to a malicious executable.

**Action:** Document the assumption that `$PATH` is trusted (it's a user-local dev tool). Not worth hardening beyond that unless threat model changes.

### 3. Vendored OpenSSL

**Where:** `Cargo.toml` → `openssl = { version = "0.10", features = ["vendored"] }`

**Risk:** Vendored OpenSSL pins the TLS stack. Security patches require a Martins rebuild/release; users don't pick up OS OpenSSL updates.

**Action:** Track `openssl` crate releases; bump promptly on CVEs. Or switch to `rustls` if ecosystem permits (git2 may still need OpenSSL).

### 4. Logging may capture sensitive strings

**Where:** `src/logging.rs`, tracing spans across the app

**Risk:** Debug logs in `~/.martins/logs/martins.log` may record command strings, env data, or error contexts that contain user-sensitive strings.

**Action:** Review `tracing` call sites; ensure no env or command-line args are logged at `info`/`debug` by default. Rely on `RUST_LOG` opt-in for verbose output.

## Missing Capabilities (Not Blockers, But Worth Noting)

- **No migration test** for v1→v2 state schema
- **No snapshot tests** for rendering (despite `insta` dev-dep)
- **No `tests/` integration directory** — all tests inline, CLI flows covered implicitly
- **No linter escape hatches documented** (if contributors ever need `#[allow(...)]`, pattern is undefined)
- **No crash-report upload path** — crashes go to local log file only; users can't report easily
- **Linux/Windows support deferred** — explicit design choice per README, but worth flagging if future roadmap changes

## Recommended Triage Order

1. **State save frequency** (data-loss risk) — quick win
2. **Workspace creation transactionality** (consistency risk) — medium effort, high value
3. **`.unwrap()` audit in `state.rs` and `watcher.rs`** — prevents surprise panics
4. **Path validation for workspace names** — security hygiene
5. **Split `src/app.rs`** — long-term maintainability
6. **Redraw dirty-flag** — perf / battery
7. **Modal module split** — dev ergonomics
