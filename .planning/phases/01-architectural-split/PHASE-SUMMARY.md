# Phase 1: Architectural Split — Summary

**Completed:** 2026-04-24
**Requirement:** ARCH-01

## ROADMAP Success Criteria Check

1. **`src/app.rs` ≤ ~500 lines, contains only App struct + run() loop.**
   - Status: PASS
   - Actual line count: **436**
   - File contents: module doc, imports, `SidebarItem` enum, `SelectionState`
     struct + impl, `App` struct, `TabClick` enum, `impl App` (constructor,
     `reattach_tmux_sessions` delegation, small accessors, `run` loop,
     `refresh_diff`, state helpers, PTY helpers, `sync_pty_size`,
     `build_working_map`, `active_sessions`, `tab_at_column`), tests-module
     declaration (`#[path = "app_tests.rs"]`).

2. **Event routing lives in its own module.**
   - Status: PASS
   - Location: `src/events.rs`
   - Line count: **695**
   - Functions: `handle_event`, `handle_key`, `handle_mouse`, `handle_scroll`,
     `handle_click`, `dispatch_action`, `activate_sidebar_item`,
     `handle_picker_click`, `apply_picker_outcome`, plus helpers
     (`rect_contains`, `terminal_content_rect`, `menu_action_at_column`,
     `key_to_bytes`, `move_list_selection`, `move_sidebar_to_workspace`,
     `picker_area`).
   - Note: file is above the soft ≤500 cap mentioned in the plan's
     how-to-verify list. This is pre-existing from Plan 01-03 — the
     architectural goal ("independently navigable module") is satisfied.
     See "Module Line Counts" below.

3. **Modal dispatch lives in its own module.**
   - Status: PASS
   - Location: `src/ui/modal_controller.rs`
   - Line count: **336**
   - Functions: `handle_modal_key`, `handle_modal_click`,
     `modal_button_row_y`, `is_modal_first_button`, private `rect_contains`.

4. **Workspace lifecycle lives in its own module.**
   - Status: PASS
   - Location: `src/workspace.rs`
   - Line count: **379**
   - Functions: `switch_project`, `queue_workspace_creation`,
     `confirm_delete_workspace`, `archive_active_workspace`,
     `delete_archived_workspace`, `confirm_remove_project`,
     `create_workspace`, `create_tab`, `add_project_from_path`,
     `reattach_tmux_sessions` (added in Plan 01-05), `tab_program_for_new`,
     `tab_program_for_resume`.

5. **Compiles, runs, identical behavior.**
   - Status: PASS
   - Verified automatically at the end of every plan (01-01 ... 01-05):
     - `cargo check` — PASS
     - `cargo clippy --all-targets -- -D warnings` — PASS
     - `cargo test` — PASS (97 passed, 0 failed)
   - Per-plan `how-to-verify` manual smoke tests auto-approved per user
     "full implementation in one go, validate at end" workflow; runtime
     validation deferred to end-of-milestone by user directive.
   - `cargo fmt --check` has pre-existing diffs in unrelated files
     (`src/agents.rs` and elsewhere); logged in `deferred-items.md` from
     Plan 01-01. Out-of-scope for Phase 1.

## Module Line Counts (post-phase)

| File                         | Lines |
|------------------------------|-------|
| src/app.rs                   |   436 |
| src/app_tests.rs             |    85 |
| src/events.rs                |   695 |
| src/workspace.rs             |   379 |
| src/ui/draw.rs               |   189 |
| src/ui/modal_controller.rs   |   336 |

**Notes on events.rs size:** `src/events.rs` is the combined home for every
event-routing + action-dispatch function that previously lived inline in
`src/app.rs`. Its 695 lines are the natural size of that surface, and it
is already far smaller than the pre-refactor `src/app.rs` (2053 lines).
Future decomposition (e.g., splitting `dispatch_action`'s match arms into
their own module) is out of scope for Phase 1.

## App Method Visibility Changes

Methods promoted to `pub(crate)` during extractions:

| Method                                  | Plan   | Reason |
|-----------------------------------------|--------|--------|
| `active_workspace`                      | 01-01  | draw module reads it |
| `build_working_map`                     | 01-01  | draw module uses it |
| `active_sessions`                       | 01-01  | draw module uses it |
| `active_project_mut`                    | 01-02  | modal controller needs mut project |
| `refresh_active_workspace_after_change` | 01-02  | modal controller (archive path) |
| `save_state`                            | 01-02  | modal controller (confirm paths) |
| `queue_workspace_creation`              | 01-02  | (removed in 01-05 — now `crate::workspace::*`) |
| `confirm_delete_workspace`              | 01-02  | (removed in 01-05) |
| `confirm_remove_project`                | 01-02  | (removed in 01-05) |
| `create_tab`                            | 01-02  | (removed in 01-05) |
| `add_project_from_path`                 | 01-02  | (removed in 01-05) |
| `switch_project`                        | 01-03  | (removed in 01-05) |
| `select_active_workspace`               | 01-03  | events module reads it |
| `archive_active_workspace`              | 01-03  | (removed in 01-05) |
| `delete_archived_workspace`             | 01-03  | (removed in 01-05) |
| `refresh_diff`                          | 01-03  | workspace + events call it |
| `write_active_tab_input`                | 01-03  | events module uses it |
| `copy_selection_to_clipboard`           | 01-03  | events module uses it |
| `forward_key_to_pty`                    | 01-03  | events module uses it |
| `open_new_tab_picker`                   | 01-03  | events + workspace call it |
| `tab_at_column`                         | 01-03  | events module uses it |
| `TabClick` enum                         | 01-03  | events module uses it |

Methods / items that stayed in `src/app.rs`:

- `App::new` (constructor, called from `main.rs`)
- `App::run` (main entry, called from `main.rs`)
- `App::refresh_diff` (pure App state — used by `run` + workspace.rs)
- `App::sync_pty_size` (App-internal, run-loop helper)
- `App::active_project` / `active_project_mut` / `active_workspace` (accessors)
- `App::save_state`, `refresh_active_workspace_after_change`,
  `select_active_workspace` (App-field state mutations)
- `App::write_active_tab_input`, `copy_selection_to_clipboard`,
  `forward_key_to_pty` (touch PtyManager, bind to App state)
- `App::build_working_map`, `active_sessions` (App-scope helpers)
- `App::tab_at_column` (App-visible tab hit-test)
- `App::open_new_tab_picker` (App-scope picker state)
- `SidebarItem`, `SelectionState`, `TabClick` types

## Patterns Established for Future Phases

- **Free-function + `&mut App` extraction pattern**: every extracted module
  exposes `pub async fn foo(app: &mut App, ...)` (or `pub fn ...`) instead of
  restructuring `App` into sub-structs. This keeps borrow-checker friction
  low because each function still takes a single `&mut App` at a time.

- **Sibling module for non-UI coordination**: `src/events.rs` and
  `src/workspace.rs` are top-level modules, not under `src/ui/`. UI-only
  coordination (`draw`, `modal_controller`) lives under `src/ui/`.

- **Delegator collapse when the extraction is complete**: Plans 01-02 and
  01-03 left App methods as one-line delegators; Plan 01-05 removed them
  once every call site could reach the free function directly. This is
  the correct endpoint for this pattern — delegators exist only to let
  extractions land incrementally, not to persist in the final shape.

- **Preserve call ordering in lifecycle code**: state-save points and
  subprocess invocations must match the original sequence. Audited per
  plan (01-04 was the highest-risk port; all threat-register mitigations
  preserved).

- **Tests-in-sibling-file via `#[path = "..."]`**: Plan 01-05 used this
  attribute to move `src/app.rs`'s tests module into `src/app_tests.rs`
  without converting `app` to a directory module. A viable low-cost tool
  for future slim-downs if a file's tests exceed its production code.

## Known Follow-Ups

- **Phase 2 (Event Loop Rewire)** will edit `src/app.rs::run` to introduce
  the dirty-flag and add a dedicated input-priority `tokio::select!`
  branch. The slim `run` loop makes this safe to do.

- **`src/events.rs` at 695 lines** is the largest of the extracted
  modules; if Phase 2 needs to grow it further, consider splitting
  `dispatch_action` (Actions enum match arms, ~180 lines) into a dedicated
  `src/actions.rs`.

- **CONCERNS.md items #2 (`.unwrap()` audit), #3 (modal module split),
  #4 (`#![allow(dead_code)]` in state.rs)** remain deferred.

- **Repo-wide `cargo fmt` diffs** in 13 pre-existing files (94 diffs
  total) — logged in `deferred-items.md` since Plan 01-01. Worth a one-shot
  style-only commit at the start of Phase 2.
