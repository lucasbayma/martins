---
phase: 06-text-selection
plan: 01
subsystem: selection
tags:
  - selection
  - state
  - rust
  - tdd
requirements:
  - SEL-01
  - SEL-02
  - SEL-03
  - SEL-04
dependency-graph:
  requires:
    - "App::new fixture pattern (navigation_tests.rs:56-63)"
    - "PtyManager + PtySession (Phase 1 / pty subsystem)"
  provides:
    - "SelectionState shape (start_gen, end_gen, text) — consumed by Plans 06-02..06-06"
    - "App click-counter fields (last_click_at, last_click_count, last_click_row, last_click_col) — consumed by Plan 06-03 (mouse) for double/triple-click dispatch"
    - "App::clear_selection helper — consumed by Plan 06-06 (tab/workspace switch sites)"
    - "App::inject_test_session + PtyManager::insert_for_test test seam — consumed by Plans 06-03/04/06"
  affects:
    - "Public surface of crate::app::SelectionState (Copy derive removed)"
tech-stack:
  added: []
  patterns:
    - "TDD RED/GREEN gate with binary-only crate (cargo test --bin martins selection_tests --no-run for compile-failure check)"
    - "#[cfg(test)] impl Block at end of file for test-only methods (mirrors save_state_spawn / app_tests precedent)"
key-files:
  created:
    - src/selection_tests.rs
  modified:
    - src/app.rs
    - src/main.rs
    - src/events.rs
    - src/pty/manager.rs
decisions:
  - "Drag(Left) construction in events.rs uses placeholder start_gen=0 / end_gen=None / text=None; Plan 06-03 will wire real anchoring + snapshot. This plan only commits the data shape."
  - "Click-counter logic added to handle_mouse Down(Left) in this plan (not deferred to 06-03) because Tests 3-4 require it; minimal D-16 implementation only — double/triple dispatch belongs to 06-03."
  - "clear_selection + inject_test_session marked #[allow(dead_code)] until call sites land in Plan 06-06 / Plans 06-03+. Mirrors the save_state_spawn pattern (Plan 05-02 → 05-03)."
metrics:
  duration: 5m
  tasks: 3
  files: 4
  completed_date: 2026-04-25
---

# Phase 6 Plan 1: Selection Data Foundation Summary

Extended `SelectionState` with anchored (gen, row, col) endpoints + post-scroll-off text snapshot, added click-counter state to `App`, introduced `App::clear_selection` and a `#[cfg(test)]` `inject_test_session` test seam — laying the data foundation that every downstream Phase 6 plan reads/writes.

## What Was Built

### SelectionState extension (`src/app.rs:28-60`)

Three new fields wired in place. Copy derive dropped (String is not Copy):

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionState {
    pub start_col: u16,
    pub start_row: u16,
    pub start_gen: u64,           // anchored at drag-start (D-06)
    pub end_col: u16,
    pub end_row: u16,
    pub end_gen: Option<u64>,     // None mid-drag, Some after mouse-up (D-07)
    pub dragging: bool,
    pub text: Option<String>,     // snapshot at mouse-up; survives scroll-off (RESEARCH §Q2)
}
```

Identical `normalized()` and `is_empty()` semantics — public API unchanged for downstream consumers in `events.rs`, `ui/terminal.rs`, and `App::copy_selection_to_clipboard`.

### App click-counter fields (`src/app.rs:78-100`)

Four new fields supporting D-16's 300ms multi-click cluster:

- `last_click_at: Option<Instant>`
- `last_click_count: u8`
- `last_click_row: u16`
- `last_click_col: u16`

`handle_mouse` `Down(Left)` (`src/events.rs:80-100`) updates the counter on every left-mouse-down: increments within the 300ms window at the same row, resets to 1 otherwise.

NO App-level `scroll_generation` field was added — D-05 places it on `PtySession` (lands in Plan 06-02).

### `App::clear_selection` helper (`src/app.rs:217-220`)

Centralized clear with the dirty-flag guard: `selection.take().is_some()` is the gate that avoids spurious `mark_dirty` calls when the user has no active selection. Plan 06-06 wires the seven tab/workspace switch call sites.

### Test seam: `App::inject_test_session` + `PtyManager::insert_for_test`

Both `#[cfg(test)]` `pub(crate)` — never compiled into release binaries (T-06-12 mitigation).

- `PtyManager::insert_for_test` (`src/pty/manager.rs:128-138`): pure HashMap insert, mirrors the production `spawn_tab` insert form without paying the spawn cost.
- `App::inject_test_session` (`src/app.rs:607-680`): seeds Project + Workspace + TabSpec stubs and registers a pre-built `PtySession` so `app.active_sessions().get(active_tab)` returns it. Returns the assigned `tab_id`.

### Tests (`src/selection_tests.rs`, new — 192 lines, 4 tests)

- `normalized_orders_ascending_when_reversed_endpoints` — covers row-reversed, col-reversed-same-row, and identity cases.
- `selection_state_is_empty_when_start_eq_end` — boundary semantics.
- `click_counter_increments_within_300ms_same_row` — synthesizes two `Down(Left)` events, 50ms apart, asserts counter == 2.
- `click_counter_resets_when_row_differs` — second click at a different row resets counter to 1.

All 4 pass; full test suite (113 tests) green.

## TDD Gate Compliance

Plan tasks include `tdd="true"` on Task 1 (RED) and Task 2 (GREEN). Gates verified in git log:

1. RED gate: `9076f30 test(06-01): add failing selection state tests` — tests intentionally fail to compile (19 errors against unknown fields).
2. GREEN gate: `e5f9ad0 feat(06-01): extend SelectionState with anchored endpoints + click-counter` — adds the fields the tests reference; all 4 tests pass.
3. No REFACTOR commit needed — implementation is minimal and matches the canonical shape from PATTERNS.md.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Click-counter logic must be wired in Task 2, not deferred**

- **Found during:** Task 1 acceptance criteria check.
- **Issue:** Plan Task 2 added the click-counter fields but the plan body didn't explicitly require wiring the increment/reset logic in `handle_mouse Down(Left)`. Tests 3-4 (synthesized via `events::handle_mouse`) cannot pass without it.
- **Fix:** Added a minimal D-16 increment/reset block to `events.rs` `Down(Left)` arm. Same-row, 300ms threshold reset semantics. Double/triple-click dispatch is intentionally NOT here — that's Plan 06-03's territory.
- **Files modified:** `src/events.rs`
- **Commit:** `e5f9ad0`

**2. [Rule 3 - Blocking] `Drag(Left)` SelectionState construction must adopt new field shape**

- **Found during:** Task 2 cargo build.
- **Issue:** The existing `Drag(Left)` arm in `events.rs:54` constructs `SelectionState { start_col, start_row, end_col, end_row, dragging }` — missing the three new required fields (`start_gen`, `end_gen`, `text`). Build fails until updated.
- **Fix:** Added `start_gen: 0`, `end_gen: None`, `text: None` placeholders. Plan 06-03 will replace the placeholders with real anchoring against `session.scroll_generation` and the mouse-up text snapshot.
- **Files modified:** `src/events.rs`
- **Commit:** `e5f9ad0`

**3. [Rule 3 - Blocking] `#[allow(dead_code)]` on `clear_selection` and `inject_test_session`**

- **Found during:** Task 2 cargo build (dead_code warning on `clear_selection`).
- **Issue:** Plan introduces helpers consumed by future plans. Without `#[allow(dead_code)]` the build emits warnings — project precedent (Plan 05-02 → 05-03 `save_state_spawn`) applies the same allow.
- **Fix:** Added `#[allow(dead_code)]` with documentation pointing at the consuming plan. Removed when call sites land.
- **Files modified:** `src/app.rs`
- **Commit:** `e5f9ad0` and `3c3ad69`

**4. [Rule 3 - Tooling] Binary-only crate test invocation**

- **Found during:** Task 1 RED verification.
- **Issue:** Plan's verify command was `cargo test --lib selection_tests`, but `martins` is binary-only — `cargo test --lib` errors with "no library targets found". Phase 3 Plan 03-01 already documented this as a deviation.
- **Fix:** Used `cargo test --bin martins selection_tests` for filtered runs and `cargo test` for full suite. No production code change.

## Threat Model Compliance

- **T-06-12 (Elevation of Privilege via test seam):** Mitigation applied. `inject_test_session` and `insert_for_test` are both `#[cfg(test)] pub(crate)`. Verified `cargo build` (release-shape, no `--cfg test`) compiles cleanly with no reference to either symbol — they are stripped from production binaries.
- **T-06-01 / T-06-02:** No new surface introduced in this plan (no clipboard write, no PTY data path).

No new threat surface emerged during execution.

## Acceptance Criteria

| Criterion | Result |
| --- | --- |
| `cargo build` | PASS |
| `cargo test --bin martins selection_tests` (4 tests) | PASS (4 passed) |
| `cargo test` full suite (113 tests) | PASS (113 passed, 0 failed) |
| `pub start_gen: u64` in src/app.rs | 1 match |
| `pub end_gen: Option<u64>` in src/app.rs | 1 match |
| `pub text: Option<String>` in src/app.rs | 1 match |
| `pub last_click_count: u8` in src/app.rs | 1 match |
| No App-level `scroll_generation` field | 0 matches (correct — lives on PtySession) |
| `pub(crate) fn clear_selection` in src/app.rs | 1 match |
| Copy derive removed from SelectionState | confirmed |
| `mark_dirty` call count in src/app.rs | 9 (>= 6 invariant preserved) |
| `pub(crate) fn insert_for_test` in src/pty/manager.rs | 1 match |
| `pub(crate) fn inject_test_session` in src/app.rs | 1 match |
| `#[cfg(test)] impl App` block | 1 match |

All criteria pass.

## Self-Check: PASSED

- File `src/selection_tests.rs`: FOUND
- File `src/app.rs` (modified): FOUND
- File `src/main.rs` (modified): FOUND
- File `src/events.rs` (modified): FOUND
- File `src/pty/manager.rs` (modified): FOUND
- Commit 9076f30 (Task 1 — TDD RED): FOUND
- Commit e5f9ad0 (Task 2 — TDD GREEN): FOUND
- Commit 3c3ad69 (Task 3 — test seam): FOUND
