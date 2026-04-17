//! Application state with atomic persistence.
#![allow(dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkspaceStatus {
    Active,
    Inactive,
    Archived,
    Exited(i32),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum Agent {
    #[default]
    Opencode,
    Claude,
    Codex,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabSpec {
    pub id: u32,
    pub command: String,
}

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

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("unsupported state version: {0}")]
    UnsupportedVersion(u32),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

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

impl AppState {
    /// Load state from `{repo_root}/.martins/state.json`.
    /// Falls back to backup, then default if corrupted.
    pub fn load(repo_root: &Path) -> Result<Self, StateError> {
        let state_dir = repo_root.join(".martins");
        let path = state_dir.join("state.json");
        let bak = state_dir.join("state.json.bak");

        if !path.exists() {
            return Ok(Self::default());
        }

        match Self::try_load_file(&path) {
            Ok(s) => {
                if s.version != 1 {
                    return Err(StateError::UnsupportedVersion(s.version));
                }
                Ok(s)
            }
            Err(e) => {
                tracing::warn!("state.json load failed ({}), trying backup", e);
                if bak.exists() {
                    match Self::try_load_file(&bak) {
                        Ok(s) => {
                            if s.version != 1 {
                                return Err(StateError::UnsupportedVersion(s.version));
                            }
                            tracing::warn!("recovered state from backup");
                            Ok(s)
                        }
                        Err(_) => Ok(Self::default()),
                    }
                } else {
                    Ok(Self::default())
                }
            }
        }
    }

    fn try_load_file(path: &Path) -> Result<Self, StateError> {
        let data = std::fs::read_to_string(path)?;
        let s: Self = serde_json::from_str(&data)?;
        Ok(s)
    }

    /// Atomically save state to `{repo_root}/.martins/state.json`.
    pub fn save(&self, repo_root: &Path) -> Result<(), StateError> {
        let state_dir = repo_root.join(".martins");
        std::fs::create_dir_all(&state_dir)?;

        let path = state_dir.join("state.json");
        let bak = state_dir.join("state.json.bak");
        let tmp = state_dir.join("state.json.tmp");

        // Step 1: backup existing valid state
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if serde_json::from_str::<AppState>(&content).is_ok() {
                    std::fs::copy(&path, &bak)?;
                }
            }
        }

        // Step 2: write to tmp
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&tmp, &json)?;

        // Step 3: atomic rename
        if let Err(e) = std::fs::rename(&tmp, &path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(StateError::Io(e));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(&path, perms);
        }

        Ok(())
    }

    pub fn add_workspace(&mut self, ws: Workspace) {
        self.workspaces.push(ws);
    }

    pub fn archive(&mut self, name: &str) {
        if let Some(ws) = self.workspaces.iter_mut().find(|w| w.name == name) {
            ws.status = WorkspaceStatus::Archived;
        }
    }

    pub fn unarchive(&mut self, name: &str) {
        if let Some(ws) = self.workspaces.iter_mut().find(|w| w.name == name) {
            ws.status = WorkspaceStatus::Inactive;
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.workspaces.retain(|w| w.name != name);
    }

    pub fn active(&self) -> impl Iterator<Item = &Workspace> {
        self.workspaces
            .iter()
            .filter(|w| !matches!(w.status, WorkspaceStatus::Archived))
    }

    pub fn archived(&self) -> impl Iterator<Item = &Workspace> {
        self.workspaces
            .iter()
            .filter(|w| matches!(w.status, WorkspaceStatus::Archived))
    }

    pub fn used_names(&self) -> HashSet<String> {
        self.workspaces.iter().map(|w| w.name.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_workspace(name: &str) -> Workspace {
        Workspace {
            name: name.to_string(),
            worktree_path: PathBuf::from(format!("/tmp/repo-{name}")),
            base_branch: "main".to_string(),
            agent: Agent::Opencode,
            status: WorkspaceStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            tabs: vec![],
        }
    }

    #[test]
    fn roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::default();
        state.add_workspace(sample_workspace("caetano"));
        state.save(tmp.path()).unwrap();
        let loaded = AppState::load(tmp.path()).unwrap();
        assert_eq!(state, loaded);
    }

    #[test]
    fn load_missing_returns_default() {
        let tmp = TempDir::new().unwrap();
        let state = AppState::load(tmp.path()).unwrap();
        assert_eq!(state.workspaces.len(), 0);
    }

    #[test]
    fn backup_recovery() {
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::default();
        state.add_workspace(sample_workspace("gil"));
        state.save(tmp.path()).unwrap();
        state.save(tmp.path()).unwrap();
        // Corrupt main file
        std::fs::write(tmp.path().join(".martins/state.json"), b"NOT JSON!!!").unwrap();
        // Load should fall back to backup
        let loaded = AppState::load(tmp.path()).unwrap();
        assert_eq!(loaded.workspaces[0].name, "gil");
    }

    #[test]
    fn atomic_write() {
        let tmp = TempDir::new().unwrap();
        let state = AppState::default();
        state.save(tmp.path()).unwrap();
        // tmp file should not exist after successful save
        assert!(!tmp.path().join(".martins/state.json.tmp").exists());
        assert!(tmp.path().join(".martins/state.json").exists());
    }

    #[test]
    fn unsupported_version() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".martins")).unwrap();
        std::fs::write(
            tmp.path().join(".martins/state.json"),
            r#"{"version":99,"workspaces":[]}"#,
        )
        .unwrap();
        let err = AppState::load(tmp.path()).unwrap_err();
        assert!(matches!(err, StateError::UnsupportedVersion(99)));
    }
}
