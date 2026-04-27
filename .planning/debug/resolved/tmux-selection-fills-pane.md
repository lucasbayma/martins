---
slug: tmux-selection-fills-pane
status: resolved
resolution: visual_fix_landed_with_residual_ux_concern
created: 2026-04-25
updated: 2026-04-25
related_phase: 07-tmux-native-main-screen-selection
related_gap: GAP-7-01
---

# Debug: tmux selection fills entire pane in Martins delegate path — RESOLVED

## Resolution

**Visual style fix landed (commit `c677cc5`):** Phase 6 overlay highlight switched from
XOR-toggled `Modifier::REVERSED` to tmux's default `mode-style` (fg=Black, bg=Yellow).

**Coordinate behavior confirmed correct via empirical instrumentation
(`MARTINS_MOUSE_DEBUG=1` logs):**

| Repro | Coords | Bounded correctly? |
|-------|--------|-------------------|
| Bash drag (5 rows × 13 cols) | (0,24)→(13,29) | YES |
| Opencode drag (35 rows × 95 cols) | (0,5)→(95,40) | YES |
| Small drag (1 row × 75 cols) | (5,21)→(80,21) | YES |

All three logs show `delegate=false` throughout — Phase 7's delegate path never
engages in the operator's real workflow because inner programs (opencode, vim,
etc.) always have mouse mode active. Per D-01 the overlay path is the right
path for those sessions.

## Eliminated hypotheses

- **B-2 (pane-size mismatch):** `sync_pty_size` keeps tmux + vt100 atomically synced.
- **D (drag-exits-inner-rect-drops-events):** No coord inflation in any of three repro logs.
- **E (scroll-generation false-positive):** All three logs show `start_gen == current_gen`,
  `deltas=(0,0)`. No scroll during drag.

## Residual UX concern (operator: "não corrigiu, mas está ok")

Operator's perception of "selecting tudo" on large drags is **correct
stream-selection behavior** (matches native tmux): when dragging across N
rows, the rendered highlight fills the middle (N-2) rows from col 0 to
col width-1. This is how tmux's default copy-mode renders multi-line
selections.

If operator later wants block/rectangle selection (highlight only the
rectangle between Down and Up, no full-width middle-row fill), that's a
feature toggle — not a bug fix. Tracked as a future enhancement option;
not blocking GAP-7-01 closure.

## Artifacts

- **Visual fix:** commit `c677cc5` — `src/ui/terminal.rs` + `src/selection_tests.rs` updated
- **Diagnostic instrumentation:** commit `546c410` — `MARTINS_MOUSE_DEBUG` env var
  preserved (zero-cost when off, available for future re-verification)
- **Three operator-captured logs** confirming coord correctness across delegate
  hypothesis and overlay hypothesis paths
- **145/145 tests passing**

## Operator sign-off

"Não corrigiu, mas está ok" (2026-04-25) — accepting current state. Visual
fix produced uniform yellow highlight matching tmux mode-style; cumulative
behavior on large drags is stream-selection by design.
