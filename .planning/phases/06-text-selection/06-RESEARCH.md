# Phase 6: Text Selection — Research

**Researched:** 2026-04-24
**Domain:** TUI mouse-driven text selection + clipboard integration on macOS (ratatui 0.30 / crossterm 0.29 / vt100 0.16.2 / tui-term 0.3.4)
**Confidence:** HIGH on API shapes (all crate source read directly from `~/.cargo/registry`); MEDIUM on cmd+c delivery path (terminal-emulator dependent — see Q5).

## Research Summary

CONTEXT.md has locked the UX (drag + cmd+c + Esc + multi-click + shift-click, inverted-cell highlight, anchored generation for scroll stability). This research verifies the exact API surface for each decision against the crate versions in `Cargo.toml`. Three findings drive plan shape:

1. **vt100 0.16.2 exposes NO public scroll counter.** `Screen::scrollback()` returns the user's *viewing* offset, not a total-rows-scrolled-off counter. The `Callbacks` trait has hooks for bell/resize/title but no scroll callback. Incrementing `scroll_generation` must be done by observing the `vt100::Parser::process` return from the PTY reader thread and detecting pushes to the internal scrollback — which is not directly observable either. **Recommended approach:** wrap `parser.process(bytes)` in the PTY reader thread (`src/pty/session.rs:92`) with a before/after comparison of `screen.contents()` row count changes, or more precisely, track the cursor_position `.0` (row) overflow behavior combined with cursor-movement heuristics. See §vt100 Scroll for the concrete pattern.

2. **`Screen::contents_between` iterates `visible_rows()` only — it does NOT reach scrollback.** Once a row scrolls off the top, its text is lost from any `contents_between(sr, ..., er, ...)` call where `sr` is negative-in-visible-space. This means D-08's "keep SelectionState alive with anchored gen" works for clipping the *rendered highlight* but the *copyable text* must be materialized earlier — the plan should either (a) snapshot the selection's text on mouse-up into `SelectionState.text: Option<String>` and copy from that snapshot, or (b) accept that if the user drags a selection, then lets it scroll off, then presses cmd+c, they get only the visible remnant. CONTEXT.md D-02 says "re-copies the same selection"; this combined with D-08 strongly suggests option (a).

3. **`KeyModifiers::SUPER` is NOT delivered on macOS without the kitty keyboard protocol.** Martins does not push `KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES` at startup (`src/main.rs:70` pushes only `EnableMouseCapture` + `EnableBracketedPaste`). On Terminal.app, iTerm2, and Ghostty in their default configs, cmd+c is consumed by the terminal emulator itself as "copy selected text from terminal scrollback to system clipboard" and never reaches the TUI. This is a **blocking ambiguity for SEL-02**. The plan must either (a) push `DISAMBIGUATE_ESCAPE_CODES` and accept it only works on kitty/WezTerm/Alacritty/foot, (b) document that SEL-02 "cmd+c" is actually the terminal emulator's native Copy binding (which on macOS reads the terminal's visible text — NOT Martins's `SelectionState`), or (c) since auto-copy on mouse-up (D-01) already satisfies "put text on clipboard" via pbcopy, deprioritize the cmd+c re-copy path and treat it as best-effort.

**Primary recommendation:** adopt option (a) + snapshot-on-mouse-up from finding #2 + a best-effort diffing approach for scroll_generation from finding #1. See §Open Questions for the cmd+c decision point the discuss-phase must close.

## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Auto-copy on Left-mouse-up (existing `events.rs:64-72` behavior preserved)
- **D-02:** `cmd+c` with active selection re-copies same text; both paths call `App::copy_selection_to_clipboard`
- **D-03:** `cmd+c` with no selection forwards `0x03` (SIGINT) to active PTY in Terminal mode
- **D-04:** Successful copy does NOT clear the highlight
- **D-05:** Add `scroll_generation: u64` (App or PtySession); increment from PTY drain loop
- **D-06:** `SelectionState` endpoints anchored as `(gen, screen_row, col)`
- **D-07:** Mid-drag: start anchored at first Drag event; end cursor-relative until Up, then anchored
- **D-08:** Clip highlight at visible top when anchored row scrolls off; keep SelectionState in state
- **D-10:** Always intercept Drag(Left); never forward to PTY (vim mouse-visual sacrificed)
- **D-11:** Scroll events continue forwarding as today
- **D-12:** Down(Left) clears selection then routes to handle_click
- **D-13:** Any Left-mouse-down clears highlight
- **D-14:** `Esc` clears selection IFF `app.selection.is_some()`; else falls through
- **D-15:** In scope: drag + double-click-word + triple-click-line + shift-click-extend-end
- **D-16:** 300ms click threshold; reset counter if click region differs
- **D-18:** Triple-click line = visible vt100 row (wrapped lines NOT joined)
- **D-19:** Shift-click only extends END; no-op if no selection exists
- **D-20:** Inverted-cell highlight (swap fg↔bg from existing cell); fallback ACCENT_GOLD if no fg/bg
- **D-21:** XOR `Modifier::REVERSED` on cells that already have it (distinct from vt100 reverse-video)
- **D-22:** Clear selection on tab switch AND workspace switch
- **D-23:** Every selection mutation calls `App::mark_dirty()`

### Claude's Discretion
- **D-09:** vt100 scroll-counter sourcing — researcher resolves below (§vt100 Scroll)
- **D-17:** Word boundary predicate — researcher resolves below (§Unicode)
- Visual flash on cmd+c — recommend skip
- Tracing spans for copy events — recommend skip (OBS-01 deferred to v2)

### Deferred Ideas (OUT OF SCOPE)
- Mouse-mode-aware drag forwarding (vim mouse-visual override)
- Modifier-based drag override (Option-drag to PTY)
- Visual flash on cmd+c
- Scrollback search / buffer query (SCR-01/SCR-02 v2)
- Tracing spans around selection events
- User-configurable word boundary predicate
- Keep selection across tab switch

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| SEL-01 | Drag on PTY starts selection with visible highlight tracking cursor with no lag | §ratatui Buffer Cell API (inverted-cell render), §crossterm Mouse (Drag detection); existing skeleton at `events.rs:38-88` + `terminal.rs:156-177` |
| SEL-02 | `cmd+c` while active selection copies via pbcopy | §crossterm Mouse + Key API (cmd+c delivery caveat), §Open Questions (cmd+c resolution) — SEL-02 may require `PushKeyboardEnhancementFlags` |
| SEL-03 | Click (or Esc) outside clears highlight immediately | §Esc Precedence, existing `events.rs:78-83` Down(Left) clear |
| SEL-04 | Selection stable while PTY streams output | §vt100 Scroll (anchored generation approach), §Runtime State |

## vt100 Scroll & Screen API (Q1, Q2)

**Crate version:** `vt100 = "0.16"` in `Cargo.toml` → resolved to `0.16.2` in `~/.cargo/registry/src/.../vt100-0.16.2/`.

### Q1: Scroll counter sourcing (D-09)

**Finding (HIGH confidence, source read directly):** vt100 0.16.2 does NOT expose a public "total rows scrolled off" counter. The three scroll-adjacent public APIs on `Screen` are:

```rust
// src/screen.rs:121
pub fn set_scrollback(&mut self, rows: usize);

// src/screen.rs:122-124
pub fn scrollback(&self) -> usize;  // viewer offset from top of visible screen; 0 = normal

// src/screen.rs:489-492
pub fn cursor_position(&self) -> (u16, u16);  // (row, col) within visible area
```

`screen.scrollback()` returns the **viewer's** offset when they've scrolled back through scrollback (0 means "looking at live content"). It does NOT tell you how many rows have been pushed off the top.

**Callbacks trait** (`src/callbacks.rs`): hooks for `audible_bell`, `visual_bell`, `resize`, `set_window_icon_name`, `set_window_title`, `copy_to_clipboard`, `paste_from_clipboard`, `unhandled_char`, `unhandled_control`, `unhandled_escape`, `unhandled_csi`, `unhandled_osc`. **No scroll callback.**

**Internal scroll mechanism** (`src/grid.rs:561-577`, read for understanding, NOT reachable via public API):
```rust
pub fn scroll_up(&mut self, count: u16) {
    for _ in 0..(count.min(self.size.rows - self.scroll_top)) {
        self.rows.insert(usize::from(self.scroll_bottom) + 1, self.new_row());
        let removed = self.rows.remove(usize::from(self.scroll_top));
        if self.scrollback_len > 0 && !self.scroll_region_active() {
            self.scrollback.push_back(removed);
            while self.scrollback.len() > self.scrollback_len {
                self.scrollback.pop_front();
            }
            if self.scrollback_offset > 0 {
                self.scrollback_offset =
                    self.scrollback.len().min(self.scrollback_offset + 1);
            }
        }
    }
}
```

A row pushed onto the scrollback is private state (`grid.scrollback: VecDeque<Row>` — not `pub`).

**Recommended approach (SCREEN-DIFF heuristic):** In the PTY reader thread (`src/pty/session.rs:92`, just after `parser.process(&buf[..n])`), compare `screen.contents()` hash or row-wise before/after. But `contents()` allocates a String every time — expensive.

**Better approach (CURSOR-ROW heuristic):** After each `parser.process(bytes)`, read `screen.cursor_position()`. If the cursor was previously at row `size.rows - 1` (last visible row) and the process emits `\n` or enough chars to overflow a wrap, the grid's `row_inc_scroll` at `grid.rs:621-631` will call `scroll_up(lines)` and return the number of scrolled lines — but this return value is internal. Still, we can detect scroll by: cursor_position.row stays pinned at `size.rows - 1` after the write that *would have* advanced it, and the *content of that row changes*. The cleanest observable proxy: track cursor_position.row BEFORE the process call; if cursor was at bottom AND the bytes contain `\n`/`\r\n`/text past EOL, assume scroll. This is imprecise.

**Recommended (SCROLLBACK-LEN heuristic) — BEST:** Even though `grid.scrollback: VecDeque<Row>` is private, we can infer scroll events indirectly. Actually the cleanest: **take advantage of the fact that when scroll happens, `screen.contents()` on row 0 changes** while `cursor_position()` stays at `(rows-1, col)`. But this requires a diff.

**Pragmatic recommendation for Phase 6:** Since vt100 exposes no clean counter, wrap `parser.process(bytes)` with a before-and-after comparison of the **bottom-row cursor state + first-visible-row text hash**:

```rust
// src/pty/session.rs reader thread, replacing line 92:
if let Ok(mut parser) = parser_clone.write() {
    let before_top_hash = row_hash(parser.screen(), 0);
    let before_rows = parser.screen().size().0;
    let before_cursor_row = parser.screen().cursor_position().0;
    parser.process(&buf[..n]);
    let after_top_hash = row_hash(parser.screen(), 0);
    // Heuristic: if cursor was pinned at bottom AND the top row text
    // changed, at least one row scrolled off.
    let scroll_likely = before_cursor_row >= before_rows.saturating_sub(1)
        && before_top_hash != after_top_hash;
    if scroll_likely {
        scroll_generation.fetch_add(1, Ordering::Relaxed);
    }
}

fn row_hash(screen: &vt100::Screen, row: u16) -> u64 {
    // Cheap per-row hash: iterate visible row cells via screen.cell(row, col)
    // and hash their .contents() bytes. Row width is bounded by cols (<= 500).
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let (_, cols) = screen.size();
    for col in 0..cols {
        if let Some(cell) = screen.cell(row, col) {
            cell.contents().hash(&mut h);
        }
    }
    h.finish()
}
```

**False positives:** programs that rewrite row 0 without scrolling (e.g., `\x1b[H` cursor home + new text). These are rare in normal command output. The cost of a false positive is the selection highlight visually shifts by 1 cell for one frame — acceptable for v1.

**False negatives:** a scroll that happens to reproduce the old row 0 text byte-for-byte. Effectively impossible with real command output.

**Alternative considered (REJECTED):** fork vt100 and add a scroll counter. Out of scope — CONTEXT.md deferred no such change.

**Alternative considered (REJECTED):** track own row count by parsing bytes before `parser.process`. This would require parsing ANSI sequences ourselves — exactly what vt100 exists to avoid.

**Increment source:** The PTY reader thread at `src/pty/session.rs:92` is the correct place. The counter should be an `Arc<AtomicU64>` shared between the reader thread and `App`. Add to `PtySession` (not `App`) because it's per-session — matches CONTEXT.md D-05 "App or PtySession" choice.

**MUST NOT** increment from the render path (`ui/terminal.rs`) — by the time render runs, many scrolls may have happened.

### Q2: `contents_between` behavior across scrollback (D-08)

**Finding (HIGH confidence, source `screen.rs:167-217`):**

```rust
pub fn contents_between(
    &self,
    start_row: u16,
    start_col: u16,
    end_row: u16,
    end_col: u16,
) -> String {
    // Iterates self.grid().visible_rows().enumerate().skip(start_row).take(end_row - start_row + 1)
}
```

`visible_rows()` (`grid.rs:126-144`) is defined as:
```rust
pub fn visible_rows(&self) -> impl Iterator<Item = &crate::row::Row> {
    let scrollback_len = self.scrollback.len();
    let rows_len = self.rows.len();
    self.scrollback
        .iter()
        .skip(scrollback_len - self.scrollback_offset)
        .take(rows_len)
        .chain(
            self.rows
                .iter()
                .take(rows_len.saturating_sub(self.scrollback_offset)),
        )
}
```

When `scrollback_offset == 0` (user not scrolling back), `visible_rows()` returns just `self.rows` (the live visible area). **Rows scrolled off the top are NOT reachable.**

**Implication for D-08:** Once a selection's start row scrolls off, `contents_between(old_start_row, ..., end_row, ...)` will return only the portion still visible. The D-08 decision "render only the portion still on-screen … but keep `SelectionState` in app state so the next `cmd+c` still has text to copy" is **only partially achievable**. Specifically: the clipped-rendered portion will match the copied portion, which is acceptable for SEL-04's stability claim — "the highlight stays put until the user clears it" — but the text copied after partial scroll-off will be less than the user originally selected.

**Concrete implication for the plan:** add a `text: Option<String>` field to `SelectionState`, populated at mouse-up with the full `contents_between(start_row, ..., end_row, ...)` captured at that moment. cmd+c uses this snapshot instead of re-querying the screen. On each subsequent scroll, the *render* path re-clips what's visible, but the *copied text* is the snapshot.

Alternative: re-capture text on every render frame while selection is still fully visible, and freeze it once any row has scrolled off. Simpler to just snapshot once at mouse-up (D-01 auto-copy already calls `contents_between` at mouse-up; extend that to store the result).

**Edge case (row == row, start_col >= end_col):** the `Equal` branch returns empty string (`screen.rs:206-214`). The existing code at `src/app.rs:430` passes `ec.saturating_add(1)` — inclusive end. Correct.

**Edge case (start_row > end_row):** `Greater` branch returns empty string. Existing `normalized()` prevents this.

### Q6: Mouse-mode handshake (`\x1b[?1000h` / `\x1b[?1006h`)

**Finding (HIGH confidence, source `screen.rs:1138-1213`):** vt100 DOES track mouse mode internally. `Screen::mouse_protocol_mode()` and `Screen::mouse_protocol_encoding()` are public:

```rust
// src/screen.rs:577-585
pub fn mouse_protocol_mode(&self) -> MouseProtocolMode;
pub fn mouse_protocol_encoding(&self) -> MouseProtocolEncoding;
```

`MouseProtocolMode` variants (`src/screen.rs:11-36`): `None`, `Press`, `PressRelease`, `ButtonMotion`, `AnyMotion`.

**Implication for D-10:** Since CONTEXT.md decided "always intercept Drag(Left)", we do NOT need to read `mouse_protocol_mode()` before deciding to intercept. The decision is unconditional. However, we MAY want to expose this later as a `deferred idea` signal (see CONTEXT.md deferred "Mouse-mode-aware drag forwarding"). For v1 Phase 6, ignore entirely — the interception in `src/events.rs:46-62` already does the right thing.

## Unicode / Wide-Char Handling (Q3)

**Crate:** `vt100 = "0.16.2"` (`cell.rs` + `row.rs`).

### Cell model

**Finding (HIGH confidence, source `vt100-0.16.2/src/cell.rs:1-17`):**
```rust
const CONTENT_BYTES: usize = 22;
const IS_WIDE: u8 = 0b1000_0000;
const IS_WIDE_CONTINUATION: u8 = 0b0100_0000;
const LEN_BITS: u8 = 0b0001_1111;

pub struct Cell {
    contents: [u8; 22],
    len: u8,                   // packed: IS_WIDE | IS_WIDE_CONTINUATION | LEN_BITS
    attrs: crate::attrs::Attrs,
}
const _: () = assert!(std::mem::size_of::<Cell>() == 32);
```

Public methods used for word-boundary:
```rust
pub fn contents(&self) -> &str;          // cell.rs:89
pub fn has_contents(&self) -> bool;      // cell.rs:95
pub fn is_wide(&self) -> bool;           // cell.rs:101
pub fn is_wide_continuation(&self) -> bool;  // cell.rs:109
pub fn fgcolor(&self) -> crate::Color;   // cell.rs:135
pub fn bgcolor(&self) -> crate::Color;   // cell.rs:141
pub fn bold/dim/italic/underline/inverse(&self) -> bool;  // cell.rs:148+
```

Cell access from `Screen`:
```rust
// src/screen.rs:534
pub fn cell(&self, row: u16, col: u16) -> Option<&crate::Cell>;
```

### Wide-char layout (CJK, emoji)

From `src/screen.rs:705-942` (`Screen::text(c)` — how chars land in cells):

- A wide char (`unicode_width >= 2`) is stored at column N; column N+1 gets a `set_wide_continuation(true)` marker cell with no content.
- A zero-width char (combining mark) is `append`ed to the *previous* cell (the one with content).
- A single `char` can be a grapheme with up to `CONTENT_BYTES - 4 = 18` bytes of combining characters after it.

**Iteration pattern for word-boundary detection:**
```rust
fn word_boundary_at(screen: &vt100::Screen, row: u16, col: u16) -> (u16, u16) {
    let (_, cols) = screen.size();
    let is_word_char = |s: &str| -> bool {
        s.chars().next().is_some_and(|c| {
            !c.is_whitespace()
                && !matches!(c, '[' | ']' | '(' | ')' | '<' | '>' | '{' | '}'
                              | '.' | ',' | ';' | ':' | '!' | '?'
                              | '\'' | '"' | '`' | '/' | '\\' | '|'
                              | '@' | '#' | '$' | '%' | '^' | '&' | '*'
                              | '=' | '+' | '~')
        })
    };
    // skip wide-continuation cells when walking columns
    let next_col = |c: u16| -> u16 {
        let mut nc = c + 1;
        while nc < cols
            && screen.cell(row, nc).is_some_and(|c| c.is_wide_continuation())
        {
            nc += 1;
        }
        nc
    };
    let prev_col = |c: u16| -> u16 {
        if c == 0 { return 0; }
        let mut nc = c - 1;
        while nc > 0
            && screen.cell(row, nc).is_some_and(|c| c.is_wide_continuation())
        {
            nc -= 1;
        }
        nc
    };
    // walk left from col until we hit a non-word cell
    let mut start = col;
    while start > 0 {
        let prev = prev_col(start);
        let Some(cell) = screen.cell(row, prev) else { break };
        if !cell.has_contents() || !is_word_char(cell.contents()) { break; }
        start = prev;
    }
    // walk right until we hit a non-word cell
    let mut end = col;
    while end + 1 < cols {
        let next = next_col(end);
        let Some(cell) = screen.cell(row, next) else { break };
        if !cell.has_contents() || !is_word_char(cell.contents()) { break; }
        end = next;
    }
    (start, end)
}
```

**Rationale (D-17):**
- Word char predicate: not-whitespace AND not in the punctuation blacklist. Same set as Ghostty default (`[]()<>{}.,;:!?'"\`/\\|@#$%^&*=+~`). A CJK character or emoji is a word char (it fails both blacklist and whitespace tests).
- `is_wide_continuation()` cells are skipped during the left/right walk — the "real" cell is always the first half of a wide char.
- `contents_between(sr, sc, er, ec)` internally handles wide cells correctly: `write_contents` on `row.rs` only emits the content of the non-continuation cell, so the resulting string is byte-accurate UTF-8 without duplicated wide-char halves.

**Confirmation (source `screen.rs:148-158`):** `Screen::rows` already uses the row's `write_contents` with column range — wide chars are handled. `contents_between` reuses the same infrastructure.

## crossterm Mouse + Key API (Q4, Q5)

**Crate:** `crossterm = "0.29"` → resolved `0.29.0`.

### Q4: MouseEventKind variants (double/triple click)

**Finding (HIGH confidence, source `crossterm-0.29.0/src/event.rs:800-817`):**
```rust
pub enum MouseEventKind {
    Down(MouseButton),
    Up(MouseButton),
    Drag(MouseButton),
    Moved,
    ScrollDown,
    ScrollUp,
    ScrollLeft,
    ScrollRight,
}
```

**There is no `DoubleClick` or `TripleClick` variant.** crossterm delivers raw press/release events; click-counting is our responsibility.

**Implication for D-15, D-16:** We track `(last_click_at: Instant, click_count: u8, last_click_row: u16, last_click_col: u16)` on `App`. On each `MouseEventKind::Down(Left)` inside the terminal pane:
- If `now - last_click_at < 300ms` AND the click landed within the same word/row as `last_click_row/col`: increment `click_count`.
- Else: reset `click_count = 1`.
- Dispatch on count: 1 = normal click, 2 = select word, 3 = select line.

**Same-word/same-row check:** For click_count=2 trigger, the new click must be within the same word as the first (we can use `word_boundary_at` for comparison). For click_count=3 trigger, same row. If outside, reset.

### Q5: cmd+c key detection on macOS

**Finding (HIGH-to-MEDIUM confidence):**

**Source 1 (HIGH, `crossterm-0.29.0/src/event.rs:832-848`):**
```rust
bitflags! {
    /// **Note:** `SUPER`, `HYPER`, and `META` can only be read if
    /// [`KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES`] has been enabled with
    /// [`PushKeyboardEnhancementFlags`].
    pub struct KeyModifiers: u8 {
        const SHIFT   = 0b0000_0001;
        const CONTROL = 0b0000_0010;
        const ALT     = 0b0000_0100;
        const SUPER   = 0b0000_1000;
        const HYPER   = 0b0001_0000;
        const META    = 0b0010_0000;
        const NONE    = 0b0000_0000;
    }
}
```

**Source 2 (HIGH, `crossterm-0.29.0/src/event.rs:483-491`):** `PushKeyboardEnhancementFlags` is the kitty keyboard protocol. Supported by:
- kitty
- foot
- WezTerm
- alacritty
- Neovim (embedded terminal)
- Not listed: **iTerm2, Terminal.app, Ghostty** in their default configs

**Source 3 (`src/main.rs:70`):** Martins currently executes only `EnableMouseCapture, EnableBracketedPaste`. It does NOT push `DISAMBIGUATE_ESCAPE_CODES`.

**Implication for SEL-02:** On the target terminals (Ghostty/Alacritty for user's Ghostty-feel baseline; Terminal.app/iTerm2 for broader user base):
- Alacritty: supports kitty keyboard protocol — cmd+c arrives as `KeyCode::Char('c') + KeyModifiers::SUPER` IF we push the flag.
- Ghostty: depending on version/config may or may not honor the protocol. Ghostty's documented default binds cmd+c to the terminal emulator's internal "copy" action regardless — it typically is NOT forwarded to the child process.
- Terminal.app / iTerm2: do NOT support kitty keyboard protocol. cmd+c is always consumed by the terminal emulator itself (bound to "Copy").

**The cmd+c path on macOS has three tiers of reality:**
1. **Terminal.app / iTerm2 / Ghostty-default:** cmd+c is the terminal emulator's Copy. It copies the visible scrollback selection (NOT Martins's SelectionState). Martins never sees the event. ✗
2. **Alacritty + DISAMBIGUATE pushed:** cmd+c arrives as `KeyCode::Char('c') + KeyModifiers::SUPER`. ✓
3. **kitty/WezTerm + DISAMBIGUATE pushed:** same as tier 2. ✓

**Recommendation for the plan:** Push `PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)` in `src/main.rs:70` (paired with `PopKeyboardEnhancementFlags` at restore). On terminals that don't support it, the push is a no-op at the terminal level — no harm. On terminals that do, cmd+c becomes available.

**Also recommend:** auto-copy-on-mouse-up (D-01) is the reliable path on ALL terminals. The discuss-phase already prioritized this (D-01 called out as "primary path"). SEL-02 literally says "cmd+c while selection active copies to clipboard" — auto-copy-on-mouse-up already satisfies "copies to clipboard"; cmd+c is the secondary muscle-memory path. If cmd+c doesn't reach Martins (tier 1 above), the user still has the text on their clipboard from the mouse-up auto-copy.

**Handler pattern in `src/events.rs::handle_key`:**
```rust
// Must be checked BEFORE the Terminal-mode forwarding branch at line 288.
// Match ONLY when the flag pushed successfully AND the user presses cmd+c.
if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::SUPER) {
    if app.selection.is_some() {
        app.copy_selection_to_clipboard();   // D-02
    } else if app.mode == InputMode::Terminal {
        app.write_active_tab_input(&[0x03]); // D-03: SIGINT
    }
    return;
}
```

### Existing code already handles `KeyModifiers::SUPER`?

**Finding (grep on `src/`):** No existing code uses `KeyModifiers::SUPER`. The only `SUPER` reference is the bitflag definition in the crossterm source. Ctrl+C is handled in the keymap (`src/keys.rs:120`), not as a PTY-forwarded intercept.

## Mouse-Mode Handshake (Q6)

Answered in §vt100 Scroll & Screen API Q6 above. Short answer: vt100 DOES track mouse protocol mode via `Screen::mouse_protocol_mode()`. For Phase 6, we don't need to read it — D-10 says "always intercept Drag(Left)".

## ratatui Buffer Cell API (Q7)

**Crate:** `ratatui = "0.30"` → `ratatui-0.30.0` is a meta-crate re-exporting `ratatui-core = "0.1.0"` (via `ratatui-0.30.0/src/lib.rs:432: pub use ratatui_core::{buffer, layout};`).

### Buffer cell access (`ratatui-core-0.1.0/src/buffer/buffer.rs`)

```rust
// lines 130, 150 — deprecated in 0.30 but still present
#[deprecated = "use `Buffer[(x, y)]` instead. To avoid panicking, use `Buffer::cell((x, y))`."]
pub fn get(&self, x: u16, y: u16) -> &Cell;
pub fn get_mut(&mut self, x: u16, y: u16) -> &mut Cell;

// lines 178, 209 — preferred 0.30 API
pub fn cell<P: Into<Position>>(&self, position: P) -> Option<&Cell>;
pub fn cell_mut<P: Into<Position>>(&mut self, position: P) -> Option<&mut Cell>;
```

**The existing code at `src/ui/terminal.rs:170` already uses the correct API:**
```rust
if let Some(cell) = buf.cell_mut((inner.x + col, inner.y + row)) {
    cell.set_bg(theme::ACCENT_GOLD);
    cell.set_fg(theme::BG_SURFACE);
}
```

### Cell API (`ratatui-core-0.1.0/src/buffer/cell.rs:9-47`)

```rust
pub struct Cell {
    symbol: Option<CompactString>,   // private, use symbol() / set_symbol()
    pub fg: Color,                   // PUBLIC FIELD — can read directly
    pub bg: Color,                   // PUBLIC FIELD
    pub modifier: Modifier,          // PUBLIC FIELD — bitflags, REVERSED lives here
    pub skip: bool,
}

// lines 138, 144, 153
pub const fn set_fg(&mut self, color: Color) -> &mut Self;
pub const fn set_bg(&mut self, color: Color) -> &mut Self;
pub fn set_style<S: Into<Style>>(&mut self, style: S) -> &mut Self;
pub const fn style(&self) -> Style;  // returns Style { fg, bg, add_modifier, sub_modifier }
```

### Modifier flags (`ratatui-core-0.1.0/src/style.rs:105-113`)

```rust
bitflags! {
    pub struct Modifier: u16 {
        const BOLD       = 0b0000_0000_0001;
        const DIM        = 0b0000_0000_0010;
        const ITALIC     = 0b0000_0000_0100;
        const UNDERLINED = 0b0000_0000_1000;
        const SLOW_BLINK = 0b0000_0001_0000;
        const RAPID_BLINK= 0b0000_0010_0000;
        const REVERSED   = 0b0000_0100_0000;
        const HIDDEN     = 0b0000_1000_0000;
        const CROSSED_OUT= 0b0001_0000_0000;
    }
}
```

### Inverted-cell highlight implementation (D-20, D-21)

The vt100 cell's fg/bg are already translated into ratatui `Cell.fg` / `Cell.bg` by `tui-term-0.3.4/src/vt100_imp.rs:64`:
```rust
buf_cell.set_style(Style::reset().fg(fg).bg(bg).add_modifier(modifier));
```

And `tui-term-0.3.4/src/vt100_imp.rs:54-56` already translates vt100 `inverse()` to `Modifier::REVERSED`:
```rust
if screen_cell.inverse() {
    modifier |= Modifier::REVERSED;
}
```

So by the time our selection-highlight pass runs (AFTER `frame.render_widget(pseudo_terminal, inner)` on `terminal.rs:152`), each `buf.cell_mut((x, y))` returns a `Cell` with its fg/bg/modifier already populated from vt100.

**Concrete inverted-cell pattern (replaces `terminal.rs:170-173`):**
```rust
if let Some(cell) = buf.cell_mut((inner.x + col, inner.y + row)) {
    // Read current colors BEFORE we overwrite.
    let src_fg = cell.fg;
    let src_bg = cell.bg;
    // Swap fg <-> bg. ratatui Color::Reset becomes a visible problem — see below.
    cell.set_fg(src_bg);
    cell.set_bg(src_fg);
    // Fallback: if both reset, at least use ACCENT_GOLD so the highlight is
    // visible on a default-background empty cell (e.g. after an erase).
    if src_fg == ratatui::style::Color::Reset && src_bg == ratatui::style::Color::Reset {
        cell.set_bg(theme::ACCENT_GOLD);
        cell.set_fg(theme::BG_SURFACE);
    }
    // D-21: XOR Modifier::REVERSED. If cell already had it (vt100 reverse-video),
    // removing the flag plus our fg/bg swap gives visually distinct highlight.
    cell.modifier.toggle(Modifier::REVERSED);
}
```

**Note on `Color::Reset`:** The bigger issue with naive fg↔bg swap is that tui-term sets cells to `Color::Reset` when vt100 has `Color::Default`. A cell with both fg=Reset and bg=Reset, when swapped, stays Reset — no visible change. The fallback branch handles this.

**Alternative (simpler, matches CONTEXT.md D-20 "inverted-cell"):** just XOR `Modifier::REVERSED`. ratatui renders `REVERSED` by swapping fg/bg at the terminal protocol level, which handles Reset-colors correctly (terminal emulators implement reverse as "actual-fg becomes bg and vice versa"). Trade-off: D-21 says "re-invert so highlight is distinct from underlying reverse-video" — XOR already does this.

**Recommendation:** XOR `Modifier::REVERSED` is the simpler correct implementation of both D-20 and D-21. Drop the fg/bg swap entirely:

```rust
if let Some(cell) = buf.cell_mut((inner.x + col, inner.y + row)) {
    cell.modifier.toggle(Modifier::REVERSED);
}
```

This is ~1 line per highlighted cell, handles Color::Reset correctly, and makes reverse-video cells un-reverse (D-21) — which IS visually distinct from surrounding reversed cells.

**Trade-off for discuss-phase:** pure XOR reverse loses the "distinct gold highlight" feel of the current code. CONTEXT.md D-20 explicitly says "swap fg↔bg using existing cell colors … Falls back to ACCENT_GOLD only if the source cell has no fg/bg". Strict D-20 is the manual swap + fallback. Pure REVERSED XOR is simpler but doesn't match D-20. The manual-swap code above honors D-20 literally.

## Esc Precedence Integration Point (Q8)

**Current Esc handling:**

1. `src/events.rs:650`: `KeyCode::Esc => Some(vec![0x1b])` in `key_to_bytes` — forwards Esc as raw byte to PTY when `InputMode::Terminal` is active.
2. `src/keys.rs:167-204` (`EscapeDetector`): a double-Esc state machine. Currently NOT wired into `handle_key` — appears to be a library component with tests. The terminal-mode dispatch at `events.rs:288-297` calls `app.forward_key_to_pty(&key)` unconditionally (after a Ctrl+B check), bypassing EscapeDetector.
3. `events.rs:283-286` modal handler can consume Esc first (modal dismiss).
4. `events.rs:273-277` picker consumes all keys (including Esc) when open.

**Decision tree for new Esc handling:**

```
handle_key(app, key):
  if F(1..=9): ...existing...
  if picker open: picker.on_key — already handles Esc for cancel
  if modal Loading: ignore
  if modal open (non-Loading): modal.handle_modal_key — should handle Esc for close
  if key == Esc && app.selection.is_some() {  // ← NEW BRANCH, INSERT HERE
      app.selection = None;
      app.mark_dirty();                       // D-23
      return;
  }
  if InputMode::Terminal:
      if Ctrl+B → Normal
      forward_key_to_pty (includes Esc → 0x1b)  ← still works as before when no selection
  else (Normal):
      keymap lookup
```

**Insertion point:** `src/events.rs` between line 286 (end of modal branch) and line 288 (start of Terminal-mode branch). Exact snippet:

```rust
// NEW BRANCH — between current line 286 and 288
if key.code == KeyCode::Esc
    && key.modifiers == KeyModifiers::NONE
    && app.selection.is_some()
{
    app.selection = None;
    app.mark_dirty();
    return;
}
```

**Why this position is correct:**
- AFTER picker and modal — those handle Esc for cancel and we want to preserve that.
- BEFORE the Terminal-mode branch — so Esc is not forwarded to PTY when we have a selection to clear.
- Falls through when `selection.is_none()` — Esc behaves as today (PTY-forwarded in Terminal mode, keymap-lookup in Normal mode).
- Guards on `modifiers == NONE` to avoid competing with shift+Esc or any future Esc-modifier combo.

**Confirmed no other Esc handlers compete:**
- Grep for `KeyCode::Esc` in `src/`:
  - `src/events.rs:650` (key_to_bytes — fires AFTER our new branch via `forward_key_to_pty`)
  - `src/keys.rs:180,192,195` (EscapeDetector — not wired into handle_key)
  - `src/ui/picker.rs` (resolved before modal/selection check)
  - `src/ui/modal_controller.rs` (resolved by `handle_modal_key` — before our new branch)

No competing handler.

**cmd+c precedence check:** the cmd+c branch (from §Q5) must ALSO be inserted BEFORE the terminal-mode forwarding. Place it adjacent to the new Esc branch:

```rust
// cmd+c branch
if key.code == KeyCode::Char('c')
    && key.modifiers.contains(KeyModifiers::SUPER)
{
    if let Some(sel) = &app.selection {
        if !sel.is_empty() {
            app.copy_selection_to_clipboard();  // D-02
            return;
        }
    }
    if app.mode == InputMode::Terminal {
        app.write_active_tab_input(&[0x03]);    // D-03
        return;
    }
    // fall through in Normal mode (no keymap binding for cmd+c in Normal)
}

// Esc clears selection branch (from above)
if key.code == KeyCode::Esc && key.modifiers == KeyModifiers::NONE
    && app.selection.is_some()
{ ... }
```

## Test Harness Pattern (Q9)

**Existing patterns:**

1. `src/pty_input_tests.rs` (top-level `#[cfg(test)] mod pty_input_tests;` in `main.rs:21`) — `#[tokio::test] async fn` tests that construct a real `PtySession` via `spawn`, write bytes, poll the parser. Heavy (spawn /bin/cat) but real.
2. `src/navigation_tests.rs` — same pattern, builds `App` via `App::new(GlobalState, state_path)` with temp fixtures. Tests timing-sensitive non-blocking guarantees.
3. `src/app_tests.rs` — `#[path] mod tests;` from `src/app.rs:556-558`. Uses `init_repo` helper + `tempfile::TempDir` to build real git repos as fixtures.
4. `src/events.rs:683-695` — inline unit tests for pure helpers (menu click ranges).
5. `src/ui/terminal.rs:182-214` — ratatui `TestBackend` pattern for render tests.

**Recommended test strategy for Phase 6:**

### Handler-level tests (pure state mutation, no PTY)

Create `src/selection_tests.rs` + register `#[cfg(test)] mod selection_tests;` in `main.rs`. Mirror `navigation_tests.rs` structure.

**MouseEvent driving:** `crossterm::event::MouseEvent` is a plain struct. Construct directly:
```rust
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
let ev = MouseEvent {
    kind: MouseEventKind::Drag(MouseButton::Left),
    column: 10,
    row: 5,
    modifiers: KeyModifiers::NONE,
};
handle_mouse(&mut app, ev).await;
assert!(app.selection.is_some());
```

**Requirement:** `App.last_panes` must be set to a valid `PaneRects` for `handle_mouse` to consider mouse-in-terminal (see `events.rs:39-43`). Build a stub:
```rust
let inner = Rect { x: 0, y: 2, width: 80, height: 20 };
let terminal = Rect { x: 0, y: 0, width: 80, height: 24 };
app.last_panes = Some(PaneRects { terminal, left: None, right: None, menu_bar: Rect::default(), status_bar: Rect::default() });
```

### Key-event tests (cmd+c, Esc)

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
let key = KeyEvent {
    code: KeyCode::Char('c'),
    modifiers: KeyModifiers::SUPER,
    kind: KeyEventKind::Press,
    state: KeyEventState::empty(),
};
handle_key(&mut app, key).await;
// assert side effect: pbcopy was called or cmd was formed
```

**Pbcopy side-effect verification:** `copy_selection_to_clipboard` spawns `pbcopy` as a subprocess (`src/app.rs:437`). Test strategies:
- (a) Run `pbpaste` after the test (only on macOS, may be flaky with parallel tests).
- (b) Extract the text formation into a pure helper `SelectionState::materialize_text(&self, &vt100::Screen) -> String` and unit-test that; leave the pbcopy spawn as a thin wrapper.
- Recommended: (b). Matches the project convention of testing pure helpers and letting subprocess spawn be an untested seam.

### Scroll-stability test (SEL-04 regression guard)

```rust
#[tokio::test]
async fn selection_survives_streaming_output() {
    // 1. spawn real /bin/cat session
    let session = PtySession::spawn(/*cat*/).unwrap();
    let mut parser = session.parser.write().unwrap();
    // 2. write some lines, establish a selection at row 3
    parser.process(b"line1\nline2\nline3\nline4\nline5\n");
    app.selection = Some(SelectionState { /* anchored at gen=0, row=2..4 */ });
    let before_text = app.materialize_selection_text();
    // 3. stream 30 more lines to scroll the 24-row screen
    for i in 0..30 { parser.process(format!("line{i}\n").as_bytes()); }
    // 4. assert: selection still points at the right text OR has been clipped
    let after_text = app.materialize_selection_text();
    // after stream, entire selection should have scrolled off -> text is empty
    // but app.selection is still Some (per D-08)
    assert!(app.selection.is_some());
}
```

**Mock vt100 screen?** Not needed. Real `vt100::Parser::new(24, 80, 1000)` is cheap to construct and takes `&[u8]` input directly. No I/O required. Use real parsers for all tests.

### Inverted-cell render test

Use `TestBackend` (mirrors `terminal.rs:182-214`):
```rust
#[test]
fn selection_xors_reversed_modifier_on_cell() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| {
        // render terminal with a selection covering cells (5..10, 3..4)
        render(frame, ..., Some(&SelectionState { /*...*/ }));
    }).unwrap();
    let buf = terminal.backend().buffer();
    // assert cells in selection have Modifier::REVERSED toggled
    let cell = buf.cell((6, 3)).unwrap();
    assert!(cell.modifier.contains(Modifier::REVERSED));
}
```

## Validation Architecture

**Source:** `workflow.nyquist_validation` — not set in `.planning/config.json`; treat as enabled.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (Rust built-in `#[test]` + `tokio::test` for async) |
| Config file | `Cargo.toml` (line 42-46: `[dev-dependencies]` — insta, tempfile, assert_cmd, predicates) |
| Quick run command | `cargo test --lib selection` (filter by new module name) |
| Full suite command | `cargo test` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SEL-01 | Drag produces selection, tracks cursor | unit | `cargo test --lib selection_drag_creates_selection` | ❌ Wave 0 — `src/selection_tests.rs` |
| SEL-01 | Inverted highlight rendered per cell | render | `cargo test --lib selection_highlights_cells_inverted` | ❌ Wave 0 |
| SEL-02 | `cmd+c` with active selection triggers `copy_selection_to_clipboard` (text materialization) | unit | `cargo test --lib cmd_c_with_selection_materializes_snapshot` | ❌ Wave 0 |
| SEL-02 | `cmd+c` with no selection forwards 0x03 to PTY in Terminal mode | unit | `cargo test --lib cmd_c_without_selection_sends_sigint` | ❌ Wave 0 |
| SEL-03 | Left-click-down outside clears selection | unit | `cargo test --lib click_clears_selection` | ❌ Wave 0 |
| SEL-03 | `Esc` with active selection clears it | unit | `cargo test --lib esc_clears_active_selection` | ❌ Wave 0 |
| SEL-03 | `Esc` with no selection falls through to PTY | unit | `cargo test --lib esc_without_selection_forwards_to_pty` | ❌ Wave 0 |
| SEL-04 | Selection survives scroll (anchored gen translation) | integration | `cargo test --lib selection_survives_scroll` | ❌ Wave 0 |
| SEL-04 | Selection text snapshot captured at mouse-up | unit | `cargo test --lib mouse_up_snapshots_selection_text` | ❌ Wave 0 |
| D-15 | Double-click selects word | unit | `cargo test --lib double_click_selects_word` | ❌ Wave 0 |
| D-15 | Triple-click selects line | unit | `cargo test --lib triple_click_selects_line` | ❌ Wave 0 |
| D-15 | Shift-click extends end | unit | `cargo test --lib shift_click_extends_end_anchor` | ❌ Wave 0 |
| D-22 | Tab switch clears selection | unit | `cargo test --lib tab_switch_clears_selection` | ❌ Wave 0 |
| D-22 | Workspace switch clears selection | unit | `cargo test --lib workspace_switch_clears_selection` | ❌ Wave 0 |
| D-21 | XOR REVERSED on already-reversed cells | render | `cargo test --lib already_reversed_cell_un_reverses_under_selection` | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo test --lib selection` (targeted filter)
- **Per wave merge:** `cargo test` (full suite, includes pty_input_tests + navigation_tests + new selection_tests)
- **Phase gate:** Full suite green before `/gsd-verify-work`; plus manual UAT for SEL-01 feel tests and SEL-04 (stream `yes` while dragging).

### Wave 0 Gaps

- [ ] `src/selection_tests.rs` — new test module for all 15 tests in the map above
- [ ] `#[cfg(test)] mod selection_tests;` registration in `src/main.rs` (follow precedent from `pty_input_tests` at line 21 and `navigation_tests` at line 24)
- [ ] No framework install needed — `cargo test` works today

### Manual UAT (subjective feel tests per CONTEXT.md "Ghostty baseline")

1. Drag-select in PTY pane while `cat /etc/hosts` scrolls in background — highlight stays stable (SEL-04)
2. cmd+c with selection active — verify `pbpaste` in external terminal shows the selected text (SEL-02)
3. cmd+c with no selection in Terminal mode — verify the child process in the active tab receives Ctrl+C (e.g. interrupts a `sleep`)
4. Double-click a word — word is highlighted (D-15)
5. Triple-click a line — line is highlighted (D-15)
6. Shift-click after a selection — end extends (D-15)
7. Esc after drag — highlight clears (SEL-03)
8. Click-outside after drag — highlight clears (SEL-03)
9. Tab switch after drag — highlight clears (D-22)
10. Workspace switch after drag — highlight clears (D-22)

## Runtime State Inventory

> Phase 6 is a feature addition, not a rename/refactor. This section exists only to confirm no runtime state is touched.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | None — Phase 6 writes no data to disk. `~/.martins/state.json` schema unchanged; no new migrations. | none |
| Live service config | None. | none |
| OS-registered state | None. | none |
| Secrets/env vars | None. | none |
| Build artifacts | `target/` rebuild triggered by Rust changes (normal). No stale artifacts. | none |

**Nothing found in any category. Verified by reading `src/state.rs` and the App struct fields against what this phase will add (`scroll_generation: AtomicU64`, `SelectionState` extensions).**

## Environment Availability

> Phase 6 is code-only within the existing stack. All dependencies already in `Cargo.toml`.

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `pbcopy` | `App::copy_selection_to_clipboard` (already used) | ✓ | macOS built-in | — |
| `pbpaste` | UAT verification only | ✓ | macOS built-in | — |
| `vt100` | All selection code | ✓ | 0.16.2 | — |
| `crossterm` | Mouse + key events | ✓ | 0.29.0 | — |
| `ratatui` / `ratatui-core` | Buffer cell API | ✓ | 0.30.0 / 0.1.0 | — |
| `tui-term` | PseudoTerminal widget | ✓ | 0.3.4 | — |

**Missing dependencies with no fallback:** None.
**Missing dependencies with fallback:** None.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `PushKeyboardEnhancementFlags(DISAMBIGUATE_ESCAPE_CODES)` is a no-op on terminals that don't support it (Terminal.app, iTerm2, Ghostty-default) | §Q5 | If the push causes a visible garbage character, first-run UX degrades. MITIGATION: test on Terminal.app; if garbage appears, conditionalize via `query_keyboard_enhancement_flags` (`crossterm-0.29.0/src/terminal/sys/unix.rs:197`). |
| A2 | Ghostty consumes cmd+c at the terminal-emulator level and does NOT forward it, regardless of kitty protocol push | §Q5 | If Ghostty forwards cmd+c when the flag is pushed, Great — cmd+c works in Ghostty too. If it doesn't forward even with the flag, SEL-02 is only satisfied by auto-copy-on-mouse-up (D-01) in Ghostty. MITIGATION: user UAT on Ghostty. Source: web search, Jan-Feb 2026 Ghostty discussions — unverified in this session. |
| A3 | The SCROLLBACK-LEN heuristic in §Q1 produces acceptably few false positives on real command output | §Q1 | If a TUI program that redraws row 0 often (e.g., `htop` full-screen) fires many false scroll-generation increments, the anchored-row translation produces visual jitter. MITIGATION: disable scroll tracking for vt100 alternate-screen mode (`screen.alternate_screen()` returns true for htop etc.); selection is unlikely to be used in that mode anyway. |
| A4 | `DISAMBIGUATE_ESCAPE_CODES` alone is sufficient for SUPER modifier delivery (no need for `REPORT_EVENT_TYPES` / `REPORT_ALTERNATE_KEYS`) | §Q5 | If SUPER still doesn't arrive, may need to combine flags. crossterm source line 842 documentation says DISAMBIGUATE is enough. Unverified on real hardware. |
| A5 | User's current Ctrl+C Quit binding (`src/keys.rs:120`) in Normal mode does not conflict with new cmd+c-in-Terminal-mode branch | §Esc Precedence | Low risk — cmd+c is KeyModifiers::SUPER; ctrl+c is KeyModifiers::CONTROL. Distinct keymap entries. |

## Open Questions / Risks

### OQ-1: cmd+c delivery on macOS Terminal.app / iTerm2

**What we know:** Without kitty keyboard protocol, cmd+c is consumed by the terminal emulator as its native Copy action. `PushKeyboardEnhancementFlags` MAY enable SUPER delivery on Alacritty/kitty/WezTerm/foot but has no documented effect on Terminal.app/iTerm2.

**What's unclear:** Whether Ghostty-in-default-config forwards cmd+c when the TUI pushes `DISAMBIGUATE_ESCAPE_CODES`. Whether Alacritty 0.13+ honors the push.

**Recommendation for discuss-phase:**
- **Option A:** Push the flag; document SEL-02 as "cmd+c works in Alacritty/WezTerm/kitty (cross-verified by user); on Terminal.app/iTerm2/Ghostty-default, cmd+c is the native terminal Copy (which copies the visible scrollback, not Martins's SelectionState — but that is acceptable since the auto-copy on mouse-up already put the selected text on the clipboard via pbcopy)."
- **Option B:** Don't push the flag; document that SEL-02 via cmd+c is best-effort and rely on auto-copy-on-mouse-up as the guaranteed path.
- **Option C (Recommended):** Push the flag + document Option A's truth + add an Activity Monitor UAT showing auto-copy lands the text on the clipboard regardless of which path fires. SEL-02's literal spec is "cmd+c while selection active puts text on clipboard" — the clipboard already has the text from mouse-up. The test the user runs ("verify via `pbpaste`") passes regardless.

### OQ-2: Terminal.app reset of kitty keyboard protocol state on exit

**What we know:** `PopKeyboardEnhancementFlags` writes `CSI < 1 u`. Terminal.app may not parse this — it could leave residual state on exit.

**What's unclear:** Whether this produces visible garbage in the user's shell after Martins exits.

**Recommendation:** Test on Terminal.app; if garbage appears, wrap the push in a query (`crossterm::terminal::supports_keyboard_enhancement()` — not listed above, verify existence) before pushing.

### OQ-3: Scroll counter false positives during htop-like full-screen redraws

See A3 in the Assumptions Log. If the plan enters the htop-selection edge case, ship a conditional that skips `scroll_generation` increments when `screen.alternate_screen()` is true.

### OQ-4: D-20 strict interpretation vs. pure REVERSED XOR

See §ratatui Buffer Cell API. CONTEXT.md D-20 calls for a literal fg/bg swap with ACCENT_GOLD fallback. A pure XOR of `Modifier::REVERSED` is simpler and handles `Color::Reset` correctly but doesn't match D-20's letter. Discuss-phase should confirm whether XOR satisfies D-20 or whether the full swap is required.

## Sources

### Primary (HIGH confidence)
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/vt100-0.16.2/src/screen.rs` — `contents_between`, `cell`, `cursor_position`, `scrollback`, `mouse_protocol_mode`
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/vt100-0.16.2/src/grid.rs` — `scroll_up`, `visible_rows`, scrollback internals
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/vt100-0.16.2/src/cell.rs` — `Cell::contents`, `is_wide`, `is_wide_continuation`
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/vt100-0.16.2/src/callbacks.rs` — confirms no scroll callback
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/crossterm-0.29.0/src/event.rs:800-900` — `MouseEventKind`, `KeyModifiers`, `PushKeyboardEnhancementFlags` docs
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/crossterm-0.29.0/src/event/sys/unix/parse.rs:300-325` — SUPER modifier derivation from CSI u mask
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ratatui-core-0.1.0/src/buffer/buffer.rs:130-213` — `cell_mut`, `cell`, `get`, `get_mut` signatures
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ratatui-core-0.1.0/src/buffer/cell.rs:1-180` — `Cell` public fields + `set_fg/set_bg/set_style`
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ratatui-core-0.1.0/src/style.rs:105-113` — `Modifier::REVERSED` flag constant
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tui-term-0.3.4/src/vt100_imp.rs:44-64` — confirms tui-term translates vt100 `inverse()` to ratatui `Modifier::REVERSED`
- `src/app.rs`, `src/events.rs`, `src/keys.rs`, `src/pty/session.rs`, `src/ui/terminal.rs`, `src/main.rs`, `src/navigation_tests.rs`, `src/pty_input_tests.rs` — existing codebase

### Secondary (MEDIUM confidence)
- Ghostty GitHub discussions #3493, #3497, #3615, #5487, #8111 (WebSearch, Jan-Feb 2026) — cmd key behavior on macOS
- kitty keyboard protocol docs (referenced in crossterm source comments)

### Tertiary (LOW confidence)
- None. All claims are either code-backed (HIGH) or explicitly flagged in Assumptions Log.

## Metadata

**Confidence breakdown:**
- Crate API shapes (vt100, crossterm, ratatui, tui-term): HIGH — all read directly from `~/.cargo/registry`
- vt100 scroll counter sourcing strategy: MEDIUM — heuristic-based since no public counter exists; false-positive rate unverified
- cmd+c delivery on macOS: MEDIUM-to-LOW — depends on terminal emulator config; multiple viable paths; OQ-1 calls out resolution
- Inverted-cell rendering: HIGH — ratatui 0.30 Cell fields are public
- Esc precedence: HIGH — only one competing handler path (key_to_bytes), cleanly pre-empted
- Unicode / wide-char handling: HIGH — `contents_between` handles via row.write_contents; `is_wide_continuation` gives clean iteration
- Test harness: HIGH — existing precedent in navigation_tests.rs and pty_input_tests.rs

**Research date:** 2026-04-24
**Valid until:** 2026-05-24 (stable crates, 30-day shelf life; re-verify if `Cargo.toml` versions bump)

## RESEARCH COMPLETE
