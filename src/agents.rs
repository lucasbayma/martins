//! Agent detection and workspace creation orchestration.
#![allow(dead_code)]

use crate::mpb;
use crate::state::{Agent, AppState, Workspace, WorkspaceStatus};
use crate::tools::{Tool, detect};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub agent: Agent,
    pub available: bool,
    pub path: Option<PathBuf>,
}

/// Detect which agents are available on the system.
pub fn detect_agents() -> Vec<AgentInfo> {
    vec![
        AgentInfo {
            agent: Agent::Opencode,
            available: detect(&Tool::Opencode).is_some(),
            path: detect(&Tool::Opencode),
        },
        AgentInfo {
            agent: Agent::Claude,
            available: detect(&Tool::Claude).is_some(),
            path: detect(&Tool::Claude),
        },
        AgentInfo {
            agent: Agent::Codex,
            available: detect(&Tool::Codex).is_some(),
            path: detect(&Tool::Codex),
        },
    ]
}

/// Get the first available agent, or Opencode as default.
pub fn default_agent() -> Agent {
    let agents = detect_agents();
    agents
        .into_iter()
        .find(|a| a.available)
        .map(|a| a.agent)
        .unwrap_or(Agent::Opencode)
}

/// Create a workspace entry in AppState (does NOT create git worktree — that's async).
/// Returns the workspace name.
pub fn create_workspace_entry(
    state: &mut AppState,
    name: Option<String>,
    agent: Agent,
    base_branch: String,
    repo_root: &std::path::Path,
) -> Result<String, mpb::NameError> {
    let used = state.used_names();
    let name = match name {
        Some(n) if !n.is_empty() => {
            mpb::validate(&n)?;
            n
        }
        _ => mpb::generate_name(&used),
    };

    let worktree_path = repo_root.parent().unwrap_or(repo_root).join(format!(
        "{}-{}",
        repo_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repo"),
        name
    ));

    let ws = Workspace {
        name: name.clone(),
        worktree_path,
        base_branch,
        agent,
        status: WorkspaceStatus::Inactive,
        created_at: chrono::Utc::now().to_rfc3339(),
        tabs: vec![],
    };

    state.add_workspace(ws);
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn detect_agents_returns_list() {
        let agents = detect_agents();
        assert_eq!(agents.len(), 3);
        // All three agent types present
        assert!(agents.iter().any(|a| matches!(a.agent, Agent::Opencode)));
        assert!(agents.iter().any(|a| matches!(a.agent, Agent::Claude)));
        assert!(agents.iter().any(|a| matches!(a.agent, Agent::Codex)));
    }

    #[test]
    fn create_workspace_auto_name() {
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::default();
        let name = create_workspace_entry(
            &mut state,
            None,
            Agent::Opencode,
            "main".to_string(),
            tmp.path(),
        )
        .unwrap();
        assert!(!name.is_empty());
        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].name, name);
    }

    #[test]
    fn create_workspace_custom_name() {
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::default();
        let name = create_workspace_entry(
            &mut state,
            Some("caetano".to_string()),
            Agent::Claude,
            "main".to_string(),
            tmp.path(),
        )
        .unwrap();
        assert_eq!(name, "caetano");
    }

    #[test]
    fn create_workspace_invalid_name() {
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::default();
        let result = create_workspace_entry(
            &mut state,
            Some("invalid name!".to_string()),
            Agent::Opencode,
            "main".to_string(),
            tmp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn default_agent_returns_something() {
        let agent = default_agent();
        // Just verify it returns a valid agent
        let _ = format!("{:?}", agent);
    }
}
