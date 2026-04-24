---
phase: 01-architectural-split
plan: 01
subsystem: ui
tags: [refactor, extract-module, draw, tui]
one_liner: "Draw orchestration moved from App impl into src/ui/draw.rs as free functions taking &mut App; App::draw is now a one-line delegator."
requires:
  - App struct fields exposed as pub (already true pre-plan)
  - Access to App helpers: active_project, active_workspace, build_working_map, active_sessions
provides:
  - src/ui/draw.rs with pub fn draw(&mut App, &mut Frame), status_bar(&App, ...), menu_bar(&mut Frame, Rect)
  - Free-function extraction pattern for downstream plans (01-02 ... 01-05)
affects:
  - src/app.rs (-171 lines: draw/draw_status_bar/draw_menu_bar removed, delegator added)
  - src/ui/mod.rs (+1 line: pub mod draw)
  - App::active_workspace / build_working_map / active_sessions elevated to pub(crate)
tech_stack_added: []
tech_stack_patterns:
  - "Free-function module takes &mut App — preferred over impl extraction when borrow-checker allows"
  - "Render helpers remain inside their module; App only exposes the data they need"
key_files_created:
  - src/ui/draw.rs
key_files_modified:
  - src/app.rs
  - src/ui/mod.rs
  - .planning/phases/01-architectural-split/deferred-items.md
decisions:
  - "Ship draw module with zero impl-block churn: App::draw remains a method so run() keeps calling terminal.draw(|f| self.draw(f))"
  - "Elevate three App helpers to pub(crate) rather than moving them now — plan 01-05 will re-evaluate visibility after all extractions land"
  - "Defer repo-wide cargo-fmt violations — they pre-exist on origin/main and are out of scope for this plan"
metrics:
  duration: "~15 min"
  completed_date: 2026-04-24
  tasks_completed: 3
  files_created: 1
  files_modified: 3
  line_delta_app_rs: -171
---

# Phase 01 Plan 01: Extract Draw Orchestration Summary

## What Shipped

`src/app.rs` no longer owns the draw pipeline. A new `src/ui/draw.rs` module owns the `draw`, `status_bar`, and `menu_bar` free functions that previously lived as `App` methods. `App::draw` is now a one-line delegator:

```rust
fn draw(&mut self, frame: &mut Frame) {
    crate::ui::draw::draw(self, frame);
}
```

This establishes the free-function-over-&mut-App pattern that plans 01-02 through 01-05 will reuse for modal dispatch, event routing, and workspace lifecycle.

## Line Delta on src/app.rs

| Stage | Lines |
|-------|-------|
| Before (origin/main) | 2053 |
| After task 1 (new module only, app.rs unchanged) | 2053 |
| After task 2 (old methods deleted, delegator added, imports pruned) | 1882 |
| **Net delta** | **-171 lines** |

Target was ≤1900; achieved 1882.

## Commits

| Task | Hash | Message |
|------|------|---------|
| Task 1 | `29e0a8f` | feat(01-01): extract draw orchestration into src/ui/draw.rs |
| Task 2 | `8865681` | refactor(01-01): delegate App::draw to ui::draw and remove old methods |

## Verification Status

| Gate | Result |
|------|--------|
| `cargo check` | PASS |
| `cargo clippy --all-targets -- -D warnings` | PASS |
| `cargo fmt --check` (scope: this plan's new/changed lines) | PASS (see below for repo-wide state) |
| `wc -l src/app.rs < 1900` | PASS (1882) |
| Task 3 smoke-test checkpoint | Auto-approved — automated gates green, deferred to end-of-phase user validation per "full implementation in one go" workflow |

### On cargo fmt repo-wide

94 pre-existing formatting diffs exist across `src/agents.rs`, `src/cli.rs`, `src/git/*.rs`, `src/main.rs`, `src/pty/manager.rs`, `src/state.rs`, `src/tmux.rs`, `src/ui/modal.rs`, `src/ui/sidebar_left.rs`, `src/ui/terminal.rs`, and parts of `src/app.rs` unrelated to this plan. They existed on `origin/main` before any work here. Logged in `deferred-items.md`. A one-shot `cargo fmt` at the start of phase 1 (or during 01-05 slim-down) will clear them.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 — Blocking] Make three App helpers visible to the draw module**

- **Found during:** Task 1 (`cargo check` after writing `src/ui/draw.rs`)
- **Issue:** `App::active_workspace`, `App::build_working_map`, and `App::active_sessions` were private, but the new `ui::draw` functions need them. Plan text listed them under "helpers draw depends on, to remain in app.rs" without specifying visibility.
- **Fix:** Elevated all three from `fn` to `pub(crate) fn`. No behavior change; only visibility.
- **Files modified:** `src/app.rs` (3 one-word additions).
- **Commit:** `29e0a8f` (folded into Task 1)
- **Downstream impact:** Plan 01-05 may choose to move these helpers out of `App` entirely once all extractions have landed; `pub(crate)` is the right conservative step for now.

**2. [Rule 3 — Blocking] Deviate from plan's import list**

- **Found during:** Task 2 (`cargo check` flagged unused imports after removing old draw methods)
- **Issue:** Plan said "KEEP `modal::self` and `picker::self` self-imports — still used elsewhere." Actually both self-imports were unused; all remaining references use fully-qualified `crate::ui::modal::` / `crate::ui::picker::` paths.
- **Fix:** Removed both `self` self-imports (but kept the item imports: `Modal, AddProjectForm, CommandArgsForm, NewWorkspaceForm, Picker, PickerKind, PickerOutcome`). Kept `use crate::ui::preview;` — it IS still used by dispatch_action.
- **Files modified:** `src/app.rs` (use declarations)
- **Commit:** `8865681` (folded into Task 2)

### Out-of-Scope Finding (deferred)

- Pre-existing `cargo fmt --check` failures across 13 files (94 diffs). Logged in `.planning/phases/01-architectural-split/deferred-items.md` with scope and suggested one-shot fix.

## Surprises for Downstream Plans

1. **Visibility elevation.** `App::active_workspace`, `build_working_map`, and `active_sessions` are now `pub(crate)`. Plans 01-02 / 01-03 / 01-04 can use them directly; plan 01-05 should decide whether they stay on `App` or move to helper modules.
2. **Import hygiene.** `src/app.rs` still carries `use crate::ui::modal::{AddProjectForm, CommandArgsForm, Modal, NewWorkspaceForm};`, `use crate::ui::picker::{Picker, PickerKind, PickerOutcome};`, and `use crate::ui::preview;`. The first two get revisited by plan 01-02 (modal extraction). The last one remains until the preview-picker path is touched.
3. **Draw module reaches into App.** `ui::draw::draw` passes `&mut app.left_list`, `&mut app.right_list` and calls `app.build_working_map()`. This is the intended pattern for the rest of the phase — don't fight it.
4. **The `crate::ui::draw::` self-qualification inside `draw()`** (`crate::ui::draw::status_bar(app, frame, ...)`) is redundant but harmless and matches the plan's explicit call-site spec. Leave as-is; plan 01-05 can simplify.
5. **Repo-wide cargo fmt is dirty.** Any future plan that runs `cargo fmt` (not `cargo fmt --check`) will reformat 13 unrelated files — coordinate with a dedicated style commit.

## Known Stubs

None. No placeholder UI, no mock data, no TODO/FIXME introduced by this plan.

## Threat Flags

None. Pure internal refactor — no new trust boundary, no new input path, no new process spawn, no new filesystem access. The render pipeline reads the same App state from the same call site.

## Self-Check: PASSED

Files verified to exist:
- `src/ui/draw.rs` — FOUND (created in 29e0a8f)
- `src/ui/mod.rs` — contains `pub mod draw;` — FOUND
- `src/app.rs` — contains `crate::ui::draw::draw(self, frame)` — FOUND
- `src/app.rs` — no longer contains `fn draw_status_bar` or `fn draw_menu_bar` — CONFIRMED

Commits verified in git log:
- `29e0a8f` — FOUND
- `8865681` — FOUND
