# Phase 6: Text Selection - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in 06-CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-24
**Phase:** 06-text-selection
**Areas discussed:** Copy trigger, Selection stability under output, PTY mouse-capture handshake, Selection extension semantics

---

## Copy Trigger

### Q1: When does selected text land on the clipboard?

| Option | Description | Selected |
|--------|-------------|----------|
| cmd+c only | Strict SEL-02 reading; selection is visual state, copy is explicit action | |
| Auto-copy on mouse-up | Keep current behavior; releasing the mouse copies | ✓ |
| Both: auto-copy + cmd+c | Mouse-up copies AND cmd+c re-copies the active selection | (later refined to this — see Q1b) |

**User's choice:** Auto-copy on mouse-up — but follow-up Q1b promoted to "Both" because SEL-02 explicitly requires cmd+c on active selection.

### Q1b (clarifier): What does cmd+c do when a selection IS active?

| Option | Description | Selected |
|--------|-------------|----------|
| Re-copy to clipboard | Honor SEL-02 strictly; mouse-up does primary copy, cmd+c re-copies | ✓ |
| Forward to PTY as Ctrl+C anyway | Always SIGINT; drops SEL-02 | |
| No-op (selection blocks SIGINT, no copy) | Swallow cmd+c when selection active | |

**User's choice:** Re-copy to clipboard.

### Q2: What does cmd+c do when there is no active selection?

| Option | Description | Selected |
|--------|-------------|----------|
| Forward to PTY as Ctrl+C | Send 0x03 / SIGINT — Ghostty behavior | ✓ |
| Always copy, never forward | cmd+c reserved for copy; use Ctrl+C for SIGINT | |
| No-op when empty | Swallow cmd+c silently if nothing selected | |

**User's choice:** Forward to PTY as Ctrl+C.

### Q3: After a successful copy, what happens to the highlight?

| Option | Description | Selected |
|--------|-------------|----------|
| Stays until explicit clear | SEL-04 + Ghostty: copy doesn't dismiss selection | ✓ |
| Clears immediately on copy | Drop highlight in next frame after copy | |

**User's choice:** Stays until explicit clear.

---

## Selection Stability Under Output (SEL-04)

### Q4: How should the selection behave when PTY output scrolls the screen?

| Option | Description | Selected |
|--------|-------------|----------|
| Anchor to scroll generation | u64 ticks per scroll; selection follows content | ✓ |
| Pause PTY output while selection active | Buffer in tokio, drain on clear | |
| Screen-fixed (current behavior) | Selection drifts as buffer scrolls | |
| Anchor + scroll-off behavior deferred | Pick anchored, defer top-scroll-off spec | |

**User's choice:** Anchor to scroll generation. Top-scroll-off behavior locked: clip at visible top, render nothing if entirely off, retain SelectionState in app.

### Q5: If the user is mid-drag when output scrolls, what happens to the in-progress selection?

| Option | Description | Selected |
|--------|-------------|----------|
| Anchor at drag start | Start anchored at drag-start; end stays cursor-relative | ✓ |
| Both ends screen-relative until mouse-up | Freeze + anchor at mouse-up | |
| You decide | Claude's discretion | |

**User's choice:** Anchor at drag start.

---

## PTY Mouse-Capture Handshake

### Q6: How should drag-select interact with PTY apps that have requested mouse events?

| Option | Description | Selected |
|--------|-------------|----------|
| Always intercept Left-drag in martins | Martins owns drag; vim mouse-visual sacrificed | ✓ |
| Forward Left-drag to PTY when mouse-mode is on | State-dependent; martins selects when off | |
| Modifier toggles: Option+drag overrides | Default forwards; Option-drag selects | |
| Always intercept + add modifier to forward | Default selects; Option-drag forwards | |

**User's choice:** Always intercept Left-drag in martins.

### Q7: Click and Escape clearing — what's in scope for SEL-03?

| Option | Description | Selected |
|--------|-------------|----------|
| Click-anywhere + Escape both clear | Closest to Ghostty / Terminal.app | ✓ |
| Click-outside-selection + Escape; click-inside no-op | Geometric containment check | |
| Escape only clears in Normal mode; clicks always clear | Avoids stealing Esc from vim | |

**User's choice:** Click-anywhere + Escape both clear.

**Notes:** Inferred precedence (locked in CONTEXT.md D-14): Esc clears IFF a selection is active. When no selection, Esc falls through to existing path (PTY-forward in Terminal mode). Prevents stealing Esc from vim/less.

---

## Selection Extension Semantics

### Q8: Which extension features are in scope for Phase 6?

| Option | Description | Selected |
|--------|-------------|----------|
| Drag-only (v1 minimum) | Just SEL-01..04, no multi-click | ✓ (baseline) |
| Double-click selects word | Word-boundary helper + click-time-tracker | ✓ |
| Triple-click selects line | Cheap once double-click counter exists | ✓ |
| Shift-click extends selection | Move end-anchor to click point | ✓ |

**User's choice:** All four — drag (baseline), double-click word, triple-click line, shift-click extend. Full Ghostty parity in Phase 6.

### Q9: Highlight visual style?

| Option | Description | Selected |
|--------|-------------|----------|
| Stays gold-accent (current) | ACCENT_GOLD bg + BG_SURFACE fg | |
| Switch to inverted (terminal-standard) | Per-cell fg↔bg swap; matches Terminal.app/Ghostty | ✓ |
| You decide | Claude's discretion | |

**User's choice:** Switch to inverted (terminal-standard).

---

## Claude's Discretion

- vt100 scroll-counter sourcing (D-09): wrap parser advance vs use vt100's own scrollback API surface. Researcher to verify what the crate exposes.
- Word boundary predicate (D-17): Unicode word break recommended, but exact regex/predicate left to researcher with `vt100::Screen` wide-char awareness in mind.
- Whether to flash visual feedback on cmd+c re-copy (recommend skip, no requirement either way).
- Whether to add tracing spans around selection events (recommend skip — OBS-01 deferred to v2).

## Deferred Ideas

- Mouse-mode-aware drag forwarding (rejected for Phase 6, possible v2 setting).
- Option/Shift modifier to forward Left-drag to PTY (rejected for Phase 6).
- Visual flash on cmd+c re-copy (not requested).
- Keep selection across tab switch (rejected; per-session generation makes cross-session selection meaningless).
- Tracing spans / observability (already deferred to v2 OBS-01).
- User-configurable word-boundary predicate (out of scope).
