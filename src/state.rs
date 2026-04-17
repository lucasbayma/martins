//! Module doc.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkspaceStatus {
    Active,
    Inactive,
    Archived,
    Exited(i32),
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Agent {
    Opencode,
    Claude,
    Codex,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabSpec {
    pub id: u32,
    pub command: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Workspace {
    pub name: String,
    pub worktree_path: PathBuf,
    pub base_branch: String,
    pub agent: Agent,
    pub status: WorkspaceStatus,
    pub created_at: String,
    pub tabs: Vec<TabSpec>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppState {
    pub version: u32,
    pub workspaces: Vec<Workspace>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            version: 1,
            workspaces: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let state = AppState {
            version: 1,
            workspaces: vec![Workspace {
                name: "caetano".to_string(),
                worktree_path: PathBuf::from("/tmp/repo-caetano"),
                base_branch: "main".to_string(),
                agent: Agent::Opencode,
                status: WorkspaceStatus::Active,
                created_at: "2024-01-01T00:00:00Z".to_string(),
                tabs: vec![],
            }],
        };
        let json = serde_json::to_string(&state).unwrap();
        println!("{}", json);
        let restored: AppState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, restored);
    }
}
