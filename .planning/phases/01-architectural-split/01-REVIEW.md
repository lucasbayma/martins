---
phase: 01-architectural-split
reviewed: 2026-04-24T00:00:00Z
depth: standard
files_reviewed: 8
files_reviewed_list:
  - src/app.rs
  - src/app_tests.rs
  - src/events.rs
  - src/main.rs
  - src/ui/draw.rs
  - src/ui/mod.rs
  - src/ui/modal_controller.rs
  - src/workspace.rs
findings:
  critical: 0
  warning: 2
  info: 6
  total: 8
status: issues_found
---

# Phase 1: Code Review Report

**Reviewed:** 2026-04-24
**Depth:** standard
**Files Reviewed:** 8
**Status:** issues_found

## Summary

Phase 1 was a structural refactor: `src/app.rs` went from 2053 lines to 436 lines, with behaviour relocated to `src/ui/draw.rs`, `src/ui/modal_controller.rs`, `src/events.rs`, and `src/workspace.rs`. I diffed each extracted function against the pre-refactor snapshot (commit `9859b94^`) on the critical subprocess pathways (tmux, git worktree, fs, `save_state`) and found **no semantic divergence** — the free-function bodies are byte-for-byte equivalent to the old `&mut self` methods, down to call ordering and error-handling nuances.

`cargo check` and `cargo clippy --all-targets -- -D warnings` both pass cleanly.

The findings below are all things that also existed in pre-refactor code. I'm flagging them because the extraction surfaces them against fresh module boundaries — they are not regressions, but they are now easier to fix or document than they were inside a 2053-line `App` impl. None are critical. Two warnings identify latent correctness issues; six info items cover maintainability and small inconsistencies.

## Warnings

### WR-01: `create_workspace` ignores `create_tab` error after worktree is already on disk

**File:** `src/workspace.rs:265`
**Issue:** `create_workspace` returns `Ok(())` unconditionally after calling `let _ = create_tab(app, "shell".to_string()).await;`. If `create_tab` fails — e.g. tmux `new-session` fails under `spawn_blocking` or `pty_manager.spawn_tab` returns `Err` — the workspace struct is already persisted (`save_state` was called at line 263) and the git worktree already exists on disk, but the user sees a success modal dismissal and no terminal tab. The workspace becomes zombie-like: present in state, no running tmux session. This predates the refactor (pre-refactor `src/app.rs:1691` had the same `let _ = self.create_tab(...)`) but the extraction makes it more visible since `create_workspace` and `create_tab` now sit next to each other as peers.
**Fix:** Either propagate the tab-creation error, or at minimum log it so diagnosis is possible:
```rust
if let Err(error) = create_tab(app, "shell".to_string()).await {
    tracing::error!("workspace '{}' created but shell tab failed: {error}", ws_name);
}
Ok(())
```
If propagating is desirable, wrap in a rollback that kills the tmux session and removes the worktree before returning `Err`. That's a behaviour change, not a refactor fix, so log-only is the safer minimal patch.

### WR-02: `handle_modal_click` leaves `CommandArgs` modal open after successful tab creation

**File:** `src/ui/modal_controller.rs:303-315`
**Issue:** On the "OK" button click path for `Modal::CommandArgs`, a successful `create_tab(...)` call falls through without resetting `self.modal`. Because `handle_modal_click` matches on `self.modal.clone()` (line 144) rather than `std::mem::take`, `self.modal` still holds the `CommandArgs(form)` after the match. Result: click-submit keeps the modal open over the newly-focused terminal, while key-submit (which *does* use `take` at `modal_controller.rs:13`) closes it. This is pre-existing behaviour (pre-refactor `src/app.rs:1099-1101` is identical) but the divergent key-vs-click semantics is easier to notice now that both handlers sit in one 337-line file.
**Fix:** Close the modal explicitly on the success branch, matching the key-press path:
```rust
if is_modal_first_button(modal_area, col, 12) {
    let command = if form.args_input.trim().is_empty() {
        form.agent.clone()
    } else {
        format!("{} {}", form.agent, form.args_input.trim())
    };
    if let Err(error) = crate::workspace::create_tab(app, command).await {
        tracing::error!("failed to create tab: {error}");
    }
    app.modal = Modal::None;  // <-- add this
} else {
    app.modal = Modal::None;
}
```
The `NewWorkspace` click arm has the same pattern, but it's deliberate there because `queue_workspace_creation` sets `Modal::Loading`. `CommandArgs` has no such follow-up.

## Info

### IN-01: `confirm_delete_workspace` does not remove the worktree from disk

**File:** `src/workspace.rs:152-159`
**Issue:** `confirm_delete_workspace` calls `project.remove(&name)` (which drops the state entry) and `save_state`, but never calls `std::fs::remove_dir_all(&ws.worktree_path)`. Contrast `archive_active_workspace` (line 181) and `delete_archived_workspace` (line 194) — both do remove the directory. The "delete active workspace" path therefore leaves an orphan worktree under `.martins/worktrees/` after the state entry disappears. Pre-existing (pre-refactor `src/app.rs:1444-1451` is identical), but the three sibling functions now living in one file make the inconsistency obvious.
**Fix:** Capture the worktree path before removing the state entry and clean it up, mirroring `archive_active_workspace`:
```rust
pub fn confirm_delete_workspace(app: &mut App, form: &DeleteForm) {
    let name = form.workspace_name.clone();
    let worktree_path = app.active_project()
        .and_then(|p| p.active().find(|w| w.name == name))
        .map(|w| w.worktree_path.clone());
    if let Some(project) = app.active_project_mut() {
        project.remove(&name);
    }
    app.refresh_active_workspace_after_change();
    app.save_state();
    if let Some(path) = worktree_path {
        let _ = std::fs::remove_dir_all(&path);
    }
}
```
Also note: neither this function nor `confirm_delete_workspace` kills the tmux sessions for the tabs in that workspace — again pre-existing but worth documenting.

### IN-02: `reattach_tmux_sessions` uses a blocking 200ms `std::thread::sleep` on an async runtime

**File:** `src/workspace.rs:79`
**Issue:** `std::thread::sleep(Duration::from_millis(200))` inside `reattach_tmux_sessions` blocks the tokio worker thread. It's called once during `App::new` (before the event loop starts) so it's not user-visible in practice, but `reattach_tmux_sessions` is `pub(crate)` and synchronous, so nothing stops a future caller from invoking it from the event loop. Pre-existing (pre-refactor `src/app.rs:209` identical).
**Fix:** Either mark the function `async` and use `tokio::time::sleep`, or document with a comment that this function is only safe to call outside the event loop:
```rust
// Safety: called once from App::new before the tokio event loop starts.
// Do not call from inside the run loop — this blocks the worker thread.
```

### IN-03: `copy_selection_to_clipboard` silently drops pbcopy errors

**File:** `src/app.rs:315-324`
**Issue:** The `let _ = std::process::Command::new("pbcopy")...` swallows every failure mode (pbcopy missing, spawn error, broken pipe on stdin write, non-zero exit). On macOS-only this is usually fine, but "copy appeared to work" with no feedback is hard to diagnose when it doesn't. Pre-existing.
**Fix:** Log failures at `tracing::debug!` level so they show up with `RUST_LOG=debug` without bothering users:
```rust
if let Err(error) = std::process::Command::new("pbcopy")
    .stdin(std::process::Stdio::piped())
    .spawn()
    .and_then(|mut child| { /* ... */ })
{
    tracing::debug!("pbcopy failed: {error}");
}
```

### IN-04: `sync_pty_size` fires `tmux resize-session` per tab via a fire-and-forget `spawn_blocking`

**File:** `src/app.rs:355-364`
**Issue:** The returned `JoinHandle` from `tokio::task::spawn_blocking` is dropped, so resize errors are invisible and the task runs detached. Not wrong — resize failures are cosmetic — but the current implementation has no backpressure: on a rapid sequence of resize events, multiple tasks can queue up and complete out of order. In practice the event-loop gate `(rows, cols) == self.last_pty_size` on line 342 prevents duplicates for the same size, but concurrent resizes with different sizes can interleave. Pre-existing.
**Fix:** Optional. If it ever matters, serialize resizes through a channel or use `tokio::task::JoinSet`. Otherwise a comment noting the fire-and-forget intent is enough.

### IN-05: `SidebarItem::ArchivedHeader` click has no explicit `return`; relies on fallthrough to outer `return;`

**File:** `src/events.rs:202-209`
**Issue:** The `ArchivedHeader` match arm toggles `archived_expanded` and falls through. Because the outer `if let Some(left) = panes.left && ... { match item { ... } return; }` block ends with `return;` at line 223, this works correctly, but the reliance on fallthrough control flow is a footgun — if someone later adds code after the match but before the `return`, `ArchivedHeader` clicks would suddenly execute it. Pre-existing.
**Fix:** Add an explicit `return;` inside the arm, or (better) restructure so each arm yields a value and the side-effects happen after:
```rust
SidebarItem::ArchivedHeader(project_idx) => {
    if let Some(project) = app.global_state.projects.get(project_idx) {
        let id = project.id.clone();
        if !app.archived_expanded.remove(&id) {
            app.archived_expanded.insert(id);
        }
    }
    // explicit: nothing else to do on this path
}
```
The existing behaviour is correct; this is a defensive-code suggestion.

### IN-06: `tab_program_for_new` shell-quotes a path with a hand-rolled `replace('\'', "'\\''")`

**File:** `src/workspace.rs:349-356`
**Issue:** The escape `path.replace('\'', "'\\''")` is the standard POSIX single-quote escape and works, but this is a subprocess boundary driven by user-derived input (workspace-relative paths from the diff sidebar). The resulting string is passed as a shell command to `/bin/sh`/`/bin/zsh` via tmux's new-session. A path containing non-UTF8 bytes, control characters, or unusual characters reaches the shell unchecked beyond the quote escape. Realistic exploitation requires controlling a filename in the repo, so severity is very low on a single-user TUI; still, the escape should at minimum be concentrated in a named helper that's unit-testable. Pre-existing.
**Fix:** Extract to `fn shell_single_quote(s: &str) -> String { format!("'{}'", s.replace('\'', "'\\''")) }` and add a test covering embedded quotes, backslashes, and newlines. Optional hardening: reject filenames that contain newlines or non-UTF8 bytes early in `Action::ClickFile`.

---

_Reviewed: 2026-04-24_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
