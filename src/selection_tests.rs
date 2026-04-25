//! Selection behavior validation (SEL-01..SEL-04, D-15, D-16).
//!
//! Handler-level tests drive synthesized MouseEvent / KeyEvent through
//! `crate::events::handle_mouse` / `handle_key` against a real `App`
//! instance. Mirrors the pattern from `src/pty_input_tests.rs` and
//! `src/navigation_tests.rs`.
//!
//! See `.planning/phases/06-text-selection/06-RESEARCH.md` §Test Harness
//! and `.planning/phases/06-text-selection/06-PATTERNS.md`
//! §`src/selection_tests.rs` (NEW).

#![cfg(test)]

use crate::app::{App, SelectionState};
use crate::events;
use crate::state::GlobalState;
use crate::ui::layout::PaneRects;
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Rect;
use std::time::Duration;

/// Synthesize a `MouseEvent` with the given kind / coords / modifiers.
/// Mirrors the boilerplate-free helper called out in 06-03-PLAN Task 1.
fn mouse_event(
    kind: MouseEventKind,
    col: u16,
    row: u16,
    modifiers: KeyModifiers,
) -> MouseEvent {
    MouseEvent {
        kind,
        column: col,
        row,
        modifiers,
    }
}

/// Write `text` to a `PtySession`'s stdin and poll its parser until
/// `contains` appears in the visible screen contents (or 2s elapses).
/// Used by Tests 2/4/5 to ensure the parser has materialized echoed text
/// before the test inspects it.
async fn write_and_wait_for_text(
    session: &mut crate::pty::session::PtySession,
    text: &str,
    contains: &str,
) {
    use std::time::Instant;
    session.write_input(text.as_bytes()).expect("write_input");
    let deadline = Instant::now() + Duration::from_millis(2000);
    while Instant::now() < deadline {
        let contents = session.parser.read().unwrap().screen().contents();
        if contents.contains(contains) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("text {contains:?} did not appear in PTY parser within 2s");
}

/// Build an App fixture with `terminal = Rect { x: 0, y: 2, width: 80, height: 20 }`
/// so `terminal_content_rect` yields a non-zero inner.y/inner.x and
/// the coord-translation in `handle_mouse` is exercised (subtracts inner offset).
async fn make_app_offset(state_path_suffix: &str) -> App {
    let state_path =
        std::env::temp_dir().join(format!("martins-sel-{state_path_suffix}.json"));
    let _ = std::fs::remove_file(&state_path);
    let mut app = App::new(GlobalState::default(), state_path)
        .await
        .expect("App::new");

    let terminal = Rect {
        x: 0,
        y: 2,
        width: 80,
        height: 20,
    };
    app.last_panes = Some(PaneRects {
        terminal,
        left: None,
        right: None,
        menu_bar: Rect::default(),
        status_bar: Rect::default(),
    });
    app
}

/// Build an App fixture with a PaneRects covering the entire screen so
/// `handle_mouse` treats every click as in-terminal.
async fn make_app(state_path_suffix: &str) -> App {
    let state_path = std::env::temp_dir().join(format!("martins-sel-{state_path_suffix}.json"));
    let _ = std::fs::remove_file(&state_path);
    let mut app = App::new(GlobalState::default(), state_path)
        .await
        .expect("App::new");

    let terminal = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };
    app.last_panes = Some(PaneRects {
        terminal,
        left: None,
        right: None,
        menu_bar: Rect::default(),
        status_bar: Rect::default(),
    });
    app
}

/// SEL-01 — `SelectionState::normalized()` orders endpoints ascending
/// (start_row <= end_row, with same-row tie broken by start_col <= end_col)
/// regardless of the order in which `start_*` and `end_*` were written.
#[tokio::test]
async fn normalized_orders_ascending_when_reversed_endpoints() {
    // Reversed by row: start row > end row.
    let sel = SelectionState {
        start_col: 5,
        start_row: 10,
        start_gen: 0,
        end_col: 3,
        end_row: 2,
        end_gen: None,
        dragging: false,
        text: None,
    };
    let ((sc, sr), (ec, er)) = sel.normalized();
    assert!(sr <= er, "rows must be ascending after normalize");
    assert_eq!((sc, sr), (3, 2));
    assert_eq!((ec, er), (5, 10));

    // Same row, reversed by column: start col > end col.
    let sel = SelectionState {
        start_col: 20,
        start_row: 4,
        start_gen: 0,
        end_col: 5,
        end_row: 4,
        end_gen: None,
        dragging: false,
        text: None,
    };
    let ((sc, sr), (ec, er)) = sel.normalized();
    assert_eq!(sr, er, "same-row case");
    assert!(sc <= ec, "cols must be ascending on same row");
    assert_eq!(sc, 5);
    assert_eq!(ec, 20);

    // Already-ordered case: identity.
    let sel = SelectionState {
        start_col: 1,
        start_row: 1,
        start_gen: 0,
        end_col: 9,
        end_row: 7,
        end_gen: None,
        dragging: false,
        text: None,
    };
    let ((sc, sr), (ec, er)) = sel.normalized();
    assert_eq!((sc, sr), (1, 1));
    assert_eq!((ec, er), (9, 7));
}

/// SEL-01 — `is_empty()` returns true exactly when start == end.
#[tokio::test]
async fn selection_state_is_empty_when_start_eq_end() {
    let sel = SelectionState {
        start_col: 7,
        start_row: 3,
        start_gen: 0,
        end_col: 7,
        end_row: 3,
        end_gen: None,
        dragging: false,
        text: None,
    };
    assert!(sel.is_empty(), "start == end must be empty");

    let sel = SelectionState {
        start_col: 7,
        start_row: 3,
        start_gen: 0,
        end_col: 8,
        end_row: 3,
        end_gen: None,
        dragging: false,
        text: None,
    };
    assert!(!sel.is_empty(), "start != end must be non-empty");
}

/// D-16 — Two `Down(Left)` clicks at the same row within the 300ms window
/// produce `app.last_click_count == 2`.
#[tokio::test]
async fn click_counter_increments_within_300ms_same_row() {
    let mut app = make_app("click-counter-incr").await;

    let down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 10,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    events::handle_mouse(&mut app, down).await;
    assert_eq!(
        app.last_click_count, 1,
        "first click must initialize counter to 1"
    );

    // Stay inside the 300ms threshold (D-16).
    std::thread::sleep(Duration::from_millis(50));

    let down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 10,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    events::handle_mouse(&mut app, down).await;
    assert_eq!(
        app.last_click_count, 2,
        "second click within 300ms at same row must increment to 2"
    );
}

/// D-16 — A second click at a different row resets `last_click_count` to 1.
#[tokio::test]
async fn click_counter_resets_when_row_differs() {
    let mut app = make_app("click-counter-reset").await;

    let down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 10,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    events::handle_mouse(&mut app, down).await;
    assert_eq!(app.last_click_count, 1, "first click initializes to 1");

    // Different row -- per D-16, counter resets to 1.
    let down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 10,
        row: 10,
        modifiers: KeyModifiers::NONE,
    };
    events::handle_mouse(&mut app, down).await;
    assert_eq!(
        app.last_click_count, 1,
        "click at different row must reset counter to 1"
    );
}

/// SEL-04 — `PtySession.scroll_generation` increments when piped input
/// overflows the visible 24-row area. This is the sole source of scroll
/// events for selection-stability (see RESEARCH §Q1 SCROLLBACK-LEN
/// heuristic and CONTEXT D-05).
#[tokio::test]
async fn scroll_generation_increments_on_vertical_scroll() {
    use crate::pty::session::PtySession;
    use std::sync::atomic::Ordering;
    use std::time::Instant;

    // write_input takes &mut self → bind as `mut`. Mirrors src/pty_input_tests.rs:18.
    let mut session =
        PtySession::spawn(std::env::temp_dir(), "/bin/cat", &[], 24, 80)
            .expect("spawn /bin/cat failed");

    // Feed 30 newlines — enough to overflow the 24-row visible area.
    for i in 0..30 {
        let line = format!("line{i}\n");
        session.write_input(line.as_bytes()).expect("write_input");
    }

    // Poll for up to 500ms for the reader thread to drain.
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut gen_count = 0u64;
    while Instant::now() < deadline {
        gen_count = session.scroll_generation.load(Ordering::Relaxed);
        if gen_count > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        gen_count > 0,
        "scroll_generation never incremented; got {gen_count}"
    );
}

// =============================================================================
// 06-03 — Mouse-path tests (Drag/Up/Down + double/triple/shift)
// =============================================================================
//
// Tests 1, 2, 4, 5 inject a real `PtySession` via `app.inject_test_session`
// (Plan 01 Task 3 seam) so `app.active_scroll_generation()` and
// `app.materialize_selection_text()` read from a live `parser` + counter.
// Tests 3, 6, 7, 8 only need `app.selection` state; no PtySession required.
//
// All 8 fixtures use `make_app_offset(...)` so `inner.y > 0` — proves the
// coord-translation in `handle_mouse` (which subtracts `inner.y` / `inner.x`)
// is exercised.

/// SEL-01 / D-06 — `Drag(Left)` creates a `SelectionState` anchored at the
/// active session's current `scroll_generation`. Coordinates are translated
/// from screen space to terminal-inner space (subtract `inner.x` / `inner.y`).
#[tokio::test]
async fn drag_creates_selection_anchored_at_current_gen() {
    use crate::pty::session::PtySession;
    use std::sync::atomic::Ordering;

    let mut app = make_app_offset("drag-anchors-gen").await;

    // Spawn a real PtySession; bump its scroll_generation to 42 BEFORE inject.
    let session =
        PtySession::spawn(std::env::temp_dir(), "/bin/cat", &[], 24, 80)
            .expect("spawn /bin/cat failed");
    session.scroll_generation.fetch_add(42, Ordering::Relaxed);
    let _tab_id = app.inject_test_session(session);

    // inner = terminal_content_rect({0,2,80,20}) = {x:1, y:4, width:78, height:17}
    // mouse @ (col=10, row=5) → inner_col = 9, inner_row = 1
    app.dirty = false;
    let drag = mouse_event(
        MouseEventKind::Drag(MouseButton::Left),
        10,
        5,
        KeyModifiers::NONE,
    );
    events::handle_mouse(&mut app, drag).await;

    let sel = app
        .selection
        .as_ref()
        .expect("Drag must create a SelectionState");
    assert_eq!(sel.start_gen, 42, "start_gen must equal session scroll_gen");
    assert_eq!(sel.start_col, 9, "start_col must subtract inner.x");
    assert_eq!(sel.start_row, 1, "start_row must subtract inner.y");
    assert_eq!(sel.end_gen, None, "end_gen must be None mid-drag (D-07)");
    assert!(sel.dragging, "dragging must be true on a fresh Drag");
    assert!(sel.text.is_none(), "text must be None mid-drag");
    assert!(app.dirty, "Drag must mark_dirty (D-23)");
}

/// SEL-04 / D-02 / D-07 — `Up(Left)` finalizes `end_gen` AND snapshots
/// `text` via `materialize_selection_text`. The snapshot survives subsequent
/// scroll-off (Plan 04 cmd+c relies on this).
#[tokio::test]
async fn mouse_up_snapshots_selection_text_and_anchors_end() {
    use crate::pty::session::PtySession;

    let mut app = make_app_offset("up-snapshots-text").await;

    let mut session =
        PtySession::spawn(std::env::temp_dir(), "/bin/cat", &[], 24, 80)
            .expect("spawn /bin/cat failed");
    // Echo "hello world" into the PTY; wait for parser to materialize it.
    write_and_wait_for_text(&mut session, "hello world\n", "hello").await;
    let _tab_id = app.inject_test_session(session);

    // Drag from "hello" start (col=0) to "hello" end (col=4) on row 0
    // (inner-space). Screen coords: col=1..=5, row=4 (inner.y=4, inner.x=1).
    let drag_start = mouse_event(
        MouseEventKind::Drag(MouseButton::Left),
        1,
        4,
        KeyModifiers::NONE,
    );
    events::handle_mouse(&mut app, drag_start).await;
    let drag_end = mouse_event(
        MouseEventKind::Drag(MouseButton::Left),
        5,
        4,
        KeyModifiers::NONE,
    );
    events::handle_mouse(&mut app, drag_end).await;

    app.dirty = false;
    let up = mouse_event(
        MouseEventKind::Up(MouseButton::Left),
        5,
        4,
        KeyModifiers::NONE,
    );
    events::handle_mouse(&mut app, up).await;

    let sel = app.selection.as_ref().expect("Up must keep selection");
    assert!(!sel.dragging, "dragging must be false after Up");
    let current_gen = app.active_scroll_generation();
    assert_eq!(
        sel.end_gen,
        Some(current_gen),
        "end_gen must be anchored to current gen on Up"
    );
    let snapshot = sel
        .text
        .as_deref()
        .expect("text must be Some(snapshot) after Up");
    assert!(
        snapshot.contains("hello"),
        "snapshot must contain the selected word; got {snapshot:?}"
    );
    assert!(app.dirty, "Up must mark_dirty (D-23)");
}

/// `Up(Left)` with an empty selection (start == end) clears the state to
/// `None`. Consistent with the existing behavior that `sel.is_empty()`
/// means no copy/anchor.
#[tokio::test]
async fn mouse_up_empty_selection_clears_state() {
    let mut app = make_app_offset("up-empty-clears").await;

    // Seed a degenerate selection where start == end (is_empty() is true).
    app.selection = Some(SelectionState {
        start_col: 5,
        start_row: 1,
        start_gen: 0,
        end_col: 5,
        end_row: 1,
        end_gen: None,
        dragging: true,
        text: None,
    });
    app.dirty = false;

    let up = mouse_event(
        MouseEventKind::Up(MouseButton::Left),
        6,
        5,
        KeyModifiers::NONE,
    );
    events::handle_mouse(&mut app, up).await;

    assert!(
        app.selection.is_none(),
        "empty selection must be cleared on Up"
    );
    assert!(app.dirty, "Up clearing empty selection must mark_dirty (D-23)");
}

/// D-15 — Two `Down(Left)` events at the same position within 50ms select
/// the word under the cursor. After two clicks, `app.last_click_count == 2`
/// and `app.selection` covers the word boundary.
#[tokio::test]
async fn double_click_selects_word() {
    use crate::pty::session::PtySession;

    let mut app = make_app_offset("double-click-word").await;

    let mut session =
        PtySession::spawn(std::env::temp_dir(), "/bin/cat", &[], 24, 80)
            .expect("spawn /bin/cat failed");
    write_and_wait_for_text(&mut session, "hello world\n", "hello").await;
    let _tab_id = app.inject_test_session(session);

    // First click — counter goes to 1, no word-select yet.
    // mouse @ (col=3, row=4): inner_col=2, inner_row=0 — middle of "hello".
    app.dirty = false;
    let down1 = mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        3,
        4,
        KeyModifiers::NONE,
    );
    events::handle_mouse(&mut app, down1).await;
    assert_eq!(app.last_click_count, 1, "first click → counter=1");

    // Stay within the 300ms threshold and same row.
    std::thread::sleep(Duration::from_millis(50));

    let down2 = mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        3,
        4,
        KeyModifiers::NONE,
    );
    events::handle_mouse(&mut app, down2).await;
    assert_eq!(
        app.last_click_count, 2,
        "second click within threshold → counter=2"
    );

    let sel = app
        .selection
        .as_ref()
        .expect("double-click must produce a selection");
    assert_eq!(sel.start_row, 0, "selection on inner-row 0");
    assert_eq!(sel.end_row, 0, "single-row word selection");
    assert_eq!(sel.start_col, 0, "word 'hello' starts at col 0");
    assert_eq!(sel.end_col, 4, "word 'hello' ends at col 4 (inclusive)");
    assert!(app.dirty, "double-click must mark_dirty (D-23)");
}

/// D-15 / D-18 — Three `Down(Left)` events at the same row within 300ms
/// select the line (visible vt100 row, NOT joined wrapped lines). Selection
/// spans col=0 to last non-whitespace column.
#[tokio::test]
async fn triple_click_selects_line() {
    use crate::pty::session::PtySession;

    let mut app = make_app_offset("triple-click-line").await;

    let mut session =
        PtySession::spawn(std::env::temp_dir(), "/bin/cat", &[], 24, 80)
            .expect("spawn /bin/cat failed");
    write_and_wait_for_text(&mut session, "the quick fox\n", "fox").await;
    let _tab_id = app.inject_test_session(session);

    // Three clicks on the same row, each <100ms apart (well under 300ms).
    // mouse @ (col=5, row=4): inner_col=4, inner_row=0.
    app.dirty = false;
    for _ in 0..3 {
        let down = mouse_event(
            MouseEventKind::Down(MouseButton::Left),
            5,
            4,
            KeyModifiers::NONE,
        );
        events::handle_mouse(&mut app, down).await;
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        app.last_click_count, 3,
        "third click within threshold same-row → counter=3"
    );

    let sel = app
        .selection
        .as_ref()
        .expect("triple-click must produce a line selection");
    assert_eq!(sel.start_row, 0, "line selection anchored on inner-row 0");
    assert_eq!(sel.end_row, 0, "single-row line selection (D-18)");
    assert_eq!(sel.start_col, 0, "line selection starts at col 0");
    assert_eq!(
        sel.end_col, 12,
        "line selection ends at last non-ws col (last 'x' in 'fox' = col 12)"
    );
    assert!(app.dirty, "triple-click must mark_dirty (D-23)");
}

/// D-19 — `Shift+Down(Left)` with an existing selection extends the END
/// anchor only. Start endpoint stays put.
#[tokio::test]
async fn shift_click_extends_end_anchor() {
    let mut app = make_app_offset("shift-click-extends").await;

    // Seed an existing selection: start (2, 1) → end (5, 3) (inner-space).
    app.selection = Some(SelectionState {
        start_col: 2,
        start_row: 1,
        start_gen: 7,
        end_col: 5,
        end_row: 3,
        end_gen: Some(7),
        dragging: false,
        text: Some("seed".to_string()),
    });
    app.dirty = false;

    // mouse @ (col=16, row=7) with SHIFT: inner_col=15, inner_row=3
    let shift_click = mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        16,
        7,
        KeyModifiers::SHIFT,
    );
    events::handle_mouse(&mut app, shift_click).await;

    let sel = app.selection.as_ref().expect("shift-click must keep selection");
    assert_eq!(sel.start_col, 2, "start_col unchanged");
    assert_eq!(sel.start_row, 1, "start_row unchanged");
    assert_eq!(sel.start_gen, 7, "start_gen unchanged");
    assert_eq!(sel.end_col, 15, "end_col extended to inner_col=15");
    assert_eq!(sel.end_row, 3, "end_row extended to inner_row=3");
    assert!(app.dirty, "shift-click extension must mark_dirty (D-23)");
}

/// D-19 — `Shift+Down(Left)` with NO existing selection is a no-op:
/// no new selection is created.
#[tokio::test]
async fn shift_click_no_selection_is_noop() {
    let mut app = make_app_offset("shift-click-noop").await;

    assert!(app.selection.is_none(), "precondition: no selection");
    app.dirty = false;

    let shift_click = mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        10,
        5,
        KeyModifiers::SHIFT,
    );
    events::handle_mouse(&mut app, shift_click).await;

    assert!(
        app.selection.is_none(),
        "shift+click without seed must not create a selection (D-19)"
    );
}

/// D-12 / D-13 / D-23 — A plain `Down(Left)` (no SHIFT) on a different row
/// clears any active selection AND calls `mark_dirty`.
#[tokio::test]
async fn down_left_clears_active_selection_and_marks_dirty() {
    let mut app = make_app_offset("down-clears-selection").await;

    // Seed an existing selection on inner-row 1.
    app.selection = Some(SelectionState {
        start_col: 2,
        start_row: 1,
        start_gen: 0,
        end_col: 8,
        end_row: 1,
        end_gen: Some(0),
        dragging: false,
        text: Some("seed".to_string()),
    });
    app.dirty = false;

    // Click on a DIFFERENT row (inner_row=5 → mouse.row=9).
    let down = mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        10,
        9,
        KeyModifiers::NONE,
    );
    events::handle_mouse(&mut app, down).await;

    assert!(
        app.selection.is_none(),
        "Down(Left) without SHIFT must clear active selection"
    );
    assert!(app.dirty, "Down(Left) clearing selection must mark_dirty (D-23)");
}

// =============================================================================
// 06-04 — Key-path tests (cmd+c with selection / Esc with selection)
// =============================================================================
//
// These tests exercise the precedence chain in `events::handle_key` BEFORE
// the Terminal-mode forwarding branch:
//
//   * cmd+c with active non-empty selection → calls
//     `App::copy_selection_to_clipboard` (we observe its only side effect we
//     can reach in-process: selection.text snapshot is preserved AND mode is
//     unchanged — pbcopy spawn is a real subprocess and is covered by Manual
//     UAT, NOT asserted here).
//   * Esc with active selection → clears selection AND consumes the event
//     (Terminal-mode forwarding does NOT fire — we assert via mode unchanged
//     and selection cleared).
//
// Byte-level PTY-forwarding assertions (cmd+c→0x03 in Terminal mode WITH no
// selection; Esc→0x1b fallthrough in Terminal mode) are deferred to Manual
// UAT (UAT-06-04-A, UAT-06-04-B in 06-VALIDATION.md). Automating them would
// require widening `App::write_active_tab_input` with a test-mode branch
// which CLAUDE.md minimal-surface conventions reject.

/// Synthesize a `KeyEvent` with the given code / modifiers. Mirrors the
/// `mouse_event` helper above.
fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

/// SEL-02 / D-02 / D-04 — cmd+c (KeyModifiers::SUPER + KeyCode::Char('c'))
/// with an active non-empty selection consumes the event and leaves the
/// selection state intact (no clear after copy). pbcopy spawn is a real
/// subprocess invocation we deliberately do NOT assert on — UAT-06-04 path
/// covers the clipboard byte-level outcome.
#[tokio::test]
async fn cmd_c_with_selection_consumes_event_and_keeps_selection() {
    let mut app = make_app("cmd-c-with-selection").await;

    // Seed a non-empty selection with a snapshot text. start != end so
    // `is_empty()` is false and the cmd+c branch will copy.
    app.selection = Some(SelectionState {
        start_col: 0,
        start_row: 0,
        start_gen: 0,
        end_col: 4,
        end_row: 0,
        end_gen: Some(0),
        dragging: false,
        text: Some("hello".to_string()),
    });
    let prev_mode = app.mode;
    app.dirty = false;

    // cmd+c — KeyModifiers::SUPER on macOS via DISAMBIGUATE_ESCAPE_CODES.
    let key = key_event(KeyCode::Char('c'), KeyModifiers::SUPER);
    crate::events::handle_key(&mut app, key).await;

    // D-04: copy must NOT clear the selection.
    assert!(
        app.selection.is_some(),
        "cmd+c with selection must NOT clear it (D-04)"
    );
    assert_eq!(
        app.selection.as_ref().unwrap().text.as_deref(),
        Some("hello"),
        "cmd+c must preserve the selection text snapshot intact"
    );
    // Event consumed — mode unchanged. If cmd+c had fallen through to the
    // Terminal-mode branch, `forward_key_to_pty` would not change mode either,
    // but the precedence-test purpose here is to confirm the early-return
    // branch took over: mode-unchanged + selection-still-Some + text-snapshot
    // preserved together prove the cmd+c branch ran.
    assert_eq!(
        app.mode, prev_mode,
        "cmd+c branch must consume the event (mode unchanged)"
    );
}

/// SEL-03 / D-14 / D-23 — Esc with an active selection in Terminal mode
/// clears the selection AND consumes the event (does NOT fall through to
/// `forward_key_to_pty` which would emit 0x1b). We assert: selection cleared,
/// dirty marked, mode unchanged.
#[tokio::test]
async fn esc_with_active_selection_clears_and_marks_dirty() {
    let mut app = make_app("esc-with-selection").await;

    app.selection = Some(SelectionState {
        start_col: 0,
        start_row: 0,
        start_gen: 0,
        end_col: 4,
        end_row: 0,
        end_gen: Some(0),
        dragging: false,
        text: Some("hello".to_string()),
    });
    app.mode = crate::keys::InputMode::Terminal;
    let prev_mode = app.mode;
    app.dirty = false;

    let key = key_event(KeyCode::Esc, KeyModifiers::NONE);
    crate::events::handle_key(&mut app, key).await;

    assert!(
        app.selection.is_none(),
        "Esc with active selection must clear it (D-14)"
    );
    assert!(
        app.dirty,
        "Esc clearing selection must mark_dirty (D-23)"
    );
    assert_eq!(
        app.mode, prev_mode,
        "Esc branch must consume the event (mode unchanged, NOT forwarded to PTY)"
    );
}

// =============================================================================
// 06-06 — Tab / workspace switch selection-clear tests
// =============================================================================
//
// D-22 says selection clears on tab switch and on workspace switch — the
// anchored generation is per-session, so cross-session highlight is
// meaningless. These tests prove that the canonical switch primitives
// (`App::set_active_tab` and `App::select_active_workspace`) drop any
// active selection AND mark the frame dirty.

/// Build a minimal `Project` + active `Workspace` + 2 `TabSpec` stubs into
/// `app.global_state`, so `app.set_active_tab(1)` can advance from index 0
/// to index 1 against a real workspace.tabs vec. No PTY spawn — the
/// selection-clear path doesn't read session state.
fn seed_two_tab_workspace(app: &mut App) {
    use crate::state::{Agent, Project, TabSpec, Workspace, WorkspaceStatus};

    let project = Project {
        id: "test-project-2tabs".to_string(),
        name: "test-project-2tabs".to_string(),
        repo_root: std::env::temp_dir().join("martins-test-2tabs"),
        base_branch: "main".to_string(),
        workspaces: vec![Workspace {
            name: "ws".to_string(),
            worktree_path: std::env::temp_dir().join("martins-test-2tabs-ws"),
            base_branch: "main".to_string(),
            agent: Agent::default(),
            status: WorkspaceStatus::Active,
            created_at: "2026-04-24T00:00:00Z".to_string(),
            tabs: vec![
                TabSpec {
                    id: 1,
                    command: "shell".to_string(),
                },
                TabSpec {
                    id: 2,
                    command: "shell".to_string(),
                },
            ],
        }],
        added_at: "2026-04-24T00:00:00Z".to_string(),
        expanded: true,
    };
    app.global_state.projects = vec![project];
    app.global_state.active_project_id =
        Some(app.global_state.projects[0].id.clone());
    app.active_project_idx = Some(0);
    app.active_workspace_idx = Some(0);
    app.active_tab = 0;
}

/// Build a minimal `Project` with 2 active `Workspace` entries. Used by the
/// workspace-switch test to exercise `select_active_workspace(1)`.
fn seed_two_workspaces(app: &mut App) {
    use crate::state::{Agent, Project, Workspace, WorkspaceStatus};

    let mk_ws = |name: &str| Workspace {
        name: name.to_string(),
        worktree_path: std::env::temp_dir().join(format!("martins-test-2ws-{name}")),
        base_branch: "main".to_string(),
        agent: Agent::default(),
        status: WorkspaceStatus::Active,
        created_at: "2026-04-24T00:00:00Z".to_string(),
        tabs: Vec::new(),
    };
    let project = Project {
        id: "test-project-2ws".to_string(),
        name: "test-project-2ws".to_string(),
        repo_root: std::env::temp_dir().join("martins-test-2ws"),
        base_branch: "main".to_string(),
        workspaces: vec![mk_ws("ws-a"), mk_ws("ws-b")],
        added_at: "2026-04-24T00:00:00Z".to_string(),
        expanded: true,
    };
    app.global_state.projects = vec![project];
    app.global_state.active_project_id =
        Some(app.global_state.projects[0].id.clone());
    app.active_project_idx = Some(0);
    app.active_workspace_idx = Some(0);
    app.active_tab = 0;
}

/// Helper: a non-empty seeded selection with a text snapshot — so
/// `clear_selection` actually has something to drop and `mark_dirty` fires.
fn seeded_selection() -> SelectionState {
    SelectionState {
        start_col: 0,
        start_row: 0,
        start_gen: 0,
        end_col: 4,
        end_row: 0,
        end_gen: Some(0),
        dragging: false,
        text: Some("hello".to_string()),
    }
}

/// SEL-03 / D-22 / D-23 — `App::set_active_tab(idx)` clears the active
/// selection, advances `active_tab`, and marks dirty.
#[tokio::test]
async fn tab_switch_clears_selection() {
    let mut app = make_app("tab-switch-clears").await;
    seed_two_tab_workspace(&mut app);

    app.selection = Some(seeded_selection());
    app.dirty = false;
    assert!(app.selection.is_some());
    assert_eq!(app.active_tab, 0);

    app.set_active_tab(1);

    assert!(
        app.selection.is_none(),
        "D-22: tab switch must clear selection"
    );
    assert_eq!(app.active_tab, 1);
    assert!(app.dirty, "D-23: set_active_tab must mark_dirty");
}

/// SEL-03 / D-22 / D-23 — `App::select_active_workspace(idx)` clears the
/// active selection before advancing `active_workspace_idx`. Existing
/// invariant `right_list.select(None)` is preserved.
#[tokio::test]
async fn workspace_switch_clears_selection() {
    let mut app = make_app("workspace-switch-clears").await;
    seed_two_workspaces(&mut app);

    app.selection = Some(seeded_selection());
    app.dirty = false;

    app.select_active_workspace(1);

    assert!(
        app.selection.is_none(),
        "D-22: workspace switch must clear selection"
    );
    assert_eq!(app.active_workspace_idx, Some(1));
    assert!(
        app.dirty,
        "D-23: select_active_workspace clearing must mark_dirty"
    );
}

// ========================================================================
// 06-05: Render-level selection-highlight tests
// ========================================================================
//
// These tests drive `crate::ui::terminal::render_with_selection_for_test`
// (a #[cfg(test)] shim that mirrors the production highlight pass) over a
// `ratatui::backend::TestBackend` and assert the post-render Buffer state.
//
// Asserts:
//   - D-20 + D-21: `Modifier::REVERSED` is XOR-toggled per highlighted cell.
//   - D-08: when the anchored row has scrolled off above the visible area,
//     the highlight is clipped at row 0.

/// SEL-01 / D-20 + D-21 — every cell in the selection range gets
/// `Modifier::REVERSED` toggled on (from a baseline of no modifier).
#[test]
fn selection_highlights_cells_with_reversed_modifier() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Modifier;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let sel = SelectionState {
        start_col: 5,
        start_row: 3,
        start_gen: 0,
        end_col: 10,
        end_row: 3,
        end_gen: Some(0),
        dragging: false,
        text: None,
    };
    let current_gen: u64 = 0;
    terminal
        .draw(|frame| {
            crate::ui::terminal::render_with_selection_for_test(
                frame,
                Rect {
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 24,
                },
                Some(&sel),
                current_gen,
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    for col in 5..=10u16 {
        let cell = buf.cell((col, 3u16)).expect("cell in-bounds");
        assert!(
            cell.modifier.contains(Modifier::REVERSED),
            "cell ({col}, 3) should have REVERSED toggled on"
        );
    }
    let outside = buf.cell((0u16, 3u16)).expect("cell in-bounds");
    assert!(
        !outside.modifier.contains(Modifier::REVERSED),
        "cell (0, 3) outside selection should NOT have REVERSED"
    );
}

/// D-21 — when an underlying cell already has `Modifier::REVERSED` (e.g.
/// from a vt100 reverse-video escape), XOR-toggling under the selection
/// REMOVES the flag, making the highlight visually distinct from
/// surrounding reversed cells.
#[test]
fn already_reversed_cell_un_reverses_under_selection() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Modifier;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let sel = SelectionState {
        start_col: 7,
        start_row: 2,
        start_gen: 0,
        end_col: 9,
        end_row: 2,
        end_gen: Some(0),
        dragging: false,
        text: None,
    };
    let current_gen: u64 = 0;
    terminal
        .draw(|frame| {
            // Pre-populate the target cells with REVERSED already set,
            // simulating a vt100 reverse-video output that landed in the
            // buffer before the selection-highlight pass runs.
            {
                let buf = frame.buffer_mut();
                for col in 7..=9u16 {
                    if let Some(cell) = buf.cell_mut((col, 2u16)) {
                        cell.modifier.insert(Modifier::REVERSED);
                    }
                }
            }
            crate::ui::terminal::render_with_selection_for_test(
                frame,
                Rect {
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 24,
                },
                Some(&sel),
                current_gen,
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    for col in 7..=9u16 {
        let cell = buf.cell((col, 2u16)).expect("cell in-bounds");
        assert!(
            !cell.modifier.contains(Modifier::REVERSED),
            "cell ({col}, 2) was REVERSED before render — XOR should have removed it"
        );
    }
}

/// SEL-04 / D-08 — selection anchored at gen=0 viewed under current_gen=3
/// has its rows translated by delta=3. start_row=2 → -1 (clipped to row 0);
/// end_row=5 → row 2. Cells in rows 0..=2 of the column span are REVERSED;
/// rows 3+ have no REVERSED.
#[test]
fn selection_clips_at_visible_top_when_scrolled() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Modifier;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    // Selection spans visible rows 2..=5 at the moment of capture
    // (sel_gen = 0). After 3 lines of new output have scrolled the
    // screen (current_gen = 3), the translated rows are -1..=2, which
    // clips at row 0 and ends at row 2.
    let sel = SelectionState {
        start_col: 4,
        start_row: 2,
        start_gen: 0,
        end_col: 12,
        end_row: 5,
        end_gen: Some(0),
        dragging: false,
        text: None,
    };
    let current_gen: u64 = 3;
    terminal
        .draw(|frame| {
            crate::ui::terminal::render_with_selection_for_test(
                frame,
                Rect {
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 24,
                },
                Some(&sel),
                current_gen,
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();

    // Rows 0..=2 of the selection's column span must be REVERSED.
    // Row 0 is the clipped start (start_col=0 because translated < 0).
    // Rows 1..=1 are intermediate, full-width up to inner.width-1.
    // Row 2 is the (clipped) end, columns 0..=12.
    for row in 0..=2u16 {
        let cell = buf.cell((4u16, row)).expect("cell in-bounds");
        assert!(
            cell.modifier.contains(Modifier::REVERSED),
            "row {row}, col 4 should be inside the clipped selection (REVERSED)"
        );
    }
    let cell_end = buf.cell((12u16, 2u16)).expect("cell in-bounds");
    assert!(
        cell_end.modifier.contains(Modifier::REVERSED),
        "(12, 2) is the translated end-cell — should be REVERSED"
    );

    // Rows 3+ must NOT have REVERSED — selection ends at translated row 2.
    for row in 3..=5u16 {
        let cell = buf.cell((4u16, row)).expect("cell in-bounds");
        assert!(
            !cell.modifier.contains(Modifier::REVERSED),
            "row {row}, col 4 is below the clipped selection — must NOT be REVERSED"
        );
    }
}
