---
phase: 07-tmux-native-main-screen-selection
plan: 05
subsystem: events / tests
tags: [phase-7, events, handle-key, cmd-c, esc, precedence, wave-3, tdd]
dependency_graph:
  requires:
    - "src/events.rs::encode_sgr_mouse (Plan 07-01)"
    - "src/tmux.rs::save_buffer_to_pbcopy (Plan 07-02)"
    - "src/app.rs::active_session_delegates_to_tmux + active_tmux_session_name + tmux_in_copy_mode + tmux_in_copy_mode_set + write_active_tab_input + copy_selection_to_clipboard (Plans 07-03 + Phase 6)"
    - "src/events.rs::handle_mouse forward branch sets tmux_in_copy_mode (Plan 07-04)"
    - "tokio::task::spawn_blocking (existing dependency)"
  provides:
    - "src/events.rs::handle_key cmd+c 3-tier precedence (Tier 1 overlay → Tier 2 tmux save-buffer → Tier 3 SIGINT)"
    - "src/events.rs::handle_key Esc 3-tier precedence (Tier 1 overlay clear → Tier 2 forward 0x1b → Tier 3 PTY/keymap fall-through)"
    - "src/tmux_native_selection_tests.rs TM-CMDC-02 + TM-CANCEL-01 + TM-ESC-02 invariants"
  affects:
    - "Plan 07-06 (manual UAT) — exercises both precedence chains end-to-end via UAT-7-D Esc cancel, UAT-7-F cmd+c via tmux buffer, UAT-7-J tab switch cancel"
tech-stack:
  added: []
  patterns:
    - "Tiered precedence chain: short-circuit-by-Tier with explicit `return` after each match — no `else` ladders"
    - "Off-thread blocking subprocess: `tokio::task::spawn_blocking(move || ...)` with cloned session name — keeps cmd+c sub-50ms even on cold tmux client"
    - "Atomic flag clear before forward: `tmux_in_copy_mode_set(false)` BEFORE returning from Esc Tier 2 (no observable round-trip needed because tmux's `cancel` binding from Plan 07-02 exits copy-mode synchronously on Esc receipt)"
    - "Subprocess invariant tests over fixture-free integration: helpers must not panic on failure paths; behavior under load verified by Plan 07-06 UAT"
key-files:
  created: []
  modified:
    - "src/events.rs (handle_key cmd+c branch lines 481-518; handle_key Esc branch lines 520-540) — +47 lines, -10 lines"
    - "src/tmux_native_selection_tests.rs (3 new invariant tests appended after TM-DISPATCH-04) — +55 lines"
decisions:
  - "Tier 2 cmd+c spawns blocking subprocess off the event thread per PLAN.md `<behavior>` — 'sub-millisecond' return preserves cmd+c responsiveness even on first invocation when tmux client warms up"
  - "Esc Tier 2 forwards a LONE 0x1b byte (not 0x1b followed by introducer) — tmux's copy-mode-vi `bind-key Escape send-keys -X cancel` (Plan 07-02 override) interprets exactly that as cancel"
  - "Esc Tier 2 clears `tmux_in_copy_mode` flag synchronously after byte forward — no wait for round-trip; the flag is Martins-side state per RESEARCH §State Source Option (a)"
  - "`KeyModifiers::NONE` check stays at top of Esc branch — Esc-with-modifiers (e.g. shift+esc) intentionally NOT intercepted, falls through to existing path"
  - "Test strategy follows PLAN.md `<behavior>` Task 2: subprocess-helper invariants instead of full App fixture (out of validation budget). End-to-end behavior is the responsibility of Plan 07-06 manual UAT, not unit tests"
metrics:
  duration_seconds: 160
  duration_human: "~3m"
  completed: 2026-04-25
  tasks: 2
  files_modified: 2
  tests_added: 3
  test_total_before: 142
  test_total_after: 145
---

# Phase 7 Plan 05: handle_key cmd+c + Esc 3-tier precedence — Summary

**One-liner:** Inserted Tier-2 (`tokio::task::spawn_blocking` → `tmux save-buffer | pbcopy`) into `handle_key`'s cmd+c branch and Tier-2 (`write_active_tab_input(&[0x1b])` + `tmux_in_copy_mode_set(false)`) into the Esc branch — yielding 3-tier precedence chains for both keys; added 3 subprocess-helper invariant tests (TM-CMDC-02, TM-CANCEL-01, TM-ESC-02).

## Performance

- **Duration:** ~3 min (160 s)
- **Started:** 2026-04-25T14:40:59Z
- **Completed:** 2026-04-25T14:43:39Z
- **Tasks:** 2
- **Files modified:** 2 (`src/events.rs`, `src/tmux_native_selection_tests.rs`)

## Tasks Completed

| Task | Name                                                                                        | Commit    | Files                                |
| ---- | ------------------------------------------------------------------------------------------- | --------- | ------------------------------------ |
| 1    | Insert Tier-2 into cmd+c precedence + Tier-2 into Esc precedence                            | `397c0aa` | src/events.rs                        |
| 2    | Add TM-CMDC-02 + TM-CANCEL-01 + TM-ESC-02 subprocess-invariant tests                        | `f704035` | src/tmux_native_selection_tests.rs   |

## Implementation Detail — handle_key Precedence Chains

**Location:** `src/events.rs:481-540` — replaces the existing Phase 6 cmd+c branch (lines 481-497 pre-edit) and Phase 6 Esc branch (lines 499-507 pre-edit).

### cmd+c (lines 481-518)

```rust
if key.code == KeyCode::Char('c')
    && key.modifiers.contains(KeyModifiers::SUPER)
{
    // Tier 1 (Phase 6, unchanged): overlay selection.
    if let Some(sel) = &app.selection {
        if !sel.is_empty() {
            app.copy_selection_to_clipboard();
            return;
        }
    }
    // Tier 2 (Phase 7 D-10): tmux paste-buffer when delegating.
    if app.active_session_delegates_to_tmux() {
        if let Some(session_name) = app.active_tmux_session_name() {
            tokio::task::spawn_blocking(move || {
                crate::tmux::save_buffer_to_pbcopy(&session_name);
            });
            return;
        }
    }
    // Tier 3 (Phase 6, unchanged): SIGINT in Terminal mode.
    if app.mode == InputMode::Terminal {
        app.write_active_tab_input(&[0x03]);
        return;
    }
    // Normal mode + no selection + not delegating — fall through to keymap.
}
```

### Esc (lines 520-540)

```rust
if key.code == KeyCode::Esc && key.modifiers == KeyModifiers::NONE {
    // Tier 1: overlay selection clear.
    if app.selection.is_some() {
        app.selection = None;
        app.mark_dirty();
        return;
    }
    // Tier 2 (Phase 7): forward to delegating tmux in copy-mode.
    if app.active_session_delegates_to_tmux() && app.tmux_in_copy_mode() {
        app.write_active_tab_input(&[0x1b]);
        app.tmux_in_copy_mode_set(false);
        return;
    }
    // Tier 3: fall through to existing path.
}
```

## Test Count Delta

| Suite                         | Before | After | Delta                                                   |
| ----------------------------- | -----: | ----: | ------------------------------------------------------- |
| `tmux_native_selection_tests` |     12 |    15 | +3 (TM-CMDC-02, TM-CANCEL-01, TM-ESC-02)                |
| `selection_tests`             |     35 |    35 | 0 (Phase 6 cmd+c + Esc Tier 1 unchanged — proves intent)|
| All other suites              |     95 |    95 | 0                                                       |
| **TOTAL**                     |    142 |   145 | +3                                                      |

`cargo test --bin martins -- --test-threads=2` reports `145 passed; 0 failed`. Matches plan projection exactly.

## Phase 6 Tests — Re-verified No Tier-1 Regression

Plan 07-05 acceptance criteria includes "Phase 6 selection_tests + cmd+c-with-selection + Esc-clears-selection still pass — proves Tier 1 cmd+c and Tier 1 Esc unchanged." `cargo test --bin martins selection_tests -- --test-threads=2` → 35 passed (unchanged from 142-baseline expectation; the same 35 from Plan 07-04's post-fixture-update count, which includes TM-DISPATCH-* tests that grep into `selection_tests` filter).

The cmd+c Tier 1 (overlay) and Esc Tier 1 (overlay clear) bodies are byte-for-byte unchanged from Phase 6 — `git diff 397c0aa~1 -- src/events.rs` shows the existing Tier 1 logic preserved verbatim inside the new outer block.

## Acceptance Criteria — All Met

**Task 1:**
- ✓ `grep -q 'crate::tmux::save_buffer_to_pbcopy' src/events.rs` exits 0
- ✓ `grep -q 'tokio::task::spawn_blocking' src/events.rs` exits 0
- ✓ `grep -q 'write_active_tab_input(&\[0x1b\])' src/events.rs` exits 0 (proves Esc forward byte)
- ✓ `grep -q 'tmux_in_copy_mode_set(false)' src/events.rs` exits 0
- ✓ `cargo build` exits 0
- ✓ `cargo test --bin martins -- --test-threads=2` reports 142 → 142 (baseline preserved at Task 1 commit; +3 added in Task 2 → 145)

**Task 2:**
- ✓ `cargo test --bin martins tmux_native_selection_tests -- --test-threads=2` reports 15 tests pass (12 prior + 3 new)
- ✓ `cargo test --bin martins selection_tests -- --test-threads=2` reports 35 passed (Phase 6 Tier 1 unchanged)
- ✓ `cargo test --bin martins -- --test-threads=2` total 145 green
- ✓ `grep -q 'fn save_buffer_to_pbcopy_returns_false' src/tmux_native_selection_tests.rs` exits 0
- ✓ `grep -q 'fn cancel_copy_mode_is_fire_and_forget' src/tmux_native_selection_tests.rs` exits 0
- ✓ `cargo build --release` exits 0

## Truths Affirmed (must_haves)

- ✓ **cmd+c precedence is Tier 1 (overlay sel) → Tier 2 (tmux save-buffer | pbcopy when delegating) → Tier 3 (SIGINT 0x03 in Terminal mode)** — verified at `src/events.rs:481-518` (each Tier ends with explicit `return`; ordering enforced by source-line precedence).
- ✓ **Tier 2 invocation runs `crate::tmux::save_buffer_to_pbcopy(&name)` inside `tokio::task::spawn_blocking` (off-event-thread)** — `src/events.rs:505-507`. The handler returns immediately after spawning.
- ✓ **Esc with overlay selection → Phase 6 path: clear selection + mark_dirty + return** — `src/events.rs:525-530`. Identical body to pre-edit Phase 6 branch.
- ✓ **Esc with NO overlay selection AND delegating session AND `tmux_in_copy_mode==true` → forward `\x1b` byte via `write_active_tab_input` + clear `tmux_in_copy_mode` flag locally** — `src/events.rs:531-540`.
- ✓ **Esc with NO overlay selection AND NOT delegating → falls through to existing PTY-forward path (unchanged)** — no `return` at end of the `if KeyCode::Esc` block; control flows to subsequent Terminal-mode forward logic at `src/events.rs:545+` (untouched).

## Deviations from Plan

None — plan executed exactly as written.

The plan's Task 1 `<action>` block-replacement bodies were copy-pasted verbatim. Task 2's three test functions were copy-pasted verbatim. Both `<verify><automated>` blocks called `cargo build --lib` / `cargo test --lib`; per the standing convention documented in Plans 07-01..07-04 SUMMARYs, Martins is a binary-only crate so `cargo build` / `cargo test --bin martins` were substituted (same compilation surface, same test set).

## Issues Encountered

- **Pre-existing parallel-test flakiness** under default 8-thread parallel test load (`selection_tests::scroll_generation_increments_on_vertical_scroll`) reproduces independently of this plan's changes. Mitigated by `--test-threads=2`. Out-of-scope per executor SCOPE BOUNDARY; same flake documented in Plan 07-01 SUMMARY.
- **Binary-only crate verification adjustment:** plan's `<verify><automated>` blocks called `cargo build --lib` / `cargo test --lib`. Substituted `cargo build` / `cargo test --bin martins`. Same convention as Plans 07-01..07-04.
- One pre-existing dead-code warning was resolved by Task 1: prior to Plan 07-05, `App::active_tmux_session_name`, `App::tmux_in_copy_mode`, and `App::tmux_in_copy_mode_set` (added in Plan 07-03) were all unused. Task 1 wires the three remaining unused helpers — Plan 07-03's `cargo build` warning surface shrinks accordingly.

## User Setup Required

None — all changes are pure code (event-handler logic + tests). Existing `~/.martins/state.json` and `~/.martins/tmux.conf` continue to work unchanged. The new dispatch is observable when the user presses cmd+c or Esc with a delegating tmux session active in the focused tab; Plan 07-06 manual UAT scripts validate the end-to-end behavior.

## Next Phase Readiness

- **Plan 07-06** (manual UAT): both precedence chains are now wired end-to-end. UAT-7-D (Esc cancel exits tmux copy-mode), UAT-7-F (cmd+c on tmux selection populates clipboard via `tmux save-buffer | pbcopy`), and UAT-7-J (tab switch cancels outgoing copy-mode via Plan 07-03's `set_active_tab` D-16 hook) all have their data path exercised by Plan 07-05's edits.
- **No new blockers.** Phase 7 implementation is complete; Plan 07-06 is purely UAT/documentation work.

## Threat Surface Scan

Per PLAN.md `<threat_model>`:

| Threat ID | Mitigation In Place                                                                                                                                                                                                                                                       |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| T-07-14 (DoS — `spawn_blocking` thrash) | Accepted. `tokio` default blocking-pool is 512 threads; one cmd+c per user-press is several orders of magnitude under saturation.                                                                                                            |
| T-07-15 (Information disclosure — tmux save-buffer pipes to pbcopy) | Accepted. Same content-trust boundary as Phase 6's pbcopy direct path; tmux paste-buffer was selected by the user.                                                                                                            |
| T-07-16 (Race — Esc forward + `tmux_in_copy_mode_set(false)` ordering) | Accepted. Worst case: tmux already exited copy-mode (auto-cancel from prior MouseDragEnd); next Esc has no effect; no UX harm.                                                                                                |
| T-07-17 (Stale state — `tmux_in_copy_mode` persists when tmux exited autonomously) | Mitigated upstream: Plan 07-04 Up(Left) state machine clears flag when no drag occurred; Plan 07-03 `set_active_tab` runs `cancel_copy_mode` on outgoing session before active_tab mutation. UAT-7-D verifies single Esc exits copy-mode in practice. |

No new threat surface introduced beyond what the plan declared. No threat flags raised.

## Self-Check: PASSED

- ✓ FOUND: `src/events.rs:481-518` contains cmd+c 3-tier precedence (`grep -n 'cmd+c precedence' src/events.rs` → line 481)
- ✓ FOUND: `src/events.rs:520-540` contains Esc 3-tier precedence (`grep -n 'Esc precedence' src/events.rs` → line 520)
- ✓ FOUND: `src/tmux_native_selection_tests.rs` contains all 3 new test fns (`save_buffer_to_pbcopy_returns_false_on_nonexistent_session`, `cancel_copy_mode_is_fire_and_forget_on_nonexistent_session`, `esc_byte_is_lone_0x1b`)
- ✓ FOUND: commit `397c0aa` (Task 1 — handle_key precedence chains) in `git log --all`
- ✓ FOUND: commit `f704035` (Task 2 — TM-CMDC-02 + TM-CANCEL-01 + TM-ESC-02) in `git log --all`
- ✓ PASSED: `cargo build` exits 0; `cargo build --release` exits 0
- ✓ PASSED: `cargo test --bin martins tmux_native_selection_tests -- --test-threads=2` reports 15 passed
- ✓ PASSED: `cargo test --bin martins -- --test-threads=2` reports 145 passed / 0 failed (matches plan-projected baseline)

---
*Phase: 07-tmux-native-main-screen-selection*
*Completed: 2026-04-25*
