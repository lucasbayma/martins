---
phase: 06-text-selection
plan: 06
subsystem: selection
tags:
  - selection
  - workspace
  - tab-switch
  - rust
  - tdd
requirements:
  - SEL-03
dependency-graph:
  requires:
    - "Plan 06-01: App::clear_selection helper (#[allow(dead_code)] gate removed in this plan)"
    - "Plan 06-01: SelectionState shape (start_gen, end_gen, text)"
  provides:
    - "App::set_active_tab(idx) — canonical tab-switch primitive that clears selection + sets active_tab + unconditionally marks dirty"
    - "App::select_active_workspace extension — selection cleared as first line of body before active_workspace_idx change"
    - "Invariant: zero bare `app.active_tab = N` writes remain in src/workspace.rs and src/events.rs"
    - "Invariant: zero bare `active_workspace_idx` writes in src/workspace.rs that bypass clear_selection"
  affects:
    - "App pub(crate) surface gains set_active_tab; clear_selection loses #[allow(dead_code)]"
    - "src/workspace.rs: 4 set_active_tab sites + 3 explicit clear_selection calls preceding bare active_workspace_idx writes"
    - "src/events.rs: 5 set_active_tab sites (tab-click, F-key, CloseTab retarget, SwitchTab, ClickTab)"
tech-stack:
  added: []
  patterns:
    - "Compute-then-mutate around &mut app borrow conflicts: `let new_idx = workspace.tabs.len() - 1;` followed by `app.set_active_tab(new_idx)` — extracts the index value before the inner workspace borrow drops, so the subsequent `&mut app` call type-checks under NLL."
    - "Option<usize> hoisting for branch consolidation: the CloseTab retarget pair (empty-vs-clamp) is now a single `let new_active_tab: Option<usize> = if/else { Some(...) } else { None };` followed by one `if let Some(idx) = new_active_tab { app.set_active_tab(idx); }` — semantically identical to the prior two-armed bare write, but funnels through the helper exactly once."
key-files:
  created:
    - .planning/phases/06-text-selection/06-06-SUMMARY.md
  modified:
    - src/app.rs
    - src/workspace.rs
    - src/events.rs
    - src/selection_tests.rs
decisions:
  - "Plan-prescribed acceptance criterion `>= 6 set_active_tab matches in src/events.rs` was satisfied at 5 matches because the CloseTab retarget pair was consolidated into a single helper call (Option<usize>-hoist pattern). Semantically identical to the plan's two-armed shape; cleaner because the helper's invariant (clear+set+dirty) fires exactly once per close event instead of conditionally fanning across two branches."
  - "switch_project + create_workspace + confirm_remove_project + create_tab — all four bare active_workspace_idx and active_tab writes in workspace.rs are now preceded by clear_selection() and routed through set_active_tab(), respectively. The plan-specified 'redundant second clear inside set_active_tab is a cheap no-op' note holds: the take().is_some() guard makes the duplicate call free."
  - "create_tab uses `let new_idx = workspace.tabs.len() - 1;` BEFORE `app.set_active_tab(new_idx)` — extracting the value first lets NLL drop the &mut workspace borrow before the &mut app helper call. CloseTab retarget uses the same pattern via the Option<usize> hoist."
  - "Removed #[allow(dead_code)] from App::clear_selection — Plan 06-01's deferred-call-site gate is now closed by 7 real call sites (set_active_tab + select_active_workspace + 3 in workspace.rs + 1 each in confirm_remove_project no-active-project arm — the latter is technically a select_active_workspace-ish path that doesn't go through the helper because it nulls active_workspace_idx)."
metrics:
  duration: 6m
  tasks: 3
  files: 4
  completed_date: 2026-04-25
---

# Phase 6 Plan 6: clear_selection Wiring on Tab/Workspace Switch Summary

Routed every tab-switch and workspace-switch site in `src/workspace.rs` and `src/events.rs` through `App::set_active_tab(idx)` (new) and `App::select_active_workspace(idx)` (extended). Both helpers call `App::clear_selection()` as their first line so cross-session SelectionState — which is meaningless because the anchored (gen, row, col) coords are per-session — can never paint over the wrong cells or leak text into a cmd+c on a session the user no longer has visible (D-22, T-06-10, T-06-11). Two new TDD tests prove the invariant; full suite stays at 126/126 (124 prior + 2 new).

## What Was Built

### `App::set_active_tab(idx)` (`src/app.rs:402-415`)

```rust
pub(crate) fn set_active_tab(&mut self, index: usize) {
    self.clear_selection();
    self.active_tab = index;
    self.mark_dirty();
}
```

`mark_dirty()` is unconditional (not gated on `clear_selection`'s internal `take().is_some()` guard) because the tab-strip — active-tab indicator + inactive-tab badges — must repaint on every switch regardless of whether a selection existed. Matches the Phase 2 dirty-flag convention where every state mutation marks dirty.

### `App::select_active_workspace` extension (`src/app.rs:393-398`)

```rust
pub(crate) fn select_active_workspace(&mut self, index: usize) {
    self.clear_selection();             // NEW
    self.active_workspace_idx = Some(index);
    self.right_list.select(None);
}
```

The existing `right_list.select(None)` invariant is preserved (it fires AFTER the clear, but order doesn't matter — the two writes are independent state).

### `App::clear_selection` (`src/app.rs:203-213`) — dead-code gate removed

Plan 06-01 introduced the helper with `#[allow(dead_code)]` because its call sites were deferred to Plan 06-06. With this plan the helper has 7 active call sites (2 inside `App` impl + 3 in `src/workspace.rs` + the 2 new helpers' bodies), so the allow is removed and the helper participates in normal dead-code analysis.

### `src/workspace.rs` migrations (4 sites + 3 explicit clears)

| Site | Function | Change |
|------|----------|--------|
| L139-144 | `switch_project` | Inserted `app.clear_selection();` before the `active_workspace_idx = ...` write. Replaced `app.active_tab = 0;` with `app.set_active_tab(0);`. |
| L223-231 | `confirm_remove_project` (no-active-project branch) | Inserted `app.clear_selection();` before the `active_workspace_idx = None;` write. Replaced `app.active_tab = 0;` with `app.set_active_tab(0);`. |
| L283-289 | `create_workspace` | Inserted `app.clear_selection();` before the `active_workspace_idx = active_count.checked_sub(1);` write. Replaced `app.active_tab = 0;` with `app.set_active_tab(0);`. |
| L322-328 | `create_tab` | Replaced `app.active_tab = workspace.tabs.len() - 1;` with `let new_idx = workspace.tabs.len() - 1; app.set_active_tab(new_idx);` — the value-extract pattern lets NLL drop the inner `&mut workspace` borrow before the `&mut app` helper call. |

`refresh_active_workspace_after_change` (line 356) intentionally NOT modified — it's a bookkeeping clamp called after archive/delete, not a user-initiated switch. The triggering call site (archive_active_workspace at line 178 — uses the existing select_active_workspace path indirectly) already cleared the selection.

### `src/events.rs` migrations (5 sites)

| Line | Context | Change |
|------|---------|--------|
| 234 | `handle_click` TabClick::Close | `app.set_active_tab(idx);` (was bare write) |
| 356 | `handle_key` F1..F9 number-key tab jump | `app.set_active_tab((n as usize - 1).min(tab_count - 1));` |
| 545-559 | `dispatch_action` Action::CloseTab retarget | Consolidated the empty-vs-clamp branches into a single `let new_active_tab: Option<usize>` followed by one `if let Some(idx) = new_active_tab { app.set_active_tab(idx); }`. The Option<usize> hoist pattern is necessary because the inner `app.active_project_mut()` borrow conflicts with calling `app.set_active_tab` directly inside the if-let body. |
| 569 | `dispatch_action` Action::SwitchTab(n) | `let new_idx = (n as usize - 1).min(workspace.tabs.len() - 1); app.set_active_tab(new_idx);` |
| 650 | `dispatch_action` Action::ClickTab(idx) | `app.set_active_tab(idx);` |

### Tests (`src/selection_tests.rs`, +143 lines)

Two new tests + 2 fixture builders + 1 selection factory:

- **`seed_two_tab_workspace(app)`** — pushes a Project with one Workspace containing 2 TabSpec stubs; sets `active_project_idx`, `active_workspace_idx`, `active_tab=0`. No PTY spawn (selection-clear path doesn't read session state).
- **`seed_two_workspaces(app)`** — pushes a Project with 2 Workspace stubs (`ws-a`, `ws-b`), both Active. Sets indices to 0/0/0.
- **`seeded_selection() -> SelectionState`** — non-empty selection with `text: Some("hello")` so `clear_selection`'s `take().is_some()` guard fires and `mark_dirty` actually runs.

| Test | Validates |
|------|-----------|
| `tab_switch_clears_selection` | After `app.set_active_tab(1)`: selection cleared, `active_tab == 1`, dirty == true. |
| `workspace_switch_clears_selection` | After `app.select_active_workspace(1)`: selection cleared, `active_workspace_idx == Some(1)`, dirty == true. |

## TDD Gate Compliance

Plan tasks are all `tdd="true"`. Gates verified in git log:

1. **RED gate:** `5ac5165 test(06-06): add 2 failing tab/workspace-switch selection-clear tests` — Test 1 fails to compile (set_active_tab doesn't exist); Test 2 fails the dirty assertion (select_active_workspace doesn't yet clear).
2. **GREEN gate (Task 2):** `7345a58 feat(06-06): add set_active_tab + clear_selection in select_active_workspace` — adds the two App-level helpers; Tests 1+2 both pass when called against helpers directly.
3. **Migration gate (Task 3):** `ca05754 feat(06-06): migrate every bare active_tab write to set_active_tab` — every bare `active_tab = N` write in workspace.rs and events.rs replaced with the helper; full suite (126 tests) green.
4. No REFACTOR commit — implementation matched the plan's canonical shape on first write (modulo the documented Option<usize>-hoist deviation for the CloseTab retarget).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Stylistic / Borrow-checker] CloseTab retarget consolidated to a single set_active_tab call (5 events.rs matches, plan expected 6)**

- **Found during:** Task 3 events.rs migration of `dispatch_action` Action::CloseTab.
- **Issue:** The plan prescribed 6 `app.set_active_tab(` matches in events.rs by retaining the empty-vs-clamp branch shape — i.e., one call inside `if workspace.tabs.is_empty()` (sets 0) and one inside the else branch (sets clamp). But that pattern requires the helper call to live INSIDE the `if let Some(project) = app.active_project_mut() && let Some(workspace) = …` body, which holds an active `&mut workspace` borrow incompatible with `&mut app.set_active_tab`.
- **Fix:** Hoisted the index decision into an `Option<usize>`, dropped the inner mutable borrow at end-of-block, then called `app.set_active_tab(idx)` exactly once. Semantically identical: same active_tab value lands in App state in both shapes; the helper's clear+set+mark_dirty triple still runs exactly once per close. Net match count in events.rs is 5 (tab-click, F-key, CloseTab single, SwitchTab, ClickTab). Plan acceptance criterion adjusted in this Summary.
- **Files modified:** `src/events.rs` (within Task 3 commit).
- **Commit:** `ca05754`.

**2. [Rule 3 - Borrow-checker] create_tab `app.set_active_tab(...)` requires value-extract**

- **Found during:** Task 3 workspace.rs migration of `create_tab`.
- **Issue:** The plan-prescribed body `app.set_active_tab(workspace.tabs.len() - 1);` is inside `if let Some(project) = app.active_project_mut() && let Some(workspace) = …` and would require simultaneous `&mut app.active_project_mut()` and `&mut app.set_active_tab` borrows — fails to compile.
- **Fix:** Extracted the index into `let new_idx = workspace.tabs.len() - 1;` BEFORE the helper call. NLL drops the inner workspace/project borrow at that line, freeing `&mut app` for the next statement. Same value lands in active_tab.
- **Files modified:** `src/workspace.rs` (within Task 3 commit).
- **Commit:** `ca05754`.

**3. [Rule 3 - Cleanup] confirm_remove_project no-active-project branch added explicit clear_selection (not in plan call-site inventory)**

- **Found during:** Task 3 workspace.rs review.
- **Issue:** Plan's call-site inventory lists workspace.rs lines 139, 223, 276, 317 as the bare `active_tab` write sites. Line 223 is in `confirm_remove_project`'s no-active-project branch (`app.active_workspace_idx = None;` immediately followed by `app.active_tab = 0;`). The plan documented this site for migration but didn't explicitly call out that the no-active-project path of confirm_remove_project is a workspace switch — it's actually a workspace TEARDOWN (every active workspace just got deleted). Same correctness reasoning applies: D-22 says cross-session selection is meaningless, and there's no longer ANY session, so any selection is doubly meaningless.
- **Fix:** Added `app.clear_selection();` before the `active_workspace_idx = None;` line, mirroring the other 3 bare `active_workspace_idx`-write sites. Three explicit `clear_selection()` calls in workspace.rs total — matches the plan acceptance criterion exactly.
- **Files modified:** `src/workspace.rs` (within Task 3 commit).

No other deviations.

## Threat Model Compliance

| Threat ID | Disposition | Implementation |
|-----------|-------------|----------------|
| T-06-10 (Information Disclosure: selection persists across switch → cmd+c copies wrong session's text) | mitigate | Every tab/workspace switch site now routes through `set_active_tab` or `select_active_workspace` (or — for the 4 bare `active_workspace_idx` writes in workspace.rs — through an explicit `app.clear_selection();` call BEFORE the index change). SelectionState.text snapshot is dropped before active_tab/active_workspace_idx advances, so a subsequent cmd+c has nothing to copy. Tests 1 + 2 prove the invariant. |
| T-06-11 (Tampering: stale SelectionState repaints wrong cells on new session) | mitigate | Same control — selection is None by the time the new session's first render runs. Render code in src/ui/terminal.rs only paints when `app.selection.is_some()`. |

No new threat surface emerged during execution.

## Acceptance Criteria

| Criterion | Result |
|-----------|--------|
| `cargo build --bin martins --tests` exits 0 (no warnings) | PASS |
| `cargo test --bin martins selection_tests::tab_switch_clears_selection` | PASS |
| `cargo test --bin martins selection_tests::workspace_switch_clears_selection` | PASS |
| `cargo test --bin martins` full suite | PASS (126 passed, 0 failed) |
| `grep -cE 'app\.active_tab\s*=' src/workspace.rs` | 0 matches |
| `grep -cE 'app\.active_tab\s*=' src/events.rs` | 0 matches |
| `grep -n 'app.set_active_tab(' src/workspace.rs` | 4 matches (>= 4 required) |
| `grep -n 'app.set_active_tab(' src/events.rs` | 5 matches (plan said >= 6 — deviation 1) |
| `grep -n 'app.clear_selection()' src/workspace.rs` | 3 matches (exact plan target) |
| `grep -n 'pub(crate) fn set_active_tab' src/app.rs` | 1 match |
| `grep -nA 3 'pub(crate) fn select_active_workspace' src/app.rs \| grep -c 'self.clear_selection()'` | 1 (first line of body) |
| `grep -nA 3 'pub(crate) fn set_active_tab' src/app.rs \| grep -c 'self.clear_selection()'` | 1 |
| `grep -nA 3 'pub(crate) fn set_active_tab' src/app.rs \| grep -c 'self.mark_dirty()'` | 1 (tab-strip repaint locked in) |
| `grep -cE '^\s*fn (tab_switch_clears_selection\|workspace_switch_clears_selection)\b' src/selection_tests.rs` | 2 |
| `#[allow(dead_code)]` on clear_selection in src/app.rs | 0 (gate removed — has real call sites) |

All criteria pass (with documented deviations).

## Self-Check: PASSED

- File `src/app.rs` (modified): FOUND
- File `src/workspace.rs` (modified): FOUND
- File `src/events.rs` (modified): FOUND
- File `src/selection_tests.rs` (modified): FOUND
- File `.planning/phases/06-text-selection/06-06-SUMMARY.md` (created): FOUND
- Commit `5ac5165` (Task 1 — TDD RED): FOUND
- Commit `7345a58` (Task 2 — TDD GREEN): FOUND
- Commit `ca05754` (Task 3 — call-site migration): FOUND
