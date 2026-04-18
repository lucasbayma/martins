#![allow(dead_code)]

use anyhow::{Result, bail};
use std::path::Path;
use std::process::Command;

pub fn is_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn session_exists(name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", name])
        .stderr(std::process::Stdio::null())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn new_session(
    name: &str,
    cwd: &Path,
    program: &str,
    cols: u16,
    rows: u16,
) -> Result<()> {
    let status = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s", name,
            "-x", &cols.to_string(),
            "-y", &rows.to_string(),
            program,
        ])
        .current_dir(cwd)
        .stderr(std::process::Stdio::null())
        .status()?;

    if !status.success() {
        bail!("tmux new-session failed for '{name}'");
    }

    configure_session(name);
    Ok(())
}

fn configure_session(name: &str) {
    let opts: &[(&str, &str)] = &[
        ("mouse", "on"),
        ("default-terminal", "xterm-256color"),
        ("allow-passthrough", "on"),
        ("escape-time", "0"),
    ];
    for (key, val) in opts {
        let _ = Command::new("tmux")
            .args(["set-option", "-t", name, key, val])
            .stderr(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .status();
    }
    let _ = Command::new("tmux")
        .args(["set-environment", "-t", name, "TERM", "xterm-256color"])
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status();
}

pub fn new_window(session_name: &str, cwd: &Path, program: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args([
            "new-window",
            "-t",
            session_name,
            "-c",
            &cwd.to_string_lossy(),
            program,
        ])
        .stderr(std::process::Stdio::null())
        .status()?;

    if !status.success() {
        bail!("tmux new-window failed for '{session_name}'");
    }
    Ok(())
}

pub fn resize_session(name: &str, cols: u16, rows: u16) {
    let _ = Command::new("tmux")
        .args([
            "resize-window",
            "-t", name,
            "-x", &cols.to_string(),
            "-y", &rows.to_string(),
        ])
        .stderr(std::process::Stdio::null())
        .output();
}

pub fn kill_session(name: &str) {
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", name])
        .stderr(std::process::Stdio::null())
        .output();
}

pub fn session_name(project_id: &str, workspace: &str) -> String {
    let short_id = &project_id[..project_id.len().min(8)];
    format!("martins-{short_id}-{workspace}")
}

pub fn tab_session_name(project_id: &str, workspace: &str, tab_id: u32) -> String {
    let short_id = &project_id[..project_id.len().min(8)];
    format!("martins-{short_id}-{workspace}-{tab_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_name_format() {
        let name = session_name("abcdef1234567890", "caetano");
        assert_eq!(name, "martins-abcdef12-caetano");
    }

    #[test]
    fn tab_session_name_format() {
        let name = tab_session_name("abcdef1234567890", "caetano", 2);
        assert_eq!(name, "martins-abcdef12-caetano-2");
    }

    #[test]
    fn tmux_availability() {
        let _ = is_available();
    }

    #[test]
    fn nonexistent_session() {
        assert!(!session_exists("martins-nonexistent-test-session"));
    }
}
