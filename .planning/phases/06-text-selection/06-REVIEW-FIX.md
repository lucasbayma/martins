---
phase: 06-text-selection
fixed_at: 2026-04-25T12:14:43Z
review_path: .planning/phases/06-text-selection/06-REVIEW.md
iteration: 1
findings_in_scope: 4
fixed: 4
skipped: 0
deferred: 5
status: applied
---

# Phase 6: Code Review Fix Report

**Fixed at:** 2026-04-25T12:14:43Z
**Source review:** `.planning/phases/06-text-selection/06-REVIEW.md`
**Iteration:** 1

## Summary

- Findings in scope (minors): **4**
- Fixed: **4**
- Skipped: **0**
- Deferred (nits, advisory only): **5**

Scope clarification (per orchestrator prompt): REVIEW.md uses
`blocker / major / minor / nit` taxonomy. The user invoked
`/gsd-code-review-fix 06` with no flags. Since 0 blockers and 0 majors
exist, the 4 minors form the actionable scope. The 5 nits are
doc-comment additions and an `#[allow(dead_code)]` removal — deferred
as advisory.

## Fixed Issues

### MINOR-01: Empty snapshot stored as `Some("")` defeats fallback re-materialization

- **Severity:** minor
- **Files modified:** `src/events.rs` (lines 80-93)
- **Commit:** `e21c889`
- **Applied fix:** In `MouseEventKind::Up(MouseButton::Left)`, gated
  the snapshot store on `!text.is_empty()`. When
  `materialize_selection_text` returns an empty string (parser
  `try_read` contention with the PTY reader thread, OR genuinely
  empty visible content), `sel.text` now stays as `None` instead of
  becoming `Some("")`. This restores the live re-materialization
  fallback in `copy_selection_to_clipboard` — the `unwrap_or_else`
  path now engages on subsequent `cmd+c` rather than being
  short-circuited by a `Some("")` value that yielded an empty
  `pbcopy`.
- **Original issue:** `src/events.rs:80-81` + `src/app.rs:485-488` —
  Empty snapshot stored as `Some("")` defeats live-rematerialization
  fallback in `copy_selection_to_clipboard`.

### MINOR-02: Triple-click on blank/whitespace-only row produces invisible empty selection

- **Severity:** minor
- **Files modified:** `src/app.rs` (`select_line_at`, lines 709-731)
- **Commit:** `68577e5`
- **Applied fix:** In `select_line_at`, after constructing
  `new_sel`, added an `if new_sel.is_empty() { return; }` guard
  before the `materialize_selection_text` call and the
  `app.selection = Some(...)` write. Triple-click on a blank or
  whitespace-only row now leaves any prior selection untouched and
  skips `mark_dirty` — matches Ghostty's no-op semantics and
  eliminates the UX papercut where the user had to issue a
  dismiss-click for an invisible-but-present selection.
- **Original issue:** `src/app.rs:677-725` — `select_line_at` on a
  blank/whitespace-only row produces a non-empty
  `Option<SelectionState>` that `is_empty()`.

### MINOR-03: `last_click_col` written but never read in same-cluster comparison

- **Severity:** minor
- **Files modified:** `src/app.rs` (field decl line 102, init line
  173 — both removed), `src/events.rs` (write sites at 155 and 175 —
  both removed)
- **Commit:** `3a67b1d`
- **Applied fix:** Per scope-clarification recommendation (a),
  deleted the `last_click_col: u16` field, its initializer in
  `App::new`, and both write sites in `events.rs::handle_mouse`.
  Same-cluster comparison continues to use only `last_click_row`
  (matching Ghostty's row-only tolerance per RESEARCH §Q4 / D-16).
  No tests reference the field; verified compile-clean via
  `cargo build --bin martins --tests`.
- **Original issue:** `src/app.rs:102, 173`; `src/events.rs:144,
  164` — `last_click_col` is dead bookkeeping.

### MINOR-04: 500ms polling deadline flakes scroll-generation test on cold runs

- **Severity:** minor
- **Files modified:** `src/selection_tests.rs`
  (`scroll_generation_increments_on_vertical_scroll`, lines 280-285)
- **Commit:** `37a0057`
- **Applied fix:** Bumped the polling deadline from
  `Duration::from_millis(500)` to `Duration::from_millis(2000)`,
  matching the budget used by `write_and_wait_for_text` (file:48).
  Polling cadence (20ms sleep + early-exit on first non-zero load)
  unchanged — the test still passes warm runs in <100ms but no
  longer flakes on cold/parallel runs. Production code is correct;
  this fix is purely test-environmental.
- **Original issue:** `src/selection_tests.rs:263-294` — 500ms
  polling deadline against real `/bin/cat` PTY flakes on cold runs.

## Deferred (Nit-Tier, Advisory Only)

The following 5 nits from REVIEW.md are doc-comment additions and an
`#[allow(dead_code)]` removal. They are nice-to-haves, not actionable
bugs, and are deferred per the orchestrator's scope clarification.

- **NIT-01** — `src/app.rs:388-406`: Add explanatory comment to
  `select_active_workspace` about the asymmetric `mark_dirty`
  semantics relative to `set_active_tab`.
- **NIT-02** — `src/app.rs:512-526`: Add `tracing::trace!` log on
  the `parser.try_read()` failure path of
  `materialize_selection_text`, OR change return type to
  `Option<String>` to disambiguate the three failure modes.
- **NIT-03** — `src/app.rs:867-941`: Remove `#[allow(dead_code)]`
  from `inject_test_session` — the function now has live test-build
  callers, so the allow is redundant.
- **NIT-04** — `src/app.rs:43-49`: Document the size invariant
  on `SelectionState.text` snapshot (≤ visible-area bytes; do not
  widen to scrollback).
- **NIT-05** — `src/pty/session.rs:231-240`: Add doc comment on
  `row_hash` clarifying that `DefaultHasher` (SipHash with
  randomized seed) is process-local and must not be persisted or
  compared across processes.

## Verification

Test result after all 4 fixes:

```
cargo test --bin martins
test result: ok. 129 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.72s
```

Build verification:

```
cargo build --bin martins         # clean, no warnings
cargo build --bin martins --tests # clean, no warnings
```

All 129 tests passed on the first run after fixes (no flake observed
post-MINOR-04 deadline bump).

## Commit Log

| ID         | Commit    | Files |
| ---------- | --------- | ----- |
| MINOR-01   | `e21c889` | `src/events.rs` |
| MINOR-02   | `68577e5` | `src/app.rs` |
| MINOR-03   | `3a67b1d` | `src/app.rs`, `src/events.rs` |
| MINOR-04   | `37a0057` | `src/selection_tests.rs` |

---

_Fixed: 2026-04-25T12:14:43Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
