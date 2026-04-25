//! Phase 7 — tmux-native main-screen selection.
//!
//! Test gates: TM-ENC-01..06 (this plan, 07-01), TM-DISPATCH-01..04 (07-04),
//! TM-CMDC-01..03 (07-05), TM-ESC-01..03 (07-05), TM-CONF-01 (07-02 — actually
//! lives inline in src/tmux.rs::tests), TM-CANCEL-01 (07-03).
//!
//! See `.planning/phases/07-tmux-native-main-screen-selection/07-VALIDATION.md` and
//! `.planning/phases/07-tmux-native-main-screen-selection/07-RESEARCH.md` §Validation Architecture.

#![cfg(test)]

use crate::events::encode_sgr_mouse;
use crossterm::event::{KeyModifiers, MouseButton, MouseEventKind};

// =============================================================================
// TM-ENC-01..06 — encode_sgr_mouse pure-fn unit tests (Plan 07-01)
// =============================================================================

#[test]
fn encode_sgr_down_left_no_mods() {
    let bytes = encode_sgr_mouse(
        MouseEventKind::Down(MouseButton::Left),
        KeyModifiers::NONE,
        9,
        4,
    )
    .expect("Down(Left) should encode");
    assert_eq!(bytes, b"\x1b[<0;10;5M", "TM-ENC-01: Down(Left) at (9,4) → press, button=0, 1-based coords");
}

#[test]
fn encode_sgr_drag_left_no_mods() {
    let bytes = encode_sgr_mouse(
        MouseEventKind::Drag(MouseButton::Left),
        KeyModifiers::NONE,
        9,
        4,
    )
    .expect("Drag(Left) should encode");
    assert_eq!(bytes, b"\x1b[<32;10;5M", "TM-ENC-02: Drag(Left) → motion bit (32) + button=0");
}

#[test]
fn encode_sgr_up_left_release() {
    let bytes = encode_sgr_mouse(
        MouseEventKind::Up(MouseButton::Left),
        KeyModifiers::NONE,
        9,
        4,
    )
    .expect("Up(Left) should encode");
    assert_eq!(bytes, b"\x1b[<0;10;5m", "TM-ENC-03: Up(Left) → lowercase 'm' = release");
}

#[test]
fn encode_sgr_down_left_shift() {
    let bytes = encode_sgr_mouse(
        MouseEventKind::Down(MouseButton::Left),
        KeyModifiers::SHIFT,
        9,
        4,
    )
    .expect("Shift+Down(Left) should encode");
    assert_eq!(bytes, b"\x1b[<4;10;5M", "TM-ENC-04: D-18 shift+click extend, 0+4=4");
}

#[test]
fn encode_sgr_down_left_alt() {
    let bytes = encode_sgr_mouse(
        MouseEventKind::Down(MouseButton::Left),
        KeyModifiers::ALT,
        9,
        4,
    )
    .expect("Alt+Down(Left) should encode");
    assert_eq!(bytes, b"\x1b[<8;10;5M", "TM-ENC-05: rectangle-select bonus, 0+8=8");
}

#[test]
fn encode_sgr_drag_left_shift_alt() {
    let bytes = encode_sgr_mouse(
        MouseEventKind::Drag(MouseButton::Left),
        KeyModifiers::SHIFT | KeyModifiers::ALT,
        9,
        4,
    )
    .expect("Shift+Alt+Drag(Left) should encode");
    assert_eq!(bytes, b"\x1b[<44;10;5M", "TM-ENC-06: 32 (motion) + 4 (shift) + 8 (alt) = 44");
}

// Negative tests — confirm encoder filters out events that should NOT be forwarded.

#[test]
fn encode_sgr_moved_returns_none() {
    let result = encode_sgr_mouse(
        MouseEventKind::Moved,
        KeyModifiers::NONE,
        9,
        4,
    );
    assert!(result.is_none(), "Moved-without-button must NOT be forwarded as SGR");
}

#[test]
fn encode_sgr_down_right_returns_none() {
    let result = encode_sgr_mouse(
        MouseEventKind::Down(MouseButton::Right),
        KeyModifiers::NONE,
        9,
        4,
    );
    assert!(result.is_none(), "Right-click is out of scope per CONTEXT.md Deferred");
}
