//! PTY session lifecycle: spawn, read, resize, kill.

#![allow(dead_code)]

use anyhow::{Result, anyhow};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::oneshot;

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
}

impl PtySession {
    /// Spawn a new PTY session running `program` with `args` in `cwd`.
    pub fn spawn(cwd: PathBuf, program: &str, args: &[&str], rows: u16, cols: u16) -> Result<Self> {
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

        let child = pair.slave.spawn_command(cmd)?;
        let master = pair.master;
        let writer = master.take_writer()?;
        let reader = master.try_clone_reader()?;

        let parser = Arc::new(RwLock::new(vt100::Parser::new(rows, cols, 1000)));
        let parser_clone = Arc::clone(&parser);

        let status = Arc::new(Mutex::new(PtyStatus::Running));
        let status_clone = Arc::clone(&status);

        let (exit_tx, exit_rx) = oneshot::channel::<i32>();

        std::thread::spawn(move || {
            let mut reader = reader;
            let mut child = child;
            let mut buf = [0u8; 4096];

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
                            parser.process(&buf[..n]);
                        }
                    }
                }
            }
        });

        Ok(Self {
            id: fastrand::u64(..),
            parser,
            master: Some(master),
            writer: Some(writer),
            status,
            exit_rx: Some(exit_rx),
        })
    }

    /// Write bytes to the PTY (forwarded to child stdin).
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
