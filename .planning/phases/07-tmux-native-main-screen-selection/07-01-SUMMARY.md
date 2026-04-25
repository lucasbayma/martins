---
phase: 07-tmux-native-main-screen-selection
plan: 01
subsystem: events / tests
tags: [phase-7, tmux, sgr, mouse-encoding, wave-0-tests, encoder]
requires:
  - crossterm::event::{MouseEventKind, MouseButton, KeyModifiers}
provides:
  - "crate::events::encode_sgr_mouse(MouseEventKind, KeyModifiers, u16, u16) -> Option<Vec<u8>>"
  - "src/tmux_native_selection_tests.rs (8 tests; pre-staged module for TM-DISPATCH/CMDC/ESC/CANCEL)"
affects:
  - "src/events.rs (encoder fn added at top, before handle_event)"
  - "src/main.rs (mod tmux_native_selection_tests registration)"
tech_stack:
  added: []
  patterns:
    - "pure-fn SGR encoder mirrors inline encodes at events.rs:230 (scroll wheel) and events.rs:291 (sidebar click forward)"
    - "test module mirrors src/selection_tests.rs / src/pty_input_tests.rs shape (#![cfg(test)] header + crate:: imports)"
key_files:
  created:
    - src/tmux_native_selection_tests.rs
  modified:
    - src/events.rs
    - src/main.rs
decisions:
  - "Encoder lives inline in src/events.rs (not extracted to src/sgr.rs) per 07-PATTERNS.md classification — ~35 LOC including doc-comment doesn't warrant a new module"
  - "Encoder returns Option<Vec<u8>> with None for non-forwarded events (Moved, Down(Right), ScrollLeft/Right) — keeps Phase 7 scope filter at the encoder boundary so callers don't repeat the match"
metrics:
  duration_seconds: 160
  completed: 2026-04-25
  tests_added: 8
  test_total_before: 130
  test_total_after: 138
---

# Phase 7 Plan 1: Wave-0 SGR Encoder + Test Scaffolding — Summary

**One-liner:** Added pure `encode_sgr_mouse(MouseEventKind, KeyModifiers, u16, u16) -> Option<Vec<u8>>` free fn in `src/events.rs` and a new `src/tmux_native_selection_tests.rs` module with 8 unit tests (TM-ENC-01..06 + 2 negatives) gated and registered via `src/main.rs`.

## Outcome

- **Encoder:** `pub(crate) fn encode_sgr_mouse` at `src/events.rs:32-67` — produces exact SGR (1006) byte sequences per 07-RESEARCH.md §SGR Mouse Encoding (`\x1b[<{cb};{col};{row}{M|m}` with button bits, motion bit 32 for Drag, modifier bits SHIFT+4/ALT+8/CONTROL+16, 1-based xterm coords).
- **Test module:** `src/tmux_native_selection_tests.rs` (114 LOC) — registered in `src/main.rs:29-30` as a `#[cfg(test)]` module. Pre-stages the namespace for downstream Phase 7 plans (TM-DISPATCH-01..04, TM-CMDC-01..03, TM-ESC-01..03, TM-CANCEL-01).
- **Tests:** 8 added, all passing on `cargo test --bin martins tmux_native_selection_tests` (TM-ENC-01..06 + `encode_sgr_moved_returns_none` + `encode_sgr_down_right_returns_none`).
- **Suite delta:** 130 → 138 tests passing, zero regressions.

## Tasks Executed

| # | Task | Status | Commit |
|---|------|--------|--------|
| 1 | Add `encode_sgr_mouse` pure fn to `src/events.rs` | done | `e707bfe` |
| 2 | Create `src/tmux_native_selection_tests.rs` + register in `src/main.rs` | done | `c6f13d7` |

## Verification Results

```
$ cargo build
   Compiling martins v0.7.0
warning: function `encode_sgr_mouse` is never used  (Task 1 only — resolved by Task 2 tests)
    Finished `dev` profile

$ cargo test --bin martins tmux_native_selection_tests
running 8 tests
test tmux_native_selection_tests::encode_sgr_down_left_no_mods       ... ok
test tmux_native_selection_tests::encode_sgr_drag_left_no_mods       ... ok
test tmux_native_selection_tests::encode_sgr_up_left_release         ... ok
test tmux_native_selection_tests::encode_sgr_down_left_shift         ... ok
test tmux_native_selection_tests::encode_sgr_down_left_alt           ... ok
test tmux_native_selection_tests::encode_sgr_drag_left_shift_alt     ... ok
test tmux_native_selection_tests::encode_sgr_moved_returns_none      ... ok
test tmux_native_selection_tests::encode_sgr_down_right_returns_none ... ok
test result: ok. 8 passed; 0 failed

$ cargo test --bin martins
test result: ok. 138 passed; 0 failed; 0 ignored

$ cargo build --release
warning: function `encode_sgr_mouse` is never used  (consumer arrives in Plan 07-04)
    Finished `release` profile
```

## Plan Compliance

| Success Criterion | Status |
|-------------------|--------|
| `pub(crate) fn encode_sgr_mouse` exists in `src/events.rs` with body from RESEARCH §SGR Mouse Encoding | OK |
| `src/tmux_native_selection_tests.rs` exists with 8 tests | OK |
| `src/main.rs` registers new test module | OK |
| All 8 new tests pass; existing suite still green; cargo build clean | OK (138 vs. expected 137 — see deviations) |

## Deviations from Plan

### [Rule 1 — Doc Adjustment] Test count baseline was 130, not 129

- **Found during:** Task 2 verification.
- **Issue:** PLAN.md §`<acceptance_criteria>` for Task 2 stated "Phase 6 closed at 129 tests green per STATE.md — new total should be 137".
- **Reality:** Baseline at this worktree's base commit (`b2791665`) is 130 tests passing — Plan 07-02 already shipped its RED-gate test (`tmux::tests::ensure_config_writes_phase7_bindings`, commit 5666ad8) before this plan's wave. Adding 8 new tests yields 138 total, not 137.
- **Action:** No code change required — the *intent* of the criterion (zero regressions, exactly +8) is met. Documented here so the verifier doesn't flag the "137 vs 138" discrepancy.

### [Rule 1 — Verification Adjustment] Used `--bin martins` instead of `--lib`

- **Found during:** Task 1 verification.
- **Issue:** PLAN.md `<verify><automated>` blocks called `cargo build --lib` and `cargo test --lib`.
- **Reality:** Martins is a single-binary crate (no `[lib]` target in `Cargo.toml`); `cargo build --lib` errors with `no library targets found in package martins`.
- **Fix:** Substituted `cargo build` (binary build) and `cargo test --bin martins`. Both commands cover the same compilation surface and test set — verification semantics preserved.
- **Files modified:** None (verification command only).

### Auto-fixed Issues

None — encoder body and test bodies were used verbatim from the plan's `<action>` blocks.

## Pre-existing Test Flakiness Observed (Out of Scope)

On the first full-suite run, `tmux::tests::ensure_config_writes_phase7_bindings` (Plan 07-02's RED test) reported FAILED with the same test passing in isolation and on rerun. Root cause: parallel-test filesystem race on the shared `~/.martins/tmux.conf` written by `ensure_config()`. This pre-dates Plan 07-01 (introduced by commit `5666ad8` of Plan 07-02) and is **not** caused by my changes — the encoder + tests added here are pure (no FS I/O). Logged for the verifier; resolution belongs to Plan 07-02.

## Threat Model Verification

Per PLAN.md `<threat_model>`:

- **T-07-01 (Tampering):** Output is `format!` of u8 `cb`, u16 `col`, u16 `row`, `char` `trailing` — all typed values, no string concatenation of external input. Confirmed.
- **T-07-02 (Information disclosure):** Encoder is a pure function; no I/O, no logging, no global state read. Confirmed.
- **T-07-03 (DoS):** Single ~16-byte `Vec<u8>` allocation per call. Confirmed.

No new threat surface introduced. No `Threat Flags` section needed.

## Downstream Hooks

- **Plan 07-04** (`handle_mouse` conditional intercept): can `use crate::events::encode_sgr_mouse;` directly. The pub(crate) visibility supports same-crate consumers without re-exports.
- **Plan 07-04..07-05**: extend `src/tmux_native_selection_tests.rs` with TM-DISPATCH-01..04, TM-CMDC-01..03, TM-ESC-01..03 sections. Module registration in `src/main.rs:29-30` is one-and-done — no further `mod` lines needed.
- **OQ-4 (RESEARCH §Open Questions):** Inline encodes at `src/events.rs:230` (scroll wheel) and `src/events.rs:291` (sidebar click) NOT migrated this plan; deferred to 07-04 if planner judges cleaner there.

## Key Files

```
src/events.rs                            modified  (+35 LOC: encoder fn at line 32)
src/main.rs                              modified  (+3 LOC: cfg(test) mod registration)
src/tmux_native_selection_tests.rs       created   (114 LOC: 8 tests)
.planning/phases/07-tmux-native-main-screen-selection/07-01-SUMMARY.md  created
```

## Self-Check: PASSED

- `src/events.rs` exists, contains `pub(crate) fn encode_sgr_mouse` at line 32: FOUND
- `src/tmux_native_selection_tests.rs` exists with 8 `#[test]` fns: FOUND
- `src/main.rs` registers `mod tmux_native_selection_tests`: FOUND
- Commit `e707bfe` (Task 1 — feat encoder): FOUND in `git log`
- Commit `c6f13d7` (Task 2 — test module + registration): FOUND in `git log`
- `cargo build --release` clean: PASSED
- `cargo test --bin martins` 138/138 passing: PASSED
