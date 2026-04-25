---
phase: 07-tmux-native-main-screen-selection
reviewed: 2026-04-25T00:00:00Z
depth: standard
files_reviewed: 7
files_reviewed_list:
  - src/app.rs
  - src/events.rs
  - src/main.rs
  - src/pty/session.rs
  - src/selection_tests.rs
  - src/tmux.rs
  - src/tmux_native_selection_tests.rs
findings:
  critical: 0
  warning: 2
  info: 5
  total: 7
status: issues_found
---

# Phase 7: Code Review Report

**Reviewed:** 2026-04-25
**Depth:** standard
**Files Reviewed:** 7
**Status:** issues_found
**Diff base:** 4921dd4

## Summary

Phase 7 introduces a clean conditional intercept of mouse events that forwards left-button SGR (1006) sequences to the wrapped tmux PTY when the inner program does not own mouse mode and is not on alternate screen, deferring visual feedback to tmux's native copy-mode-vi. The implementation is small, well-commented, and largely correct: the SGR encoder is a pure function with thorough unit coverage (TM-ENC-01..06), the cmd+c/Esc precedence chains are tiered with explicit comments per tier, and subprocess helpers (`save_buffer_to_pbcopy`, `cancel_copy_mode`) gracefully handle missing-session / no-buffer error paths.

Two correctness concerns surfaced. The most significant is a state-inconsistency bug in `App::set_active_tab`: when the outgoing tab is in copy-mode and we issue `tmux send-keys -X cancel`, we never reset the outgoing session's `tmux_in_copy_mode` AtomicBool flag. After switching back to that tab, the flag is still `true` while tmux is no longer in copy-mode, so the next Esc forwards `\x1b` to a non-copy-mode tmux that passes it straight through to the inner program (WR-01). The second is a smaller drag-state-machine race when delegation flips off mid-gesture (WR-02). Five informational items follow.

## Warnings

### WR-01: Stale `tmux_in_copy_mode` flag on outgoing tab after `set_active_tab`

**File:** `src/app.rs:400-411`
**Issue:** `set_active_tab` calls `crate::tmux::cancel_copy_mode(&name)` on the OUTGOING tab's tmux session, but never clears that outgoing session's `tmux_in_copy_mode` AtomicBool. After switching to another tab and back, the flag remains `true` while tmux has actually exited copy-mode (per the cancel). The next Esc keystroke on the returned-to tab will:
1. `tmux_in_copy_mode()` returns `true` (stale).
2. Esc-Tier-2 forwards `\x1b` to the PTY.
3. tmux is NOT in copy-mode, so it passes `\x1b` through to the inner program.

This breaks Esc semantics on the second visit to a previously-copy-mode tab. Note the symmetric local-clear in events.rs:539 (`tmux_in_copy_mode_set(false)` after Esc-Tier-2 forward) — `set_active_tab` should mirror that contract.

**Fix:** Clear the outgoing session's flag at the same call site:

```rust
pub(crate) fn set_active_tab(&mut self, index: usize) {
    if let Some(name) = self.active_tmux_session_name() {
        crate::tmux::cancel_copy_mode(&name);
    }
    // Clear the OUTGOING session's tmux_in_copy_mode flag — `cancel_copy_mode`
    // exited tmux's copy-mode but the AtomicBool is per-session and tmux_in_copy_mode_set
    // only touches the *current* active_tab, so do it BEFORE we mutate active_tab.
    let sessions = self.active_sessions();
    if let Some((_, session)) = sessions.get(self.active_tab) {
        session.tmux_in_copy_mode.store(false, std::sync::atomic::Ordering::Relaxed);
        session.tmux_drag_seen.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    drop(sessions);
    self.clear_selection();
    self.active_tab = index;
    self.mark_dirty();
}
```

Alternatively introduce a small `tmux_in_copy_mode_set_at(idx, value)` helper or have `set_active_tab` operate on a captured outgoing session reference before mutation.

### WR-02: Orphaned tmux state when delegation flips off mid-gesture

**File:** `src/events.rs:87-136`
**Issue:** The Phase 7 conditional-intercept block evaluates `app.active_session_delegates_to_tmux()` per event. Between a `Down(Left)` and `Up(Left)`, the inner program can begin emitting DECSET 1000h / 1049h (e.g. user holds the button while a TUI launches). The Down was forwarded into tmux, setting `tmux_in_copy_mode = true`, but the Up runs through the overlay path because delegation now returns false. tmux never receives the matching Up — its button-state machine is left thinking the button is held, and `tmux_in_copy_mode` stays stuck `true` until the next legitimate forwarded Up. A subsequent Esc on this tab triggers Esc-Tier-2 spuriously.

The mirror-image scenario (Down via overlay, Up while delegating) similarly leaks: `app.selection.take()` returns `None` so the overlay branch is a no-op, but tmux receives only the synthesized Up — also a single-event protocol violation.

This is a genuine edge case (it requires DECSET inside a single mouse gesture), but the symptom (stuck-button + stuck-flag persisting until next Up) is sticky enough to be worth defending against.

**Fix:** Latch the delegation decision at `Down(Left)` time per gesture. Cheapest implementation: store an `AtomicBool` `gesture_was_delegating: Arc<AtomicBool>` on `PtySession` (set on forwarded Down, cleared on forwarded Up); within `handle_mouse`, if the latched flag is true, force the forwarding branch for Drag/Up regardless of the live `delegates_to_tmux()` value. Reset the latch on tab switch (alongside the WR-01 fix).

Lower-cost alternative: when delegation flips false on an Up(Left) and `tmux_in_copy_mode` was set, synthesize and forward the Up byte before falling through to the overlay path. Document the asymmetric forward as a "drain on de-delegation" comment.

## Info

### IN-01: `encode_sgr_mouse` returns `Some(...)` for variants no caller exercises

**File:** `src/events.rs:38-45`
**Issue:** The encoder returns Some for `ScrollUp` (button 64) and `ScrollDown` (button 65) plus modifier folding, but the only call site (the Phase 7 intercept block at events.rs:108) only forwards Down/Drag/Up(Left) — scroll falls through to the legacy inline encoder at events.rs:287-292, which does NOT apply modifiers and uses different coordinate handling (`saturating_add(1).max(1)` vs the encoder's `local_col + 1`). If a future caller wires `encode_sgr_mouse` to scroll forwarding without auditing this divergence, scroll behavior will silently change.
**Fix:** Either (a) drop the ScrollUp/ScrollDown arms of the match — they are dead code today — or (b) migrate the scroll branch at events.rs:287-292 to call `encode_sgr_mouse` and reconcile the +1.max(1) saturation. Option (b) eliminates the duplicate-encoder concern flagged by the encoder's own doc comment ("single source of truth for SGR").

### IN-02: `tmux_in_copy_mode = true` is set eagerly on Down, before tmux has actually entered copy-mode

**File:** `src/events.rs:120`
**Issue:** A bare single-click in tmux does NOT enter copy-mode; only a drag does. The state machine sets `in_copy_mode = true` on Down anyway and then resets to false on Up if no Drag was seen. Between Down and Up the flag is logically inaccurate. Esc pressed during a held click (rare but reachable) would route through Tier-2 forwarding even though tmux is not in copy-mode. The downstream effect is benign (tmux passes `\x1b` through), so this is informational, not a warning.
**Fix:** Move the `in_copy_mode = true` set to the Drag(Left) arm — the unambiguous "copy-mode has now started" event. Down would only update the click-cluster counter; Drag would set both `drag_seen` and `in_copy_mode`; Up would clear `drag_seen` only. This also simplifies the asymmetry the comment at events.rs:113-118 has to explain.

### IN-03: `Arc<AtomicBool>` on `PtySession` without cross-thread sharing

**File:** `src/pty/session.rs:40-45, 93-94`
**Issue:** The two new flags `tmux_in_copy_mode` and `tmux_drag_seen` are wrapped in `Arc` despite being read/written exclusively from the App event-loop task. No `Arc::clone` is performed at session-spawn (compare `scroll_gen_clone` at session.rs:91 which IS shared with the PTY reader thread). The `Arc` is overhead with no current consumer.
**Fix:** Either drop to plain `AtomicBool` (or even `Cell<bool>` / `bool` since access is single-threaded) — or add a comment justifying the Arc as forward-compat for a future consumer.

### IN-04: `save_buffer_to_pbcopy` discards pbcopy's exit status; `true` return overstates success

**File:** `src/tmux.rs:169-194`
**Issue:** The function returns `true` after `let _ = pbcopy.wait();` — a successful tmux save-buffer + a failed pbcopy wait would still claim success. The doc comment promises "true on full success (buffer non-empty AND pbcopy succeeded)". Today the only caller (`events.rs:506`) discards the bool, so this is cosmetic, but the doc/behavior mismatch is a maintenance hazard.
**Fix:** Capture the wait result and return `pbcopy.wait().map(|s| s.success()).unwrap_or(false)`. Or drop the doc claim of "AND pbcopy succeeded" if the looser semantic is intentional.

### IN-05: `flip_active_parser_to_mouse_mode` test helper holds a write lock across `expect`

**File:** `src/selection_tests.rs:56-63`
**Issue:** The helper acquires `parser.write()` and immediately calls `parser.process(...)`. If `parser.write()` panics (poisoned lock), the test fails fine. If the test panics later while holding the lock... it doesn't — the lock is dropped at the helper's end-of-scope. This is fine. The actual nit: the comment block above (lines 41-55) is excellent context documentation but currently mixed with a one-line implementation. Consider extracting the `\x1b[?1006h` vs `\x1b[?1000h` warning into a constant or shared comment in `tmux_native_selection_tests.rs:131-136` (where the same caveat is documented again) so the truth has a single home.
**Fix:** No code change required; opportunistically deduplicate the DECSET-mode comment between the two test files when next touched.

---

_Reviewed: 2026-04-25_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
