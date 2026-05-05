//! Agent detection and workspace creation orchestration.
#![allow(dead_code)]

use crate::mpb;
use crate::state::{Agent, Project, Workspace, WorkspaceStatus};
use crate::tools::{Tool, detect};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub agent: Agent,
    pub available: bool,
    pub path: Option<PathBuf>,
}

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
        AgentInfo {
            agent: Agent::Gsd,
            available: detect(&Tool::Gsd).is_some(),
            path: detect(&Tool::Gsd),
        },
    ]
}

pub fn default_agent() -> Agent {
    let agents = detect_agents();
    agents
        .into_iter()
        .find(|a| a.available)
        .map(|a| a.agent)
        .unwrap_or(Agent::Opencode)
}

pub fn create_workspace_entry(
    project: &mut Project,
    name: Option<String>,
    agent: Agent,
) -> Result<String, mpb::NameError> {
    let used = project.used_names();
    let name = match name {
        Some(n) if !n.is_empty() => {
            mpb::validate(&n)?;
            n
        }
        _ => mpb::generate_name(&used),
    };

    let repo_name = project
        .repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo");
    let base_dir = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join(".martins")
        .join("workspaces")
        .join(repo_name);
    let worktree_path = base_dir.join(&name);

    let ws = Workspace {
        name: name.clone(),
        worktree_path,
        base_branch: project.base_branch.clone(),
        agent,
        status: WorkspaceStatus::Inactive,
        created_at: chrono::Utc::now().to_rfc3339(),
        tabs: vec![],
    };

    project.add_workspace(ws);
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn detect_agents_returns_list() {
        let agents = detect_agents();
        assert_eq!(agents.len(), 4);
        assert!(agents.iter().any(|a| matches!(a.agent, Agent::Opencode)));
        assert!(agents.iter().any(|a| matches!(a.agent, Agent::Claude)));
        assert!(agents.iter().any(|a| matches!(a.agent, Agent::Codex)));
        assert!(agents.iter().any(|a| matches!(a.agent, Agent::Gsd)));
    }

    #[test]
    fn create_workspace_auto_name() {
        let tmp = TempDir::new().unwrap();
        let mut project = Project::new(tmp.path().to_path_buf(), "main".to_string());
        let name = create_workspace_entry(&mut project, None, Agent::Opencode).unwrap();
        assert!(!name.is_empty());
        assert_eq!(project.workspaces.len(), 1);
        assert_eq!(project.workspaces[0].name, name);
    }

    #[test]
    fn create_workspace_custom_name() {
        let tmp = TempDir::new().unwrap();
        let mut project = Project::new(tmp.path().to_path_buf(), "main".to_string());
        let name =
            create_workspace_entry(&mut project, Some("caetano".to_string()), Agent::Claude)
                .unwrap();
        assert_eq!(name, "caetano");
    }

    #[test]
    fn create_workspace_invalid_name() {
        let tmp = TempDir::new().unwrap();
        let mut project = Project::new(tmp.path().to_path_buf(), "main".to_string());
        let result = create_workspace_entry(
            &mut project,
            Some("invalid name!".to_string()),
            Agent::Opencode,
        );
        assert!(result.is_err());
    }

    #[test]
    fn default_agent_returns_something() {
        let agent = default_agent();
        let _ = format!("{:?}", agent);
    }
}
