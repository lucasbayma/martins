//! Spawn $EDITOR for a file, restoring terminal after exit.

#![allow(dead_code)]

use anyhow::{Context, Result};
use std::path::Path;

/// Open `path` in the user's $EDITOR (or vi as fallback).
/// Suspends the TUI, runs the editor, then returns.
/// The caller is responsible for re-entering raw mode after this returns.
pub fn open_in_editor(path: &Path) -> Result<()> {
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());

    crossterm::terminal::disable_raw_mode().context("failed to disable raw mode")?;
    crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)
        .context("failed to leave alternate screen")?;

    let status = std::process::Command::new(&editor)
        .arg(path)
        .status()
        .with_context(|| format!("failed to spawn editor '{}'", editor))?;

    if !status.success() {
        tracing::warn!("editor '{}' exited with status: {}", editor, status);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn editor_env_fallback() {
        let editor = std::env::var("EDITOR")
            .or_else(|_| std::env::var("VISUAL"))
            .unwrap_or_else(|_| "vi".to_string());
        assert!(!editor.is_empty());
    }
}
