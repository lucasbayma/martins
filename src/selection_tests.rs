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
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
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
