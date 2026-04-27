//! PTY session lifecycle: spawn, read, resize, kill.

#![allow(dead_code)]

use anyhow::{Result, anyhow};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::{Notify, oneshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PtyStatus {
    Running,
    Exited(i32),
}

pub struct PtySession {
    pub id: u64,
    pub parser: Arc<RwLock<vt100::Parser>>,
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    status: Arc<Mutex<PtyStatus>>,
    pub exit_rx: Option<oneshot::Receiver<i32>>,
    pub last_output: Arc<Mutex<std::time::Instant>>,
    /// Per-session counter incremented by the PTY reader thread when a
    /// vertical scroll is inferred (see RESEARCH §Q1 SCROLLBACK-LEN
    /// heuristic). Plans 06-03 (drag anchor) and 06-05 (render
    /// translation) read this to keep selections stable across scroll.
    pub scroll_generation: Arc<std::sync::atomic::AtomicU64>,
    /// Phase 7: set on forwarded Down(Left) when delegating to tmux; cleared
    /// on Up(Left) without prior Drag (single click, no copy-mode entered),
    /// on tab-switch (via App::set_active_tab cancel), on Esc-cancel forward,
    /// and on observed copy-mode-exit. Read by handle_key Esc / click-outside
    /// handlers (Plan 07-05) to decide whether to forward `\x1b` byte or run
    /// `tmux send-keys -X cancel`.
    ///
    /// Per 07-RESEARCH.md §State Source — Option (a) Martins-side state machine
    /// (no subprocess polling on hot paths).
    pub tmux_in_copy_mode: Arc<std::sync::atomic::AtomicBool>,
    /// Phase 7: transient flag — set on forwarded Drag(Left); read+cleared
    /// on Up(Left) to distinguish "click without drag" (Up clears in_copy_mode
    /// because tmux never entered copy-mode) from "click→drag→release" (Up
    /// keeps in_copy_mode = true because tmux is now showing a selection).
    pub tmux_drag_seen: Arc<std::sync::atomic::AtomicBool>,
    /// WR-02 (Phase 07 review): per-gesture delegation latch. Set on forwarded
    /// `Down(Left)` when delegation is active, cleared on forwarded `Up(Left)`
    /// (after the matching release reaches tmux), or on tab-switch via
    /// `App::set_active_tab`. While true, `handle_mouse` forces the
    /// forwarding branch for `Drag/Up(Left)` regardless of the live
    /// `active_session_delegates_to_tmux()` value. Prevents the orphaned
    /// half-gesture scenario where the inner program toggles DECSET 1000h /
    /// 1049h between Down and Up — without the latch, tmux would never
    /// receive the matching Up, leaving its button-state machine stuck and
    /// `tmux_in_copy_mode` permanently true until the next forwarded Up.
    pub tmux_gesture_delegating: Arc<std::sync::atomic::AtomicBool>,
}

impl PtySession {
    pub fn spawn(cwd: PathBuf, program: &str, args: &[&str], rows: u16, cols: u16) -> Result<Self> {
        Self::spawn_with_notify(cwd, program, args, rows, cols, None)
    }

    pub fn spawn_with_notify(
        cwd: PathBuf,
        program: &str,
        args: &[&str],
        rows: u16,
        cols: u16,
        output_notify: Option<Arc<Notify>>,
    ) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(program);
        for arg in args {
            cmd.arg(arg);
        }
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");

        let child = pair.slave.spawn_command(cmd)?;
        let master = pair.master;
        let writer = master.take_writer()?;
        let reader = master.try_clone_reader()?;

        let parser = Arc::new(RwLock::new(vt100::Parser::new(rows, cols, 1000)));
        let parser_clone = Arc::clone(&parser);

        let status = Arc::new(Mutex::new(PtyStatus::Running));
        let status_clone = Arc::clone(&status);

        let last_output = Arc::new(Mutex::new(std::time::Instant::now()));
        let last_output_clone = Arc::clone(&last_output);

        let scroll_gen = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let scroll_gen_clone = Arc::clone(&scroll_gen);

        let tmux_in_copy_mode = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let tmux_drag_seen = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let tmux_gesture_delegating = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let (exit_tx, exit_rx) = oneshot::channel::<i32>();

        std::thread::spawn(move || {
            let mut reader = reader;
            let mut child = child;
            let mut buf = [0u8; 16384];
            let mut last_notify = std::time::Instant::now();

            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        let code = child
                            .wait()
                            .ok()
                            .map(|status| status.exit_code() as i32)
                            .unwrap_or(-1);

                        *status_clone.lock().unwrap() = PtyStatus::Exited(code);
                        let _ = exit_tx.send(code);
                        break;
                    }
                    Ok(n) => {
                        if let Ok(mut parser) = parser_clone.write() {
                            // SCROLLBACK-LEN heuristic (RESEARCH §Q1): infer a
                            // vertical scroll happened by checking that the
                            // cursor was at (or past) the bottom row before the
                            // process() call AND the top visible row hash
                            // changed across the call.
                            let (rows, cols) = parser.screen().size();
                            let before_cursor_row =
                                parser.screen().cursor_position().0;
                            let before_top_hash = row_hash(parser.screen(), 0, cols);
                            parser.process(&buf[..n]);
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
                }
            }

            if let Some(notify) = &output_notify {
                notify.notify_one();
            }
        });

        Ok(Self {
            id: fastrand::u64(..),
            parser,
            master: Some(master),
            writer: Some(writer),
            status,
            exit_rx: Some(exit_rx),
            last_output,
            scroll_generation: scroll_gen,
            tmux_in_copy_mode,
            tmux_drag_seen,
            tmux_gesture_delegating,
        })
    }

    pub fn is_exited(&self) -> bool {
        matches!(*self.status.lock().unwrap(), PtyStatus::Exited(_))
    }

    pub fn is_working(&self, threshold: std::time::Duration) -> bool {
        self.last_output
            .lock()
            .map(|t| t.elapsed() < threshold)
            .unwrap_or(false)
    }

    /// Write bytes to the PTY master writer.
    ///
    /// This is **synchronous by design** (PTY-01, PTY-02). Keystroke-sized
    /// writes (≤8 bytes) never block on a macOS PTY slave buffer (typical
    /// buffer size 4–16 KiB). Do NOT move this onto a `tokio::task::spawn`:
    /// the synchronous `write_all` + `flush` guarantees the keystroke lands
    /// in the child's stdin before the caller returns, which preserves the
    /// ordering of rapid keystrokes typed into the PTY pane.
    ///
    /// Large writes (paste >4 KiB) may block briefly; that case is
    /// acceptable because a user pasting is aware of the I/O. If a future
    /// profile flags paste blocking the event loop, chunk the paste write
    /// across multiple select iterations — do NOT make keystroke writes
    /// async.
    ///
    /// See `.planning/phases/03-pty-input-fluidity/03-RESEARCH.md` §Common
    /// Pitfalls #2.
    pub fn write_input(&mut self, data: &[u8]) -> Result<()> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| anyhow!("PTY session writer is closed"))?;

        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    /// Resize the PTY and update the vt100 parser.
    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        let master = self
            .master
            .as_ref()
            .ok_or_else(|| anyhow!("PTY session is closed"))?;

        master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        if let Ok(mut parser) = self.parser.write() {
            parser.screen_mut().set_size(rows, cols);
        }

        Ok(())
    }

    /// Kill the child process by closing the PTY handles.
    pub fn kill(&mut self) -> Result<()> {
        let _ = self.writer.take();
        let _ = self.master.take();
        Ok(())
    }

    /// Get current status.
    pub fn status(&self) -> PtyStatus {
        self.status.lock().unwrap().clone()
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

/// Hash the contents of one visible row in the vt100 screen. Used by the
/// PTY reader thread's SCROLLBACK-LEN heuristic (RESEARCH §Q1) to detect
/// whether the top row's text changed across a `parser.process()` call —
/// a strong signal that a vertical scroll happened. Cost is O(cols),
/// negligible compared to the vt100 parse itself.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn spawn_echo() {
        let mut session = PtySession::spawn(std::env::temp_dir(), "/bin/echo", &["hello"], 24, 80)
            .expect("spawn failed");

        let exit_rx = session.exit_rx.take().unwrap();
        let code = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { tokio::time::timeout(Duration::from_secs(5), exit_rx).await });

        assert!(code.is_ok(), "timed out waiting for exit");
        let code = code.unwrap().unwrap_or(-1);
        assert_eq!(code, 0);

        let contents = session.parser.read().unwrap().screen().contents();
        assert!(
            contents.contains("hello"),
            "expected 'hello' in output, got: {contents:?}"
        );
    }

    #[test]
    fn eof_exit_code() {
        let mut session =
            PtySession::spawn(std::env::temp_dir(), "/bin/sh", &["-c", "exit 42"], 24, 80)
                .expect("spawn failed");

        let exit_rx = session.exit_rx.take().unwrap();
        let code = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { tokio::time::timeout(Duration::from_secs(5), exit_rx).await });

        assert!(code.is_ok(), "timed out waiting for exit");
        let code = code.unwrap().unwrap_or(-1);
        assert_eq!(code, 42);
    }

    #[test]
    fn resize_updates_parser() {
        let session =
            PtySession::spawn(std::env::temp_dir(), "/bin/sh", &["-c", "sleep 1"], 24, 80)
                .expect("spawn failed");

        session.resize(40, 120).expect("resize failed");
        let size = session.parser.read().unwrap().screen().size();
        assert_eq!(size, (40, 120));
    }
}
