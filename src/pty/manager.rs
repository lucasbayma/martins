//! PTY session manager: manages multiple sessions keyed by (workspace_id, tab_id).

#![allow(dead_code)]

use crate::pty::session::PtySession;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

pub type WorkspaceId = String;
pub type TabId = u32;

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
    sessions: HashMap<(WorkspaceId, TabId), PtySession>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn tab_count(&self, ws_id: &str) -> usize {
        self.sessions
            .keys()
            .filter(|(workspace_id, _)| workspace_id == ws_id)
            .count()
    }

    pub fn spawn_tab(
        &mut self,
        ws_id: WorkspaceId,
        tab_id: TabId,
        cwd: PathBuf,
        program: &str,
        args: &[&str],
    ) -> Result<(), ManagerError> {
        if self.tab_count(&ws_id) >= 5 {
            return Err(ManagerError::TabLimit);
        }

        let session = PtySession::spawn(cwd, program, args, 24, 80)?;
        self.sessions.insert((ws_id, tab_id), session);
        Ok(())
    }

    pub fn write_input(
        &mut self,
        ws_id: &str,
        tab_id: TabId,
        data: &[u8],
    ) -> Result<(), ManagerError> {
        let session = self
            .sessions
            .get_mut(&(ws_id.to_string(), tab_id))
            .ok_or(ManagerError::NotFound)?;

        session.write_input(data).map_err(ManagerError::Spawn)
    }

    pub fn resize_all_for(&self, ws_id: &str, rows: u16, cols: u16) -> Result<(), ManagerError> {
        for ((workspace_id, _), session) in &self.sessions {
            if workspace_id == ws_id {
                session.resize(rows, cols).map_err(ManagerError::Spawn)?;
            }
        }

        Ok(())
    }

    pub fn close_tab(&mut self, ws_id: &str, tab_id: TabId) {
        self.sessions.remove(&(ws_id.to_string(), tab_id));
    }

    pub fn close_workspace(&mut self, ws_id: &str) {
        self.sessions
            .retain(|(workspace_id, _), _| workspace_id != ws_id);
    }

    pub fn get_session(&self, ws_id: &str, tab_id: TabId) -> Option<&PtySession> {
        self.sessions.get(&(ws_id.to_string(), tab_id))
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
        let ws = "test-ws".to_string();

        for i in 0..5 {
            mgr.spawn_tab(
                ws.clone(),
                i,
                std::env::temp_dir(),
                "/bin/sh",
                &["-c", "sleep 1"],
            )
            .unwrap();
        }

        let result = mgr.spawn_tab(
            ws.clone(),
            5,
            std::env::temp_dir(),
            "/bin/sh",
            &["-c", "sleep 1"],
        );

        assert!(matches!(result, Err(ManagerError::TabLimit)));
    }

    #[test]
    fn close_tab_removes_session() {
        let mut mgr = PtyManager::new();

        mgr.spawn_tab(
            "ws1".to_string(),
            0,
            std::env::temp_dir(),
            "/bin/sh",
            &["-c", "sleep 1"],
        )
        .unwrap();

        assert_eq!(mgr.tab_count("ws1"), 1);
        mgr.close_tab("ws1", 0);
        assert_eq!(mgr.tab_count("ws1"), 0);
    }
}
