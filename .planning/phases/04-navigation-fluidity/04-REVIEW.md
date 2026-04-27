---
phase: 04-navigation-fluidity
reviewed: 2026-04-24T00:00:00Z
depth: standard
files_reviewed: 5
files_reviewed_list:
  - src/navigation_tests.rs
  - src/main.rs
  - src/app.rs
  - src/events.rs
  - src/workspace.rs
findings:
  critical: 0
  warning: 3
  info: 5
  total: 8
status: issues_found
---

# Phase 4: Code Review Report

**Reviewed:** 2026-04-24
**Depth:** standard
**Files Reviewed:** 5
**Status:** issues_found

## Summary

Phase 4 (Navigation Fluidity) cleanly splits the diff refresh into two APIs: the
existing async `refresh_diff` (still used for `App::new`, the file watcher, and
the 5s safety-net tick) and a new fire-and-forget `refresh_diff_spawn` that
offloads git2 work onto a detached tokio task. The 6th `tokio::select!` branch
drains results back onto `app.modified_files`. Channel lifecycle and `biased`
ordering are correct. Test fixtures are well-scoped.

Three correctness concerns are worth addressing before Phase 5:

1. **Stale-diff overwrite race** on rapid workspace switching — detached spawned
   tasks can complete out-of-order and overwrite `modified_files` with results
   for a workspace the user already navigated away from (WR-01).
2. **Unbounded channel with no backpressure** — a burst of nav events on a very
   large repo queues unbounded vectors in the mpsc buffer; while this is not a
   practical memory concern at current scale, it is a latent resource issue and
   makes WR-01 worse because every queued message is eventually applied (WR-02).
3. **Missing `mark_dirty` in `select_active_workspace`** — the function is
   always paired with `refresh_diff_spawn()` today, which masks the bug, but
   the helper is not self-sufficient and is a footgun for future callers
   (WR-03).

No critical (security/data-loss/crash) issues.

## Warnings

### WR-01: Out-of-order diff results overwrite with stale data

**File:** `src/app.rs:306-325` (`refresh_diff_spawn`) and `src/app.rs:247-258` (6th select branch)

**Issue:** `refresh_diff_spawn` spawns a detached `tokio::spawn` per call and
clones `self.diff_tx`. Rapid navigation (user presses Down-arrow 5 times, or
clicks several workspaces quickly) spawns 5 concurrent git2 jobs. The jobs race:
the order of `tx.send(files)` hitting the receiver is not guaranteed to match
the spawn order. A slow job for workspace A started first can complete *after*
a fast job for workspace B started later, and the 6th select branch will
blindly overwrite `modified_files` with A's (now-stale) diff — displaying files
that do not belong to the currently active workspace.

The drain branch has no epoch/generation token to discriminate: it just does
`self.modified_files = files;`.

**Fix:** Tag each spawn with a monotonically increasing epoch; ignore results
whose epoch is not the latest. Minimal patch:

```rust
// In App struct:
pub(crate) diff_epoch: u64,
pub(crate) diff_tx: tokio::sync::mpsc::UnboundedSender<(u64, Vec<diff::FileEntry>)>,
pub(crate) diff_rx: tokio::sync::mpsc::UnboundedReceiver<(u64, Vec<diff::FileEntry>)>,

// In refresh_diff_spawn:
self.diff_epoch = self.diff_epoch.wrapping_add(1);
let epoch = self.diff_epoch;
let tx = self.diff_tx.clone();
tokio::spawn(async move {
    if let Ok(files) = diff::modified_files(path, base_branch).await {
        let _ = tx.send((epoch, files));
    }
});

// In 6th select branch:
Some((epoch, files)) = self.diff_rx.recv() => {
    if epoch != self.diff_epoch { continue; } // drop stale
    self.modified_files = files;
    // ...
}
```

Alternative: store an `AbortHandle` on App and `.abort()` the in-flight task
before spawning the next one. Epoch approach is simpler and avoids aborting a
task that may be milliseconds from completing useful work.

### WR-02: Unbounded mpsc channel accumulates stale vectors under burst nav

**File:** `src/app.rs:117-118` (channel construction)

**Issue:** `tokio::sync::mpsc::unbounded_channel` has no backpressure. If the
user hammers the sidebar (Tab-repeat at 30Hz) while the event loop is briefly
stalled (e.g., drawing or processing a large paste), detached refresh tasks
all enqueue their `Vec<FileEntry>` into the channel. Once the loop resumes, the
6th select branch fires once per iteration to drain one message — on each
drain it reassigns `modified_files` and calls `mark_dirty`, triggering a full
redraw. For N queued messages the UI redraws N times with intermediate,
already-superseded states before settling.

Combined with WR-01, this is how a visible "flicker through stale diff lists"
would manifest.

**Fix:** Two options, pick one:

1. Bounded channel of capacity 1 with `try_send` that drops on full:
   ```rust
   let (diff_tx, diff_rx) = tokio::sync::mpsc::channel(1);
   // in spawn body:
   let _ = tx.try_send(files); // best-effort; drop if a newer result is queued
   ```
   This pairs naturally with WR-01's epoch if you keep unbounded, but a
   bounded channel with capacity 1 is a simpler structural guarantee.

2. Drain-to-latest in the select arm: after receiving one, call
   `while let Ok(next) = self.diff_rx.try_recv() { files = next; }` so only
   the newest enqueued result is applied. Cheap and local.

### WR-03: `select_active_workspace` does not mark dirty

**File:** `src/app.rs:344-347`

**Issue:**
```rust
pub(crate) fn select_active_workspace(&mut self, index: usize) {
    self.active_workspace_idx = Some(index);
    self.right_list.select(None);
}
```

This mutates user-visible state (active workspace, right-list selection) but
never calls `self.mark_dirty()`. Today every caller follows it with
`refresh_diff_spawn()` (which *does* mark dirty), masking the bug. But the
function's contract is "change the active workspace" — callers should not have
to remember to also mark dirty, and a future call-site that selects a
workspace without a diff refresh (e.g., during restore or reconciliation) will
leave the UI stale.

The `workspace_switch_paints_pty_first` test specifically relies on the
`refresh_diff_spawn` tail setting dirty, not `select_active_workspace` — so
the test would continue to pass even if this were made self-sufficient.

**Fix:**
```rust
pub(crate) fn select_active_workspace(&mut self, index: usize) {
    self.active_workspace_idx = Some(index);
    self.right_list.select(None);
    self.mark_dirty();
}
```

## Info

### IN-01: Detached spawn has no error path for diff failures

**File:** `src/app.rs:319-323`

**Issue:** `if let Ok(files) = diff::modified_files(...).await { tx.send(files) }`
silently drops errors. The blocking `refresh_diff` exhibits the same pattern
(line 277), so behavior is consistent — but there is now no way to surface a
transient git2 failure at all on the nav hot path. The 5s safety-net
refresh_tick will eventually retry, which is probably fine, but a
`tracing::warn!` on Err would aid debugging.

**Fix:** Log the error:
```rust
match diff::modified_files(path, base_branch).await {
    Ok(files) => { let _ = tx.send(files); }
    Err(e) => tracing::warn!("refresh_diff_spawn: {e}"),
}
```

### IN-02: Mouse-click highlight now fires for non-activation variants

**File:** `src/events.rs:176`

**Issue:** The new `app.left_list.select(Some(local_row));` is hoisted BEFORE
the `match item {}`, so it runs unconditionally for every `SidebarItem`
variant including `ArchivedHeader` (toggles expand/collapse only),
`ArchivedWorkspace` in its non-delete branch (no-op currently), and the
delete-zone sub-cases that open a confirm modal without switching workspace.

This is likely intentional — it matches the visible click — but note that
clicking an `ArchivedHeader` now advances the sidebar selection cursor to the
header row, which previously was not selectable via keyboard navigation
(`move_sidebar_to_workspace` skips non-Workspace items). The next keyboard
Up/Down from there will land on a non-Workspace row, then
`move_sidebar_to_workspace` will again skip to the next Workspace — so
observable UX is fine, but the invariant "left_list index points at a Workspace
item" is violated transiently.

**Fix:** No action required if the current visual behavior is desired. If the
intent was "highlight only on workspace/project activation", move the `select`
call into the specific match arms that do navigate.

### IN-03: Test state files are never cleaned up post-test

**File:** `src/navigation_tests.rs:58-59, 86-87, 125-126, 156-157`

**Issue:** Each `#[tokio::test]` does
```rust
let state_path = std::env::temp_dir().join("martins-nav-*.json");
let _ = std::fs::remove_file(&state_path);
```
at entry to start clean, but there is no teardown. Stale state files
accumulate in `/tmp` across runs. Fixed names (not per-test-run unique) mean
that two `cargo test` invocations running in different worktrees on the same
machine could theoretically race on the same file, though tokio-test single-
threaded runtime mitigates this within one test binary.

**Fix:** Either (a) use `TempDir` for state files like the existing
`refresh_diff_spawn_is_nonblocking` does for the repo, or (b) add a
`Drop`-style cleanup. Example (a):
```rust
let tmp_state = TempDir::new().expect("TempDir");
let state_path = tmp_state.path().join("state.json");
```

### IN-04: `SelectionState::dragging` is set but never read

**File:** `src/app.rs:34` + `src/events.rs:59`

**Issue:** The `dragging: bool` field on `SelectionState` is assigned `true`
on drag-start (events.rs:59) and never read anywhere. Not touched by Phase 4
but adjacent to the reviewed surface. Dead code.

**Fix:** Remove the field, or wire it into terminal-content rendering to
suppress the "click-to-copy" hint while a drag is in progress. Out of scope
for Phase 4; flag for a follow-up cleanup.

### IN-05: In-flight spawned tasks continue writing to dropped receiver at shutdown

**File:** `src/app.rs:260-264`

**Issue:** When `self.should_quit` breaks the loop, `App::run` returns and
`App` is dropped — which drops `diff_rx`. Any still-running `refresh_diff_spawn`
tasks hold a cloned `diff_tx` and will attempt `tx.send(files)` when git2
finishes; the send returns `Err` (receiver gone) and is swallowed by the `let _ =`.
Tasks then exit naturally. No leak in the problematic sense (git2 work
completes, memory frees), but the tokio runtime may briefly outlive the event
loop with these tasks pending. Not a correctness issue — tokio's main runtime
drop will wait or abort as configured — but worth noting: shutdown is "fire
and forget" on these tasks, not cooperatively cancelled.

**Fix:** If deterministic shutdown matters for Phase 5, track `JoinHandle`s and
`.abort()` them before returning from `run`. Otherwise, leave as-is.

---

_Reviewed: 2026-04-24_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
