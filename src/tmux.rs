#![allow(dead_code)]

use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
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

fn ensure_config() -> PathBuf {
    let config_path = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join(".martins")
        .join("tmux.conf");

    let _ = std::fs::create_dir_all(config_path.parent().unwrap());
    let _ = std::fs::write(
        &config_path,
        "set -g mouse on\n\
         set -g default-terminal \"xterm-256color\"\n\
         set -g allow-passthrough off\n\
         set -g escape-time 0\n\
         setw -g alternate-screen off\n",
    );
    config_path
}

pub fn enforce_session_options(name: &str) {
    let opts: &[(&[&str], &str, &str)] = &[
        (&["set-option", "-t"], "mouse", "on"),
        (&["set-window-option", "-t"], "alternate-screen", "off"),
        (&["set-option", "-t"], "allow-passthrough", "off"),
    ];
    for (cmd, key, val) in opts {
        let mut args: Vec<&str> = cmd.to_vec();
        args.push(name);
        args.push(key);
        args.push(val);
        let _ = Command::new("tmux")
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

pub fn new_session(
    name: &str,
    cwd: &Path,
    program: &str,
    cols: u16,
    rows: u16,
) -> Result<()> {
    let config = ensure_config();
    let status = Command::new("tmux")
        .args([
            "-f", &config.to_string_lossy(),
            "new-session",
            "-d",
            "-s", name,
            "-x", &cols.to_string(),
            "-y", &rows.to_string(),
        ])
        .current_dir(cwd)
        .env("TERM", "xterm-256color")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;

    if !status.success() {
        bail!("tmux new-session failed for '{name}'");
    }

    enforce_session_options(name);

    std::thread::sleep(std::time::Duration::from_millis(200));

    let _ = Command::new("tmux")
        .args(["send-keys", "-t", name, program, "Enter"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    Ok(())
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
        .stdout(std::process::Stdio::null())
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

pub fn pane_command(name: &str) -> Option<String> {
    Command::new("tmux")
        .args(["list-panes", "-t", name, "-F", "#{pane_current_command}"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn send_key(name: &str, key: &str) {
    let _ = Command::new("tmux")
        .args(["send-keys", "-t", name, key])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
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
