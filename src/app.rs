//! Application state and main event loop.

use crate::git::diff;
use crate::keys::{Action, InputMode, Keymap};
use crate::pty::manager::PtyManager;
use crate::state::{GlobalState, Project, Workspace};
use crate::ui::layout::{self, LayoutState, PaneRects};
use crate::ui::modal::{Modal, NewWorkspaceForm};
use crate::ui::picker::{Picker, PickerKind, PickerOutcome};
use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyEvent, MouseEvent};
use futures::StreamExt;
use ratatui::{
    DefaultTerminal, Frame,
    layout::Rect,
    widgets::ListState,
};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::interval;

#[derive(Debug, Clone)]
pub enum SidebarItem {
    RemoveProject(usize),
    Workspace(usize, usize),
    AddProject,
    NewWorkspace(usize),
    ArchivedHeader(usize),
    ArchivedWorkspace(usize, usize),
}



#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionState {
    pub start_col: u16,
    pub start_row: u16,
    pub end_col: u16,
    pub end_row: u16,
    pub dragging: bool,
}

impl SelectionState {
    pub fn normalized(&self) -> ((u16, u16), (u16, u16)) {
        if self.start_row < self.end_row
            || (self.start_row == self.end_row && self.start_col <= self.end_col)
        {
            ((self.start_col, self.start_row), (self.end_col, self.end_row))
        } else {
            ((self.end_col, self.end_row), (self.start_col, self.start_row))
        }
    }

    pub fn is_empty(&self) -> bool {
        self.start_col == self.end_col && self.start_row == self.end_row
    }
}

pub struct App {
    pub global_state: GlobalState,
    pub active_project_idx: Option<usize>,
    pub layout: LayoutState,
    pub mode: InputMode,
    pub modal: Modal,
    pub picker: Option<Picker>,
    pub preview_lines: Option<(PathBuf, Vec<String>)>,
    pub left_list: ListState,
    pub right_list: ListState,
    pub pty_manager: PtyManager,
    pub active_workspace_idx: Option<usize>,
    pub active_tab: usize,
    pub modified_files: Vec<crate::git::diff::FileEntry>,

    pub keymap: Keymap,
    pub should_quit: bool,
    pub watcher: Option<crate::watcher::Watcher>,
    pub last_panes: Option<PaneRects>,
    pub sidebar_items: Vec<SidebarItem>,
    pub state_path: PathBuf,
    pub last_pty_size: (u16, u16),
    pub selection: Option<SelectionState>,
    pub last_frame_area: Rect,
    pub pending_workspace: Option<Option<String>>,
    pub archived_expanded: std::collections::HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabClick {
    Select(usize),
    Close(usize),
    Add,
}

impl App {
    pub async fn new(mut global_state: GlobalState, state_path: PathBuf) -> Result<Self> {
        let active_project_idx = global_state
            .active_project_id
            .as_ref()
            .and_then(|id| global_state.projects.iter().position(|project| &project.id == id))
            .or_else(|| (!global_state.projects.is_empty()).then_some(0));

        if let Some(idx) = active_project_idx {
            global_state.active_project_id = Some(global_state.projects[idx].id.clone());
        }

        let active_workspace_idx = active_project_idx
            .and_then(|idx| global_state.projects.get(idx))
            .and_then(|project| project.active().next().map(|_| 0));

        let watcher = if let Some(project) = active_project_idx.and_then(|idx| global_state.projects.get(idx)) {
            let mut watcher = crate::watcher::Watcher::new().ok();
            if let Some(w) = &mut watcher {
                let _ = w.watch(&project.repo_root);
            }
            watcher
        } else {
            None
        };

        let mut app = Self {
            global_state,
            active_project_idx,
            layout: LayoutState::new(),
            mode: InputMode::Terminal,
            modal: Modal::None,
            picker: None,
            preview_lines: None,
            left_list: ListState::default(),
            right_list: ListState::default(),
            pty_manager: PtyManager::new(),
            active_workspace_idx,
            active_tab: 0,
            modified_files: Vec::new(),

            keymap: Keymap::default_keymap(),
            should_quit: false,
            watcher,
            last_panes: None,
            sidebar_items: Vec::new(),
            state_path,
            last_pty_size: (24, 80),
            selection: None,
            last_frame_area: Rect::default(),
            pending_workspace: None,
            archived_expanded: std::collections::HashSet::new(),
        };
        app.refresh_diff().await;
        app.reattach_tmux_sessions();
        Ok(app)
    }

    fn reattach_tmux_sessions(&mut self) {
        if !crate::tmux::is_available() {
            return;
        }

        let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((80, 24));
        let frame_rect = Rect::new(0, 0, term_cols, term_rows);
        let panes = layout::compute(frame_rect, &self.layout);
        let rows = panes.terminal.height.saturating_sub(3);
        let cols = panes.terminal.width.saturating_sub(2);
        self.last_pty_size = (rows, cols);

        let existing_sessions = std::process::Command::new("tmux")
            .args(["list-sessions", "-F", "#{session_name}"])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<std::collections::HashSet<_>>()
            })
            .unwrap_or_default();

        let mut created_sessions = Vec::new();

        for project in &self.global_state.projects {
            for workspace in project.workspaces.iter().filter(|w| {
                !matches!(w.status, crate::state::WorkspaceStatus::Archived)
            }) {
                for tab in &workspace.tabs {
                    let tmux_name =
                        crate::tmux::tab_session_name(&project.id, &workspace.name, tab.id);
                    if !existing_sessions.contains(&tmux_name) {
                        let program = crate::workspace::tab_program_for_resume(&tab.command);
                        let _ = crate::tmux::new_session(
                            &tmux_name,
                            &workspace.worktree_path,
                            &program,
                            cols,
                            rows,
                        );
                        created_sessions.push(tmux_name);
                    }
                }
            }
        }

        if !created_sessions.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }

        for project in &self.global_state.projects {
            for workspace in project.active() {
                for tab in &workspace.tabs {
                    let tmux_name =
                        crate::tmux::tab_session_name(&project.id, &workspace.name, tab.id);

                    crate::tmux::enforce_session_options(&tmux_name);
                    crate::tmux::resize_session(&tmux_name, cols, rows);

                    if tab.command != "shell" {
                        let current = crate::tmux::pane_command(&tmux_name);
                        let is_shell = current.as_deref().is_none_or(|cmd| {
                            matches!(cmd, "bash" | "zsh" | "sh" | "fish" | "dash")
                        });
                        if is_shell {
                            let program = crate::workspace::tab_program_for_resume(&tab.command);
                            crate::tmux::send_key(&tmux_name, &program);
                            crate::tmux::send_key(&tmux_name, "Enter");
                        }
                    }

                    let _ = self.pty_manager.spawn_tab(
                        project.id.clone(),
                        workspace.name.clone(),
                        tab.id,
                        workspace.worktree_path.clone(),
                        "tmux",
                        &["attach-session", "-t", &tmux_name],
                        rows,
                        cols,
                    );
                }
            }
        }
    }

    pub fn active_project(&self) -> Option<&Project> {
        self.active_project_idx
            .and_then(|idx| self.global_state.projects.get(idx))
    }

    pub(crate) fn active_project_mut(&mut self) -> Option<&mut Project> {
        self.active_project_idx
            .and_then(|idx| self.global_state.projects.get_mut(idx))
    }

    pub(crate) fn active_workspace(&self) -> Option<&Workspace> {
        self.active_project()
            .and_then(|project| self.active_workspace_idx.and_then(|idx| project.active().nth(idx)))
    }

    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let mut events = EventStream::new();
        let mut refresh_tick = interval(Duration::from_secs(5));
        let mut status_tick = interval(Duration::from_secs(1));

        loop {
            terminal.draw(|frame| self.draw(frame))?;
            self.sync_pty_size();

            if let Some(name) = self.pending_workspace.take() {
                match self.create_workspace(name).await {
                    Ok(()) => self.modal = Modal::None,
                    Err(error) => {
                        self.modal = Modal::NewWorkspace(NewWorkspaceForm {
                            name_input: String::new(),
                            error: Some(error),
                        });
                    }
                }
                continue;
            }

            if self.should_quit {
                break;
            }

            tokio::select! {
                Some(Ok(event)) = events.next() => {
                    self.handle_event(event).await;
                }
                _ = self.pty_manager.output_notify.notified() => {}
                _ = status_tick.tick() => {}
                _ = refresh_tick.tick() => {
                    self.refresh_diff().await;
                }
                Some(event) = async {
                    if let Some(w) = &mut self.watcher {
                        w.next_event().await
                    } else {
                        futures::future::pending::<Option<crate::watcher::FsEvent>>().await
                    }
                } => {
                    let _ = event;
                    self.refresh_diff().await;
                }
            }
        }

        self.save_state();
        Ok(())
    }

    pub(crate) async fn refresh_diff(&mut self) {
        let (path, base_branch) = match (self.active_project(), self.active_workspace()) {
            (Some(_), Some(ws)) => (ws.worktree_path.clone(), ws.base_branch.clone()),
            (Some(p), None) => (p.repo_root.clone(), p.base_branch.clone()),
            _ => {
                self.modified_files.clear();
                self.right_list.select(None);
                return;
            }
        };

        if let Ok(files) = diff::modified_files(path, base_branch).await {
            self.modified_files = files;

            if self.modified_files.is_empty() {
                self.right_list.select(None);
            } else if self.right_list.selected().is_none() {
                self.right_list.select(Some(0));
            } else if let Some(selected) = self.right_list.selected() {
                self.right_list.select(Some(selected.min(self.modified_files.len() - 1)));
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        crate::ui::draw::draw(self, frame);
    }

    async fn handle_event(&mut self, event: Event) {
        crate::events::handle_event(self, event).await;
    }

    // Delegators retained for API shape symmetry with handle_event.
    // Internal callers inside event routing use crate::events::* directly.
    #[allow(dead_code)]
    async fn handle_mouse(&mut self, mouse: MouseEvent) {
        crate::events::handle_mouse(self, mouse).await;
    }

    #[allow(dead_code)]
    fn handle_scroll(&mut self, col: u16, row: u16, delta: isize) {
        crate::events::handle_scroll(self, col, row, delta);
    }

    #[allow(dead_code)]
    async fn handle_click(&mut self, col: u16, row: u16) {
        crate::events::handle_click(self, col, row).await;
    }

    #[allow(dead_code)]
    async fn handle_key(&mut self, key: KeyEvent) {
        crate::events::handle_key(self, key).await;
    }

    #[allow(dead_code)]
    async fn handle_picker_click(&mut self, col: u16, row: u16) -> bool {
        crate::events::handle_picker_click(self, col, row).await
    }

    #[allow(dead_code)]
    async fn apply_picker_outcome(&mut self, outcome: PickerOutcome) {
        crate::events::apply_picker_outcome(self, outcome).await;
    }

    #[allow(dead_code)]
    async fn dispatch_action(&mut self, action: Action) {
        crate::events::dispatch_action(self, action).await;
    }

    #[allow(dead_code)]
    async fn activate_sidebar_item(&mut self, index: usize) {
        crate::events::activate_sidebar_item(self, index).await;
    }

    pub(crate) fn open_new_tab_picker(&mut self) {
        self.picker = Some(Picker::new(
            vec![
                "opencode".to_string(),
                "claude".to_string(),
                "codex".to_string(),
                "aider".to_string(),
                "gemini".to_string(),
                "amp".to_string(),
                "goose".to_string(),
                "cline".to_string(),
                "shell".to_string(),
            ],
            PickerKind::NewTab,
        ));
    }

    pub(crate) fn select_active_workspace(&mut self, index: usize) {
        self.active_workspace_idx = Some(index);
        self.right_list.select(None);
    }

    pub(crate) fn refresh_active_workspace_after_change(&mut self) {
        let active_count = self.active_project().map(|project| project.active().count()).unwrap_or(0);
        self.active_workspace_idx = if active_count == 0 {
            None
        } else {
            Some(self.active_workspace_idx.unwrap_or(0).min(active_count - 1))
        };
    }

    pub(crate) fn save_state(&self) {
        if let Err(error) = self.global_state.save(&self.state_path) {
            tracing::error!("failed to save state: {error}");
        }
    }

    pub(crate) async fn switch_project(&mut self, idx: usize) {
        crate::workspace::switch_project(self, idx).await;
    }

    pub(crate) fn queue_workspace_creation(&mut self, form: &NewWorkspaceForm) {
        crate::workspace::queue_workspace_creation(self, form);
    }

    pub(crate) fn confirm_delete_workspace(&mut self, form: &crate::ui::modal::DeleteForm) {
        crate::workspace::confirm_delete_workspace(self, form);
    }

    pub(crate) fn archive_active_workspace(&mut self) {
        crate::workspace::archive_active_workspace(self);
    }

    pub(crate) fn delete_archived_workspace(&mut self, project_idx: usize, archived_idx: usize) {
        crate::workspace::delete_archived_workspace(self, project_idx, archived_idx);
    }

    pub(crate) async fn confirm_remove_project(&mut self, form: &crate::ui::modal::RemoveProjectForm) {
        crate::workspace::confirm_remove_project(self, form).await;
    }

    pub(crate) fn write_active_tab_input(&mut self, bytes: &[u8]) {
        let Some(project) = self.active_project() else { return };
        let Some(workspace) = self.active_workspace() else { return };

        let sessions = self.active_sessions();
        let Some((tab_id, session)) = sessions.get(self.active_tab) else {
            self.mode = InputMode::Normal;
            return;
        };

        if session.is_exited() {
            self.mode = InputMode::Normal;
            return;
        }

        let project_id = project.id.clone();
        let ws_name = workspace.name.clone();
        let tab_id = *tab_id;

        let _ = self.pty_manager.write_input(&project_id, &ws_name, tab_id, bytes);
    }

    pub(crate) fn copy_selection_to_clipboard(&self) {
        let Some(sel) = &self.selection else { return };
        if sel.is_empty() {
            return;
        }

        let sessions = self.active_sessions();
        let Some((_, session)) = sessions.get(self.active_tab) else { return };
        let Ok(parser) = session.parser.try_read() else { return };

        let screen = parser.screen();
        let ((sc, sr), (ec, er)) = sel.normalized();
        let text = screen.contents_between(sr, sc, er, ec.saturating_add(1));
        let trimmed = text.trim_end().to_string();

        if trimmed.is_empty() {
            return;
        }

        let _ = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(trimmed.as_bytes())?;
                }
                child.wait().map(|_| ())
            });
    }

    pub(crate) fn forward_key_to_pty(&mut self, key: &KeyEvent) {
        let Some(bytes) = crate::events::key_to_bytes(key) else { return };
        self.write_active_tab_input(&bytes);
    }

    fn sync_pty_size(&mut self) {
        let Some(panes) = &self.last_panes else { return };

        let rows = panes.terminal.height.saturating_sub(3);
        let cols = panes.terminal.width.saturating_sub(2);

        if rows == 0 || cols == 0 {
            return;
        }

        if (rows, cols) == self.last_pty_size {
            return;
        }

        self.last_pty_size = (rows, cols);

        let Some(project) = self.active_project() else { return };
        let Some(workspace) = self.active_workspace() else { return };

        let project_id = project.id.clone();
        let ws_name = workspace.name.clone();
        let _ = self.pty_manager.resize_all_for(&project_id, &ws_name, rows, cols);

        let tab_tmux_names: Vec<String> = workspace
            .tabs
            .iter()
            .map(|tab| crate::tmux::tab_session_name(&project_id, &ws_name, tab.id))
            .collect();
        tokio::task::spawn_blocking(move || {
            for name in tab_tmux_names {
                crate::tmux::resize_session(&name, cols, rows);
            }
        });
    }

    pub(crate) fn build_working_map(&self) -> std::collections::HashMap<(String, String), bool> {
        use std::time::Duration;
        let threshold = Duration::from_secs(2);
        let mut map = std::collections::HashMap::new();

        for project in &self.global_state.projects {
            for workspace in project.active() {
                if workspace.tabs.is_empty() {
                    continue;
                }
                let any_working = workspace.tabs.iter().any(|tab| {
                    self.pty_manager
                        .get_session(&project.id, &workspace.name, tab.id)
                        .map(|s| s.is_working(threshold))
                        .unwrap_or(false)
                });
                map.insert((project.id.clone(), workspace.name.clone()), any_working);
            }
        }
        map
    }

    pub(crate) fn active_sessions(&self) -> Vec<(u32, &crate::pty::session::PtySession)> {
        let Some(project) = self.active_project() else {
            return Vec::new();
        };
        let Some(workspace) = self.active_workspace() else {
            return Vec::new();
        };

        workspace
            .tabs
            .iter()
            .filter_map(|tab| {
                self.pty_manager
                    .get_session(&project.id, &workspace.name, tab.id)
                    .map(|session| (tab.id, session))
            })
            .collect()
    }

    async fn create_workspace(&mut self, name: Option<String>) -> Result<(), String> {
        crate::workspace::create_workspace(self, name).await
    }

    pub(crate) async fn create_tab(&mut self, command: String) -> Result<(), String> {
        crate::workspace::create_tab(self, command).await
    }

    pub(crate) async fn add_project_from_path(&mut self, path: String) -> Result<(), String> {
        crate::workspace::add_project_from_path(self, path).await
    }

    pub(crate) fn tab_at_column(&self, terminal: Rect, col: u16) -> Option<TabClick> {
        let workspace = self.active_workspace()?;
        let mut current_col = terminal.x;
        for (idx, tab) in workspace.tabs.iter().enumerate() {
            let label = crate::ui::terminal::tab_label(&tab.command);
            let width = format!(" {label} ✕ ").chars().count() as u16;
            if col >= current_col && col < current_col + width {
                let close_start = current_col + width.saturating_sub(2);
                return if col >= close_start {
                    Some(TabClick::Close(idx))
                } else {
                    Some(TabClick::Select(idx))
                };
            }
            current_col += width;
        }

        let add_width = " [+] ".chars().count() as u16;
        if col >= current_col && col < current_col + add_width {
            return Some(TabClick::Add);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Agent, Project, TabSpec};
    use git2::Repository;
    use std::path::Path;
    use tempfile::TempDir;

    fn init_repo(dir: &Path) -> Project {
        let repo = Repository::init(dir).unwrap();
        let sig = git2::Signature::now("test", "test@example.com").unwrap();
        std::fs::write(dir.join("initial.txt"), b"initial").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("initial.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let branch = repo.head().unwrap().shorthand().unwrap_or("main").to_string();
        Project::new(dir.to_path_buf(), branch)
    }

    #[tokio::test]
    async fn app_new_without_git_repo() {
        let app = App::new(GlobalState::default(), std::env::temp_dir().join("martins-test.json"))
            .await
            .unwrap();
        assert_eq!(app.active_project_idx, None);
        assert!(!app.should_quit);
    }

    #[tokio::test]
    async fn switch_project_updates_context() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();
        let mut state = GlobalState::default();
        let project1 = init_repo(tmp1.path());
        let project2 = init_repo(tmp2.path());
        state.active_project_id = Some(project1.id.clone());
        state.projects.push(project1);
        state.projects.push(project2.clone());

        let mut app = App::new(state, std::env::temp_dir().join("martins-switch.json"))
            .await
            .unwrap();
        app.switch_project(1).await;
        assert_eq!(app.active_project_idx, Some(1));
        assert_eq!(app.active_project().map(|p| p.id.as_str()), Some(project2.id.as_str()));
    }

    #[tokio::test]
    async fn tab_click_detects_select_close_and_add() {
        let tmp = TempDir::new().unwrap();
        let mut state = GlobalState::default();
        let mut project = init_repo(tmp.path());
        project.add_workspace(Workspace {
            name: "caetano".to_string(),
            worktree_path: tmp.path().join("caetano"),
            base_branch: "main".to_string(),
            agent: Agent::Opencode,
            status: crate::state::WorkspaceStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            tabs: vec![
                TabSpec { id: 0, command: "opencode".to_string() },
                TabSpec { id: 1, command: "shell".to_string() },
            ],
        });
        state.active_project_id = Some(project.id.clone());
        state.projects.push(project);

        let app = App::new(state, std::env::temp_dir().join("martins-tab-click.json"))
            .await
            .unwrap();
        let terminal = Rect { x: 0, y: 0, width: 80, height: 20 };

        assert_eq!(app.tab_at_column(terminal, 1), Some(TabClick::Select(0)));
        assert_eq!(app.tab_at_column(terminal, 10), Some(TabClick::Close(0)));
        assert_eq!(app.tab_at_column(terminal, 13), Some(TabClick::Select(1)));
        assert_eq!(app.tab_at_column(terminal, 19), Some(TabClick::Close(1)));
        assert_eq!(app.tab_at_column(terminal, 21), Some(TabClick::Add));
    }
}
