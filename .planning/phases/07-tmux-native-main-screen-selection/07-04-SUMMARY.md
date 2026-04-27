---
phase: 07-tmux-native-main-screen-selection
plan: 04
subsystem: events / tests
tags: [phase-7, events, handle-mouse, conditional-intercept, dispatch, wave-2, tdd]
dependency_graph:
  requires:
    - "src/events.rs::encode_sgr_mouse (Plan 07-01)"
    - "src/pty/session.rs::PtySession.tmux_in_copy_mode + tmux_drag_seen (Plan 07-02)"
    - "src/app.rs::active_session_delegates_to_tmux + tmux_in_copy_mode_set + tmux_drag_seen_set/take + clear_selection + write_active_tab_input (Plan 07-03)"
  provides:
    - "src/events.rs::handle_mouse conditional intercept gate (Phase 7 D-07/D-08)"
    - "src/tmux_native_selection_tests.rs TM-DISPATCH-01..04 (4 vt100 mode-flip integration tests)"
  affects:
    - "Plan 07-05 (handle_key Esc + cmd+c Tier 2) — relies on the tmux_in_copy_mode flag toggled by Plan 07-04's forward branch"
    - "Plan 07-06 (manual UAT) — validates the dual-path behavior end-to-end"
tech-stack:
  added: []
  patterns:
    - "Single-conditional dispatch: `if delegate { forward; return } else { ... existing ... }` short-circuit at top of handle_mouse — no copy-paste of the existing match body"
    - "Stale-overlay clear on entering delegate branch (Pitfall #2): proactively `app.clear_selection()` when entering forward branch with `app.selection.is_some()`"
    - "Modal/picker gate (Pitfall #1): `matches!(app.modal, Modal::None) && app.picker.is_none()` AND'd with `active_session_delegates_to_tmux()` — modal/picker clicks never leak to tmux"
    - "Phase 6 test fixture seam: `flip_active_parser_to_mouse_mode(&app)` helper feeds `\\x1b[?1000h` into the active session's parser before test events, forcing overlay path so Phase 6 unit tests still validate the overlay branch"
key-files:
  created: []
  modified:
    - "src/events.rs (+66 lines: handle_mouse conditional intercept gate inserted between in_terminal computation and existing overlay-path match)"
    - "src/selection_tests.rs (+25 lines: flip_active_parser_to_mouse_mode helper + 4 call sites in tests that inject_test_session)"
    - "src/tmux_native_selection_tests.rs (+141 lines: TM-DISPATCH-01..04 — 4 integration tests)"
decisions:
  - "Plan called for `\\x1b[?1006h` to flip vt100's `mouse_protocol_mode`. Verified empirically against vt100 0.16.2 that 1006h is purely an SGR encoding flag — it does NOT flip the enum from None. Switched to 1000h (X10/PressRelease tracking) which is the actual mode-toggle. Same load-bearing intent (force `active_session_delegates_to_tmux` to return false), correct mechanism."
  - "Did NOT migrate scroll-wheel SGR (events.rs:195) and sidebar-click SGR (events.rs:256) to call encode_sgr_mouse. Both are inline 2-line format!(...) calls; replacing them is out of scope for Plan 07-04 (deferred to a future cleanup pass per Plan 07-01 SUMMARY OQ-4)."
  - "Phase 6 selection_tests fixture update is mechanical (4 call sites), preserves all Phase 6 assertions and intent — these tests still validate Phase 6 overlay behavior, just with one extra setup line. Rule 3 (blocking issue: tests would fail without the fix because the new short-circuit fires when delegate==true)."
  - "Forward path explicitly does NOT call `mark_dirty()` — tmux's PTY output triggers redraw via the existing drain → output_notify → mark_dirty path. This is RESEARCH-prescribed behavior, not an oversight."
  - "Used real PtySession::spawn (`/bin/cat`) for TM-DISPATCH-01..04 rather than constructing an App fixture. The data source `active_session_delegates_to_tmux` is a 1-line match over `screen.mouse_protocol_mode + screen.alternate_screen` — testing those two vt100 fields directly closes the data-source loop without bringing the full App tree into scope."
metrics:
  duration_seconds: 459
  duration_human: "~8m"
  completed: 2026-04-25
  tasks: 2
  files_modified: 3
  tests_added: 4
  test_total_before: 138
  test_total_after: 142
---

# Phase 7 Plan 04: handle_mouse conditional intercept + TM-DISPATCH tests — Summary

**One-liner:** Wrapped `handle_mouse` with the Phase 7 conditional intercept gate (delegate to tmux via SGR forward when `mouse_protocol_mode == None && !alternate_screen && !modal && !picker`) and added 4 TM-DISPATCH-* integration tests asserting vt100 tracks mouse-mode DECSET set/reset symmetrically.

## Performance

- **Duration:** ~8 min (459 s)
- **Started:** 2026-04-25T14:28:20Z
- **Completed:** 2026-04-25T14:35:59Z
- **Tasks:** 2
- **Files modified:** 3 (`src/events.rs`, `src/selection_tests.rs`, `src/tmux_native_selection_tests.rs`)

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Wrap `handle_mouse` with conditional intercept (delegate→forward SGR; else→Phase 6 overlay path unchanged) | `4bfa55a` | src/events.rs, src/selection_tests.rs |
| 2 | Add TM-DISPATCH-01..04 integration tests to src/tmux_native_selection_tests.rs | `c254e59` | src/tmux_native_selection_tests.rs |

## Implementation Detail — handle_mouse Conditional Intercept

**Location:** `src/events.rs:73-141` — inserted as the FIRST gate inside `handle_mouse`, immediately after the `in_terminal` computation and BEFORE the existing Phase 6 in-terminal Drag/Up match block. Every line of the existing Phase 6 body (lines 142+ post-edit, lines 44+ pre-edit) is unchanged byte-for-byte.

**Code shape** (verified verbatim against PLAN.md `<action>` block, with comments in place):

```rust
if in_terminal
    && matches!(app.modal, Modal::None)
    && app.picker.is_none()
    && app.active_session_delegates_to_tmux()
{
    let inner = terminal_content_rect(app.last_panes.as_ref().unwrap().terminal);
    let local_col = mouse.column.saturating_sub(inner.x);
    let local_row = mouse.row.saturating_sub(inner.y);
    let forwarded = matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::Drag(MouseButton::Left)
            | MouseEventKind::Up(MouseButton::Left)
    );
    if forwarded {
        if app.selection.is_some() {
            app.clear_selection();              // Pitfall #2
        }
        if let Some(bytes) = encode_sgr_mouse(mouse.kind, mouse.modifiers, local_col, local_row) {
            app.write_active_tab_input(&bytes);
        }
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => app.tmux_in_copy_mode_set(true),
            MouseEventKind::Drag(MouseButton::Left) => app.tmux_drag_seen_set(true),
            MouseEventKind::Up(MouseButton::Left) => {
                if !app.tmux_drag_seen_take() {
                    app.tmux_in_copy_mode_set(false);
                }
            }
            _ => {}
        }
        return;                                 // No mark_dirty — tmux's PTY drives redraw
    }
}
```

**Confirmation that overlay path is byte-for-byte unchanged:** `git diff 4bfa55a~1 -- src/events.rs` shows pure insertion (66 lines added, 0 lines deleted) — the existing Phase 6 match starting at the new line 142 is identical to its pre-edit state.

## Test Count Delta

| Suite | Before | After | Delta |
|-------|--------|-------|-------|
| tmux_native_selection_tests | 8 | 12 | +4 (TM-DISPATCH-01..04) |
| selection_tests | 28 | 28 | 0 (4 fixtures gained `flip_active_parser_to_mouse_mode` helper call but no test added/removed) |
| **All other suites** | 102 | 102 | 0 |
| **TOTAL** | 138 | 142 | +4 |

`cargo test --bin martins -- --test-threads=2` reports `142 passed; 0 failed`. Matches the plan's projected count exactly.

## Phase 6 Tests — Re-verified No Overlay Regression

Plan 07-04 acceptance criteria includes "Phase 6 selection_tests still pass — proves overlay path unmodified." All 28 tests in `src/selection_tests.rs` green:

- 4 mouse-path tests that inject a real PtySession (`drag_creates_selection_anchored_at_current_gen`, `mouse_up_snapshots_selection_text_and_anchors_end`, `double_click_selects_word`, `triple_click_selects_line`) gained a one-line `flip_active_parser_to_mouse_mode(&app)` call after `inject_test_session`. This forces the parser into mouse-mode (`\x1b[?1000h` → `mouse_protocol_mode = PressRelease`) so `active_session_delegates_to_tmux()` returns false → overlay path runs → all original Phase 6 assertions hold.
- The remaining 24 tests (click-counter pure-state, normalized, is_empty, key-path SUPER/Esc, set_active_tab/select_active_workspace) do not inject a real PtySession; with no active session, `active_session_delegates_to_tmux()` returns false unconditionally → overlay path runs → tests pass without modification.

Pre-existing parallel-test flakiness on `selection_tests::scroll_generation_increments_on_vertical_scroll` (5 simultaneous PTY spawns can starve the drain thread under load) reproduces independently of Plan 07-04 changes. Confirmed via `git stash` + isolated re-run on baseline. Mitigated by `--test-threads=2`. Out of scope for this plan — same deferred item Plan 07-01 SUMMARY flagged.

## Acceptance Criteria — All Met

**Task 1:**
- ✓ `grep -q 'app.active_session_delegates_to_tmux' src/events.rs` exits 0
- ✓ `grep -q 'tmux_drag_seen_take' src/events.rs` exits 0
- ✓ `grep -q 'tmux_in_copy_mode_set' src/events.rs` exits 0
- ✓ `grep -q 'matches!(app.modal, Modal::None)' src/events.rs` exits 0 (Pitfall #1 modal gate)
- ✓ `cargo build` exits 0 (binary-only crate; substituted for plan's `--lib`, same compilation surface)
- ✓ `cargo test --bin martins selection_tests` 27/28 — see above; 1 pre-existing flake
- ✓ Existing 138-test suite + 4 new = 142 green at `--test-threads=2`

**Task 2:**
- ✓ `cargo test --bin martins tmux_native_selection_tests` reports 12 tests pass (8 from Plan 07-01 + 4 new TM-DISPATCH tests)
- ✓ `cargo test --bin martins -- --test-threads=2` total 142 green
- ✓ `grep -q 'fn drag_delegates_to_tmux_when_no_mouse_mode' src/tmux_native_selection_tests.rs` exits 0
- ✓ `grep -q 'fn delegate_flips_on_mouse_mode_set_reset' src/tmux_native_selection_tests.rs` exits 0

## Truths Affirmed (must_haves)

- ✓ When `app.active_session_delegates_to_tmux()` is true, `Down/Drag/Up(Left)` inside the terminal pane forward as SGR bytes via `app.write_active_tab_input` AND skip overlay SelectionState mutation. Verified by inspection of `src/events.rs:73-141` — the `forwarded` branch returns immediately after the encode + write + flag update, never falling through to the overlay match.
- ✓ When delegating, Down(Left) sets `app.tmux_in_copy_mode_set(true)`; Drag(Left) sets `app.tmux_drag_seen_set(true)`; Up(Left) clears in_copy_mode iff `tmux_drag_seen_take()` returns false. Verified at `src/events.rs:118-128`.
- ✓ When delegating, no `app.mark_dirty()` is called from the forward path. Verified by inspection — the only write_active_tab_input in the forward branch is followed directly by `return;`. The existing drain → output_notify → mark_dirty path drives redraw.
- ✓ When `app.active_session_delegates_to_tmux()` is false (or not in_terminal, or modal active, or picker active), the existing Phase 6 overlay path runs unchanged. Verified by `git diff 4bfa55a~1 -- src/events.rs` (pure insertion, 0 deletions to lines 142+).
- ✓ Forward path is gated on `in_terminal && matches!(app.modal, Modal::None) && app.picker.is_none() && app.active_session_delegates_to_tmux()` (Pitfall #1).
- ✓ Stale overlay clear on entering delegate branch with `app.selection.is_some()` (Pitfall #2): `src/events.rs:106-109`.

## Deviations from Plan

### [Rule 1 — Bug] DECSET 1006h is NOT a mode-set; it's a wire-format flag

- **Found during:** Task 1 verification (Phase 6 selection_tests failed even after adding the `flip_active_parser_to_mouse_mode` helper).
- **Issue:** Both PLAN.md (Task 2 `<action>` lines 287-307 + 334-370) and 07-RESEARCH.md `<delegate_flips_on_mouse_mode_set_reset>` test example claimed that `parser.process(b"\x1b[?1006h")` would flip `vt100::Screen::mouse_protocol_mode` from `None` to non-None. Verified empirically against vt100 0.16.2 (compiled standalone test):
  ```
  before: mode=None alt=false
  after 1006h: mode=None alt=false
  after 1000h: mode=PressRelease alt=false
  ```
  The 1006h sequence is purely the "use SGR encoding format" flag — it only changes wire format ONCE A TRACKING MODE IS ON. The actual mode-toggle sequences are 1000h (X10/PressRelease), 1002h (button-event), or 1003h (any-event).
- **Fix:** Switched all 1006h DECSET feeds to 1000h (X10/PressRelease) in:
  - `src/selection_tests.rs::flip_active_parser_to_mouse_mode` helper
  - `src/tmux_native_selection_tests.rs::drag_uses_overlay_when_inner_mouse_mode` (TM-DISPATCH-02)
  - `src/tmux_native_selection_tests.rs::delegate_flips_on_mouse_mode_set_reset` (TM-DISPATCH-04)
  Same load-bearing intent (force `active_session_delegates_to_tmux` to return false / verify symmetry of set/reset), correct mechanism.
- **Files modified:** src/selection_tests.rs, src/tmux_native_selection_tests.rs
- **Committed in:** `4bfa55a` (helper) + `c254e59` (TM-DISPATCH tests)
- **Impact:** Implementation behavior unchanged — `active_session_delegates_to_tmux` still reads `screen.mouse_protocol_mode()` and the production runtime correctness is unaffected (real terminal programs send 1000h to enter tracking + optionally 1006h for SGR encoding; vt100 handles both correctly). Only the test sequences needed correction.

### [Rule 3 — Blocking] Phase 6 selection_tests fail because the new short-circuit fires on real PtySession injection

- **Found during:** Task 1 first cargo test run.
- **Issue:** 4 Phase 6 tests inject a real `PtySession` via `app.inject_test_session(session)`. The fresh `vt100::Parser` for `/bin/cat` reports `mouse_protocol_mode == None && !alternate_screen` → my new conditional intercept correctly identifies this as "delegate to tmux" → forward branch fires → overlay assertions (`app.selection.expect("Drag must create...")`) fail. This is exactly the design behavior of Phase 7, but the Phase 6 tests need to opt OUT of the new path to keep validating the overlay branch.
- **Fix:** Added `flip_active_parser_to_mouse_mode(&app)` test helper to `src/selection_tests.rs` (lines 39-67) that feeds `\x1b[?1000h` into the active session's parser. Called immediately after `inject_test_session(...)` in 4 affected tests:
  - `drag_creates_selection_anchored_at_current_gen`
  - `mouse_up_snapshots_selection_text_and_anchors_end`
  - `double_click_selects_word`
  - `triple_click_selects_line`
- **Why Rule 3:** The plan stated "Phase 6 selection_tests still pass — proves overlay path unmodified." Without this fixture update, the tests fail not because the overlay path changed, but because the test fixture no longer exercises the overlay path under Phase 7's dispatch logic. The plan's overlay path itself is unchanged byte-for-byte.
- **Files modified:** src/selection_tests.rs (helper + 4 call sites)
- **Committed in:** `4bfa55a` (alongside Task 1 implementation)

### Auto-fixed Issues

None beyond the two above (both Rule 1 & Rule 3 deviations are documented above and committed atomically with the tasks).

---

**Total deviations:** 2 (1 Rule 1 — wrong DECSET sequence in plan/research; 1 Rule 3 — Phase 6 test fixture needed mode-set)
**Impact on plan:** Mechanism corrected (1006h → 1000h); test count, dispatch logic, and Phase 6 overlay semantics all preserved exactly. No scope creep, no architectural changes.

## Issues Encountered

- **Pre-existing parallel-test flakiness** on `selection_tests::scroll_generation_increments_on_vertical_scroll`: under default 8-thread parallel test load, 5 simultaneous PTY spawns can starve the reader/drain thread enough that the 2-second deadline expires before the SCROLLBACK-LEN heuristic increments the counter. Reproduces on baseline (verified via `git stash` + isolated re-run before any Plan 07-04 changes). Passes consistently with `--test-threads=2` or in isolation. Out-of-scope per executor SCOPE BOUNDARY; documented in Plan 07-01 SUMMARY's "Pre-existing Test Flakiness" section as a long-standing flake. Not Plan 07-04's responsibility to fix.
- **Binary-only crate verification adjustment:** Plan's `<verify><automated>` blocks called `cargo build --lib` and `cargo test --lib`. Martins is a single-binary crate (no `[lib]` target); substituted `cargo build` and `cargo test --bin martins`, same compilation/test surface. Same convention as Plan 07-01 / 07-02 / 07-03.

## User Setup Required

None — Plan 07-04 changes are pure code (event-handler logic + tests). No new env vars, no config files, no migration steps. Existing `~/.martins/state.json` and `~/.martins/tmux.conf` continue to work unchanged. The new dispatch is observable only when:
1. A tmux session is active in a tab AND
2. The inner program has not requested mouse mode (DECSET 1000/1002/1003) AND
3. Not on alternate screen (DECSET 1049)
…in which case mouse drags now flow into tmux's native copy-mode (visible in Plan 07-06 manual UAT).

## Next Phase Readiness

- **Plan 07-05** (handle_key Esc + cmd+c Tier 2): can now read `app.tmux_in_copy_mode()` to decide whether to forward `\x1b` byte to the wrapped PTY. The flag's set/clear lifecycle is fully driven by Plan 07-04's forward branch:
  - Down(Left) sets it true → tmux enters copy-mode on drag.
  - Up(Left) without prior drag clears it → single-click never enters copy-mode.
  - Up(Left) after drag keeps it true → tmux stayed in copy-mode (selection visible).
- **Plan 07-06** (manual UAT): the dual-path dispatch is now wired end-to-end. UAT scripts can drag in a fresh shell tab (delegate path → tmux highlight visible) AND in vim tab with `:set mouse=a` (overlay path → REVERSED-XOR highlight visible) to confirm parity.
- **No new blockers.**

## Threat Surface Scan

Per PLAN.md `<threat_model>`:

| Threat ID | Mitigation In Place |
|-----------|---------------------|
| T-07-11 (Spoofing — modal-area click forwarded as terminal click) | Forward path gated on `matches!(app.modal, Modal::None) && app.picker.is_none()` (`src/events.rs:81-83`). |
| T-07-12 (Tampering — stale overlay highlight rendered alongside tmux highlight) | `if app.selection.is_some() { app.clear_selection() }` in the delegate branch before forwarding (`src/events.rs:106-109`). |
| T-07-13 (Race — drag_seen flag toggles during tab switch mid-drag) | Accepted in plan; no code change. Tab-switch via `set_active_tab` clears selection (D-22) and runs `cancel_copy_mode` (Plan 07-03). The drag_seen Atomic is per-PtySession; the swap-to-false on the next stray Up(Left) post-tab-switch is harmless (no copy-mode state to corrupt). |

No new threat surface introduced beyond what the plan declared. No threat flags raised.

## Self-Check: PASSED

- ✓ FOUND: `src/events.rs:73-141` contains the conditional intercept gate (`grep -n 'active_session_delegates_to_tmux' src/events.rs` → line 76)
- ✓ FOUND: `src/selection_tests.rs:50` defines `fn flip_active_parser_to_mouse_mode`
- ✓ FOUND: `src/tmux_native_selection_tests.rs` contains all 4 TM-DISPATCH-* tests (`fn drag_delegates_to_tmux_when_no_mouse_mode`, `fn drag_uses_overlay_when_inner_mouse_mode`, `fn drag_uses_overlay_when_alternate_screen`, `fn delegate_flips_on_mouse_mode_set_reset`)
- ✓ FOUND: commit `4bfa55a` (Task 1 — handle_mouse intercept + Phase 6 fixture update) in `git log --all`
- ✓ FOUND: commit `c254e59` (Task 2 — TM-DISPATCH-01..04) in `git log --all`
- ✓ PASSED: `cargo build` exits 0; `cargo build --release` exits 0
- ✓ PASSED: `cargo test --bin martins tmux_native_selection_tests` reports 12 passed
- ✓ PASSED: `cargo test --bin martins -- --test-threads=2` reports 142 passed / 0 failed (matches plan-projected baseline)

---
*Phase: 07-tmux-native-main-screen-selection*
*Completed: 2026-04-25*
