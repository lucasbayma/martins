---
phase: 07-tmux-native-main-screen-selection
plan: 03
subsystem: app
tags: [phase-7, app, helpers, vt100, set-active-tab, d-07, d-10, d-14, d-16]
dependency_graph:
  requires:
    - "src/pty/session.rs::PtySession.tmux_in_copy_mode (Plan 07-02)"
    - "src/pty/session.rs::PtySession.tmux_drag_seen (Plan 07-02)"
    - "src/tmux.rs::cancel_copy_mode (Plan 07-02)"
    - "src/tmux.rs::tab_session_name (existing pre-Phase-7)"
    - "vt100 0.16.2 ::Screen::mouse_protocol_mode + alternate_screen"
  provides:
    - "App::active_session_delegates_to_tmux() -> bool"
    - "App::active_tmux_session_name() -> Option<String>"
    - "App::tmux_in_copy_mode() -> bool"
    - "App::tmux_in_copy_mode_set(bool)"
    - "App::tmux_drag_seen_set(bool)"
    - "App::tmux_drag_seen_take() -> bool"
    - "App::set_active_tab D-16 cancel-outgoing extension"
  affects:
    - "Plan 07-04 (handle_mouse conditional intercept) — consumes all 6 helpers"
    - "Plan 07-05 (handle_key Esc + cmd+c Tier 2) — consumes active_session_delegates_to_tmux + active_tmux_session_name + tmux_in_copy_mode + tmux_in_copy_mode_set"
tech-stack:
  added: []
  patterns:
    - "vt100-state read via try_read + conservative fallback (mirrors active_scroll_generation)"
    - "Atomic load/store on active-session field with active_sessions().get(active_tab) guard"
    - "swap(false, Relaxed) for take-and-clear semantics"
    - "Fire-and-forget subprocess prepended BEFORE state mutation (set_active_tab D-16)"
key-files:
  created: []
  modified:
    - "src/app.rs (+88 lines: 5 helper fns + 7-line set_active_tab D-16 extension)"
decisions:
  - "Doc-comment on active_tmux_session_name says 'active tab' generically; the D-16 outgoing-session semantics are encoded in CALL ORDERING in set_active_tab (read name BEFORE mutating active_tab), not in the helper itself. Intentional and load-bearing per plan note."
  - "tmux_drag_seen_take uses AtomicBool::swap(false, Relaxed) for atomic read-and-clear (matches plan behavior contract: 'load+store-false atomically — use swap')."
  - "All 5 helpers and the D-16 extension follow the active_scroll_generation precedent exactly: active_sessions().get(active_tab) lookup → graceful fall-through with safe default on missing session."
  - "No #[allow(dead_code)] added per plan — Plan 07-04/07-05 wire all helpers in waves 2/3; transient dead_code warnings accepted."
metrics:
  duration: "~10m"
  completed: "2026-04-25"
---

# Phase 7 Plan 03: App-side helpers + set_active_tab D-16 — Summary

**One-liner:** 5 new pub(crate) App helper fns (vt100-state delegate gate, tmux session name synthesis, atomic in-copy-mode + drag-seen accessors) plus a 7-line set_active_tab extension that fires `crate::tmux::cancel_copy_mode` on the outgoing session before the active_tab mutation (D-16).

## Plan Outcome

Goal achieved: every helper Plan 07-04 and 07-05 need to bridge between PtySession atomic flags / vt100 parser state and the event handlers is now in place. set_active_tab is wired to cancel the outgoing tmux copy-mode selection per D-16. All helpers are inert (no consumers in this plan) — Wave 2 (07-04) and Wave 3 (07-05) will exercise them.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Add 5 helper fns + extend set_active_tab with D-16 cancel | `7a0d2ba` | src/app.rs |

## Files Modified

- **src/app.rs** — added 6 pub(crate) helper additions to the App impl block (5 brand-new fns, plus the D-16 extension to existing `set_active_tab`):
  - Line 400: `set_active_tab` body extended with `if let Some(name) = self.active_tmux_session_name() { crate::tmux::cancel_copy_mode(&name); }` BEFORE `clear_selection() / active_tab = index / mark_dirty()`. Reads the OUTGOING session name (call ordering is load-bearing).
  - Line 554: `active_session_delegates_to_tmux(&self) -> bool` — `try_read` on parser, returns `mouse_protocol_mode == None && !alternate_screen`. Conservative fallback to `false` on lock contention or missing session.
  - Line 570: `active_tmux_session_name(&self) -> Option<String>` — chains `active_project()? / active_workspace()? / workspace.tabs.get(self.active_tab)?` and synthesizes via `crate::tmux::tab_session_name`.
  - Line 581: `tmux_in_copy_mode(&self) -> bool` — atomic load on PtySession.tmux_in_copy_mode (Relaxed). False on missing session.
  - Line 593: `tmux_in_copy_mode_set(&self, value: bool)` — atomic store (Relaxed). No-op on missing session.
  - Line 604: `tmux_drag_seen_set(&self, value: bool)` — atomic store (Relaxed). No-op on missing session.
  - Line 617: `tmux_drag_seen_take(&self) -> bool` — atomic `swap(false, Relaxed)` for read-and-clear. False on missing session.

## Acceptance Criteria — All Met

- ✓ `grep -q 'fn active_session_delegates_to_tmux' src/app.rs` exits 0
- ✓ `grep -q 'fn active_tmux_session_name' src/app.rs` exits 0
- ✓ `grep -q 'fn tmux_in_copy_mode' src/app.rs` exits 0 (matches both reader + setter)
- ✓ `grep -q 'fn tmux_drag_seen_set' src/app.rs` AND `grep -q 'fn tmux_drag_seen_take' src/app.rs` exit 0
- ✓ `grep -q 'crate::tmux::cancel_copy_mode' src/app.rs` exits 0 (proves D-16 wiring in `set_active_tab`)
- ✓ `cargo build` exits 0 (warnings are dead-code only — accepted per plan)
- ✓ Existing 138-test suite still green (Phase 6 baseline 129 + Plan 07-01's 8 + Plan 07-02's 1 — exactly matches the 138 expected by plan verification)

## Truths Affirmed (must_haves)

- ✓ **`active_session_delegates_to_tmux` returns true iff `mouse_protocol_mode == None` AND screen is NOT alternate.** Verified by inspecting line 554-568: returns `matches!(screen.mouse_protocol_mode(), vt100::MouseProtocolMode::None) && !screen.alternate_screen()`. False on missing session OR parser contention (try_read fail).
- ✓ **`active_tmux_session_name` returns the canonical `martins-{shortid}-{ws}-{tab}` for the active tab.** Verified by inspecting line 570-575: delegates to `crate::tmux::tab_session_name(&project.id, &workspace.name, tab.id)` — same fn used by all other Phase 7 session-name consumers.
- ✓ **`tmux_in_copy_mode / tmux_in_copy_mode_set / tmux_drag_seen_set / tmux_drag_seen_take` read+write the PtySession atomic flags.** Verified at lines 581-624: each helper uses `active_sessions().get(self.active_tab)` to find the active PtySession then `Ordering::Relaxed` load/store/swap on the corresponding `Arc<AtomicBool>` field. Behavior on missing session: read paths return `false`; write paths are no-ops.
- ✓ **`set_active_tab` fires `crate::tmux::cancel_copy_mode` on the OUTGOING tab BEFORE mutating active_tab (D-16).** Verified at line 400-411: the cancel call is at line 405 (3rd line of body, before `self.clear_selection()` / `self.active_tab = index` / `self.mark_dirty()`). `active_tmux_session_name()` reads the OUTGOING session name because `self.active_tab` has not yet been reassigned.

## Downstream Hooks (Plan 07-04 / 07-05 can call)

From `src/events.rs::handle_mouse` (Plan 07-04, Wave 2):
```rust
if app.active_session_delegates_to_tmux() {
    // Forward SGR bytes via app.write_active_tab_input(&bytes)
    // On Down(Left): app.tmux_in_copy_mode_set(true)
    // On Drag(Left): app.tmux_drag_seen_set(true)
    // On Up(Left): if !app.tmux_drag_seen_take() { app.tmux_in_copy_mode_set(false) }
}
```

From `src/events.rs::handle_key` (Plan 07-05, Wave 3):
```rust
// cmd+c Tier 2 — tmux paste-buffer fallback:
if app.active_session_delegates_to_tmux() {
    if let Some(session_name) = app.active_tmux_session_name() {
        tokio::task::spawn_blocking(move || {
            crate::tmux::save_buffer_to_pbcopy(&session_name);
        });
    }
}

// Esc — forward to delegating session in copy-mode:
if app.active_session_delegates_to_tmux() && app.tmux_in_copy_mode() {
    app.write_active_tab_input(&[0x1b]);
    app.tmux_in_copy_mode_set(false);
}
```

## Deviations from Plan

None. Plan executed exactly as written.

## Threat Surface Scan

No new security-relevant surface introduced. The helpers wrap pre-existing PtySession fields and pre-existing `crate::tmux::*` subprocess helpers (Plan 07-02). Threat register entries T-07-08 (parser lock contention via try_read), T-07-09 (Relaxed ordering race), T-07-10 (subprocess thrash on tab-switch) are mitigated/accepted exactly as the plan's threat_model declared. No additional flags raised.

## Self-Check: PASSED

- ✓ FOUND: `src/app.rs` modifications confirmed via `grep` (all 6 fn additions + cancel_copy_mode wiring)
- ✓ FOUND: commit `7a0d2ba` in `git log --all`
- ✓ FOUND: `cargo build` exits 0
- ✓ FOUND: `cargo test --bin martins` reports 138 passed / 0 failed (matches plan-expected baseline)
