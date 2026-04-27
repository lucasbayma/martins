# Phase 6: Text Selection - Context

**Gathered:** 2026-04-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Drag-select text in the PTY main pane with a visible highlight, copy via `cmd+c` (and on mouse-up), clear via click outside or Escape. Highlight survives streaming PTY output until the user explicitly clears it. Matches Ghostty's feel.

Requirements covered: SEL-01, SEL-02, SEL-03, SEL-04 (see REQUIREMENTS.md).

</domain>

<decisions>
## Implementation Decisions

### Copy Trigger (SEL-02)
- **D-01:** Auto-copy on Left-mouse-up — keep current `events.rs:64-72` behavior. Releasing the drag puts text on the clipboard via `pbcopy`.
- **D-02:** `cmd+c` while a selection is active also re-copies the same selection to the clipboard. Both paths converge on `App::copy_selection_to_clipboard`. This is what literally satisfies SEL-02.
- **D-03:** `cmd+c` while NO selection is active forwards `0x03` (Ctrl+C / SIGINT) to the active PTY tab. macOS-native muscle memory; matches Ghostty.
- **D-04:** Successful copy does NOT clear the highlight. Selection persists until click-outside or Escape. Lets user copy, paste, then re-copy.

### Selection Stability Under Streaming Output (SEL-04)
- **D-05:** Add `scroll_generation: u64` to `App` (or to the active `PtySession`). Increment every time the vt100 screen scrolls (i.e., a new line is appended that pushes the top row off). Source the increment from the PTY drain loop, not from render.
- **D-06:** `SelectionState` stores both endpoints as anchored coords: `(gen, screen_row, col)`. On each render frame, compute `delta = current_gen - sel_gen` and translate to current screen row: `current_row = anchored_row - delta`.
- **D-07:** Mid-drag: the start endpoint is anchored at drag-start (its `gen` is captured when `MouseEventKind::Drag` first creates the SelectionState). The end endpoint stays cursor-relative (always `(current_gen, mouse.row, mouse.col)`) until `MouseEventKind::Up`, at which point it gets anchored at the current generation.
- **D-08:** When an anchored row scrolls past the top of the visible region (`current_row < 0`), clip the highlight at the visible top — render only the portion of the selection still on-screen. If the entire selection has scrolled off, render nothing but keep `SelectionState` in app state (so the next `cmd+c` still has text to copy from `vt100::Screen::contents_between`).
- **D-09:** **Claude's discretion:** how to read the vt100 scroll counter. Two viable approaches — (a) wrap the parser advance call to detect scrolls by diffing `screen.cursor_position()` / row count before/after each PTY read, or (b) fork-style: track `total_scrollback_rows` from vt100 directly if the API exposes it. Researcher to pick based on what `vt100::Screen` actually exposes.

### PTY Mouse-Mode Handshake
- **D-10:** Martins always intercepts `MouseEventKind::Drag(Left)` inside the terminal pane — never forwarded to the PTY, regardless of whether the underlying app requested mouse events (`\x1b[?1000h` / `\x1b[?1006h`). vim's mouse-visual mode is sacrificed; keyboard `v` still works.
- **D-11:** `MouseEventKind::ScrollUp/ScrollDown` continues to forward as today (`events.rs:84-86,90-105`). No change.
- **D-12:** `MouseEventKind::Down(Left)` continues to clear any active selection then route to `handle_click` (`events.rs:78-83`). No change.

### Clearing (SEL-03)
- **D-13:** Any Left-mouse-down anywhere on screen clears the highlight (current behavior — keep).
- **D-14:** `Esc` key clears the selection IFF a selection is currently active. When no selection is active, `Esc` falls through to its existing handler — including PTY-forwarding in `InputMode::Terminal` (`events.rs:650`). This precedence prevents stealing Esc from vim/less/htop running inside the PTY.

### Selection Extension Semantics
- **D-15:** **In scope:** drag-select (baseline), double-click → select word, triple-click → select line, shift+left-click → extend selection's end anchor to the click point.
- **D-16:** Click-counter logic — track `(last_click_at: Instant, click_count: u8)` on App. Threshold: 300ms between clicks for double/triple. If a click lands outside the same word/line region as the previous click, reset the counter.
- **D-17:** Word boundary definition — Claude's discretion, but recommend Unicode word break (split on whitespace + ASCII punctuation `[]()<>{}.,;:!?'"\`/\\|@#$%^&*=+~` and unicode whitespace). Researcher to confirm whether vt100's `Screen::contents_between` is byte-accurate enough or if column-aware iteration is needed for wide chars.
- **D-18:** Triple-click "line" = the visible logical row in the vt100 screen. Wrapped lines are NOT joined.
- **D-19:** Shift-click only extends the END anchor; it does NOT move the start anchor. If no selection exists, shift+click is a no-op (does not start a new one).

### Highlight Style
- **D-20:** Replace gold-accent recolor with **inverted-cell** highlight: per highlighted cell, swap fg↔bg using existing cell colors (read each `Cell` and write a new style with bg=cell.fg, fg=cell.bg). Falls back to ACCENT_GOLD only if the source cell has no fg/bg (unlikely with vt100 output).
- **D-21:** Cells already containing `Modifier::REVERSED` (a vt100-rendered reverse-video cell, e.g. status line) — re-invert (i.e., un-reverse) so the highlight is visually distinct from the underlying reverse-video. Same logical operation: XOR the REVERSED modifier.

### Tab/Workspace Switching
- **D-22:** Selection clears on tab switch and on workspace switch. The anchored generation is per-session; cross-session highlight is meaningless. Plan should add the clear in the existing tab-switch and workspace-switch code paths.

### Dirty-Flag Coupling (carries forward from Phase 2)
- **D-23:** Every selection mutation must call `App::mark_dirty()`: `Drag` create/extend, `Up` finalize, `Down` clear, `Esc` clear, `cmd+c` (no — read-only, no redraw needed unless we add a flash), tab switch, workspace switch, and the per-frame anchor-translation when scroll_gen changes (the PTY drain that bumps `scroll_generation` already calls mark_dirty today, so this is implicit).

### Claude's Discretion
- D-09 (vt100 scroll-counter sourcing)
- D-17 (word boundary regex/predicate)
- Whether to add a brief visual flash on cmd+c re-copy (no requirement either way; recommend skip)
- Whether to log copy events via tracing (recommend skip — local dev tool, no need)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project Spec
- `.planning/PROJECT.md` — Core value (Ghostty/Alacritty feel parity), constraints (macOS-only, ratatui/crossterm/tokio stack)
- `.planning/REQUIREMENTS.md` §Text Selection — SEL-01..SEL-04 acceptance criteria
- `.planning/ROADMAP.md` §Phase 6 — Goal + Success Criteria 1-4

### Existing Code (skeleton in place — extend, don't rewrite)
- `src/app.rs:28-51` — `SelectionState` struct + `normalized()` + `is_empty()`
- `src/app.rs:78` — `App.selection: Option<SelectionState>`
- `src/app.rs:418-447` — `App::copy_selection_to_clipboard` (vt100 `screen.contents_between` + spawn `pbcopy`)
- `src/events.rs:38-88` — `handle_mouse` Drag/Up/Down branches (the modification surface)
- `src/events.rs:625-660` (approx) — `key_to_bytes` (where `cmd+c` and `Esc` precedence will hook)
- `src/ui/terminal.rs:156-177` — current highlight render (the surface where inverted-cell logic lands)
- `src/ui/terminal.rs:145-154` — `PseudoTerminal::new(parser.screen())` render (where scroll_generation must be observable)
- `src/pty/session.rs:221,251` — vt100 parser access pattern
- `src/keys.rs` — Keymap / Action / InputMode (where `Esc` precedence is decided)

### Codebase Maps (for stack-level reference, no decisions)
- `.planning/codebase/STRUCTURE.md` — file layout
- `.planning/codebase/ARCHITECTURE.md` — event-loop model
- `.planning/codebase/CONVENTIONS.md` — naming + module patterns
- `.planning/codebase/TESTING.md` — inline `#[cfg(test)]` pattern + `pty_input_tests.rs` precedent

### Prior Phase Artifacts (relevant context, not requirements)
- `.planning/phases/02-event-loop-rewire/` — dirty-flag rendering (`App::mark_dirty`) + biased select
- `.planning/phases/03-pty-input-fluidity/` — PTY input regression tests (`src/pty_input_tests.rs`) — pattern reuse for selection tests

### External
- vt100 crate docs (`Screen::contents_between`, `Screen::cursor_position`, `Screen::size`, scrollback API surface) — researcher to verify
- crossterm `MouseEventKind` variants — already in use
- Ghostty's text-selection feel is the qualitative baseline (no ref doc — user UAT)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`SelectionState` struct** (`src/app.rs:29`) — already has start/end/dragging fields and `normalized()`/`is_empty()` helpers. Will be extended to carry per-endpoint `scroll_generation: u64` and per-endpoint anchor state (start always anchored, end optionally anchored at mouse-up).
- **`App::copy_selection_to_clipboard`** (`src/app.rs:418`) — works today via `pbcopy`. The `cmd+c` keybinding will simply call this function. Idempotent; safe to invoke from both mouse-up and key-press paths.
- **`vt100::Screen::contents_between(sr, sc, er, ec+1)`** — already used at `src/app.rs:430`. Confirmed working for the current screen. Researcher to verify behavior when start row has scrolled into vt100's scrollback buffer (if vt100 even exposes scrollback through that API).
- **Inner-rect arithmetic** in `terminal_content_rect` (`events.rs`) and the `inner` rect in `terminal.rs:141` — selection coords are stored relative to `inner`. No change needed.
- **`pty_input_tests.rs`** — established pattern for terminal/PTY integration tests in this codebase. Selection tests should mirror its structure.

### Established Patterns
- **Free-function event handlers** (`crate::events::*`) — Phase 1 convention. New `handle_double_click`, `handle_triple_click`, `handle_shift_click` (or extension of `handle_mouse`) goes in `src/events.rs` as free functions taking `&mut App`.
- **Dirty-flag every state mutation** — Phase 2. Every `app.selection = ...` or `app.selection = None` must be followed by `app.mark_dirty()`. The current `events.rs:38-88` already does this implicitly via the next render — but explicit `mark_dirty()` after each mutation is the project convention.
- **`pub(crate)` for cross-module helpers** — Phase 1 pattern. New helpers like `selection::word_boundary_at(&Screen, row, col)` should be `pub(crate)` from a new module or extension methods on `SelectionState`.
- **Inline `#[cfg(test)]` modules** — Phase 1 + Phase 3. Tests live next to the code they test.
- **Subprocess-spawn pattern** — `pbcopy` invocation already follows the spawn-blocking-but-fire-and-forget pattern (`src/app.rs:437-446`). For the cmd+c key path, no additional async needed.

### Integration Points
- **Keymap dispatch** (`src/keys.rs` + `src/events.rs::handle_key`) — `cmd+c` (`KeyCode::Char('c')` + `KeyModifiers::SUPER`) needs a new branch. Must check `app.selection.is_some()` first; if yes, call `copy_selection_to_clipboard`; if no, forward `0x03` to PTY in Terminal mode (or route through existing keymap logic in Normal mode).
- **`Esc` precedence** — Currently in `events.rs::key_to_bytes` (line ~650), `KeyCode::Esc => Some(vec![0x1b])`. New precedence: in `handle_key`, check `app.selection.is_some()` BEFORE falling through to `key_to_bytes`. If selection active → clear and consume; else → existing path.
- **PTY drain → scroll_generation** — wherever `parser.process(bytes)` is called (likely `src/pty/session.rs` or `src/pty/manager.rs` drain loop), wrap with a before/after diff to detect scrolls. Increment `app.scroll_generation` (or session-local generation) and call `mark_dirty()`.
- **Tab switch / workspace switch** — find existing `set_active_tab` and `select_active_workspace` (likely in `src/workspace.rs` or `src/app.rs`). Add `app.selection = None` at entry.
- **Render highlight** — `src/ui/terminal.rs:156-177`. Replace the gold-accent recolor with per-cell fg↔bg swap. Read `buf.cell(...)` for source cell, write inverted style to `buf.cell_mut(...)`. XOR `Modifier::REVERSED` on already-reversed cells.

</code_context>

<specifics>
## Specific Ideas

- **Reference baseline:** Ghostty. The user explicitly compares feel against Ghostty/Alacritty (PROJECT.md core value). When in doubt about UX, ask "what does Ghostty do?"
- **Inverted highlight is a deliberate change** from the existing gold-accent — user picked it to match terminal-standard / iTerm / Terminal.app / Ghostty. The gold-accent existed only as a Phase 1 placeholder.
- **Auto-copy on mouse-up is the primary path.** `cmd+c` is a redundant but required-by-spec second path. Don't remove the auto-copy in favor of cmd+c-only.
- **Multi-click and shift-click are bundled into Phase 6** even though they go beyond the minimal SEL-01..04 reading. User wants full Ghostty parity in this phase, not a minimum-viable cut.

</specifics>

<deferred>
## Deferred Ideas

- **Mouse-mode-aware drag forwarding** — option to let vim/htop receive Left-drag bytes when they have requested mouse events. Rejected for Phase 6 in favor of "martins always wins". Could be a future v2 setting (`forward_mouse_to_pty: bool`) if a user complains.
- **Modifier-based mouse override** (Option-drag forwards to PTY) — same idea, rejected in Phase 6 for simplicity.
- **Visual flash on cmd+c re-copy** — UX polish to confirm the action. Not requested, not required, no plan.
- **Scrollback search / buffer query** — already deferred to v2 (REQUIREMENTS.md SCR-01/SCR-02).
- **Tracing spans around selection events** — would help diagnose feel regressions, but OBS-01 is already deferred to v2.
- **Word boundary as user-configurable predicate** — for non-ASCII or domain-specific (URLs, paths). Not in scope; ship a sensible default.
- **Keep selection across tab switch** — rejected; per-session generation makes cross-session selection meaningless.

</deferred>

---

*Phase: 06-text-selection*
*Context gathered: 2026-04-24*
