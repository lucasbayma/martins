# Phase 7: tmux-native main-screen selection - Context

**Gathered:** 2026-04-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Migrate text selection in the main PTY pane from Martins' REVERSED-XOR overlay (built in Phase 6) to the underlying tmux session's native copy-mode, so selection feels indistinguishable from running tmux directly. The overlay stays as fallback for sessions where the inner program has requested mouse mode (vim with `mouse=a`, htop, btop, etc.) — tmux copy-mode would conflict with those.

Operator-flagged during Phase 6 UAT 2026-04-25 — current overlay works (SEL-01..04 passed) but feels non-native vs tmux's own selection.

Requirements: TBD (no REQ-IDs allocated — this is a polish/feel iteration on the already-validated SEL-01..04 surface, not a new requirement).

</domain>

<decisions>
## Implementation Decisions

### Trigger Scope (when does tmux own selection?)
- **D-01:** Tmux owns selection in the main pane **only when the inner program has not requested mouse mode** (i.e., not currently running vim `mouse=a`, htop, btop, etc.). When the inner program has requested mouse mode, the Phase 6 overlay path runs instead.
- **D-02:** Maintain a per-session `mouse_requested: bool` flag on `PtySession`. The PTY drain loop watches output bytes for the mode-set sequences `\x1b[?1000h`, `\x1b[?1002h`, `\x1b[?1003h`, `\x1b[?1006h` (set → flag = true) and `\x1b[?1000l`, `\x1b[?1002l`, `\x1b[?1003l`, `\x1b[?1006l` (reset → flag = false). Sourced from the drain, not from render or per-frame polling.
- **D-03:** Do NOT query tmux on demand (`display-message -p '#{mouse_any_flag}'`) — subprocess on every drag-start risks reintroducing the lag this milestone fights.

### Mouse Delegation Model (how does Martins drive tmux when delegating?)
- **D-04:** When delegating, Martins forwards raw SGR mouse events to the tmux client's PTY: `\x1b[<0;col;rowM` on press, `\x1b[<0;col;rowM` on drag-move (with appropriate button-mask byte), `\x1b[<0;col;rowm` on release. Tmux's `mouse on` handles copy-mode entry, drag highlight, and selection finalize natively — same code path as running tmux directly.
- **D-05:** Do NOT drive tmux via `tmux send-keys -X` control commands (begin-selection, cursor-up, etc.) for drag tracking. One subprocess per mouse-move event would tank latency.
- **D-06:** Coords already match — Martins owns the pane size and the tmux client runs at that exact size. Forwarded SGR coords map 1:1 onto tmux's view.

### Drag Interception (cross-cutting with Phase 6 D-10)
- **D-07:** Phase 6 D-10 ("always intercept Drag(Left), never forward") is replaced by **conditional intercept**: when `mouse_requested == false`, forward `Down(Left)` / `Drag(Left)` / `Up(Left)` as SGR sequences to the tmux PTY and skip Martins' SelectionState mutation. When `mouse_requested == true`, fall back to Phase 6 behavior (intercept and use overlay).
- **D-08:** This is Phase 7's core wiring change in `src/events.rs:46+` (handle_mouse Drag/Up/Down branches).

### cmd+c → Clipboard Wiring
- **D-09:** Add a tmux.conf binding to martins' generated `~/.martins/tmux.conf`:
  ```
  bind-key -T copy-mode-vi MouseDragEnd1Pane send -X copy-pipe-and-cancel "pbcopy"
  bind-key -T copy-mode-vi y send -X copy-pipe-and-cancel "pbcopy"
  bind-key -T copy-mode Enter send -X copy-pipe-and-cancel "pbcopy"
  ```
  The drag-end is auto-piped to `pbcopy` — selection is on the macOS clipboard the moment the user releases the mouse, no extra hop.
- **D-10:** `cmd+c` while a tmux selection is active re-copies via `tmux save-buffer - | pbcopy` (read most recent buffer). Both the auto-on-release path and cmd+c re-copy converge on the macOS clipboard.
- **D-11:** Phase 6 D-03 holds: `cmd+c` with **no** selection (in either path — overlay empty AND tmux buffer empty/absent) forwards `0x03` (SIGINT) to the active PTY. Macros: check `App::selection.is_some()` first (overlay path), else check `tmux list-buffers -t <session>` returns non-empty (tmux path), else SIGINT.

### Existing Overlay Fate (Phase 6 primitives)
- **D-12:** **Keep all Phase 6 overlay primitives** as the alt-screen / mouse-app fallback path. Specifically: `SelectionState`, `scroll_generation` anchoring, REVERSED-XOR render in `src/ui/terminal.rs:156-177`, double/triple-click word/line, shift-click extend. None of this code is deleted — it runs end-to-end whenever `mouse_requested == true`.
- **D-13:** When `mouse_requested == false`, the overlay sleeps: no `SelectionState` mutation, no XOR render. Tmux owns the visual feedback on its own (its own native highlight rendered through the PTY).

### Clearing Semantics (Esc / click-outside)
- **D-14:** Phase 6 Esc-precedence (D-14 there) holds: if a selection is active in either path, Esc clears it; else Esc forwards to PTY. The "clear" action depends on the active path:
  - Overlay path active → clear `App::selection` (current Phase 6 behavior).
  - Tmux path active (tmux is in copy-mode, indicated by `#{pane_in_mode}` or by Martins tracking that we forwarded a press without a release-after-cancel) → forward Esc into the tmux PTY (or run `tmux send-keys -X cancel -t <session>`).
- **D-15:** Click-outside (`MouseEventKind::Down(Left)` not initiating a drag) similarly converges: clear overlay if active; else if tmux in copy-mode, send `cancel`. Phase 6 D-13 stays.

### Tab / Workspace Switch
- **D-16:** Phase 6 D-22 holds — selection clears on tab and workspace switch, in both paths. For the tmux path, the switch handler runs `tmux send-keys -X cancel -t <outgoing_session>` (gated on `#{pane_in_mode}` to avoid spurious sends when not in copy-mode). Selection state never crosses sessions.

### Word / Line Click Semantics in tmux Path
- **D-17:** Add tmux.conf bindings for native word/line selection:
  ```
  bind-key -T root DoubleClick1Pane select-pane \; copy-mode -M \; send -X select-word
  bind-key -T root TripleClick1Pane select-pane \; copy-mode -M \; send -X select-line
  ```
  Tmux owns click-counter timing and word boundaries in the tmux path. Phase 6's hand-rolled `(last_click_at, click_count)` only runs in the overlay path.
- **D-18:** Phase 6 D-19 (shift+click extend) — for tmux path, this lands as `\x1b[<4;col;rowM` (button=0 + shift=4 modifier in SGR). Tmux's copy-mode handles shift-click extend natively. Confirm in research.

### Claude's Discretion
- D-04 detail: exact SGR button-mask bytes for drag-move (button=32+0 for left-button-held) — researcher to verify against crossterm's `MouseEventKind::Drag` payload and tmux's expected wire format.
- D-09 detail: whether `MouseDragEnd1Pane` works in `copy-mode-vi` table or also needs a `copy-mode` table entry — tmux's keymode default depends on `mode-keys`; recommend explicit binding in both tables.
- D-14 detail: how Martins knows tmux is in copy-mode without polling. Two options: (a) track Martins' own state machine (we forwarded a press, tmux should be in copy-mode until we forward a cancel), or (b) cache `#{pane_in_mode}` on session and refresh on Esc/click. Researcher to recommend.
- Whether to bind `Escape` in `copy-mode-vi` to `cancel` explicitly (likely already default — verify).
- Whether to log copy events via tracing — recommend skip (consistent with Phase 6 D-09 stance).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project Spec
- `.planning/PROJECT.md` — Core value (Ghostty/Alacritty feel parity), constraints (macOS-only, ratatui/crossterm/tokio stack, tmux as session backend)
- `.planning/REQUIREMENTS.md` §Text Selection — SEL-01..SEL-04 already validated in Phase 6; Phase 7 is feel-iteration on the same surface, no new REQ-IDs
- `.planning/ROADMAP.md` §Phase 7 — Goal + investigation surface

### Phase 6 Artifacts (the surface Phase 7 modifies)
- `.planning/phases/06-text-selection/06-CONTEXT.md` — D-01..D-23, especially D-10 (always-intercept Drag(Left), now being replaced by D-07 here)
- `.planning/phases/06-text-selection/06-RESEARCH.md` — vt100 / crossterm / pbcopy investigation, scroll_generation anchoring math
- `.planning/phases/06-text-selection/06-HUMAN-UAT.md` §Forward-Looking Notes — operator preference that drove this phase
- `.planning/phases/06-text-selection/06-PATTERNS.md` — file-modification analogs

### Existing Code (modification surface)
- `src/tmux.rs:24-41` — `ensure_config()` writes `~/.martins/tmux.conf`. Phase 7 adds copy-pipe + DoubleClick/TripleClick bindings here.
- `src/tmux.rs:43-60` — `enforce_session_options` (already sets `mouse on`, `alternate-screen off`, `allow-passthrough off` per session — these still apply)
- `src/tmux.rs:146-152` — `send_key` helper (may extend with `send_x_command` for `copy-mode -M`, `cancel`, etc.)
- `src/tmux.rs:134-144` — `pane_command` (Phase 7 may add `pane_in_mode` query helper)
- `src/events.rs:38-88` — `handle_mouse` Drag/Up/Down branches. **Core wiring change:** conditional intercept based on `mouse_requested` flag (D-07).
- `src/events.rs:625-660` (approx) — `key_to_bytes` / cmd+c handling. Extend Phase 6 D-03 path to also check tmux buffer (D-11).
- `src/pty/session.rs` — Add `mouse_requested: bool` field on `PtySession`; PTY drain loop watches mode-set/reset sequences (D-02).
- `src/ui/terminal.rs:156-177` — Phase 6 highlight render. **Untouched in tmux path** — tmux's own highlight renders through the PTY. Stays active in overlay path.
- `src/app.rs:418-447` — `App::copy_selection_to_clipboard`. Extend with a tmux-buffer fallback for cmd+c re-copy (D-10).

### Codebase Maps
- `.planning/codebase/INTEGRATIONS.md` §tmux — subprocess wrapping, `~/.martins/tmux.conf`, session-per-tab model
- `.planning/codebase/STACK.md:33,57,101,122` — tmux as external binary, `which = "6"` for availability check, generated tmux.conf
- `.planning/codebase/STRUCTURE.md:22,33,75,126,202` — `src/tmux.rs` and `src/ui/terminal.rs` placement
- `.planning/codebase/ARCHITECTURE.md:40-42` — PTY/tmux module boundary
- `.planning/codebase/CONCERNS.md` — known perf concerns (no per-event subprocesses on hot paths)
- `.planning/codebase/TESTING.md` — inline `#[cfg(test)]` pattern; precedent: `src/pty_input_tests.rs` (Phase 3) and Phase 6's selection tests in `src/events.rs` / `src/app.rs`

### External Reference
- tmux man page (`tmux(1)`) §COPY MODE, §MOUSE SUPPORT — `bind-key -T copy-mode-vi`, `MouseDragEnd1Pane`, `DoubleClick1Pane`, `select-word`, `select-line`, `copy-pipe-and-cancel`, `cancel`, `#{pane_in_mode}`
- xterm SGR mouse-mode spec: DECSET 1006 (`\x1b[?1006h`), button-mask + modifier-mask byte format (`\x1b[<{button};{col};{row}{M|m}`)
- Phase 2 `App::mark_dirty` pattern (carried forward from Phase 6 D-23) — selection mutations in either path call mark_dirty

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **Phase 6 overlay end-to-end** — `SelectionState`, `scroll_generation`, REVERSED-XOR render, double/triple-click counter, shift-click extend, `App::copy_selection_to_clipboard`. None of this code is deleted in Phase 7; it runs as the fallback path.
- **`src/tmux.rs::ensure_config`** — already generates `~/.martins/tmux.conf` on every `new_session`. Phase 7 extends the config string with copy-pipe + DoubleClick/TripleClick + Esc bindings. No new file, no new write site.
- **`src/tmux.rs::send_key`** and `pane_command` — patterns for shelling out to tmux for control operations. Add `send -X` and `display-message -p` analogues if needed.
- **PTY drain loop** in `src/pty/session.rs` — already byte-scans output for vt100 parser feed. Adding mode-set/reset detection is a small extension.

### Established Patterns
- Mode-set/reset detection follows the same byte-watching style the vt100 parser uses internally. Detection is done at the byte level before/after vt100 advance, not by reading vt100 state.
- Subprocess invocations to tmux are kept off the per-event hot path (Phase 5 lesson). Pattern: bind in tmux.conf rather than `tmux send-keys` per event.
- All selection-mutating events call `App::mark_dirty()` (Phase 2 / Phase 6 D-23). Same applies for tmux-path selection start/end — though tmux's own redraw via PTY output already triggers dirty.

### Integration Points
- `src/events.rs::handle_mouse` Drag/Up/Down branch — split into overlay path (existing) and tmux-forward path (new), gated on `pty_session.mouse_requested`.
- `src/events.rs::key_to_bytes` cmd+c — extend the no-overlay-selection branch to check tmux buffer before SIGINT.
- `src/events.rs` Esc / click-outside clearing — extend to issue `tmux send-keys -X cancel` if tmux path is active.
- Tab/workspace switch handlers (where Phase 6 D-22 already clears `App::selection`) — extend to call `cancel` on outgoing tmux session if it's in copy-mode.

</code_context>

<specifics>
## Specific Ideas

- The qualitative target: PTY-pane selection should feel **identical to running tmux directly** in Ghostty. The user runs both daily and the difference is what flagged this in Phase 6 UAT.
- The auto-copy on mouse-up via `copy-pipe-and-cancel` is the headline behavior — selection lives on the macOS clipboard the moment the drag ends, no separate cmd+c needed (though cmd+c re-copies the latest buffer per D-10).
- Don't reinvent click-counter timing in tmux mode — D-17 explicitly defers to tmux's native double/triple-click bindings.

</specifics>

<deferred>
## Deferred Ideas

- Right-click context menu (paste, copy as plain/rich, etc.) — not in scope; macOS terminal users don't expect it from tmux-native UX.
- Block/rectangle selection (Alt+drag in tmux) — could fall out for free from D-04 (forwarding raw SGR with Alt modifier byte). Confirm in research; if free, document as bonus; if not, defer.
- Search-in-scrollback (`?` in tmux copy-mode) — out of scope; the Roadmap explicitly defers scrollback search (PROJECT.md "Out of Scope").
- Customizing tmux's own highlight color to match Martins' theme — defer; tmux uses xterm-256 reverse-video by default which is acceptable.

</deferred>

---

*Phase: 07-tmux-native-main-screen-selection*
*Context gathered: 2026-04-25*
