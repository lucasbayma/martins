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

// =============================================================================
// TM-DISPATCH-01..04 — vt100 mouse-mode + alternate-screen as delegate signal
// (Plan 07-04 — gates `App::active_session_delegates_to_tmux`)
//
// We do NOT construct a full App fixture here (App requires a populated
// project/workspace tree, modal state, keymap, etc. — out of budget). Instead
// we assert against vt100::Screen state directly: the helper is a one-line
// match over `screen.mouse_protocol_mode() == None && !screen.alternate_screen()`,
// so verifying vt100 tracks the DECSET sequences correctly closes the loop.
//
// End-to-end behavior (handle_mouse → forward bytes when delegate==true) is
// verified by Plan 07-06 manual UAT (UAT-7-A..K dual-path Ghostty parity).
//
// Note on DECSET sequences: `\x1b[?1006h` (SGR encoding flag) does NOT flip
// `mouse_protocol_mode` — it only toggles wire format once a tracking mode
// is on. To enter a tracking mode that flips the enum away from `None`,
// programs send `\x1b[?1000h` (X10/PressRelease), `\x1b[?1002h` (button-event),
// or `\x1b[?1003h` (any-event). Tests below use 1000h. (Plan 07-04's
// PLAN.md examples used 1006h verbatim; verified inert against vt100 0.16.2
// during Wave 2 implementation — switched to 1000h. See
// 07-04-SUMMARY.md §Deviations.)
// =============================================================================

#[test]
fn drag_delegates_to_tmux_when_no_mouse_mode() {
    // TM-DISPATCH-01: freshly-spawned shell session has no mouse mode and no
    // alternate screen — App::active_session_delegates_to_tmux would return true.
    let session = crate::pty::session::PtySession::spawn(
        std::env::temp_dir(),
        "/bin/cat",
        &[],
        24,
        80,
    )
    .expect("spawn /bin/cat");
    // Allow brief moment for the PTY to initialize.
    std::thread::sleep(std::time::Duration::from_millis(50));
    let parser = session.parser.read().expect("parser read");
    let screen = parser.screen();
    assert_eq!(
        screen.mouse_protocol_mode(),
        vt100::MouseProtocolMode::None,
        "fresh shell session must have mouse_protocol_mode == None"
    );
    assert!(
        !screen.alternate_screen(),
        "fresh shell session must NOT be on alternate screen"
    );
}

#[test]
fn drag_uses_overlay_when_inner_mouse_mode() {
    // TM-DISPATCH-02: feed DECSET 1000 (X10/PressRelease tracking on); delegate
    // should flip to false because mouse_protocol_mode is now non-None.
    let session = crate::pty::session::PtySession::spawn(
        std::env::temp_dir(),
        "/bin/cat",
        &[],
        24,
        80,
    )
    .expect("spawn /bin/cat");
    std::thread::sleep(std::time::Duration::from_millis(50));
    {
        let mut parser = session.parser.write().expect("parser write");
        parser.process(b"\x1b[?1000h");
    }
    let parser = session.parser.read().expect("parser read");
    let screen = parser.screen();
    assert_ne!(
        screen.mouse_protocol_mode(),
        vt100::MouseProtocolMode::None,
        "after DECSET 1000h, mouse_protocol_mode must be non-None — overlay path should run"
    );
}

#[test]
fn drag_uses_overlay_when_alternate_screen() {
    // TM-DISPATCH-03: feed DECSET 1049 (alternate screen on); delegate should be false
    // even if mouse_protocol_mode is still None (vim/htop case).
    let session = crate::pty::session::PtySession::spawn(
        std::env::temp_dir(),
        "/bin/cat",
        &[],
        24,
        80,
    )
    .expect("spawn /bin/cat");
    std::thread::sleep(std::time::Duration::from_millis(50));
    {
        let mut parser = session.parser.write().expect("parser write");
        parser.process(b"\x1b[?1049h");
    }
    let parser = session.parser.read().expect("parser read");
    let screen = parser.screen();
    assert!(
        screen.alternate_screen(),
        "after DECSET 1049h, screen must report alternate_screen == true — overlay path should run"
    );
}

#[test]
fn delegate_flips_on_mouse_mode_set_reset() {
    // TM-DISPATCH-04: confirm vt100 tracks mouse-tracking DECSET set/reset symmetrically.
    // Uses 1000h/1000l (X10/PressRelease tracking) which is the actual mode-toggle
    // sequence; 1006h is purely an SGR-encoding flag and does not flip the enum.
    let session = crate::pty::session::PtySession::spawn(
        std::env::temp_dir(),
        "/bin/cat",
        &[],
        24,
        80,
    )
    .expect("spawn /bin/cat");
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Initial: None.
    {
        let parser = session.parser.read().expect("parser read");
        assert_eq!(parser.screen().mouse_protocol_mode(), vt100::MouseProtocolMode::None);
    }
    // Set 1000: must flip non-None.
    {
        let mut parser = session.parser.write().expect("parser write");
        parser.process(b"\x1b[?1000h");
    }
    {
        let parser = session.parser.read().expect("parser read");
        assert_ne!(parser.screen().mouse_protocol_mode(), vt100::MouseProtocolMode::None);
    }
    // Reset 1000: must flip back to None.
    {
        let mut parser = session.parser.write().expect("parser write");
        parser.process(b"\x1b[?1000l");
    }
    {
        let parser = session.parser.read().expect("parser read");
        assert_eq!(parser.screen().mouse_protocol_mode(), vt100::MouseProtocolMode::None);
    }
}
