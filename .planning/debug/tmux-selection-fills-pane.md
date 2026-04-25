---
slug: tmux-selection-fills-pane
status: instrumentation_pending
trigger: |
  The internal selection is still not the same as the native tmux (first image) on Martins.
  The second image selection should be like the first one.
created: 2026-04-25
updated: 2026-04-25
related_phase: 07-tmux-native-main-screen-selection
related_gap: GAP-7-01
---

# Debug: tmux selection fills entire pane in Martins delegate path

## Symptoms

### Expected behavior
Native tmux stream-selection bounds when dragging in Martins' wrapped tmux session: first line starts at the click column, middle lines fully highlighted, last line ends at the release column. Should feel indistinguishable from running tmux directly in Ghostty (Phase 7's headline goal).

### Actual behavior
Selection in Martins is a near-full-pane rectangle: the white highlight covers essentially the entire visible content area (full width × most of height) regardless of where the user actually dragged. Visual comparison: native tmux (image 1) shows a properly-bounded stream-selection across a few lines; Martins (image 2) shows the highlight filling the whole pane.

### Error messages
None — no panic, no log error. Pure visual/behavioral regression.

### Timeline
Surfaced post-Phase-7 sign-off (2026-04-25). Operator typed "approved" via the resume-signal contract without per-row UAT walkthrough; subsequent dual-pane comparison vs. native tmux exposed the gap. Phase 7 reopened — see `.planning/phases/07-tmux-native-main-screen-selection/07-VERIFICATION.md` GAP-7-01.

### Reproduction
1. Run `target/release/martins` (currently built at `b2791665..fe739bd`)
2. Open a project, switch to a tab — operator's repro session was inside opencode (a TUI program)
3. Drag-select with mouse on the PTY pane content
4. Compare against native tmux selection in side-by-side Ghostty pane

**Repro state still ambiguous on which path engages:** if the inner program is opencode/vim/htop (DECSET 1000/1002/1003), `active_session_delegates_to_tmux()` returns `false` and the Phase 6 overlay path runs. If the inner program is plain `bash`/`zsh` with no mouse-mode, the delegate path runs.

## Hypotheses (carried over from GAP-7-01)

### Hypothesis A: overlay-in-mouse-app
Operator was inside opencode (a mouse-app TUI that sets DECSET 1000+). `active_session_delegates_to_tmux()` returned `false` per design (D-01: mouse-app sessions retain Phase 6 overlay path). The visual mismatch is a pre-existing Phase 6 overlay rendering quirk, not a Phase 7 regression.

**Testable:** Reproduce in plain `bash` with `cat /usr/share/dict/words | head -30` — should engage delegate path. If bug ALSO appears, hypothesis A is falsified.

### Hypothesis B: delegate-coord-inflation
Delegate path engaged but SGR bytes encode coords that tmux interprets as a near-full-pane drag. Suspect causes:
1. `local_col`/`local_row` in `src/events.rs:92-94` are `saturating_sub(inner.x/y)` but **not clamped** to `inner.width-1`/`inner.height-1`. If a Drag event reports a column past `inner.x + inner.width` (mouse moved out of inner rect mid-drag), the encoded col exceeds tmux's pane width, which tmux clamps to "rightmost column" — selection visually extends to right edge.
2. Wrapped tmux pane size differs from Martins inner rect (e.g., off-by-1 from titlebar/border). Then mapping (Martins col, row) → (tmux col, row) via local subtraction is correct but tmux's idea of "last cell" differs.
3. `mouse_protocol_mode` flips between Down(Left) and Up(Left) (REVIEW WR-02): Down arrives in non-mouse-app state (delegate path enters), inner program enables mouse mode, Up arrives in mouse-app state (delegate path exits) — tmux sees Down + dangling state, no Up, drags "to wherever".

**Testable:** Add `eprintln!` in `handle_mouse` delegate branch to log `(local_col, local_row, inner.width, inner.height, kind)` for each forwarded event during repro. Inspect what coords tmux receives.

### Hypothesis C: state-machine bug at Up(Left)
Code review IN-02 noted: `tmux_in_copy_mode` is set to `true` on Down(Left) before tmux actually enters copy-mode (single click doesn't enter it). If the state machine pathway also affects byte forwarding, drags could be interpreted by tmux as belonging to a different gesture.

**Testable:** Same logging as Hypothesis B, plus instrument `tmux_in_copy_mode` and `tmux_drag_seen` at each transition.

### Hypothesis D: drag-exits-inner-rect-drops-events (NEW — emerged from code re-read)
The delegate-path gate at `events.rs:87-90` requires `in_terminal == true` for ALL events, including mid-gesture Drag and Up. If during a drag the cursor leaves the inner rect (e.g., onto the left/right border, into the sidebar, into the status bar, or even just below the bottom row), those Drag/Up events are dropped — they reach neither tmux nor the overlay path with proper state. tmux is left with a dangling Down: button still "held" from tmux's perspective. The visible selection persists at whatever the LAST in-rect drag coordinate was — and if the user's drag-direction was generally downward/rightward, that last point is near the (max_col, max_row) corner of the pane → selection covers (start_col,1)→(max,max) → near-full-pane stream rectangle.

This mechanism MATCHES the visual signature ("near-full-pane rectangle") more cleanly than B-1 alone, because it explains both:
- Why the highlight covers the WHOLE pane content (selection extends to where the user *almost* released, which was near a pane edge), and
- Why tmux state stays stuck (no Up = button held, copy-mode latched on).

**Testable:**
- Instrument `handle_mouse` to log every mouse event with `(kind, mouse.column, mouse.row, in_terminal, delegate_active)`. Repro the gesture; check whether the final Drag/Up events show `in_terminal=false`.
- Equivalently: try the gesture WHILE staying strictly inside the inner rect (mouse never crosses borders). If selection is bounded correctly, hypothesis D is confirmed.

### Hypothesis E: scroll-generation false-positive inflates overlay translation (overlay path only)
Independent secondary candidate (overlay path). The PTY-reader thread's SCROLLBACK-LEN heuristic at `src/pty/session.rs:120-134` infers a scroll happened if `cursor was at last row AND top row hash changed`. For TUI programs that redraw frequently with the cursor near the bottom (opencode's prompt area), every redraw can be a false positive, bumping `scroll_generation` rapidly. During an overlay drag started seconds earlier, `start_gen` is captured at Down-time; `current_gen` on each render frame is much higher. `start_delta = current_gen - sel.start_gen` becomes large, `sr_translated` (`src/ui/terminal.rs:168`) goes deeply negative, clamped to 0. End-during-drag has `end_gen = None` (delta = 0), so `er_translated` follows the live cursor. Net visual: highlight from row 0 to current cursor row — **fills the upper portion of the pane regardless of where the user actually started the drag**.

**Testable:**
- Instrument `terminal.rs:render_with_selection_for_test` codepath to log `(sr_raw, er_raw, sr_translated, er_translated, start_delta, end_delta, scroll_generation, start_gen)` whenever a selection is rendered.
- Equivalently: in opencode, drag a small selection in the middle of the pane and observe whether the highlight starts from top row 0.

## Current Focus

hypothesis: Hypothesis D (drag-exits-inner-rect-drops-events) is the single strongest match for the visual signature; Hypothesis E is a secondary candidate that would also explain it for the overlay path specifically.
test: Add eprintln instrumentation at TWO sites simultaneously and ship a debug build:
  Site 1 (events.rs::handle_mouse, top of fn — every event):
    eprintln!("[mouse] kind={:?} ({},{}) inner=({},{},{}x{}) in_term={} delegate={}",
      mouse.kind, mouse.column, mouse.row, inner.x, inner.y, inner.width, inner.height,
      in_terminal, delegate_active);
  Site 2 (terminal.rs render selection block — every selection render):
    eprintln!("[sel-render] raw=({},{})→({},{}) gens=start{}/end{:?}/curr{} translated={}→{} clamped={}→{}",
      sc_raw, sr_raw, ec_raw, er_raw, sel.start_gen, sel.end_gen, current_gen,
      sr_translated, er_translated, sr, er);
  Run `cargo build --release`, run `target/release/martins 2>/tmp/martins-debug.log`, perform the gesture in (a) plain bash and (b) opencode, then attach the relevant log slices.
expecting: One of:
  - In-rect drag → in_term=true throughout AND coords match drag bounds → falsifies B/D
  - Drag exits inner rect → trailing events show in_term=false → confirms hypothesis D
  - Overlay-path render shows sr clamped to 0 with current_gen >> start_gen → confirms hypothesis E
  - Delegate-path engages in opencode (mouse_protocol_mode=None) → falsifies hypothesis A and points to B/C/D
next_action: Apply the two-site instrumentation, rebuild release, hand to operator for repro, classify based on log output. Then ship a defense-in-depth fix bundle:
  1. WR-02 fix: latch delegation at Down(Left) on the active PtySession; force-forward subsequent Drag/Up of the same gesture regardless of live `delegates_to_tmux()` value.
  2. Drain-on-Up: if delegation was latched at Down but the Up event arrives with in_terminal=false, still forward the Up byte (clamped to inner.width-1/height-1) so tmux never gets a stuck-button.
  3. Defensive coord clamp in delegate path: `local_col.min(inner.width.saturating_sub(1))`, `local_row.min(inner.height.saturating_sub(1))` — match the overlay path's discipline.
  4. WR-01 fix: clear outgoing session's `tmux_in_copy_mode` and `tmux_drag_seen` flags inside `set_active_tab` BEFORE the active_tab mutation.
  5. IN-02 fix: move `tmux_in_copy_mode_set(true)` from Down(Left) to Drag(Left) — only set the flag when tmux has actually entered copy-mode.
  6. (If hypothesis E confirmed) Tighten the SCROLLBACK-LEN heuristic: require cursor was ALSO at last column, OR check vt100's own scroll counter if available, OR compare bottom-row hash before/after as a second corroborating signal.
reasoning_checkpoint: |
  Why prioritize Hypothesis D over A/B/C?
  - D is the single hypothesis that explains the precise visual signature (full-pane rectangle, not arbitrary partial drag) without requiring exotic state.
  - The `in_terminal` gate is currently identical for Down/Drag/Up — there's no asymmetry that drains the gesture if it leaves the rect mid-drag, which is exactly the failure mode D describes.
  - The fix for D (drain-on-Up + per-gesture latch) also subsumes WR-02 and most of B-1, so it's high-leverage.

  Why not commit to a fix without instrumentation?
  - Without empirical evidence of which path engaged in the operator's repro, fixing the delegate path may not address an overlay-path-only bug (E).
  - The instrumentation is cheap (two eprintln sites), the rebuild is a few seconds, and the operator already has a repro setup. The marginal cost is low; the cost of shipping a wrong-target fix is one more verification cycle.

## Evidence

- 2026-04-25 [code-read] events.rs:73-141 — confirmed delegate-path gate `in_terminal && Modal::None && picker.is_none() && active_session_delegates_to_tmux()` applies uniformly to Down/Drag/Up. No drain-on-Up. No per-gesture latch. Mid-gesture path flip (WR-02) and out-of-rect drift (hypothesis D) are both unhandled.
- 2026-04-25 [code-read] events.rs:92-94 — `local_col` and `local_row` use `saturating_sub` only, no upper clamp. The `in_terminal` gate prevents the obvious overflow only as long as in_terminal stays true; once it flips false mid-gesture, the event is dropped entirely (not clamped+forwarded).
- 2026-04-25 [code-read] app.rs:855-887 (`sync_pty_size`) — both vt100 PTY parser AND the wrapped tmux session are resized to `(rows = terminal.height - 3, cols = terminal.width - 2)`, exactly matching `terminal_content_rect`. Eliminates hypothesis B-2 (pane-size mismatch).
- 2026-04-25 [code-read] tmux.rs:24-46 (`ensure_config`) — does NOT include `set -g status off`. tmux's status bar is on by default → tmux's pane content occupies `(rows - 1)` rows, with row `rows` being the status bar. Mouse events Martins sends with `local_row = inner.height - 1` → `row = inner.height` reach tmux's status bar row. Not a fill-the-pane cause on its own (tmux ignores status-bar drags), but it does mean the bottom row of Martins's inner rect renders tmux's status bar text, slightly shrinking the user-visible "pane content" region by one row.
- 2026-04-25 [code-read] pty/session.rs:120-134 — SCROLLBACK-LEN heuristic: `scrolled = before_cursor_row >= rows-1 && before_top_hash != after_top_hash`. False-positive prone for TUI programs that redraw frequently with cursor near the bottom (opencode prompt). Each false-positive bumps scroll_generation, inflating overlay-path `start_delta` (hypothesis E).
- 2026-04-25 [code-read] terminal.rs:157-198 — overlay rendering iterates `sr..=er` × `0..=width-1` (full row for middle rows). With `sr` clamped to 0 (from negative translation) and `er` near the live cursor row, the highlight visually approximates a full-pane rectangle. Confirms hypothesis E's mechanism.
- 2026-04-25 [code-read] events.rs:104-107 — delegate path clears stale overlay selection on forwarded events. Inverse cleanup (clear tmux state when overlay takes over mid-gesture) does NOT exist, confirming WR-02 mechanism.

## Eliminated

- **Hypothesis B-2 (tmux pane-size mismatch):** `sync_pty_size` keeps tmux session size in sync with `terminal_content_rect` via the same `(height-3, width-2)` formula. Both the PTY and tmux are resized atomically. Mismatch can only occur transiently during a resize event and would not produce a steady "fill-the-pane" state.

## Resolution

(populated when fix is verified)
