//! Module doc.

use thiserror::Error;

#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),
    #[error("PTY error: {0}")]
    Pty(String),
    #[error("State error: {0}")]
    State(String),
    #[error("Config error: {0}")]
    Config(String),
}
