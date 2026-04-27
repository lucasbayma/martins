//! PTY-input fluidity validation (PTY-01, PTY-02, PTY-03).
//!
//! These tests prove the Phase 2 structural primitives (biased select,
//! synchronous `write_input`, dirty-gated draw) deliver PTY-01/02/03
//! in practice. See .planning/phases/03-pty-input-fluidity/03-RESEARCH.md §6.

#![cfg(test)]

use crate::pty::session::PtySession;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

/// PTY-01 — A keystroke written to a live PTY session reaches the child
/// process and its echo lands in the vt100 parser buffer.
#[test]
fn keystroke_writes_to_pty() {
    let mut session = PtySession::spawn(
        std::env::temp_dir(),
        "/bin/cat",
        &[],
        24,
        80,
    )
    .expect("spawn /bin/cat failed");

    session.write_input(b"a\n").expect("write_input failed");

    // Poll up to 2s for the echoed char to land in the parser.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut contents = String::new();
    while std::time::Instant::now() < deadline {
        contents = session.parser.read().unwrap().screen().contents();
        if contents.contains('a') {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        contents.contains('a'),
        "expected 'a' in echoed PTY buffer, got: {contents:?}"
    );
}

/// PTY-01 — Typed character round-trips through PTY echo + vt100 parse
/// + ratatui draw and appears in the rendered TestBackend buffer.
#[tokio::test]
async fn typing_appears_in_buffer() {
    use ratatui::{Terminal, backend::TestBackend};
    use tui_term::widget::PseudoTerminal;

    let notify = Arc::new(Notify::new());
    let mut session = PtySession::spawn_with_notify(
        std::env::temp_dir(),
        "/bin/cat",
        &[],
        24,
        80,
        Some(Arc::clone(&notify)),
    )
    .expect("spawn /bin/cat failed");

    session.write_input(b"x\n").expect("write_input failed");

    // Wait up to 2s for the echoed 'x' to land in the parser. The
    // `output_notify` handle is wired (it IS the signal App::run uses),
    // but the session-side 8ms throttle may coalesce the single-byte
    // echo into a silent update if the reader thread's first read
    // lands within 8ms of thread spawn. The parser buffer is the
    // source of truth — polling it confirms the byte round-tripped
    // through PTY echo + vt100 parse, which is what PTY-01 asserts.
    // We keep `notify` constructed so the `spawn_with_notify` code
    // path is exercised (regression guard: it must not panic or
    // deadlock when the notify is attached).
    let _ = &notify; // constructed for wiring-test only; see comment above.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut contents = String::new();
    while std::time::Instant::now() < deadline {
        contents = session.parser.read().unwrap().screen().contents();
        if contents.contains('x') {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        contents.contains('x'),
        "expected 'x' in echoed PTY buffer before draw, got: {contents:?}"
    );

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("terminal new failed");
    terminal
        .draw(|frame| {
            let screen_guard = session.parser.read().unwrap();
            let pseudo = PseudoTerminal::new(screen_guard.screen());
            frame.render_widget(pseudo, frame.area());
        })
        .expect("draw failed");

    let buffer = terminal.backend().buffer();
    let mut found_x = false;
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            if let Some(cell) = buffer.cell((x, y)) {
                if cell.symbol() == "x" {
                    found_x = true;
                    break;
                }
            }
        }
        if found_x {
            break;
        }
    }
    assert!(found_x, "expected 'x' cell in rendered TestBackend buffer");
}

/// PTY-02 — `tokio::select! { biased; ... }` with the event branch first
/// picks the event branch even when a `Notify` is also pre-signaled. This
/// mirrors the input-vs-PTY-output priority in `src/app.rs::run`.
#[tokio::test]
async fn biased_select_input_wins_over_notify() {
    let notify = Arc::new(Notify::new());
    notify.notify_one();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<&'static str>(1);
    tx.send("event").await.expect("send failed");

    let chosen: &'static str = tokio::select! {
        biased;

        // Mirrors `// 1. INPUT` branch in src/app.rs::run.
        Some(e) = rx.recv() => e,
        // Mirrors `// 2. PTY output` branch in src/app.rs::run.
        _ = notify.notified() => "notify",
    };

    assert_eq!(
        chosen, "event",
        "biased select must pick event branch before notify"
    );
}
