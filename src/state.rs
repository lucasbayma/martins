//! Application state with atomic persistence.
//! v2: multi-project model (Project → Workspace hierarchy).
#![allow(dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// ── Workspace types (unchanged from v1) ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkspaceStatus {
    Active,
    Inactive,
    Archived,
    Deleted,
    Exited(i32),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum Agent {
    #[default]
    Opencode,
    Claude,
    Codex,
    Gsd,
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

// ── Project ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub repo_root: PathBuf,
    pub base_branch: String,
    pub workspaces: Vec<Workspace>,
    pub added_at: String,
    #[serde(default = "default_expanded")]
    pub expanded: bool,
}

fn default_expanded() -> bool {
    true
}

impl Project {
    pub fn new(repo_root: PathBuf, base_branch: String) -> Self {
        let id = crate::config::hash_repo_path(&repo_root);
        let name = repo_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repo")
            .to_string();

        Self {
            id,
            name,
            repo_root,
            base_branch,
            workspaces: Vec::new(),
            added_at: chrono::Utc::now().to_rfc3339(),
            expanded: true,
        }
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

    pub fn delete_workspace(&mut self, name: &str) {
        if let Some(ws) = self.workspaces.iter_mut().find(|w| w.name == name) {
            ws.status = WorkspaceStatus::Deleted;
        }
    }

    pub fn used_names(&self) -> HashSet<String> {
        self.workspaces.iter().map(|w| w.name.clone()).collect()
    }
}

// ── GlobalState (v2) ─────────────────────────────────────────────────

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
pub struct GlobalState {
    pub version: u32,
    pub projects: Vec<Project>,
    pub active_project_id: Option<String>,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self {
            version: 2,
            projects: Vec::new(),
            active_project_id: None,
        }
    }
}

impl GlobalState {
    /// Load global state from the given path.
    /// Falls back to backup, then default if corrupted.
    pub fn load(path: &Path) -> Result<Self, StateError> {
        let bak = path.with_extension("json.bak");

        if !path.exists() {
            return Ok(Self::default());
        }

        match Self::try_load_file(path) {
            Ok(s) => Ok(s),
            Err(e) => {
                tracing::warn!("global state load failed ({}), trying backup", e);
                if bak.exists() {
                    match Self::try_load_file(&bak) {
                        Ok(s) => {
                            tracing::warn!("recovered global state from backup");
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
        if s.version != 2 {
            return Err(StateError::UnsupportedVersion(s.version));
        }
        Ok(s)
    }

    /// Atomically save global state.
    pub fn save(&self, path: &Path) -> Result<(), StateError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let bak = path.with_extension("json.bak");
        let tmp = path.with_extension("json.tmp");

        // Backup existing valid state
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if serde_json::from_str::<GlobalState>(&content).is_ok() {
                    std::fs::copy(path, &bak)?;
                }
            }
        }

        // Write to tmp
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&tmp, &json)?;

        // Atomic rename
        if let Err(e) = std::fs::rename(&tmp, path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(StateError::Io(e));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(path, perms);
        }

        Ok(())
    }

    /// Add a project. Returns the project ID.
    pub fn add_project(&mut self, repo_root: &Path, base_branch: String) -> String {
        let project = Project::new(repo_root.to_path_buf(), base_branch);
        let id = project.id.clone();
        self.projects.push(project);
        id
    }

    pub fn remove_project(&mut self, id: &str) {
        self.projects.retain(|p| p.id != id);
        if self.active_project_id.as_deref() == Some(id) {
            self.active_project_id = self.projects.first().map(|p| p.id.clone());
        }
    }

    pub fn find_project(&self, id: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    pub fn find_project_mut(&mut self, id: &str) -> Option<&mut Project> {
        self.projects.iter_mut().find(|p| p.id == id)
    }

    pub fn active_project(&self) -> Option<&Project> {
        self.active_project_id
            .as_ref()
            .and_then(|id| self.find_project(id))
    }

    pub fn active_project_mut(&mut self) -> Option<&mut Project> {
        let id = self.active_project_id.clone();
        id.and_then(move |id| self.find_project_mut(&id))
    }

    /// Ensure a project exists for the given repo root.
    /// Returns the project ID (existing or newly created).
    pub fn ensure_project(&mut self, repo_root: &Path, base_branch: String) -> String {
        let id = crate::config::hash_repo_path(repo_root);
        if self.find_project(&id).is_some() {
            return id;
        }
        self.add_project(repo_root, base_branch)
    }

    /// Migrate from a v1 per-repo AppState to GlobalState.
    pub fn migrate_from_v1(repo_root: &Path) -> Result<Self, StateError> {
        let v1_path = repo_root.join(".martins").join("state.json");
        if !v1_path.exists() {
            return Ok(Self::default());
        }

        let data = std::fs::read_to_string(&v1_path)?;
        let v1: LegacyAppState = serde_json::from_str(&data)?;

        if v1.version != 1 {
            return Err(StateError::UnsupportedVersion(v1.version));
        }

        let base_branch = v1
            .workspaces
            .first()
            .map(|w| w.base_branch.clone())
            .unwrap_or_else(|| "main".to_string());

        let mut project = Project::new(repo_root.to_path_buf(), base_branch);
        project.workspaces = v1.workspaces;

        let id = project.id.clone();

        Ok(Self {
            version: 2,
            projects: vec![project],
            active_project_id: Some(id),
        })
    }
}

// ── Legacy v1 type for migration ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyAppState {
    pub version: u32,
    pub workspaces: Vec<Workspace>,
}

// ── Tests ────────────────────────────────────────────────────────────

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

    fn sample_project(name: &str) -> Project {
        let mut p = Project::new(PathBuf::from(format!("/tmp/{name}")), "main".to_string());
        p.name = name.to_string();
        p
    }

    #[test]
    fn roundtrip_global_state() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("state.json");

        let mut state = GlobalState::default();
        let mut p1 = sample_project("alpha");
        p1.add_workspace(sample_workspace("caetano"));
        p1.add_workspace(sample_workspace("gil"));
        let mut p2 = sample_project("beta");
        p2.add_workspace(sample_workspace("elis"));
        p2.add_workspace(sample_workspace("chico"));

        state.projects.push(p1);
        state.projects.push(p2);
        state.active_project_id = Some(state.projects[0].id.clone());

        state.save(&path).unwrap();
        let loaded = GlobalState::load(&path).unwrap();

        assert_eq!(state, loaded);
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.projects.len(), 2);
        assert_eq!(loaded.projects[0].workspaces.len(), 2);
        assert_eq!(loaded.projects[1].workspaces.len(), 2);
    }

    #[test]
    fn migration_from_v1() {
        let tmp = TempDir::new().unwrap();
        let martins_dir = tmp.path().join(".martins");
        std::fs::create_dir_all(&martins_dir).unwrap();

        let v1_json = serde_json::json!({
            "version": 1,
            "workspaces": [
                {
                    "name": "caetano",
                    "worktree_path": "/tmp/repo-caetano",
                    "base_branch": "main",
                    "agent": "Opencode",
                    "status": "Active",
                    "created_at": "2024-01-01T00:00:00Z",
                    "tabs": []
                },
                {
                    "name": "gil",
                    "worktree_path": "/tmp/repo-gil",
                    "base_branch": "main",
                    "agent": "Claude",
                    "status": "Inactive",
                    "created_at": "2024-01-02T00:00:00Z",
                    "tabs": []
                }
            ]
        });

        std::fs::write(
            martins_dir.join("state.json"),
            serde_json::to_string_pretty(&v1_json).unwrap(),
        )
        .unwrap();

        let migrated = GlobalState::migrate_from_v1(tmp.path()).unwrap();
        assert_eq!(migrated.version, 2);
        assert_eq!(migrated.projects.len(), 1);
        assert_eq!(migrated.projects[0].workspaces.len(), 2);
        assert_eq!(migrated.projects[0].workspaces[0].name, "caetano");
        assert_eq!(migrated.projects[0].workspaces[1].name, "gil");
        assert!(migrated.active_project_id.is_some());
    }

    #[test]
    fn project_add_remove() {
        let mut state = GlobalState::default();
        let id1 = state.add_project(Path::new("/tmp/alpha"), "main".to_string());
        let id2 = state.add_project(Path::new("/tmp/beta"), "main".to_string());
        assert_eq!(state.projects.len(), 2);

        state.remove_project(&id1);
        assert_eq!(state.projects.len(), 1);
        assert_eq!(state.projects[0].id, id2);
    }

    #[test]
    fn workspace_scoped_to_project() {
        let mut state = GlobalState::default();
        let id1 = state.add_project(Path::new("/tmp/alpha"), "main".to_string());
        let id2 = state.add_project(Path::new("/tmp/beta"), "main".to_string());

        state
            .find_project_mut(&id1)
            .unwrap()
            .add_workspace(sample_workspace("caetano"));
        state
            .find_project_mut(&id2)
            .unwrap()
            .add_workspace(sample_workspace("caetano"));

        let p1_names = state.find_project(&id1).unwrap().used_names();
        assert_eq!(p1_names.len(), 1);
        assert!(p1_names.contains("caetano"));
        assert_eq!(state.find_project(&id2).unwrap().used_names().len(), 1);
    }

    #[test]
    fn ensure_project_idempotent() {
        let mut state = GlobalState::default();
        let id1 = state.ensure_project(Path::new("/tmp/alpha"), "main".to_string());
        let id2 = state.ensure_project(Path::new("/tmp/alpha"), "main".to_string());
        assert_eq!(id1, id2);
        assert_eq!(state.projects.len(), 1);
    }

    #[test]
    fn load_missing_returns_default() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("state.json");
        let state = GlobalState::load(&path).unwrap();
        assert_eq!(state.projects.len(), 0);
    }

    #[test]
    fn backup_recovery() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("state.json");

        let mut state = GlobalState::default();
        state.add_project(Path::new("/tmp/alpha"), "main".to_string());
        state.save(&path).unwrap();
        state.save(&path).unwrap();


        std::fs::write(&path, b"NOT JSON!!!").unwrap();
        let loaded = GlobalState::load(&path).unwrap();
        assert_eq!(loaded.projects.len(), 1);
    }

    #[test]
    fn project_workspace_lifecycle() {
        let mut project = sample_project("test");
        project.add_workspace(sample_workspace("caetano"));
        project.add_workspace(sample_workspace("gil"));
        assert_eq!(project.active().count(), 2);

        project.archive("caetano");
        assert_eq!(project.active().count(), 1);
        assert_eq!(project.archived().count(), 1);

        project.unarchive("caetano");
        assert_eq!(project.active().count(), 2);

        project.remove("gil");
        assert_eq!(project.active().count(), 1);
    }

    #[test]
    fn atomic_write() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("state.json");
        let state = GlobalState::default();
        state.save(&path).unwrap();
        assert!(!tmp.path().join("state.json.tmp").exists());
        assert!(path.exists());
    }
}
