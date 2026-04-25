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
