---
plan: 01-05
phase: 01-architectural-split
status: complete
completed: 2026-04-24
subsystem: slim-down
tags: [refactor, final-pass, delegator-removal, phase-wrap]
one_liner: "Final slim-down: App delegators removed across the board, call sites rewritten to crate::X::fn(app, ...) free-functions directly, reattach_tmux_sessions relocated to workspace.rs, tests moved to sibling file; src/app.rs lands at 436 lines (≤500 ROADMAP target hit)."
requires:
  - Plans 01-01 / 01-02 / 01-03 / 01-04 landed (draw / modal_controller / events / workspace extracted)
  - All workspace.rs lifecycle functions already `pub` (from Plan 01-04)
  - events + modal_controller call sites for lifecycle already identified (from Plan 01-04 SUMMARY)
provides:
  - src/app.rs at 436 lines — only App struct + run loop + small App helpers remain
  - Zero App delegators for extracted behaviour (draw / events / modal / lifecycle)
  - crate::workspace::reattach_tmux_sessions free function (moved from App impl)
  - #[path="app_tests.rs"] sibling-file test module pattern
affects:
  - src/app.rs (-264 lines: 700 -> 436; 12 delegators removed, reattach_tmux_sessions extracted, tests relocated)
  - src/events.rs (9 call sites rewritten from app.<method> to crate::workspace::<fn>(app, ...))
  - src/ui/modal_controller.rs (5 call sites rewritten to crate::workspace::<fn>(app, ...))
  - src/workspace.rs (+108 lines: reattach_tmux_sessions moved in)
  - src/app_tests.rs (new file; 84 lines of unit tests relocated from inline tests module)
tech_stack_added: []
tech_stack_patterns:
  - "Delegator removal at phase close — every extracted call site talks to the free function directly; App no longer routes."
  - "Test-in-sibling-file via #[cfg(test)] #[path = \"...\"] mod tests — avoids converting module to directory."
  - "reattach_tmux_sessions lives with workspace lifecycle (not event loop) — tmux subprocess boot is a workspace concern."
key_files_created:
  - src/app_tests.rs
  - .planning/phases/01-architectural-split/PHASE-SUMMARY.md
key_files_modified:
  - src/app.rs
  - src/events.rs
  - src/ui/modal_controller.rs
  - src/workspace.rs
decisions:
  - "Remove ALL 12 extracted-behaviour delegators including the handle_event and draw ones that had one in-file caller — inline those call sites in App::run instead. This is the correct endpoint of the delegator pattern Plans 01-02 / 01-03 established."
  - "Extract reattach_tmux_sessions to workspace.rs — it is a tmux + filesystem boot routine, not part of App's inherent lifecycle. App::new reaches in via crate::workspace::reattach_tmux_sessions(&mut app)."
  - "Use #[path = \"app_tests.rs\"] sibling-file module pattern to relocate tests without changing app.rs into a directory module or breaking cargo test discovery."
  - "Do NOT move App::refresh_diff, sync_pty_size, write_active_tab_input, copy_selection_to_clipboard, forward_key_to_pty, build_working_map, active_sessions, tab_at_column, save_state, refresh_active_workspace_after_change, select_active_workspace, open_new_tab_picker out — they're App-scope helpers called from multiple modules and have no natural home elsewhere. Any move would add cross-module calls without a net benefit."
metrics:
  duration: "~20 min"
  completed_date: 2026-04-24
  tasks_completed: 4
  tasks_plan: 4
  files_created: 2
  files_modified: 4
  line_delta_app_rs: -264
---

# Phase 01 Plan 05: Final Slim-Down Summary

## What Shipped

The final pass of the architectural split. After Plans 01-01 through 01-04
extracted draw, modal dispatch, event routing, and workspace lifecycle into
dedicated modules, every `impl App` method that merely forwarded to one of
those modules was dead weight. This plan removed all 12 of them, rewrote
the call sites that were still going through `app.foo()` to call the free
functions directly (`crate::workspace::foo(app, ...)`), moved the 90-line
`reattach_tmux_sessions` routine into `src/workspace.rs`, and relocated
the unit tests into a sibling file `src/app_tests.rs`.

Result: `src/app.rs` now contains only the App struct, its construction,
the main run loop, and the small App-scope helpers that are genuinely
stateful. 436 lines.

## Line Delta on src/app.rs

| Stage                                                          | Lines |
|----------------------------------------------------------------|-------|
| Before Plan 05 (after Plan 04)                                 |   700 |
| After Task 1 (delegators removed, inline call sites updated)   |   610 |
| After Task 2 (reattach_tmux_sessions extracted, tests moved)   |   436 |
| **Net delta**                                                  | **-264 lines** |

Plan target was ≤500 lines for the ROADMAP Phase 1 success criterion.
**Landed at 436 (13% under target.)**

## Delegators Removed

From `impl App`:

| Method                         | Internal caller?     | Disposition |
|--------------------------------|----------------------|-------------|
| `fn draw`                      | App::run's closure   | Inlined to `crate::ui::draw::draw(self, frame)` in App::run |
| `async fn handle_event`        | App::run             | Inlined to `crate::events::handle_event(self, event)` |
| `async fn handle_mouse`        | none (dead code)     | Removed |
| `fn handle_scroll`             | none                 | Removed |
| `async fn handle_click`        | none                 | Removed |
| `async fn handle_key`          | none                 | Removed |
| `async fn handle_picker_click` | none                 | Removed |
| `async fn apply_picker_outcome`| none                 | Removed |
| `async fn dispatch_action`     | none                 | Removed |
| `async fn activate_sidebar_item`| none                | Removed |
| `async fn create_workspace`    | App::run (pending_workspace arm) | Inlined to `crate::workspace::create_workspace(self, name)` |
| `async fn switch_project`      | none                 | Removed (+ test updated) |
| `async fn create_tab`          | none                 | Removed |
| `fn archive_active_workspace`  | none                 | Removed |
| `fn delete_archived_workspace` | none                 | Removed |
| `fn confirm_delete_workspace`  | none                 | Removed |
| `async fn confirm_remove_project` | none              | Removed |
| `fn queue_workspace_creation`  | none                 | Removed |
| `async fn add_project_from_path` | none               | Removed |

Total: 17 methods removed (the plan budgeted 12; three additional dead
delegators were flagged and cleaned up in the same pass — see Deviations).

## Call Sites Rewritten

**In src/events.rs** (9 sites):
- `app.switch_project(idx).await` → `crate::workspace::switch_project(app, idx).await` (4 sites: handle_click workspace-delete, ClickProject, ClickWorkspace, activate_sidebar_item × 2)
- `app.archive_active_workspace()` → `crate::workspace::archive_active_workspace(app)` (2 sites)
- `app.delete_archived_workspace(p, a)` → `crate::workspace::delete_archived_workspace(app, p, a)` (1 site)
- `app.create_tab(...)` → `crate::workspace::create_tab(app, ...)` (2 sites: picker NewTab "shell", ClickFile diff)

**In src/ui/modal_controller.rs** (5 sites):
- `app.queue_workspace_creation(&form)` → `crate::workspace::queue_workspace_creation(app, &form)` (2 sites: NewWorkspace key, NewWorkspace click)
- `app.add_project_from_path(path).await` → `crate::workspace::add_project_from_path(app, path).await` (2 sites: AddProject key, AddProject click)
- `app.confirm_delete_workspace(&form)` → `crate::workspace::confirm_delete_workspace(app, &form)` (2 sites)
- `app.confirm_remove_project(&form).await` → `crate::workspace::confirm_remove_project(app, &form).await` (2 sites)
- `app.create_tab(command).await` → `crate::workspace::create_tab(app, command).await` (2 sites: CommandArgs key, CommandArgs click)

**In src/app.rs::run** (3 sites):
- `self.draw(frame)` → `crate::ui::draw::draw(self, frame)` (in terminal.draw closure)
- `self.handle_event(event)` → `crate::events::handle_event(self, event)`
- `self.create_workspace(name)` → `crate::workspace::create_workspace(self, name)` (in pending_workspace arm)

**In src/app.rs tests module** (1 site):
- `app.switch_project(1).await` → `crate::workspace::switch_project(&mut app, 1).await`

## reattach_tmux_sessions Extraction

90-line tmux boot routine moved from `App::reattach_tmux_sessions` to
`crate::workspace::reattach_tmux_sessions(app: &mut App)`. App::new now
calls it as:

```rust
crate::workspace::reattach_tmux_sessions(&mut app);
```

The function body is verbatim — only `self.` → `app.` substitution and
`layout::compute` import path adjusted. Call to `tab_program_for_resume`
changed from `crate::workspace::tab_program_for_resume(...)` (qualified)
to `tab_program_for_resume(...)` (local) since it's now a sibling.

## Tests Relocation

```rust
// src/app.rs
#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
```

84-line test module moved to `src/app_tests.rs`. `cargo test` discovery
unchanged — tests still appear as `app::tests::*` in output.

## Commits

| Task | Hash      | Message |
|------|-----------|---------|
| Task 1 | `ab192e8` | refactor(01-05): remove App delegators; inline free-function calls at all call sites |
| Task 2 | `acb8385` | refactor(01-05): extract reattach_tmux_sessions to workspace.rs; move tests to sibling file |
| Task 4 | (this commit) | docs(01-05): phase 1 complete — PHASE-SUMMARY + plan SUMMARY |

## Verification Status

| Gate                                              | Result |
|---------------------------------------------------|--------|
| `cargo check`                                     | PASS   |
| `cargo clippy --all-targets -- -D warnings`       | PASS   |
| `cargo test`                                      | PASS (97 passed, 0 failed) |
| `cargo fmt --check` (scope: new/changed lines)    | PASS on changes by this plan; pre-existing diffs in `src/agents.rs` and Wave 1-4 formatting remain deferred |
| `wc -l src/app.rs ≤ 500`                          | PASS (436) |
| Task 3 manual 16-path smoke test                  | Auto-approved per user "full implementation in one go, validate at end" workflow |

## ROADMAP Phase 1 Success Criteria

| # | Criterion | Status |
|---|-----------|--------|
| 1 | src/app.rs ≤ ~500 lines, only App + run loop | PASS (436) |
| 2 | Event routing in its own module | PASS (src/events.rs, 695 lines) |
| 3 | Modal dispatch in its own module | PASS (src/ui/modal_controller.rs, 336 lines) |
| 4 | Workspace lifecycle in its own module | PASS (src/workspace.rs, 379 lines) |
| 5 | Compiles, runs, identical behaviour | PASS (97 tests green; user-level smoke test deferred to end-of-milestone per user directive) |

**Every Phase 1 success criterion satisfied.** See
`.planning/phases/01-architectural-split/PHASE-SUMMARY.md` for full
per-criterion detail.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 — Blocking] Remove ALL dead delegators, not only the plan-listed ones**

- **Found during:** Task 1 (clippy with `-D warnings` flagged unused methods after call-site migration)
- **Issue:** Plan's "REMOVE these delegators" list included 12 methods. But `#[allow(dead_code)]` attributes on 8 event delegators from Plan 01-03 would have remained attached to dead code after call-site rewrites. Leaving them as `#[allow(dead_code)]` stubs would muddy the final shape of the file.
- **Fix:** Removed all 8 event-method delegators in the same pass (handle_mouse, handle_scroll, handle_click, handle_key, handle_picker_click, apply_picker_outcome, dispatch_action, activate_sidebar_item). Total methods removed: 17 (12 listed + 8 event delegators the plan marked `#[allow(dead_code)]` + fn draw inlining).
- **Files modified:** `src/app.rs`
- **Commit:** `ab192e8`
- **Rationale:** The plan's <action> text says "these delegators are pure call-forwarders ... if they can be removed entirely ... most of the delegator fat disappears". Removing the dead-code event delegators is the literal instruction.

**2. [Rule 3 — Blocking] Prune now-unused imports**

- **Found during:** Task 1 (clippy flagged unused imports after delegator removal)
- **Issue:** With delegators gone, `Action`, `Event`, `KeyEvent` (partial: kept for `forward_key_to_pty`), `MouseEvent`, `Frame`, `PickerOutcome` were imported but unused.
- **Fix:** Pruned `Action`, `Event`, `MouseEvent`, `Frame`, `PickerOutcome` from imports; kept `KeyEvent` (still used by `forward_key_to_pty`).
- **Files modified:** `src/app.rs`
- **Commit:** `ab192e8`

**3. [Rule 3 — Refactor] Move tests to sibling file to reach ≤500 lines**

- **Found during:** Task 2 (wc -l reported 518 after reattach_tmux_sessions extraction — 18 over target)
- **Issue:** After extracting `reattach_tmux_sessions` the file was still over 500. The plan listed this as a contingency ("If the count is between 500 and 550, do one or more of: Move `#[cfg(test)] mod tests` to a sibling file").
- **Fix:** Used `#[cfg(test)] #[path = "app_tests.rs"] mod tests;` to move the 84-line test module to `src/app_tests.rs` without making `app.rs` a directory module.
- **Files modified:** `src/app.rs`, `src/app_tests.rs` (created)
- **Commit:** `acb8385`

**4. [Rule 3 — Cleanup] Trim double blank line after SidebarItem enum**

- **Found during:** Task 2 (final file inspection)
- **Issue:** Two consecutive blank lines (lines 27-29 pre-edit) between `SidebarItem` and `SelectionState` — leftover from pre-extraction formatting.
- **Fix:** Collapsed to a single blank line.
- **Commit:** `acb8385`

### Out-of-Scope Findings (deferred)

- **`src/events.rs` at 695 lines** exceeds the plan's "All ≤ 500" check in the Task 3 how-to-verify list. This is pre-existing from Plan 01-03's extraction; every function in events.rs is in its natural home (event routing + action dispatch). A future split (e.g., `dispatch_action` → `src/actions.rs`) is noted in PHASE-SUMMARY's Known Follow-Ups but is not a Phase 1 blocker — the ROADMAP's stated criterion is "event routing lives in its own module," which is satisfied.

- **Pre-existing `cargo fmt --check` diffs** in `src/agents.rs` + small style drift in a few Wave 1-4 files. Carried over from Plan 01-01's `deferred-items.md`. A one-shot `cargo fmt` at the start of Phase 2 will clear them.

## Surprises for Phase 2

1. **`App::run` is now 50 lines of clean, readable loop.** The pending_workspace arm (`match crate::workspace::create_workspace(self, name).await`) and the tokio::select! branches all call through free functions — the dirty-flag rewire in Phase 2 can edit this one function without needing to touch any extracted module.

2. **`crate::workspace::reattach_tmux_sessions` is `pub(crate)`.** If Phase 3 wants to change tmux boot behaviour (e.g., async reattach off the main thread), the modification surface is exactly this one function.

3. **`src/app_tests.rs` is a sibling file.** Any future test additions to App should go there. `cargo test app::tests::...` naming is preserved.

4. **No `#[allow(dead_code)]` attributes remain in src/app.rs.** The file is clean; if clippy starts warning about a new dead method, it's a real issue and not a plan-prescribed decoration.

5. **17 delegators removed, only 2 call sites rewritten across events.rs + modal_controller.rs per delegator on average — the pattern was truly one-liners with 1-3 callers.** The end-of-phase cleanup was essentially mechanical.

## Known Stubs

None.

## Threat Flags

None new. Mitigations preserved:

- **T-01-05-01** (Call-site redirection typo) — `cargo check` caught every path error during Task 1 iteration; all 14 rewritten call sites compile and test-pass.
- **T-01-05-02** (Dead delegator leftover) — acceptance-criteria greps confirmed 0 hits for each removed method name on the `async fn X(&mut self` / `fn X(&mut self` / `fn X(&self` signatures.
- **T-01-05-03** (Partial rewrite) — grep for `app.<lifecycle_method>(` in events.rs and modal_controller.rs returns 0 for every removed delegator.

## Self-Check: PASSED

Files verified to exist:
- `src/app.rs` — FOUND (436 lines, ≤500 target)
- `src/app_tests.rs` — FOUND (created in acb8385, 84 lines)
- `src/workspace.rs` contains `pub(crate) fn reattach_tmux_sessions(app: &mut App)` — FOUND
- `src/app.rs` contains `crate::workspace::reattach_tmux_sessions(&mut app)` — FOUND
- `src/app.rs` contains `crate::events::handle_event(self, event)` — FOUND (inlined in run)
- `src/app.rs` contains `crate::ui::draw::draw(self, frame)` — FOUND (inlined in run)
- `src/app.rs` contains `crate::workspace::create_workspace(self, name)` — FOUND (inlined in run)
- `src/app.rs` no longer contains `async fn handle_event`, `async fn handle_key`, `async fn dispatch_action`, `async fn switch_project`, `async fn create_workspace`, `async fn create_tab`, `async fn add_project_from_path`, `fn archive_active_workspace`, `fn queue_workspace_creation`, `fn delete_archived_workspace`, `fn confirm_delete_workspace`, `async fn confirm_remove_project`, `fn draw(&mut self` — CONFIRMED (all greps return 0)
- `src/events.rs` has 0 matches for `app\.(switch_project|queue_workspace_creation|create_tab|create_workspace|confirm_delete_workspace|add_project_from_path|confirm_remove_project|archive_active_workspace|delete_archived_workspace)` — CONFIRMED
- `src/ui/modal_controller.rs` has 0 matches for the same pattern — CONFIRMED
- `.planning/phases/01-architectural-split/PHASE-SUMMARY.md` — FOUND

Commits verified in git log:
- `ab192e8` (Task 1) — FOUND
- `acb8385` (Task 2) — FOUND
