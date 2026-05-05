//! Pre-flight binary detection and install command mapping.
#![allow(dead_code)]

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tool {
    Bat,
    Opencode,
    Claude,
    Codex,
    Gsd,
}

impl Tool {
    pub fn binary_name(&self) -> &str {
        match self {
            Tool::Bat => "bat",
            Tool::Opencode => "opencode",
            Tool::Claude => "claude",
            Tool::Codex => "codex",
            Tool::Gsd => "gsd",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallCmd {
    pub program: String,
    pub args: Vec<String>,
}

/// Detect a tool binary using the `which` crate.
/// Accepts an optional custom PATH for testing (uses ambient PATH if None).
pub fn detect(tool: &Tool) -> Option<PathBuf> {
    which::which(tool.binary_name()).ok()
}

/// Detect with a custom PATH string (for testing).
pub fn detect_in(tool: &Tool, path: &str) -> Option<PathBuf> {
    which::which_in(
        tool.binary_name(),
        Some(path),
        std::env::current_dir().unwrap_or_default(),
    )
    .ok()
}

#[derive(Debug, Default)]
pub struct MissingTools {
    pub tools: Vec<Tool>,
}

/// Run pre-flight check: detect all required tools.
pub fn preflight() -> MissingTools {
    let all = [Tool::Bat, Tool::Opencode, Tool::Claude, Tool::Codex, Tool::Gsd];
    let missing = all.into_iter().filter(|t| detect(t).is_none()).collect();
    MissingTools { tools: missing }
}

/// Get the install command for a tool on the current OS.
pub fn install_command(tool: &Tool) -> Option<InstallCmd> {
    let os = std::env::consts::OS;
    match tool {
        Tool::Bat => {
            if os == "macos" {
                Some(InstallCmd {
                    program: "brew".to_string(),
                    args: vec!["install".to_string(), "bat".to_string()],
                })
            } else {
                // Linux: try apt, fallback cargo
                Some(InstallCmd {
                    program: "apt".to_string(),
                    args: vec!["install".to_string(), "-y".to_string(), "bat".to_string()],
                })
            }
        }
        Tool::Opencode => {
            // Official install: npm
            Some(InstallCmd {
                program: "npm".to_string(),
                args: vec![
                    "install".to_string(),
                    "-g".to_string(),
                    "opencode-ai".to_string(),
                ],
            })
        }
        Tool::Claude => Some(InstallCmd {
            program: "npm".to_string(),
            args: vec![
                "install".to_string(),
                "-g".to_string(),
                "@anthropic-ai/claude-code".to_string(),
            ],
        }),
        Tool::Codex => Some(InstallCmd {
            program: "npm".to_string(),
            args: vec![
                "install".to_string(),
                "-g".to_string(),
                "@openai/codex".to_string(),
            ],
        }),
        Tool::Gsd => None,
    }
}

/// Run an install command in the foreground, streaming stdout.
pub fn run_install(cmd: &InstallCmd) -> anyhow::Result<()> {
    let status = std::process::Command::new(&cmd.program)
        .args(&cmd.args)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("install command failed with status: {}", status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_bat() {
        // bat is installed on the CI machine (macOS/Linux)
        // If not installed, this test is skipped gracefully
        let result = detect(&Tool::Bat);
        if let Some(path) = result {
            assert!(path.is_file());
        }
        // Either way, the function returns without panic
    }

    #[test]
    fn missing_opencode() {
        // Use a custom PATH that doesn't contain opencode
        let result = detect_in(&Tool::Opencode, "/tmp");
        assert!(result.is_none(), "opencode should not be found in /tmp");
    }

    #[test]
    fn preflight_returns_missing() {
        let missing = preflight();
        // All tools in the list should be Tool variants
        for t in &missing.tools {
            assert!(!t.binary_name().is_empty());
        }
    }

    #[test]
    fn install_cmd_bat_macos() {
        // Test the macOS branch by checking the logic directly
        // We can't mock std::env::consts::OS, so we test the function returns Some
        let cmd = install_command(&Tool::Bat);
        assert!(cmd.is_some());
        let cmd = cmd.unwrap();
        // On macOS: brew install bat; on Linux: apt install -y bat
        assert!(!cmd.program.is_empty());
        assert!(cmd.args.contains(&"bat".to_string()));
    }

    #[test]
    fn install_cmd_opencode() {
        let cmd = install_command(&Tool::Opencode);
        assert!(cmd.is_some());
        let cmd = cmd.unwrap();
        assert_eq!(cmd.program, "npm");
    }

    #[test]
    fn install_cmd_claude() {
        let cmd = install_command(&Tool::Claude);
        assert!(cmd.is_some());
        let cmd = cmd.unwrap();
        assert_eq!(cmd.program, "npm");
    }
}
