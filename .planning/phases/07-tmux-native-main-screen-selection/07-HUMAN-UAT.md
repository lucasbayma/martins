# Phase 7 — Human UAT

**Phase:** 07-tmux-native-main-screen-selection
**Date:** TBD (operator-filled)
**Operator:** lucasobayma@gmail.com

## Setup

- Ghostty terminal A (top half of screen): run `tmux new -s baseline` directly. This is the reference baseline for "what tmux selection feels like natively in Ghostty".
- Ghostty terminal B (bottom half): run `target/release/martins` (Phase 7 build). This is the unit under test.
- Open both side-by-side at roughly equal pane sizes so feel comparison is direct.
- Have a third terminal pane available with `pbpaste` ready for clipboard verification.

## Cross-Path UAT Cases

> Maps to 07-RESEARCH.md §Validation Architecture > Manual UAT.

| ID | Path | Procedure | Expected | Result | Notes |
|----|------|-----------|----------|--------|-------|
| UAT-7-A | tmux native (delegate) | In Martins active tab running `bash` (no mouse-app), drag-select a line | Highlight in tmux's own reverse-video; `pbpaste` returns selected text after release. **Feel matches Ghostty A side-by-side.** |  |  |
| UAT-7-B | tmux native (delegate) | Double-click a word in `bash` | Word highlights and lands on clipboard immediately |  |  |
| UAT-7-C | tmux native (delegate) | Triple-click a line | Line highlights and lands on clipboard |  |  |
| UAT-7-D | tmux native (delegate) | Drag-select then press `Esc` | Selection clears + copy-mode exits in **single press** (verifies Plan 07-02 Esc override) |  |  |
| UAT-7-E | tmux native (delegate) | Drag-select then click outside the selection | Selection clears (tmux's own behavior + Plan 07-04 forward Down(Left) re-enters select) |  |  |
| UAT-7-F | tmux native (delegate) | Drag-select then press `cmd+c` | `pbpaste` returns selected text (Plan 07-05 Tier 2 path) |  |  |
| UAT-7-G | overlay (mouse-app: vim) | Run `vim`, `:set mouse=a`, drag-select | Phase 6 overlay highlight (REVERSED) appears, NOT tmux's. SEL-01..04 still hold. |  |  |
| UAT-7-H | overlay (mouse-app: htop) | Run `htop`, drag-select | Same as UAT-7-G — overlay path active because htop sets DECSET 1003 |  |  |
| UAT-7-I | overlay → tmux transition | In tab: run `vim`, drag-select (overlay), Esc, `:q`, then drag-select again | After `:q`, vim resets mouse mode; next drag uses tmux native. **No stale REVERSED highlight from vim session** (Pitfall #2 mitigation in Plan 07-04) |  |  |
| UAT-7-J | tab switch with active tmux selection | Tab 1 (delegate path): drag-select, leave selected. F-key to tab 2. | Tab 1's tmux selection canceled (verify by switching back: no highlight) — Plan 07-03 set_active_tab D-16 |  |  |
| UAT-7-K | cmd+c precedence | (a) overlay sel + cmd+c → overlay text; (b) clear, tmux sel + cmd+c → tmux buffer text; (c) clear all, in Terminal mode, cmd+c → SIGINT (interrupts a `sleep 30`) | All three tiers fire correctly |  |  |

## Pitfall Sweeps

| ID | Pitfall (RESEARCH) | Procedure | Pass Criterion |
|----|---------------------|-----------|----------------|
| PIT-7-1 | Pitfall #1: Modal click leak | Open Help modal (or any modal), click+drag inside the modal area | No tmux selection starts in the underlying pane (forward path correctly gated) |
| PIT-7-6 | Pitfall #6: scroll_generation false-positive on copy-mode highlight repaint | In delegate path, drag-select; while selection is shown, scroll the inner shell with mouse wheel | Selection coords stay anchored; no spurious gen-bump (verify by switching back to overlay path after — overlay selection from a different test render at correct row, not drifted) |

## Phase 6 Regression Sweep

Re-run Phase 6's UAT-6-* cases in mouse-app sessions to confirm no regression. Reference: `.planning/phases/06-text-selection/06-HUMAN-UAT.md`.

| Phase 6 ID | Status |
|------------|--------|
| UAT-6 SEL-01 (drag-select highlight tracks) | PASS / FAIL |
| UAT-6 SEL-02 (cmd+c copy in overlay path) | PASS / FAIL |
| UAT-6 SEL-03 (Esc / click-outside clears) | PASS / FAIL |
| UAT-6 SEL-04 (highlight survives PTY output) | PASS / FAIL |

## Operator Sign-Off

- [ ] All UAT-7-A..K marked PASS (or any FAIL has documented mitigation/follow-up)
- [ ] PIT-7-1 + PIT-7-6 marked PASS
- [ ] Phase 6 regression sweep all PASS
- [ ] Subjective "feels indistinguishable from Ghostty+tmux direct" confirmation: YES / NO

**Operator notes:**

(operator fills in after running)

**Forward-Looking Notes:**

(any items deferred to Phase 8 or future polish — e.g., rectangle-select via Alt-drag if user wants it; tmux 4.x compatibility audit per RESEARCH A1)

## Test Suite Status at UAT Start

```
$ cargo test --all-targets
test result: ok. {N} passed; 0 failed; 0 ignored
```

(operator pastes actual output)
