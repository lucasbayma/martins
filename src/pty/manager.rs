//! PTY session manager: manages multiple sessions keyed by project/workspace/tab.

#![allow(dead_code)]

use crate::pty::session::PtySession;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Notify;

pub type WorkspaceId = String;
pub type TabId = u32;
pub type SessionKey = (String, WorkspaceId, TabId);

#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("tab limit reached (max 5 per workspace)")]
    TabLimit,
    #[error("session not found")]
    NotFound,
    #[error("spawn error: {0}")]
    Spawn(#[from] anyhow::Error),
}

pub struct PtyManager {
    sessions: HashMap<SessionKey, PtySession>,
    pub output_notify: Arc<Notify>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            output_notify: Arc::new(Notify::new()),
        }
    }

    pub fn tab_count(&self, project_id: &str, ws_id: &str) -> usize {
        self.sessions
            .keys()
            .filter(|(session_project_id, workspace_id, _)| {
                session_project_id == project_id && workspace_id == ws_id
            })
            .count()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spawn_tab(
        &mut self,
        project_id: String,
        ws_id: WorkspaceId,
        tab_id: TabId,
        cwd: PathBuf,
        program: &str,
        args: &[&str],
        rows: u16,
        cols: u16,
    ) -> Result<(), ManagerError> {
        if self.tab_count(&project_id, &ws_id) >= 5 {
            return Err(ManagerError::TabLimit);
        }

        let session = PtySession::spawn_with_notify(
            cwd, program, args, rows, cols, Some(Arc::clone(&self.output_notify)),
        )?;
        self.sessions.insert((project_id, ws_id, tab_id), session);
        Ok(())
    }

    pub fn write_input(
        &mut self,
        project_id: &str,
        ws_id: &str,
        tab_id: TabId,
        data: &[u8],
    ) -> Result<(), ManagerError> {
        let session = self
            .sessions
            .get_mut(&(project_id.to_string(), ws_id.to_string(), tab_id))
            .ok_or(ManagerError::NotFound)?;

        session.write_input(data).map_err(ManagerError::Spawn)
    }

    pub fn resize_all_for(
        &self,
        project_id: &str,
        ws_id: &str,
        rows: u16,
        cols: u16,
    ) -> Result<(), ManagerError> {
        for ((session_project_id, workspace_id, _), session) in &self.sessions {
            if session_project_id == project_id && workspace_id == ws_id {
                session.resize(rows, cols).map_err(ManagerError::Spawn)?;
            }
        }

        Ok(())
    }

    pub fn close_tab(&mut self, project_id: &str, ws_id: &str, tab_id: TabId) {
        self.sessions
            .remove(&(project_id.to_string(), ws_id.to_string(), tab_id));
    }

    pub fn close_workspace(&mut self, project_id: &str, ws_id: &str) {
        self.sessions
            .retain(|(session_project_id, workspace_id, _), _| {
                session_project_id != project_id || workspace_id != ws_id
            });
    }

    pub fn get_session(&self, project_id: &str, ws_id: &str, tab_id: TabId) -> Option<&PtySession> {
        self.sessions
            .get(&(project_id.to_string(), ws_id.to_string(), tab_id))
    }

    /// Test-only: register a pre-built `PtySession` under the given
    /// `(project_id, ws_id, tab_id)` key without going through
    /// `spawn_tab`. Mirrors the production `spawn_tab` insert at the
    /// HashMap level — downstream selection tests use this seam to seed
    /// an active session synchronously so they can inspect
    /// `session.scroll_generation` and parser state without paying the
    /// `PtySession::spawn_with_notify` cost on every test.
    ///
    /// Gated `#[cfg(test)]` so it never appears in production binaries.
    #[cfg(test)]
    pub(crate) fn insert_for_test(
        &mut self,
        project_id: String,
        ws_id: WorkspaceId,
        tab_id: TabId,
        session: PtySession,
    ) {
        self.sessions.insert((project_id, ws_id, tab_id), session);
    }
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_limit_enforced() {
        let mut mgr = PtyManager::new();
        let project = "proj1".to_string();
        let ws = "test-ws".to_string();

        for i in 0..5 {
            mgr.spawn_tab(
                project.clone(),
                ws.clone(),
                i,
                std::env::temp_dir(),
                "/bin/sh",
                &["-c", "sleep 1"],
                24,
                80,
            )
            .unwrap();
        }

        let result = mgr.spawn_tab(
            project.clone(),
            ws.clone(),
            5,
            std::env::temp_dir(),
            "/bin/sh",
            &["-c", "sleep 1"],
            24,
            80,
        );

        assert!(matches!(result, Err(ManagerError::TabLimit)));
    }

    #[test]
    fn close_tab_removes_session() {
        let mut mgr = PtyManager::new();

        mgr.spawn_tab(
            "proj1".to_string(),
            "ws1".to_string(),
            0,
            std::env::temp_dir(),
            "/bin/sh",
            &["-c", "sleep 1"],
            24,
            80,
        )
        .unwrap();

        assert_eq!(mgr.tab_count("proj1", "ws1"), 1);
        mgr.close_tab("proj1", "ws1", 0);
        assert_eq!(mgr.tab_count("proj1", "ws1"), 0);
    }
}
