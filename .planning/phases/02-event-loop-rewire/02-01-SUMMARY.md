---
phase: 02
plan: 01
subsystem: event-loop
tags: [event-loop, rendering, dirty-flag, tokio-select, arch-02]
dependency-graph:
  requires:
    - src/app.rs::App (existing struct + impl from Phase 1)
    - src/app_tests.rs (Phase 1 #[path] module scaffold)
    - tokio::select! + tokio::time::interval
    - ratatui DefaultTerminal::draw
  provides:
    - App.dirty: pub(crate) bool (state-mutation signal)
    - App::mark_dirty(&mut self) helper (#[inline], pub(crate))
    - dirty-gated terminal.draw in App::run
    - biased; tokio::select! with input-first branch ordering
    - heartbeat_tick (5s) replacing status_tick (1s)
  affects:
    - idle CPU (expected drop to near-zero — manual UAT)
    - working-dot animation latency (2-3s -> 2-7s; documented trade-off)
    - PTY cursor blink during idle (no longer blinks — pending UAT)
tech-stack:
  added: []
  patterns:
    - dirty-flag rendering (ratatui immediate-mode guidance)
    - biased tokio::select! for input priority
    - pub(crate) visibility for test-visible state
key-files:
  created: []
  modified:
    - src/app.rs (+29 / -6)
    - src/app_tests.rs (+37 / -0)
decisions:
  - Q1 heartbeat_tick at 5s (kept as explicit select branch; raises working-dot latency ceiling to 5s)
  - Q2 cursor blink during idle accepted — UAT confirms (mitigation: 500ms blink tick if regressed)
  - Q3 unconditional self.mark_dirty() in each select arm (no per-handler "did it mutate" return values)
  - Q4 sync_pty_size() stays outside the 'if self.dirty' block (resize must work even without other state change)
metrics:
  duration: "~2m"
  completed: "2026-04-24"
requirements: [ARCH-02]
---

# Phase 02 Plan 01: Dirty-Flag Rendering Summary

Installed the `dirty: bool` primitive on `App` and rewired `App::run` so `terminal.draw()` only fires when state actually changed — making idle CPU near-zero and every redraw trigger grep-able via `mark_dirty`. Also established `biased;` + input-first branch ordering in `tokio::select!` (ARCH-03 structural predicate) and replaced the 1s `status_tick` with a 5s `heartbeat_tick` to keep the sidebar working-dot animation advancing without a high-frequency wakeup.

## What Was Built

**`src/app.rs` — 4 surgical edits:**

1. **A. Struct field.** `pub(crate) dirty: bool` inserted immediately after `should_quit` in the `App` struct (line ~70). Same visibility cluster as `should_quit`; `pub(crate)` so `app_tests.rs` can read/write directly.

2. **B. Init.** `dirty: true,` in `App::new`'s struct literal immediately after `should_quit: false,` — guarantees the very first frame renders.

3. **C. Helper.** `#[inline] pub(crate) fn mark_dirty(&mut self) { self.dirty = true; }` inserted between `active_workspace()` and `run()`. The `#[inline]` is load-bearing only for auditability — the body is trivial enough that rustc inlines regardless.

4. **D. `run` loop rewire** (lines 161–231 post-edit):
   - Dropped `let mut status_tick = interval(Duration::from_secs(1));`.
   - Added `let mut heartbeat_tick = interval(Duration::from_secs(5));` with an explanatory comment pointing at RESEARCH §2 pitfall #5.
   - Wrapped `terminal.draw(...)?;` in `if self.dirty { ...; self.dirty = false; }`.
   - `pending_workspace` fast-path now calls `self.mark_dirty();` before `continue;`.
   - `tokio::select!` gains `biased;` as its first line.
   - Branches reordered to: events → pty_notify → watcher → heartbeat → refresh (events-first satisfies ARCH-03 structurally; Plan 02-02 does the grep-verification and any final polish).
   - **Every** arm body calls `self.mark_dirty()` — 5 arms × 1 call + pending-workspace branch = **6 call sites** total (plan required ≥5).

**`src/app_tests.rs` — 3 new `#[tokio::test]` unit tests appended:**

- `app_starts_dirty` — asserts `App::new(...).dirty == true`.
- `dirty_stays_clear_when_no_mutation` — clears `dirty`, mutates nothing, asserts stays `false`.
- `mark_dirty_sets_flag` — clears `dirty`, calls `app.mark_dirty()`, asserts `true`.

## Test Results

| Gate | Command | Result |
|------|---------|--------|
| RED (Task 1) | `cargo build --tests 2>&1` | Fails with `no field 'dirty'` + `no method 'mark_dirty'` — ✓ expected |
| GREEN unit (Task 2) | `cargo test --bin martins app_starts_dirty` | 1 passed |
| GREEN unit (Task 2) | `cargo test --bin martins dirty_stays_clear_when_no_mutation` | 1 passed |
| GREEN unit (Task 2) | `cargo test --bin martins mark_dirty_sets_flag` | 1 passed |
| Full suite | `cargo test` | **100 passed, 0 failed** (97 pre-phase + 3 new) |
| Lint | `cargo clippy --all-targets -- -D warnings` | Clean |

## Grep-Based Acceptance Criteria

All satisfied on `src/app.rs`:

| Query | Expected | Actual |
|-------|----------|--------|
| `rg 'pub\(crate\) dirty: bool'` | 1 | 1 |
| `rg 'pub\(crate\) fn mark_dirty'` | 1 | 1 |
| `rg 'if self\.dirty \{'` | 1 | 1 |
| `rg 'self\.dirty = false;'` | 1 | 1 |
| `rg 'self\.mark_dirty\(\)' \| wc -l` | ≥5 | **6** |
| `rg 'biased;'` | 1 | 1 |
| `rg 'heartbeat_tick'` | ≥2 | 2 |
| `rg 'status_tick'` | 0 | **0** |
| `rg 'interval\(Duration::from_secs\(1\)\)'` | 0 | **0** |
| `rg '#\[inline\]'` | ≥1 | 1 (on mark_dirty) |

## Decision Confirmations

Each of the four plan decisions was applied as planned — no deviations required:

- **Q1 (status_tick fate):** renamed to `heartbeat_tick`, raised from 1s to 5s, kept as an explicit select branch that calls `self.mark_dirty()`. Rationale: sidebar working-dot animation still needs *some* periodic wake to flip "working→idle" when PTY output stops. 5s ceiling is documented; if UAT shows it as annoying, Phase 4 can arm a lazy 2s transition timer.
- **Q2 (cursor blink):** accepted. `tui-term::widget::PseudoTerminal` draws the cursor as part of each frame; with dirty-gated draws, idle means no frames = no cursor re-paint = no blink. If this is UAT-unacceptable, mitigation is a 500ms `blink_tick` that calls `mark_dirty` — documented but not added now.
- **Q3 (per-handler return):** skipped. Arms mark dirty unconditionally. Ratatui's double-buffer diff absorbs redundant draws (only changed cells are written to the terminal), so over-marking costs almost nothing.
- **Q4 (sync_pty_size placement):** stays outside `if self.dirty { ... }`. Terminal resize may need a size-sync *even when no other state changed*. It's already a cheap no-op when size is unchanged.

## Known Follow-Ups

Documented here, **not fixed in this plan:**

1. **Cursor blink regression (Assumption A1).** UAT required. If user finds the non-blinking idle cursor unacceptable, add a 500ms `blink_tick` branch that `mark_dirty`s. Low risk — `tui-term` doesn't rely on independent blink.
2. **Working-dot 5s latency ceiling (pitfall #5).** Transition "working" → "idle" now has up to 5s lag (was 2–3s under 1s tick). Alternative if unacceptable: Phase 4 arms a lazy transition timer when a dot goes "working" → tick once at `now + 2s` → mark dirty.
3. **`refresh_diff` 5s first-tick redundancy (Section 7 Q8).** `interval(Duration::from_secs(5))` fires immediately on first `tick()`, overlapping with the `refresh_diff` already called in `App::new`. Phase 5 may switch to `interval_at(Instant::now() + 5s, 5s)` — not a Phase 2 concern.
4. **`handle_event(Event::Resize(_, _))` empty arm (Section 2 pitfall #2).** Already handled — `mark_dirty` fires in the `events.next()` arm body *before* dispatch, so resize gets marked even though `handle_event`'s `Event::Resize` arm is `{}`.

## Deviations from Plan

**None.** Plan executed exactly as written — four edits applied verbatim, three tests added verbatim, two commits with plan-specified messages.

## Files Modified

| File | Action | Lines | Delta |
|------|--------|-------|-------|
| `src/app.rs` | modified | 161→231 (run fn) + field/init/helper | +29 / -6 |
| `src/app_tests.rs` | modified | appended after line 84 | +37 / -0 |

## Commits

| # | Task | Hash | Message |
|---|------|------|---------|
| 1 | RED — add failing tests | `86f1132` | `test(02-01): add failing unit tests for dirty-flag semantics` |
| 2 | GREEN — dirty-flag impl | `488c64e` | `feat(02-01): dirty-flag rendering — gate terminal.draw + mark_dirty helper` |

## TDD Gate Compliance

Plan-level TDD gates satisfied:
- **RED** gate: `test(02-01): ...` commit `86f1132` — compile fails with expected errors (`no field 'dirty'` + `no method 'mark_dirty'`).
- **GREEN** gate: `feat(02-01): ...` commit `488c64e` — all 3 new tests pass; 100/100 full suite green.
- **REFACTOR** gate: not applicable — GREEN shape was already target-shaped (no cleanup needed).

## Ready for Plan 02-02

Structural pre-conditions for ARCH-03 are already in place:
- `biased;` is the first line of `tokio::select!` in `run`.
- `events.next()` is the first branch after `biased;`.

Plan 02-02's remaining scope is the grep-verification pass + any final ordering polish (e.g., explicit ordering docstring, documentation comments, validation-strategy cross-links).

## Self-Check: PASSED

- File `src/app.rs` exists — **FOUND**
- File `src/app_tests.rs` exists — **FOUND**
- File `.planning/phases/02-event-loop-rewire/02-01-SUMMARY.md` exists — **FOUND (this file)**
- Commit `86f1132` exists — **FOUND**
- Commit `488c64e` exists — **FOUND**
- `cargo test` → 100/100 passed — **VERIFIED**
- `cargo clippy --all-targets -- -D warnings` → exit 0 — **VERIFIED**
- All 10 grep acceptance checks pass — **VERIFIED**
