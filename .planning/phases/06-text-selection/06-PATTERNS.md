# Phase 6: Text Selection — Pattern Map

**Mapped:** 2026-04-24
**Files analyzed:** 7 (6 modified + 1 created)
**Analogs found:** 7 / 7 (all in-codebase, no external fallback needed)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `src/app.rs` (mod) | state struct + helpers | state-mutation | `src/app.rs` itself (existing `SelectionState`, `mark_dirty`, `copy_selection_to_clipboard`) | exact (in-place extension) |
| `src/events.rs` (mod) | free-function event handler | request-response (event) | `src/events.rs::handle_mouse` / `handle_key` (same file) | exact (in-place extension) |
| `src/ui/terminal.rs` (mod) | ratatui render widget | transform (Buffer→Buffer) | `src/ui/terminal.rs:156-177` (existing gold-accent pass) | exact (replace body) |
| `src/pty/session.rs` (mod) | PTY reader thread | streaming (bytes→parser) | `src/pty/session.rs:72-110` (`std::thread::spawn` reader loop) | exact (wrap `parser.process`) |
| `src/keys.rs` (mod) | keymap — no changes required | config | `src/keys.rs::Keymap::default` | partial (Esc/cmd+c route via events.rs, NOT keymap) |
| `src/workspace.rs` + `src/app.rs` (mod) | tab/workspace mutation sites | state-mutation | `src/workspace.rs:140,224,277,317` + `src/app.rs:346 select_active_workspace` | exact (add clear line) |
| `src/selection_tests.rs` (new) | inline unit-test module | test harness | `src/pty_input_tests.rs` + `src/navigation_tests.rs` | exact (mirror structure) |

## Pattern Assignments

### `src/app.rs` — `SelectionState` struct extension + `scroll_generation` field

**Analog:** current `SelectionState` declaration at `src/app.rs:28-51` (lift and extend in place).

**Existing struct to extend** (`src/app.rs:28-51`):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionState {
    pub start_col: u16,
    pub start_row: u16,
    pub end_col: u16,
    pub end_row: u16,
    pub dragging: bool,
}

impl SelectionState {
    pub fn normalized(&self) -> ((u16, u16), (u16, u16)) { /* ... */ }
    pub fn is_empty(&self) -> bool { /* ... */ }
}
```

**Extension pattern to apply** (anchored endpoints + snapshot from RESEARCH.md §Q2):
```rust
#[derive(Debug, Clone, PartialEq, Eq)]   // drop Copy — String is not Copy
pub struct SelectionState {
    pub start_col: u16,
    pub start_row: u16,
    pub start_gen: u64,           // NEW — anchored at drag-start (D-06, D-07)
    pub end_col: u16,
    pub end_row: u16,
    pub end_gen: Option<u64>,     // NEW — None mid-drag (cursor-relative), Some after mouse-up (D-07)
    pub dragging: bool,
    pub text: Option<String>,     // NEW — snapshot at mouse-up (RESEARCH §Q2; survives scroll-off)
}
```

**Field placement on App** (`src/app.rs:78` — add alongside `selection`):
```rust
pub selection: Option<SelectionState>,
pub scroll_generation: u64,                         // NEW (D-05; or AtomicU64 in PtySession per RESEARCH §Q1)
pub last_click_at: Option<std::time::Instant>,      // NEW (D-16)
pub last_click_count: u8,                           // NEW (D-16)
pub last_click_row: u16,                            // NEW (D-16 — reset guard)
pub last_click_col: u16,                            // NEW (D-16 — reset guard)
```

**`copy_selection_to_clipboard` pattern** (`src/app.rs:418-447` — extend to use snapshot):
```rust
// Existing — DO NOT rewrite, extend:
pub(crate) fn copy_selection_to_clipboard(&self) {
    let Some(sel) = &self.selection else { return };
    if sel.is_empty() { return; }

    // NEW: prefer snapshot (D-02 re-copy after scroll-off still works):
    let text: String = if let Some(snapshot) = &sel.text {
        snapshot.clone()
    } else {
        let sessions = self.active_sessions();
        let Some((_, session)) = sessions.get(self.active_tab) else { return };
        let Ok(parser) = session.parser.try_read() else { return };
        let screen = parser.screen();
        let ((sc, sr), (ec, er)) = sel.normalized();
        screen.contents_between(sr, sc, er, ec.saturating_add(1))
            .trim_end()
            .to_string()
    };
    if text.is_empty() { return; }

    // Existing subprocess-spawn pattern (app.rs:437-446) — do NOT change:
    let _ = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait().map(|_| ())
        });
}
```

---

### `src/events.rs` — `handle_mouse` extension (Drag anchor + Down counter + Up snapshot)

**Analog:** existing `handle_mouse` at `src/events.rs:38-88`.

**Existing Drag / Up / Down branches** (`src/events.rs:38-88`) — the pattern to extend in place:
```rust
pub async fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    let in_terminal = app.last_panes.as_ref().is_some_and(|p| {
        let inner = terminal_content_rect(p.terminal);
        rect_contains(inner, mouse.column, mouse.row)
    });

    if in_terminal {
        match mouse.kind {
            MouseEventKind::Drag(MouseButton::Left) => {
                let inner = terminal_content_rect(app.last_panes.as_ref().unwrap().terminal);
                let col = mouse.column.saturating_sub(inner.x).min(inner.width.saturating_sub(1));
                let row = mouse.row.saturating_sub(inner.y).min(inner.height.saturating_sub(1));
                if let Some(sel) = &mut app.selection {
                    sel.end_col = col;
                    sel.end_row = row;
                } else {
                    app.selection = Some(SelectionState {
                        start_col: col, start_row: row,
                        end_col: col, end_row: row,
                        dragging: true,
                    });
                }
                return;
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(sel) = app.selection.take() {
                    if !sel.is_empty() {
                        app.selection = Some(sel);
                        app.copy_selection_to_clipboard();
                        return;
                    }
                }
            }
            _ => {}
        }
    }
    // Down(Left), ScrollUp/ScrollDown path — events.rs:77-87 — KEEP AS-IS
}
```

**Extension pattern** (capture current `scroll_generation` at Drag-create; snapshot text at Up; add `app.mark_dirty()` after each mutation per D-23):
```rust
MouseEventKind::Drag(MouseButton::Left) => {
    let inner = terminal_content_rect(app.last_panes.as_ref().unwrap().terminal);
    let col = mouse.column.saturating_sub(inner.x).min(inner.width.saturating_sub(1));
    let row = mouse.row.saturating_sub(inner.y).min(inner.height.saturating_sub(1));
    let current_gen = app.scroll_generation;
    if let Some(sel) = &mut app.selection {
        sel.end_col = col; sel.end_row = row;
        // end_gen stays None mid-drag (D-07)
    } else {
        app.selection = Some(SelectionState {
            start_col: col, start_row: row, start_gen: current_gen,
            end_col: col, end_row: row, end_gen: None,
            dragging: true, text: None,
        });
    }
    app.mark_dirty();   // D-23
    return;
}
MouseEventKind::Up(MouseButton::Left) => {
    if let Some(mut sel) = app.selection.take() {
        if !sel.is_empty() {
            sel.dragging = false;
            sel.end_gen = Some(app.scroll_generation);       // D-07 anchor end
            sel.text = Some(materialize_selection_text(app, &sel));  // D-02 snapshot
            app.selection = Some(sel);
            app.copy_selection_to_clipboard();
            app.mark_dirty();  // D-23
            return;
        }
    }
}
```

**Down(Left) click-counter pattern** (D-15/D-16, extends `events.rs:78-83`):
```rust
MouseEventKind::Down(MouseButton::Left) => {
    // Existing clear:
    if app.selection.is_some() {
        app.selection = None;
        app.mark_dirty();  // D-23 — NEW (current code implicitly marks via next render)
    }
    // NEW — click-counter + double/triple dispatch (only when in terminal):
    if in_terminal {
        let now = std::time::Instant::now();
        let within_threshold = app.last_click_at
            .is_some_and(|t| now.duration_since(t) < std::time::Duration::from_millis(300));
        if within_threshold && mouse.row == app.last_click_row {
            app.last_click_count = app.last_click_count.saturating_add(1);
        } else {
            app.last_click_count = 1;
        }
        app.last_click_at = Some(now);
        app.last_click_row = mouse.row;
        app.last_click_col = mouse.column;
        match app.last_click_count {
            2 => { /* select_word_at(app, row, col); */ app.mark_dirty(); return; }
            3 => { /* select_line_at(app, row); */ app.mark_dirty(); return; }
            _ => {}
        }
    }
    handle_click(app, mouse.column, mouse.row).await;
}
```

---

### `src/events.rs` — `handle_key` Esc / cmd+c branches (insertion point: between line 286 and 288)

**Analog:** `handle_key` at `src/events.rs:258-302` — the exact precedence chain to hook into.

**Existing precedence chain** (lines 258-297, the structure to preserve):
```rust
pub async fn handle_key(app: &mut App, key: KeyEvent) {
    if let KeyCode::F(n) = key.code { /* F1..F9 tab switch */ }
    if let Some(picker) = &mut app.picker { /* picker consumes keys */ return; }
    if matches!(app.modal, Modal::Loading(_)) { return; }
    if !matches!(app.modal, Modal::None) {
        crate::ui::modal_controller::handle_modal_key(app, key).await;
        return;
    }
    // ← INSERT NEW cmd+c AND Esc BRANCHES HERE (line 287, between modal and Terminal-mode)
    if app.mode == InputMode::Terminal {
        if key.code == KeyCode::Char('b')
            && key.modifiers.contains(KeyModifiers::CONTROL) {
            app.mode = InputMode::Normal;
            return;
        }
        app.forward_key_to_pty(&key);   // Esc → 0x1b falls through here TODAY
        return;
    }
    if let Some(action) = app.keymap.resolve_normal(&key).cloned() {
        dispatch_action(app, action).await;
    }
}
```

**Insertion pattern** (RESEARCH §Esc Precedence, verbatim):
```rust
// NEW — cmd+c with active selection copies; with no selection + Terminal mode forwards SIGINT
if key.code == KeyCode::Char('c')
    && key.modifiers.contains(KeyModifiers::SUPER)
{
    if let Some(sel) = &app.selection {
        if !sel.is_empty() {
            app.copy_selection_to_clipboard();     // D-02
            return;
        }
    }
    if app.mode == InputMode::Terminal {
        app.write_active_tab_input(&[0x03]);       // D-03
        return;
    }
    // fall through in Normal mode
}

// NEW — Esc clears selection IFF active; else falls through to PTY forwarding
if key.code == KeyCode::Esc
    && key.modifiers == KeyModifiers::NONE
    && app.selection.is_some()
{
    app.selection = None;
    app.mark_dirty();                               // D-23
    return;
}
```

---

### `src/ui/terminal.rs` — Inverted-cell highlight (replaces lines 170-173)

**Analog:** current gold-accent loop at `src/ui/terminal.rs:156-177`.

**Existing render pattern to preserve** (the outer iteration, `terminal.rs:156-176`):
```rust
if let Some(sel) = selection {
    if !sel.is_empty() {
        let ((sc, sr), (ec, er)) = sel.normalized();
        let buf = frame.buffer_mut();
        for row in sr..=er {
            if row >= inner.height { break; }
            let c_start = if row == sr { sc } else { 0 };
            let c_end = if row == er { ec } else { inner.width.saturating_sub(1) };
            for col in c_start..=c_end {
                if col >= inner.width { break; }
                // ↓ REPLACE THE BODY BELOW ↓
                if let Some(cell) = buf.cell_mut((inner.x + col, inner.y + row)) {
                    cell.set_bg(theme::ACCENT_GOLD);
                    cell.set_fg(theme::BG_SURFACE);
                }
            }
        }
    }
}
```

**Replacement body** (D-20 + D-21, per RESEARCH §Q7 ratatui 0.30 Cell API):
```rust
// Replaces `src/ui/terminal.rs:170-173` — body of the inner `for col` loop.
// ratatui 0.30 Cell has pub fg/bg/modifier fields and `modifier.toggle(Modifier::REVERSED)`.
if let Some(cell) = buf.cell_mut((inner.x + col, inner.y + row)) {
    use ratatui::style::Modifier;
    cell.modifier.toggle(Modifier::REVERSED);   // D-20 + D-21: XOR handles both
                                                 // "invert normal cell" and
                                                 // "un-reverse already-reversed cell"
}
```

**Note:** If planner chooses the literal D-20 interpretation (manual fg/bg swap with ACCENT_GOLD fallback) per OQ-4, use this expanded form instead:
```rust
if let Some(cell) = buf.cell_mut((inner.x + col, inner.y + row)) {
    use ratatui::style::{Color, Modifier};
    let src_fg = cell.fg;
    let src_bg = cell.bg;
    if src_fg == Color::Reset && src_bg == Color::Reset {
        cell.set_bg(theme::ACCENT_GOLD);         // D-20 fallback
        cell.set_fg(theme::BG_SURFACE);
    } else {
        cell.set_fg(src_bg);                     // D-20 swap
        cell.set_bg(src_fg);
    }
    cell.modifier.toggle(Modifier::REVERSED);    // D-21 distinct-from-reverse
}
```

**Anchored-coord translation** (D-06, add BEFORE the `for row in sr..=er` loop — uses the `scroll_generation` passed into `render()`):
```rust
// Translate anchored rows to current-screen rows (D-06).
// current_row = anchored_row - (current_gen - sel_gen)
let current_gen = scroll_generation_for_active_session;
let start_delta = current_gen.saturating_sub(sel.start_gen);
let end_delta = sel.end_gen
    .map(|g| current_gen.saturating_sub(g))
    .unwrap_or(0);  // mid-drag: end is cursor-relative → delta=0
// Compute visible rows; skip rows where translated < 0 (clip at visible top per D-08)
let sr_visible = (sel.start_row as i32).saturating_sub(start_delta as i32);
let er_visible = (sel.end_row as i32).saturating_sub(end_delta as i32);
if er_visible < 0 { /* fully scrolled off — render nothing, keep state */ return; }
let sr_clipped = sr_visible.max(0) as u16;
let er_clipped = er_visible.max(0) as u16;
// ... then iterate `for row in sr_clipped..=er_clipped`
```

---

### `src/pty/session.rs` — `scroll_generation` increment in the PTY reader thread

**Analog:** existing reader thread at `src/pty/session.rs:72-110`.

**Existing reader loop** (the exact position to wrap, `src/pty/session.rs:78-104`):
```rust
std::thread::spawn(move || {
    let mut reader = reader;
    let mut child = child;
    let mut buf = [0u8; 16384];
    let mut last_notify = std::time::Instant::now();

    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => { /* exit bookkeeping */ break; }
            Ok(n) => {
                if let Ok(mut parser) = parser_clone.write() {
                    parser.process(&buf[..n]);   // ← WRAP THIS LINE
                }
                *last_output_clone.lock().unwrap() = std::time::Instant::now();
                if let Some(notify) = &output_notify { /* 8ms throttle */ }
            }
        }
    }
});
```

**Wrap pattern** (RESEARCH §Q1 SCROLLBACK-LEN heuristic; field added to `PtySession`):
```rust
// Add to PtySession struct (line 18-26):
pub struct PtySession {
    // ... existing fields ...
    pub scroll_generation: Arc<std::sync::atomic::AtomicU64>,   // NEW
}

// Clone for move into thread (parallel to parser_clone at line 62):
let scroll_gen = Arc::new(std::sync::atomic::AtomicU64::new(0));
let scroll_gen_clone = Arc::clone(&scroll_gen);

// Replace lines 91-94 inside the `Ok(n) =>` arm:
Ok(n) => {
    if let Ok(mut parser) = parser_clone.write() {
        // Before: snapshot top row + cursor.
        let (rows, cols) = parser.screen().size();
        let before_cursor_row = parser.screen().cursor_position().0;
        let before_top_hash = row_hash(parser.screen(), 0, cols);
        parser.process(&buf[..n]);
        // After: if cursor WAS pinned at bottom AND top row text CHANGED,
        // infer at least one row scrolled off.
        let after_top_hash = row_hash(parser.screen(), 0, cols);
        let scrolled = before_cursor_row >= rows.saturating_sub(1)
            && before_top_hash != after_top_hash;
        if scrolled {
            scroll_gen_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    *last_output_clone.lock().unwrap() = std::time::Instant::now();
    if let Some(notify) = &output_notify {
        let now = std::time::Instant::now();
        if now.duration_since(last_notify).as_millis() >= 8 {
            notify.notify_one();
            last_notify = now;
        }
    }
}

// Helper (new free function in pty/session.rs):
fn row_hash(screen: &vt100::Screen, row: u16, cols: u16) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for col in 0..cols {
        if let Some(cell) = screen.cell(row, col) {
            cell.contents().hash(&mut h);
        }
    }
    h.finish()
}
```

**Where consumers read it:**
- Render (`src/ui/terminal.rs`): `session.scroll_generation.load(Ordering::Relaxed)` passed into `render()` for anchored-coord translation.
- Event loop (`src/app.rs::run`): existing `output_notify` branch at line 224 already calls `mark_dirty()` — no change needed; the selection translation happens during render as a pure function of `scroll_generation`.

---

### `src/keys.rs` — NO CHANGE (Esc / cmd+c routed through `events.rs::handle_key` directly)

**Analog:** `src/keys.rs::Keymap::default` (lines 100-153).

**Rationale:** Per RESEARCH §Esc Precedence, cmd+c and Esc-with-selection must be handled BEFORE the keymap lookup at `events.rs:299`. The keymap is consulted only in Normal mode AFTER all precedence branches. The new Esc-clear and cmd+c branches live in `handle_key` — NOT in the keymap. No `Action` variant added, no `normal.insert(...)` line added.

**Existing precedent in keymap** (for reference only — ctrl+c is keymap-routed to Quit in Normal mode, line 120):
```rust
normal.insert(key(KeyCode::Char('c'), KeyModifiers::CONTROL), Action::Quit);
```

This is fine — `KeyModifiers::CONTROL` is distinct from `KeyModifiers::SUPER` (per A5 in RESEARCH). The new cmd+c branch guards on `SUPER` and does not conflict.

---

### `src/workspace.rs` + `src/app.rs::select_active_workspace` — selection clear on tab/workspace switch

**Analog:** all existing `active_tab = …` and `select_active_workspace` call sites.

**Sites requiring a `clear_selection` line** (D-22):

| File | Line | Context | Change |
|------|------|---------|--------|
| `src/app.rs:346` | body of `select_active_workspace` | workspace switch entry | add `self.selection = None; self.mark_dirty();` at top |
| `src/workspace.rs:140` | `app.active_tab = 0;` (post-tab-create) | tab created path | add `app.selection = None; app.mark_dirty();` |
| `src/workspace.rs:224` | `app.active_tab = 0;` | tab path | add clear |
| `src/workspace.rs:277` | `app.active_tab = 0;` | tab path | add clear |
| `src/workspace.rs:317` | `app.active_tab = workspace.tabs.len() - 1;` | tab path | add clear |
| `src/events.rs:146` | `app.active_tab = idx;` (tab close click) | tab change | add clear |
| `src/events.rs:266` | `app.active_tab = (n - 1).min(...)` (F1-F9) | tab switch | add clear |
| `src/events.rs:443,522` | `app.active_tab = …` | keymap/click SwitchTab | add clear |

**Existing `select_active_workspace` pattern** (`src/app.rs:346-349`):
```rust
pub(crate) fn select_active_workspace(&mut self, index: usize) {
    self.active_workspace_idx = Some(index);
    self.right_list.select(None);
}
```

**Extension pattern** (idiomatic for D-22):
```rust
pub(crate) fn select_active_workspace(&mut self, index: usize) {
    self.active_workspace_idx = Some(index);
    self.right_list.select(None);
    self.selection = None;          // NEW — D-22
    self.mark_dirty();              // NEW — D-23
}
```

**Preferred refactor:** extract `App::clear_selection(&mut self)` so every tab/workspace mutation site has a one-line call:
```rust
pub(crate) fn clear_selection(&mut self) {
    if self.selection.take().is_some() {
        self.mark_dirty();
    }
}
```
Then sites become `app.clear_selection();` (7 call sites total — matches the inventory above).

---

### `src/selection_tests.rs` (NEW) — inline-module unit tests

**Analog:** `src/pty_input_tests.rs` + `src/navigation_tests.rs` + inline `#[cfg(test)] mod tests` at `src/events.rs:683-696`.

**Module registration pattern** (mirror `src/main.rs:21, 24`):
```rust
#[cfg(test)] mod pty_input_tests;      // existing
#[cfg(test)] mod navigation_tests;     // existing
#[cfg(test)] mod selection_tests;      // NEW
```

**Test file header pattern** (from `src/pty_input_tests.rs:1-12`):
```rust
//! Selection behavior validation (SEL-01..SEL-04, D-15).
//!
//! Handler-level tests drive synthesized MouseEvent / KeyEvent through
//! `crate::events::handle_mouse` / `handle_key` against a real `App`
//! instance. Mirrors the pattern from `src/pty_input_tests.rs` and
//! `src/navigation_tests.rs`.
//!
//! See `.planning/phases/06-text-selection/06-RESEARCH.md` §Test Harness.

#![cfg(test)]

use crate::app::{App, SelectionState};
use crate::events;
use crate::state::GlobalState;
use crate::ui::layout::PaneRects;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
                       MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
```

**`App::new` fixture pattern** (from `src/navigation_tests.rs:56-63`):
```rust
#[tokio::test]
async fn drag_creates_selection() {
    let state_path = std::env::temp_dir().join("martins-sel-drag-creates.json");
    let _ = std::fs::remove_file(&state_path);
    let mut app = App::new(GlobalState::default(), state_path)
        .await
        .expect("App::new");

    // Required fixture: PaneRects so handle_mouse treats the click as in-terminal.
    let terminal = Rect { x: 0, y: 0, width: 80, height: 24 };
    app.last_panes = Some(PaneRects {
        terminal,
        left: None, right: None,
        menu_bar: Rect::default(),
        status_bar: Rect::default(),
    });

    let drag = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 10, row: 5,
        modifiers: KeyModifiers::NONE,
    };
    events::handle_mouse(&mut app, drag).await;
    assert!(app.selection.is_some(), "Drag must create SelectionState");
    assert!(app.dirty, "Drag must mark_dirty (D-23)");
}
```

**KeyEvent synthesis pattern** (for cmd+c / Esc tests):
```rust
#[tokio::test]
async fn esc_with_active_selection_clears_and_does_not_forward() {
    let state_path = std::env::temp_dir().join("martins-sel-esc-clears.json");
    let _ = std::fs::remove_file(&state_path);
    let mut app = App::new(GlobalState::default(), state_path).await.unwrap();

    // Seed selection.
    app.selection = Some(SelectionState { /* anchored, non-empty */ ..todo!() });
    app.dirty = false;

    let key = KeyEvent {
        code: KeyCode::Esc,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    };
    events::handle_key(&mut app, key).await;
    assert!(app.selection.is_none(), "Esc must clear selection");
    assert!(app.dirty, "Esc clear must mark_dirty");
}
```

**Real vt100 parser for scroll-stability test** (from RESEARCH §Test Harness + `src/pty_input_tests.rs:27-42` pattern):
```rust
#[tokio::test]
async fn selection_survives_streaming_output() {
    use crate::pty::session::PtySession;
    let mut session = PtySession::spawn(
        std::env::temp_dir(), "/bin/cat", &[], 24, 80,
    ).expect("spawn /bin/cat failed");
    // Feed bytes directly through writer, poll parser for scroll_gen bump.
    // ... (see RESEARCH §Test Harness "Scroll-stability test" example)
}
```

**Inline unit-test pattern for pure helpers** (from `src/events.rs:683-696`):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn word_boundary_at_splits_on_whitespace() {
        // Build a vt100::Parser, write "hello world", call
        // word_boundary_at(&screen, 0, 2) — assert (0, 4).
    }
}
```

---

## Shared Patterns

### Dirty-Flag Discipline (D-23)

**Source:** `src/app.rs:171-173` + call sites at `src/app.rs:202, 220, 225, 241, 259, 317, 326`.
**Apply to:** Every `app.selection = …`, `app.selection = None`, `app.last_click_count = …` mutation in `events.rs` and `app.rs`.

**Existing helper** (`src/app.rs:170-173`):
```rust
#[inline]
pub(crate) fn mark_dirty(&mut self) {
    self.dirty = true;
}
```

**Existing call-site pattern** (`src/app.rs:316-318, 326`):
```rust
// refresh_diff_spawn guard path:
self.modified_files.clear();
self.right_list.select(None);
self.mark_dirty();
```

**Convention:** call `mark_dirty()` immediately AFTER the state mutation, in the same function — not in render, not deferred. The Phase 3 invariant (6+ calls in app.rs) must be preserved.

### Free-Function Event Handlers (Phase 1 convention)

**Source:** `src/events.rs:20-88` (`handle_event`, `handle_mouse`) + `src/events.rs:258-302` (`handle_key`).
**Apply to:** Any new selection handler (`handle_shift_click`, `handle_double_click`) — free functions in `src/events.rs`, signature `async fn name(app: &mut App, …)` or sync `fn` for pure-sync writes.

```rust
pub async fn handle_mouse(app: &mut App, mouse: MouseEvent) { /* ... */ }
pub async fn handle_key(app: &mut App, key: KeyEvent) { /* ... */ }
```

No `impl App` methods for routing — only state-mutation helpers (`copy_selection_to_clipboard`, `clear_selection`, `write_active_tab_input`) live on `App`.

### Subprocess-Spawn (fire-and-forget pbcopy)

**Source:** `src/app.rs:437-446`.
**Apply to:** `copy_selection_to_clipboard` path (already follows convention — no change needed).
Already shown above under `copy_selection_to_clipboard` pattern.

### Real-vt100 Tests (no mocks)

**Source:** `src/pty_input_tests.rs:16-43` (real `PtySession::spawn` of `/bin/cat`) + RESEARCH §Test Harness.
**Apply to:** SEL-04 scroll-stability test. Use real `vt100::Parser::new(24, 80, 1000)` for unit tests; use real `PtySession::spawn` only for end-to-end streaming test.

## No Analog Found

None. Every file in scope has a direct in-codebase analog.

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| (none) | | | All Phase 6 surfaces extend existing code; no green-field modules. |

## Metadata

**Analog search scope:**
- `src/app.rs`, `src/events.rs`, `src/keys.rs`, `src/pty/session.rs`, `src/ui/terminal.rs`, `src/workspace.rs`
- `src/pty_input_tests.rs`, `src/navigation_tests.rs`, `src/app_tests.rs`
- `src/main.rs` (module registration)

**Files scanned:** 10 production files + 3 test files
**Pattern extraction date:** 2026-04-24
**Upstream docs:** `.planning/phases/06-text-selection/06-CONTEXT.md`, `.planning/phases/06-text-selection/06-RESEARCH.md`

## PATTERN MAPPING COMPLETE
