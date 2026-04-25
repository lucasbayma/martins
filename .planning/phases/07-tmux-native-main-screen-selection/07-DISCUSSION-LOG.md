# Phase 7: tmux-native main-screen selection - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-25
**Phase:** 07-tmux-native-main-screen-selection
**Areas discussed:** Trigger scope, Mouse delegation model, cmd+c → clipboard wiring, Existing SelectionState fate, Switch behavior, Word/line clicks

---

## Trigger Scope

| Option | Description | Selected |
|--------|-------------|----------|
| Only when inner app hasn't requested mouse mode | Auto-detect via tmux pane state — if running program has set `\x1b[?1000h` (vim mouse-visual, htop), keep overlay. Otherwise hand off to tmux copy-mode. | ✓ |
| Always delegate (drop overlay entirely) | Tmux owns selection unconditionally. Cost: vim's `mouse=a` visual mode breaks. | |
| User-toggled (e.g., a key to switch modes) | Default overlay; key (e.g., `prefix [`) enters tmux copy-mode explicitly. | |
| Only when no alt-screen app is active | Detect alt-screen state via tmux pane info. `alternate-screen off` makes signal unreliable. | |

**User's choice:** Only when inner app hasn't requested mouse mode (Recommended)
**Notes:** Cascades into mouse-delegation and overlay-fate decisions. Captured as D-01 in CONTEXT.md.

---

## Mouse-Mode Detection

| Option | Description | Selected |
|--------|-------------|----------|
| Track mouse-mode escapes in PTY drain | Watch tmux PTY output for `\x1b[?1000h/l` and `\x1b[?1006h/l`. Maintain per-session `mouse_requested: bool`. Cheap, deterministic. | ✓ |
| Query tmux on demand (`display-message -p`) | Run subprocess on every drag-start. Authoritative but ~5-15ms latency per drag. | |
| Heuristic from vt100 state | Inspect vt100 parser flags. Cheapest but vt100 may not expose mouse-mode. | |

**User's choice:** Track mouse-mode escapes in PTY drain (Recommended)
**Notes:** Captured as D-02. D-03 explicitly rejects subprocess-per-drag.

---

## Mouse-App Path (when inner app HAS requested mouse mode)

| Option | Description | Selected |
|--------|-------------|----------|
| Keep current Phase 6 behavior — intercept and overlay-select | Drag(Left) creates Martins' overlay (D-10 from Phase 6). vim mouse=a stays broken, but htop/btop selection usable via overlay. | ✓ |
| Forward raw mouse events to inner app via tmux PTY | Pass SGR mouse bytes through. vim visual mode works. Cost: no Martins selection at all when mouse-app running. | |

**User's choice:** Keep current Phase 6 behavior — intercept and overlay-select (Recommended)
**Notes:** Confirms D-12 (overlay primitives kept as fallback).

---

## Mouse Delegation Model (driving tmux on left-mouse drag)

| Option | Description | Selected |
|--------|-------------|----------|
| Forward raw SGR mouse events to tmux PTY | Write `\x1b[<0;col;row{M\|m}` bytes. Tmux's `mouse on` handles copy-mode entry, drag, finalize natively. | ✓ |
| Drive via `tmux send-keys -X` control commands | `begin-selection`, cursor-move, `copy-pipe`. Subprocess per mouse-move — tanks latency. | |
| Hybrid: SGR for drag, control for entry/exit | More moving parts. | |

**User's choice:** Forward raw SGR mouse events to tmux PTY (Recommended)
**Notes:** Captured as D-04, D-05, D-06.

---

## Drag Interception Path

| Option | Description | Selected |
|--------|-------------|----------|
| Conditional intercept based on `mouse_requested` flag | Keep interception when mouse_requested=true (overlay). When false, forward Down/Drag/Up SGR to tmux PTY and skip SelectionState mutation. | ✓ |
| Always forward, retire interception | Removes Phase 6 D-10. Conflicts with Trigger Scope decision. | |

**User's choice:** Conditional intercept based on mouse-detect flag (Recommended)
**Notes:** Captured as D-07, D-08. Explicitly replaces Phase 6 D-10.

---

## cmd+c → Clipboard Source

| Option | Description | Selected |
|--------|-------------|----------|
| Bind tmux copy-pipe to pbcopy in tmux.conf | Add `MouseDragEnd1Pane send -X copy-pipe-and-cancel 'pbcopy'`. Selection on macOS clipboard the moment drag ends. cmd+c re-runs `tmux save-buffer - \| pbcopy`. | ✓ |
| Martins polls `tmux save-buffer -` on cmd+c | No tmux.conf change. Mouse-up requires copy-pipe binding anyway → collapses into option 1. | |
| Forward cmd+c into tmux copy-mode as `y` | Pure native. Only works if tmux currently in copy-mode → cmd+c after drag-end is no-op. | |

**User's choice:** Bind tmux copy-pipe to pbcopy in tmux.conf (Recommended)
**Notes:** Captured as D-09, D-10.

---

## cmd+c → SIGINT Semantics

| Option | Description | Selected |
|--------|-------------|----------|
| Keep D-03 — no selection in tmux's buffer either → SIGINT | Check overlay empty AND tmux buffer empty. If both, forward 0x03 to active PTY. Preserves Phase 6 macOS feel. | ✓ |
| Change — cmd+c is always a tmux-buffer copy attempt | Drop SIGINT semantics. Diverges from Ghostty/macOS feel. | |

**User's choice:** Keep D-03 — no selection in tmux's buffer either → SIGINT (Recommended)
**Notes:** Captured as D-11.

---

## Overlay Fate

| Option | Description | Selected |
|--------|-------------|----------|
| Keep as alt-screen/mouse-app fallback | When mouse_requested=true, overlay path runs end-to-end (drag, anchor, render, copy via vt100 contents_between). When false, overlay sleeps. No code deleted. | ✓ |
| Retire overlay entirely | Delete SelectionState, scroll-generation, REVERSED-XOR. Conflicts with Trigger Scope. | |
| Coexist always (overlay + tmux selection both visible) | Visually noisy, double clipboard writes. | |

**User's choice:** Keep as alt-screen/mouse-app fallback (Recommended)
**Notes:** Captured as D-12, D-13.

---

## Clear Semantics (Esc / click-outside)

| Option | Description | Selected |
|--------|-------------|----------|
| Both paths converge — Esc/click in Martins → also exits tmux copy-mode | If tmux in copy-mode, Esc/click runs `tmux send-keys -X cancel`. Phase 6 Esc-precedence still holds. | ✓ |
| Let tmux handle its own cancel (default `q` in copy-mode) | Don't intercept Esc/click for tmux selections. Diverges from Phase 6 Esc semantics. | |

**User's choice:** Both paths converge — Esc/click in Martins → also exits tmux copy-mode (Recommended)
**Notes:** Captured as D-14, D-15.

---

## Tab/Workspace Switch Behavior

| Option | Description | Selected |
|--------|-------------|----------|
| Force-cancel tmux copy-mode on switch | `tmux send-keys -X cancel -t <outgoing>` (gated on `#{pane_in_mode}`). Mirrors Phase 6 D-22. | ✓ |
| Let tmux preserve copy-mode per-session | Switching back resumes selection mid-drag. Breaks Phase 6 invariant. | |
| Cancel only on workspace switch, preserve on tab switch | Inconsistent. | |

**User's choice:** Force-cancel tmux copy-mode on switch (Recommended)
**Notes:** Captured as D-16.

---

## Word/Line Click Semantics in tmux Path

| Option | Description | Selected |
|--------|-------------|----------|
| Use tmux native bindings — add to tmux.conf | `DoubleClick1Pane → select-word`, `TripleClick1Pane → select-line`. Tmux owns timing and word boundaries. | ✓ |
| Keep Phase 6's rules, drive tmux via send-keys | Martins still tracks (last_click_at, click_count), issues `select-word/line`. Subprocess latency per click. | |
| Skip word/line in tmux mode | Defer. Feels regressive after Phase 6. | |

**User's choice:** Use tmux native bindings — add to tmux.conf (Recommended)
**Notes:** Captured as D-17, D-18.

---

## Claude's Discretion

- **D-04 detail:** exact SGR button-mask bytes for drag-move (button=32+0 for left-button-held) — researcher to verify against crossterm's `MouseEventKind::Drag` payload and tmux's expected wire format.
- **D-09 detail:** whether `MouseDragEnd1Pane` works in `copy-mode-vi` table or also needs a `copy-mode` table entry — depends on `mode-keys` setting; recommend explicit binding in both tables.
- **D-14 detail:** how Martins knows tmux is in copy-mode without polling — (a) track Martins' own state machine (forwarded press → expect copy-mode until forwarded cancel), or (b) cache `#{pane_in_mode}` and refresh on Esc/click. Researcher to recommend.
- Whether to bind `Escape` in `copy-mode-vi` to `cancel` explicitly (likely default — verify).
- Whether to log copy events via tracing — recommend skip (consistent with Phase 6 stance).

## Deferred Ideas

- Right-click context menu (paste, copy as plain/rich) — not in scope; tmux-native UX doesn't include it.
- Block/rectangle selection (Alt+drag) — may fall out for free from D-04 (SGR with Alt modifier). Confirm in research.
- Search-in-scrollback (`?` in copy-mode) — out of scope per PROJECT.md.
- Customizing tmux's highlight color to match Martins' theme — defer; reverse-video is acceptable.
