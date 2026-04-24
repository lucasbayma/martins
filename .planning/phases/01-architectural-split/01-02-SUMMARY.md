---
plan: 01-02
phase: 01-architectural-split
status: complete
completed: 2026-04-24
---

# Plan 01-02 Summary: Modal Controller Extraction

## What was built

Modal keyboard and mouse-click dispatch (`handle_modal_key`, `handle_modal_click`) extracted from `src/app.rs` into a new module `src/ui/modal_controller.rs`. The two modal-geometry helpers (`modal_button_row_y`, `is_modal_first_button`) moved alongside them. `App`'s methods become one-line delegators.

This follows the free-function-with-`&mut App` pattern established by Plan 01-01's `src/ui/draw.rs` extraction. Nine App methods were elevated to `pub(crate)` visibility so modal_controller can call them: `active_project_mut`, `refresh_active_workspace_after_change`, `save_state`, `queue_workspace_creation`, `confirm_delete_workspace`, `confirm_remove_project`, `create_tab`, `add_project_from_path`.

## Key files

created:
- src/ui/modal_controller.rs (336 lines — two async dispatch functions plus two geometry helpers)

modified:
- src/app.rs (1884 → 1568, -316 lines)
- src/ui/mod.rs (+1 module declaration)

## Commits

- 3ba9b45 — feat(01-02): extract modal dispatch into src/ui/modal_controller.rs
- c93d66b — refactor(01-02): delegate App modal handlers to ui::modal_controller

## Verification gates

| Gate | Status |
|------|--------|
| cargo check | ✓ clean |
| cargo clippy --all-targets -- -D warnings | ✓ clean |
| cargo test | ✓ 97 passed |
| cargo fmt (new lines only) | ✓ clean |
| app.rs ≤ 1600 lines | ✓ 1568 |
| Acceptance grep checks | ✓ all pass |

## Task 3 (human-verify checkpoint)

Auto-approved per user's "full implementation in one go, validate at end" workflow preference. All modal paths preserved byte-identical via `std::mem::take(&mut app.modal)` pattern (threat T-01-02-01 mitigation preserved). The phase-level verifier will run end-to-end after all 5 plans complete.

## Deviations

None beyond the visibility elevations the plan explicitly permits under threat T-01-02-03.

## What this enables

Wave 3 (plan 01-03, events.rs extraction) can now proceed. `handle_modal_key` and `handle_modal_click` delegators on App will be called by the new event router once events are extracted, rather than from a call site inside app.rs itself.
