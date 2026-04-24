//! Application state and main event loop.

use crate::git::{diff, repo};
use crate::keys::{Action, InputMode, Keymap};
use crate::pty::manager::PtyManager;
use crate::state::{Agent, GlobalState, Project, TabSpec, Workspace};
use crate::ui::layout::{self, LayoutState, PaneRects};
use crate::ui::modal::{AddProjectForm, CommandArgsForm, Modal, NewWorkspaceForm};
use crate::ui::picker::{Picker, PickerKind, PickerOutcome};
use crate::ui::preview;
use anyhow::Result;
use crossterm::event::{
    Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use futures::StreamExt;
use ratatui::{
    DefaultTerminal, Frame,
    layout::Rect,
    widgets::ListState,
};
use std::path::{Path, PathBuf};
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
enum TabClick {
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
                        let program = tab_program_for_resume(&tab.command);
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
                            let program = tab_program_for_resume(&tab.command);
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

    async fn refresh_diff(&mut self) {
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
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key).await,
            Event::Mouse(mouse) => self.handle_mouse(mouse).await,
            Event::Paste(text) => {
                if self.mode == InputMode::Terminal {
                    let mut buf = Vec::with_capacity(text.len() + 12);
                    buf.extend_from_slice(b"\x1b[200~");
                    buf.extend_from_slice(text.as_bytes());
                    buf.extend_from_slice(b"\x1b[201~");
                    self.write_active_tab_input(&buf);
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }

    async fn handle_mouse(&mut self, mouse: MouseEvent) {
        let in_terminal = self.last_panes.as_ref().is_some_and(|p| {
            let inner = terminal_content_rect(p.terminal);
            rect_contains(inner, mouse.column, mouse.row)
        });

        if in_terminal {
            match mouse.kind {
                MouseEventKind::Drag(MouseButton::Left) => {
                    let inner = terminal_content_rect(self.last_panes.as_ref().unwrap().terminal);
                    let col = mouse.column.saturating_sub(inner.x).min(inner.width.saturating_sub(1));
                    let row = mouse.row.saturating_sub(inner.y).min(inner.height.saturating_sub(1));
                    if let Some(sel) = &mut self.selection {
                        sel.end_col = col;
                        sel.end_row = row;
                    } else {
                        self.selection = Some(SelectionState {
                            start_col: col,
                            start_row: row,
                            end_col: col,
                            end_row: row,
                            dragging: true,
                        });
                    }
                    return;
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    if let Some(sel) = self.selection.take() {
                        if !sel.is_empty() {
                            self.selection = Some(sel);
                            self.copy_selection_to_clipboard();
                            return;
                        }
                    }
                }
                _ => {}
            }
        }

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if self.selection.is_some() {
                    self.selection = None;
                }
                self.handle_click(mouse.column, mouse.row).await;
            }
            MouseEventKind::ScrollUp => self.handle_scroll(mouse.column, mouse.row, -1),
            MouseEventKind::ScrollDown => self.handle_scroll(mouse.column, mouse.row, 1),
            _ => {}
        }
    }

    fn handle_scroll(&mut self, col: u16, row: u16, delta: isize) {
        if let Modal::AddProject(ref mut form) = self.modal {
            form.move_selection(delta);
            return;
        }

        let Some(panes) = &self.last_panes else { return };

        if rect_contains(panes.terminal, col, row) {
            let inner = terminal_content_rect(panes.terminal);
            let local_col = col.saturating_sub(inner.x).saturating_add(1).max(1);
            let local_row = row.saturating_sub(inner.y).saturating_add(1).max(1);
            let button: u8 = if delta < 0 { 64 } else { 65 };
            let seq = format!("\x1b[<{button};{local_col};{local_row}M");
            self.write_active_tab_input(seq.as_bytes());
            return;
        }

        if let Some(right) = panes.right
            && rect_contains(right, col, row)
        {
            move_list_selection(&mut self.right_list, self.modified_files.len(), delta);
            return;
        }

        if let Some(left) = panes.left
            && rect_contains(left, col, row)
        {
            move_sidebar_to_workspace(&mut self.left_list, &self.sidebar_items, delta);
            return;
        }

        move_sidebar_to_workspace(&mut self.left_list, &self.sidebar_items, delta);
    }

    async fn handle_click(&mut self, col: u16, row: u16) {
        if self.handle_picker_click(col, row).await {
            return;
        }

        if self.handle_modal_click(col, row).await {
            return;
        }

        let Some(panes) = self.last_panes.clone() else { return };

        if !rect_contains(panes.terminal, col, row) {
            self.mode = InputMode::Normal;
        }

        if rect_contains(panes.terminal, col, row) {
            if row == panes.terminal.y {
                if let Some(click) = self.tab_at_column(panes.terminal, col) {
                    match click {
                        TabClick::Select(idx) => self.dispatch_action(Action::ClickTab(idx)).await,
                        TabClick::Close(idx) => {
                            self.active_tab = idx;
                            self.dispatch_action(Action::CloseTab).await;
                        }
                        TabClick::Add => self.dispatch_action(Action::NewTab).await,
                    }
                    return;
                }
            }

            let inner = terminal_content_rect(panes.terminal);
            if rect_contains(inner, col, row) {
                let local_col = col.saturating_sub(inner.x) + 1;
                let local_row = row.saturating_sub(inner.y) + 1;
                let press = format!("\x1b[<0;{local_col};{local_row}M");
                let release = format!("\x1b[<0;{local_col};{local_row}m");
                self.write_active_tab_input(press.as_bytes());
                self.write_active_tab_input(release.as_bytes());
            }

            self.mode = InputMode::Terminal;
            return;
        }

        if let Some(left) = panes.left
            && rect_contains(left, col, row)
            && row > left.y
            && row < left.y + left.height - 1
        {
            let local_row = (row - left.y - 1) as usize;
            if let Some(item) = self.sidebar_items.get(local_row).cloned() {
                match item {
                    SidebarItem::RemoveProject(idx) => {
                        let delete_zone_start = left.x + left.width.saturating_sub(4);
                        if col >= delete_zone_start {
                            if let Some(project) = self.global_state.projects.get(idx) {
                                self.modal = Modal::ConfirmRemoveProject(crate::ui::modal::RemoveProjectForm {
                                    project_name: project.name.clone(),
                                    project_id: project.id.clone(),
                                });
                            }
                        } else {
                            self.dispatch_action(Action::ClickProject(idx)).await;
                        }
                    }
                    SidebarItem::Workspace(project_idx, workspace_idx) => {
                        let delete_zone_start = left.x + left.width.saturating_sub(4);
                        if col >= delete_zone_start {
                            if self.active_project_idx != Some(project_idx) {
                                self.switch_project(project_idx).await;
                            }
                            self.select_active_workspace(workspace_idx);
                            self.archive_active_workspace();
                        } else {
                            self.dispatch_action(Action::ClickWorkspace(project_idx, workspace_idx)).await;
                        }
                    }
                    SidebarItem::ArchivedHeader(project_idx) => {
                        if let Some(project) = self.global_state.projects.get(project_idx) {
                            let id = project.id.clone();
                            if !self.archived_expanded.remove(&id) {
                                self.archived_expanded.insert(id);
                            }
                        }
                    }
                    SidebarItem::ArchivedWorkspace(project_idx, archived_idx) => {
                        let delete_zone_start = left.x + left.width.saturating_sub(4);
                        if col >= delete_zone_start {
                            self.delete_archived_workspace(project_idx, archived_idx);
                        }
                    }
                    SidebarItem::AddProject => self.dispatch_action(Action::AddProject).await,
                    SidebarItem::NewWorkspace(project_idx) => {
                        self.dispatch_action(Action::ClickProject(project_idx)).await;
                        self.dispatch_action(Action::NewWorkspace).await;
                    }
                }
            }
            return;
        }

        if let Some(right) = panes.right
            && rect_contains(right, col, row)
            && row > right.y
            && row < right.y + right.height - 1
        {
            let local_row = (row - right.y - 1) as usize;
            let offset = self.right_list.offset();
            let absolute_idx = offset + local_row;
            if absolute_idx < self.modified_files.len() {
                self.dispatch_action(Action::ClickFile(absolute_idx)).await;
            }
            return;
        }

        if rect_contains(panes.menu_bar, col, row) {
            let local_col = col.saturating_sub(panes.menu_bar.x);
            if let Some(action) = menu_action_at_column(local_col) {
                self.dispatch_action(action).await;
            }
            return;
        }

        if rect_contains(panes.status_bar, col, row) {
            let quit_label_len = " [Quit] ".len() as u16;
            let quit_start = panes.status_bar.x + panes.status_bar.width - quit_label_len;
            if col >= quit_start {
                self.dispatch_action(Action::Quit).await;
            }
        }
    }

    async fn handle_key(&mut self, key: KeyEvent) {
        if let KeyCode::F(n) = key.code {
            if (1..=9).contains(&n) {
                let tab_count = self
                    .active_workspace()
                    .map(|ws| ws.tabs.len())
                    .unwrap_or(0);
                if tab_count > 0 {
                    self.active_tab = (n as usize - 1).min(tab_count - 1);
                    self.mode = InputMode::Terminal;
                }
                return;
            }
        }



        if let Some(picker) = &mut self.picker {
            let outcome = picker.on_key(key);
            self.apply_picker_outcome(outcome).await;
            return;
        }

        if matches!(self.modal, Modal::Loading(_)) {
            return;
        }

        if !matches!(self.modal, Modal::None) {
            self.handle_modal_key(key).await;
            return;
        }

        if self.mode == InputMode::Terminal {
            if key.code == KeyCode::Char('b')
                && key.modifiers.contains(KeyModifiers::CONTROL)
            {
                self.mode = InputMode::Normal;
                return;
            }
            self.forward_key_to_pty(&key);
            return;
        }



        if let Some(action) = self.keymap.resolve_normal(&key).cloned() {
            self.dispatch_action(action).await;
        }
    }

    async fn handle_modal_key(&mut self, key: KeyEvent) {
        let modal = std::mem::take(&mut self.modal);
        match modal {
            Modal::None => {}
            Modal::NewWorkspace(mut form) => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter => {
                    self.queue_workspace_creation(&form);
                }
                KeyCode::Backspace => {
                    form.name_input.pop();
                    form.error = None;
                    self.modal = Modal::NewWorkspace(form);
                }
                KeyCode::Char(c) => {
                    form.name_input.push(c);
                    form.error = None;
                    self.modal = Modal::NewWorkspace(form);
                }
                _ => self.modal = Modal::NewWorkspace(form),
            },
            Modal::AddProject(mut form) => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter => {
                    if let Some(entry) = form.selected_entry().cloned() {
                        if entry.is_git_repo {
                            let path = entry.path.to_string_lossy().to_string();
                            match self.add_project_from_path(path).await {
                                Ok(()) => self.modal = Modal::None,
                                Err(error) => {
                                    form.error = Some(error);
                                    self.modal = Modal::AddProject(form);
                                }
                            }
                        } else {
                            form.navigate_into(form.selected);
                            self.modal = Modal::AddProject(form);
                        }
                    } else {
                        form.navigate_up();
                        self.modal = Modal::AddProject(form);
                    }
                }
                KeyCode::Backspace => {
                    form.navigate_up();
                    self.modal = Modal::AddProject(form);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    form.move_selection(1);
                    self.modal = Modal::AddProject(form);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    form.move_selection(-1);
                    self.modal = Modal::AddProject(form);
                }
                _ => self.modal = Modal::AddProject(form),
            },
            Modal::ConfirmDelete(form) => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter => {
                    self.confirm_delete_workspace(&form);
                    self.modal = Modal::None;
                }
                _ => self.modal = Modal::ConfirmDelete(form),
            },
            Modal::ConfirmQuit => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter => {
                    self.modal = Modal::None;
                    self.should_quit = true;
                }
                _ => self.modal = Modal::ConfirmQuit,
            },
            Modal::ConfirmArchive(form) => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter => {
                    if let Some(project) = self.active_project_mut() {
                        project.archive(&form.workspace_name);
                    }
                    self.modal = Modal::None;
                    self.refresh_active_workspace_after_change();
                    self.save_state();
                }
                _ => self.modal = Modal::ConfirmArchive(form),
            },
            Modal::ConfirmRemoveProject(form) => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter => {
                    self.confirm_remove_project(&form).await;
                    self.modal = Modal::None;
                }
                _ => self.modal = Modal::ConfirmRemoveProject(form),
            },
            Modal::CommandArgs(mut form) => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter => {
                    let command = if form.args_input.trim().is_empty() {
                        form.agent.clone()
                    } else {
                        format!("{} {}", form.agent, form.args_input.trim())
                    };
                    if let Err(error) = self.create_tab(command).await {
                        tracing::error!("failed to create tab: {error}");
                    }
                }
                KeyCode::Backspace => {
                    form.args_input.pop();
                    self.modal = Modal::CommandArgs(form);
                }
                KeyCode::Char(c) => {
                    form.args_input.push(c);
                    self.modal = Modal::CommandArgs(form);
                }
                _ => self.modal = Modal::CommandArgs(form),
            },
            Modal::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')) {
                    self.modal = Modal::None;
                } else {
                    self.modal = Modal::Help;
                }
            }
            Modal::Loading(_) => {}
        }
    }

    async fn handle_modal_click(&mut self, col: u16, row: u16) -> bool {
        let frame_area = self.last_frame_area;
        if frame_area.width == 0 || frame_area.height == 0 {
            return false;
        }

        match self.modal.clone() {
            Modal::AddProject(form) => {
                let modal_area = crate::ui::modal::centered_rect(60, 70, frame_area);
                if !rect_contains(modal_area, col, row) {
                    self.modal = Modal::None;
                    return true;
                }

                let inner_y = modal_area.y + 1;
                let inner_height = modal_area.height.saturating_sub(2);

                let footer_height: u16 = if form.error.is_some() { 3 } else { 2 };
                let list_y = inner_y + 2;
                let list_height = inner_height.saturating_sub(2 + footer_height) as usize;

                if row < list_y || row >= list_y + list_height as u16 {
                    return true;
                }

                let click_row = (row - list_y) as usize;

                let scroll_offset = if form.selected >= list_height {
                    form.selected - list_height + 1
                } else {
                    0
                };

                let has_parent_row = scroll_offset == 0;
                if has_parent_row && click_row == 0 {
                    let mut form = form.clone();
                    form.navigate_up();
                    self.modal = Modal::AddProject(form);
                    return true;
                }

                let entry_offset = if has_parent_row { click_row - 1 } else { click_row };
                let entry_idx = scroll_offset + entry_offset;

                let entry = form.entries.get(entry_idx).cloned();
                let mut form = form.clone();

                if let Some(entry) = entry {
                    if entry.is_git_repo {
                        let path = entry.path.to_string_lossy().to_string();
                        match self.add_project_from_path(path).await {
                            Ok(()) => self.modal = Modal::None,
                            Err(error) => {
                                form.error = Some(error);
                                self.modal = Modal::AddProject(form);
                            }
                        }
                    } else {
                        form.navigate_into(entry_idx);
                        self.modal = Modal::AddProject(form);
                    }
                } else {
                    form.selected = entry_idx.min(form.entries.len().saturating_sub(1));
                    self.modal = Modal::AddProject(form);
                }

                true
            }
            Modal::ConfirmQuit => {
                let modal_area = crate::ui::modal::centered_rect(40, 30, frame_area);
                if !rect_contains(modal_area, col, row) {
                    self.modal = Modal::None;
                    return true;
                }

                if row == modal_button_row_y(modal_area) {
                    if is_modal_first_button(modal_area, col, 14) {
                        self.modal = Modal::None;
                        self.should_quit = true;
                    } else {
                        self.modal = Modal::None;
                    }
                }
                true
            }
            Modal::ConfirmArchive(form) => {
                let modal_area = crate::ui::modal::centered_rect(50, 30, frame_area);
                if !rect_contains(modal_area, col, row) {
                    self.modal = Modal::None;
                    return true;
                }

                if row == modal_button_row_y(modal_area) {
                    if is_modal_first_button(modal_area, col, 17) {
                        if let Some(project) = self.active_project_mut() {
                            project.archive(&form.workspace_name);
                        }
                        self.refresh_active_workspace_after_change();
                        self.save_state();
                    }
                    self.modal = Modal::None;
                }
                true
            }
            Modal::ConfirmDelete(form) => {
                let modal_area = crate::ui::modal::centered_rect(50, 40, frame_area);
                if !rect_contains(modal_area, col, row) {
                    self.modal = Modal::None;
                    return true;
                }

                if row == modal_button_row_y(modal_area) {
                    if is_modal_first_button(modal_area, col, 12) {
                        self.confirm_delete_workspace(&form);
                    }
                    self.modal = Modal::None;
                }
                true
            }
            Modal::ConfirmRemoveProject(form) => {
                let modal_area = crate::ui::modal::centered_rect(50, 35, frame_area);
                if !rect_contains(modal_area, col, row) {
                    self.modal = Modal::None;
                    return true;
                }

                if row == modal_button_row_y(modal_area) {
                    if is_modal_first_button(modal_area, col, 16) {
                        self.confirm_remove_project(&form).await;
                    }
                    self.modal = Modal::None;
                }
                true
            }
            Modal::Help => {
                let modal_area = crate::ui::modal::centered_rect(70, 80, frame_area);
                self.modal = Modal::None;
                let _ = rect_contains(modal_area, col, row);
                true
            }
            Modal::NewWorkspace(form) => {
                let modal_area = crate::ui::modal::centered_rect(50, 30, frame_area);
                if !rect_contains(modal_area, col, row) {
                    self.modal = Modal::None;
                    return true;
                }

                if row == modal_button_row_y(modal_area) {
                    if is_modal_first_button(modal_area, col, 12) {
                        self.queue_workspace_creation(&form);
                    } else {
                        self.modal = Modal::None;
                    }
                } else {
                    self.modal = Modal::NewWorkspace(form);
                }
                true
            }
            Modal::CommandArgs(form) => {
                let modal_area = crate::ui::modal::centered_rect(50, 30, frame_area);
                if !rect_contains(modal_area, col, row) {
                    self.modal = Modal::None;
                    return true;
                }

                if row == modal_button_row_y(modal_area) {
                    if is_modal_first_button(modal_area, col, 12) {
                        let command = if form.args_input.trim().is_empty() {
                            form.agent.clone()
                        } else {
                            format!("{} {}", form.agent, form.args_input.trim())
                        };
                        if let Err(error) = self.create_tab(command).await {
                            tracing::error!("failed to create tab: {error}");
                        }
                    } else {
                        self.modal = Modal::None;
                    }
                } else {
                    self.modal = Modal::CommandArgs(form);
                }
                true
            }
            _ => false,
        }
    }

    async fn handle_picker_click(&mut self, col: u16, row: u16) -> bool {
        let Some(picker) = &self.picker else { return false };

        let frame_area = self.last_frame_area;
        if frame_area.width == 0 || frame_area.height == 0 {
            return false;
        }

        let picker_area = picker_area(frame_area);
        if !rect_contains(picker_area, col, row) {
            self.picker = None;
            return true;
        }

        let list_y = picker_area.y + 3;
        let list_height = picker_area.height.saturating_sub(4);
        if row >= list_y && row < list_y + list_height {
            let click_idx = (row - list_y) as usize;
            if let Some(&item_idx) = picker.filtered.get(click_idx) {
                self.apply_picker_outcome(PickerOutcome::Selected(item_idx)).await;
                return true;
            }
        }

        true
    }

    async fn apply_picker_outcome(&mut self, outcome: PickerOutcome) {
        match outcome {
            PickerOutcome::Cancelled => self.picker = None,
            PickerOutcome::Selected(index) => {
                let kind = self.picker.as_ref().map(|picker| picker.kind.clone());
                let picked_item = self
                    .picker
                    .as_ref()
                    .and_then(|p| p.items.get(index).cloned());
                self.picker = None;
                match kind {
                    Some(PickerKind::Workspaces) => self.select_active_workspace(index),
                    Some(PickerKind::NewTab) => {
                        if let Some(command) = picked_item {
                            if command == "shell" {
                                if let Err(error) = self.create_tab("shell".to_string()).await {
                                    tracing::error!("failed to create tab: {error}");
                                }
                            } else {
                                self.modal = Modal::CommandArgs(CommandArgsForm {
                                    agent: command,
                                    args_input: String::new(),
                                });
                            }
                        }
                    }
                    Some(PickerKind::ModifiedFiles) | None => {}
                }
            }
            PickerOutcome::Continue => {}
        }
    }

    async fn dispatch_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.modal = Modal::ConfirmQuit,
            Action::NextItem => {
                move_sidebar_to_workspace(&mut self.left_list, &self.sidebar_items, 1);
                if let Some(idx) = self.left_list.selected() {
                    self.activate_sidebar_item(idx).await;
                }
            }
            Action::PrevItem => {
                move_sidebar_to_workspace(&mut self.left_list, &self.sidebar_items, -1);
                if let Some(idx) = self.left_list.selected() {
                    self.activate_sidebar_item(idx).await;
                }
            }
            Action::EnterSelected => {
                let has_tabs = self
                    .active_workspace()
                    .map(|ws| !ws.tabs.is_empty())
                    .unwrap_or(false);
                if has_tabs {
                    self.mode = InputMode::Terminal;
                } else if self.active_workspace().is_some() {
                    self.open_new_tab_picker();
                }
            }
            Action::EnterTerminalMode | Action::FocusTerminal => self.mode = InputMode::Terminal,
            Action::ToggleSidebarLeft => self.layout.toggle_left(),
            Action::ToggleSidebarRight => self.layout.toggle_right(),
            Action::OpenFuzzy => {
                let items: Vec<String> = self
                    .active_project()
                    .map(|project| project.active().map(|workspace| workspace.name.clone()).collect())
                    .unwrap_or_default();
                self.picker = Some(Picker::new(items, PickerKind::Workspaces));
            }
            Action::NewTab => {
                self.open_new_tab_picker();
            }
            Action::CloseTab => {
                let Some(project) = self.active_project() else {
                    return;
                };
                let Some(workspace) = self.active_workspace() else {
                    return;
                };
                let Some(tab) = workspace.tabs.get(self.active_tab).cloned() else {
                    return;
                };

                let project_id = project.id.clone();
                let ws_name = workspace.name.clone();
                let tmux_name = crate::tmux::tab_session_name(&project_id, &ws_name, tab.id);
                let current_active_tab = self.active_tab;

                crate::tmux::kill_session(&tmux_name);
                self.pty_manager.close_tab(&project_id, &ws_name, tab.id);

                if let Some(project) = self.active_project_mut()
                    && let Some(workspace) = project.workspaces.iter_mut().find(|workspace| workspace.name == ws_name)
                {
                    workspace.tabs.retain(|existing| existing.id != tab.id);
                    if workspace.tabs.is_empty() {
                        self.active_tab = 0;
                    } else {
                        self.active_tab = current_active_tab.min(workspace.tabs.len() - 1);
                    }
                }

                self.save_state();
            }
            Action::SwitchTab(n) => {
                let Some(workspace) = self.active_workspace() else {
                    return;
                };
                if workspace.tabs.is_empty() {
                    return;
                }

                self.active_tab = (n as usize - 1).min(workspace.tabs.len() - 1);
                self.mode = InputMode::Terminal;
            }
            Action::NewWorkspace | Action::NewWorkspaceAuto => {
                if self.active_project().is_some() {
                self.modal = Modal::NewWorkspace(NewWorkspaceForm::default());
                } else {
                    self.modal = Modal::AddProject(AddProjectForm::default());
                }
            }
            Action::AddProject => {
                self.modal = Modal::AddProject(AddProjectForm::default());
            }
            Action::ShowHelp => {
                self.modal = Modal::Help;
            }
            Action::ArchiveWorkspace => {
                if self.global_state.projects.is_empty() {
                    self.modal = Modal::AddProject(AddProjectForm::default());
                    return;
                }
                self.archive_active_workspace();
            }
            Action::Preview => {
                if let (Some(project), Some(index)) = (self.active_project(), self.right_list.selected())
                    && let Some(entry) = self.modified_files.get(index)
                {
                    let full_path = project.repo_root.join(&entry.path);
                    let lines = preview::bat_preview(&full_path, 200);
                    self.preview_lines = Some((full_path, lines));
                }
            }
            Action::UnarchiveWorkspace => {}
            Action::DeleteWorkspace => {
                if let Some(idx) = self.active_workspace_idx {
                    let name = self
                        .active_project()
                        .and_then(|project| project.active().nth(idx))
                        .map(|workspace| workspace.name.clone());
                    if let Some(name) = name {
                        self.modal = Modal::ConfirmDelete(crate::ui::modal::DeleteForm {
                            workspace_name: name,
                            unpushed_commits: 0,
                            delete_branch: false,
                        });
                    }
                }
            }
            Action::ClickProject(idx) => {
                if self.active_project_idx == Some(idx) {
                    if let Some(project) = self.global_state.projects.get_mut(idx) {
                        project.expanded = !project.expanded;
                    }
                    self.save_state();
                } else {
                    self.switch_project(idx).await;
                    if let Some(project) = self.global_state.projects.get_mut(idx) {
                        project.expanded = true;
                    }
                    self.save_state();
                }
            }
            Action::ClickWorkspace(project_idx, workspace_idx) => {
                if self.active_project_idx != Some(project_idx) {
                    self.switch_project(project_idx).await;
                }
                self.select_active_workspace(workspace_idx);
                self.refresh_diff().await;
                let has_tabs = self
                    .active_workspace()
                    .map(|ws| !ws.tabs.is_empty())
                    .unwrap_or(false);
                if has_tabs {
                    self.mode = InputMode::Terminal;
                } else {
                    self.open_new_tab_picker();
                }
            }
            Action::ClickTab(idx) => {
                self.active_tab = idx;
                self.mode = InputMode::Terminal;
            }
            Action::ClickFile(idx) => {
                self.right_list.select(Some(idx));
                if let Some(entry) = self.modified_files.get(idx).cloned() {
                    let path = entry.path.to_string_lossy().to_string();
                    if let Err(error) = self.create_tab(format!("diff {}", path)).await {
                        tracing::warn!("failed to open diff tab: {error}");
                    }
                }
            }
            Action::ToggleProjectExpand(idx) => {
                if let Some(project) = self.global_state.projects.get_mut(idx) {
                    project.expanded = !project.expanded;
                }
                self.save_state();
            }
            _ => {}
        }
    }

    async fn activate_sidebar_item(&mut self, index: usize) {
        let Some(item) = self.sidebar_items.get(index).cloned() else { return };
        match item {
            SidebarItem::RemoveProject(project_idx) => {
                if self.active_project_idx != Some(project_idx) {
                    self.switch_project(project_idx).await;
                }
            }
            SidebarItem::Workspace(project_idx, workspace_idx) => {
                if self.active_project_idx != Some(project_idx) {
                    self.switch_project(project_idx).await;
                }
                self.select_active_workspace(workspace_idx);
                self.refresh_diff().await;
            }
            _ => {}
        }
    }

    fn open_new_tab_picker(&mut self) {
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

    fn select_active_workspace(&mut self, index: usize) {
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

    async fn switch_project(&mut self, idx: usize) {
        if idx >= self.global_state.projects.len() {
            return;
        }

        let old_repo_root = self.active_project().map(|project| project.repo_root.clone());
        let new_repo_root = self.global_state.projects[idx].repo_root.clone();
        let new_project_id = self.global_state.projects[idx].id.clone();

        if let Some(watcher) = &mut self.watcher {
            if let Some(old_repo_root) = old_repo_root {
                let _ = watcher.unwatch(&old_repo_root);
            }
            let _ = watcher.watch(&new_repo_root);
        } else if let Ok(mut watcher) = crate::watcher::Watcher::new() {
            let _ = watcher.watch(&new_repo_root);
            self.watcher = Some(watcher);
        }

        self.active_project_idx = Some(idx);
        self.global_state.active_project_id = Some(new_project_id);
        self.active_workspace_idx = self.global_state.projects[idx].active().next().map(|_| 0);
        self.active_tab = 0;
        self.preview_lines = None;
        self.right_list.select(None);
        self.refresh_diff().await;
    }

    pub(crate) fn queue_workspace_creation(&mut self, form: &NewWorkspaceForm) {
        let name = (!form.name_input.is_empty()).then(|| form.name_input.clone());
        self.modal = Modal::Loading("Creating workspace...".to_string());
        self.pending_workspace = Some(name);
    }

    pub(crate) fn confirm_delete_workspace(&mut self, form: &crate::ui::modal::DeleteForm) {
        let name = form.workspace_name.clone();
        if let Some(project) = self.active_project_mut() {
            project.remove(&name);
        }
        self.refresh_active_workspace_after_change();
        self.save_state();
    }

    fn archive_active_workspace(&mut self) {
        let Some(ws) = self.active_workspace() else { return };
        let ws_name = ws.name.clone();
        let worktree_path = ws.worktree_path.clone();
        let tab_ids: Vec<u32> = ws.tabs.iter().map(|t| t.id).collect();
        let Some(project) = self.active_project() else { return };
        let project_id = project.id.clone();

        for tab_id in &tab_ids {
            let tmux_name = crate::tmux::tab_session_name(&project_id, &ws_name, *tab_id);
            crate::tmux::kill_session(&tmux_name);
            self.pty_manager.close_tab(&project_id, &ws_name, *tab_id);
        }

        if let Some(project) = self.active_project_mut() {
            project.archive(&ws_name);
        }
        self.refresh_active_workspace_after_change();
        self.save_state();

        let _ = std::fs::remove_dir_all(&worktree_path);
    }

    fn delete_archived_workspace(&mut self, project_idx: usize, archived_idx: usize) {
        let Some(project) = self.global_state.projects.get(project_idx) else { return };
        let Some(ws) = project.archived().nth(archived_idx) else { return };
        let ws_name = ws.name.clone();
        let worktree_path = ws.worktree_path.clone();

        if let Some(project) = self.global_state.projects.get_mut(project_idx) {
            project.delete_workspace(&ws_name);
        }

        let _ = std::fs::remove_dir_all(&worktree_path);
        self.save_state();
    }

    pub(crate) async fn confirm_remove_project(&mut self, form: &crate::ui::modal::RemoveProjectForm) {
        self.global_state.remove_project(&form.project_id);
        self.active_project_idx = self
            .global_state
            .active_project_id
            .as_ref()
            .and_then(|id| self.global_state.projects.iter().position(|project| &project.id == id));
        if let Some(idx) = self.active_project_idx {
            self.switch_project(idx).await;
        } else {
            self.active_workspace_idx = None;
            self.active_tab = 0;
            self.modified_files.clear();
            self.right_list.select(None);
            self.preview_lines = None;
            self.watcher = None;
        }
        self.save_state();
    }

    fn write_active_tab_input(&mut self, bytes: &[u8]) {
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

    fn copy_selection_to_clipboard(&self) {
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

    fn forward_key_to_pty(&mut self, key: &KeyEvent) {
        let Some(bytes) = key_to_bytes(key) else { return };
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
        let project = self
            .active_project_mut()
            .ok_or_else(|| "no active project".to_string())?;

        if let Some(ref n) = name {
            if project.workspaces.iter().any(|w| w.name == *n) {
                return Err(format!("workspace '{}' already exists", n));
            }
        }

        let repo_root = project.repo_root.clone();
        let base_branch = project.base_branch.clone();

        let ws_name = crate::agents::create_workspace_entry(project, name, Agent::default())
            .map_err(|error| error.to_string())?;

        let wt_path = crate::git::worktree::create(
            repo_root,
            ws_name.clone(),
            base_branch,
        )
        .await;

        if let Err(e) = &wt_path {
            if let Some(project) = self.active_project_mut() {
                project.remove(&ws_name);
            }
            return Err(e.to_string());
        }
        let wt_path = wt_path.unwrap();

        let ws = self
            .active_project_mut()
            .and_then(|p| p.workspaces.iter_mut().find(|w| w.name == ws_name))
            .ok_or_else(|| "workspace disappeared".to_string())?;
        ws.worktree_path = wt_path;
        ws.status = crate::state::WorkspaceStatus::Active;

        let active_count = self
            .active_project()
            .map(|p| p.active().count())
            .unwrap_or(0);
        self.active_workspace_idx = active_count.checked_sub(1);
        self.active_tab = 0;
        self.save_state();

        let _ = self.create_tab("shell".to_string()).await;
        Ok(())
    }

    pub(crate) async fn create_tab(&mut self, command: String) -> Result<(), String> {
        let Some(project) = self.active_project() else {
            return Err("no active project".to_string());
        };
        let Some(workspace) = self.active_workspace() else {
            return Err("no active workspace".to_string());
        };

        let project_id = project.id.clone();
        let ws_name = workspace.name.clone();
        let worktree_path = workspace.worktree_path.clone();
        let next_id = workspace.tabs.iter().map(|tab| tab.id).max().map_or(0, |id| id + 1);
        let (rows, cols) = self.last_pty_size;
        let tmux_name = crate::tmux::tab_session_name(&project_id, &ws_name, next_id);
        let program = tab_program_for_new(&command);

        let tmux_name_c = tmux_name.clone();
        let worktree_c = worktree_path.clone();
        let program_c = program.clone();
        tokio::task::spawn_blocking(move || {
            crate::tmux::new_session(&tmux_name_c, &worktree_c, &program_c, cols, rows)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        if let Some(project) = self.active_project_mut()
            && let Some(workspace) = project.workspaces.iter_mut().find(|workspace| workspace.name == ws_name)
        {
            workspace.tabs.push(TabSpec {
                id: next_id,
                command: command.clone(),
            });
            self.active_tab = workspace.tabs.len() - 1;
        }

        self.pty_manager
            .spawn_tab(
                project_id,
                ws_name,
                next_id,
                worktree_path,
                "tmux",
                &["attach-session", "-t", &tmux_name],
                rows,
                cols,
            )
            .map_err(|error| error.to_string())?;

        self.mode = InputMode::Terminal;
        self.save_state();
        Ok(())
    }

    pub(crate) async fn add_project_from_path(&mut self, path: String) -> Result<(), String> {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err("path is required".to_string());
        }

        let repo_root = repo::discover(Path::new(trimmed)).map_err(|error| error.to_string())?;
        let base_branch = repo::current_branch_async(repo_root.clone())
            .await
            .unwrap_or_else(|_| "main".to_string());
        let project_id = self.global_state.ensure_project(&repo_root, base_branch);
        let _ = crate::config::ensure_gitignore(&repo_root);
        self.global_state.active_project_id = Some(project_id.clone());
        self.active_project_idx = self
            .global_state
            .projects
            .iter()
            .position(|project| project.id == project_id);
        self.active_workspace_idx = self.active_project().and_then(|project| project.active().next().map(|_| 0));
        self.save_state();
        if let Some(idx) = self.active_project_idx {
            self.switch_project(idx).await;
        }
        Ok(())
    }

    fn tab_at_column(&self, terminal: Rect, col: u16) -> Option<TabClick> {
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

fn rect_contains(rect: Rect, col: u16, row: u16) -> bool {
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

fn modal_button_row_y(modal_area: Rect) -> u16 {
    modal_area.y + modal_area.height.saturating_sub(2)
}

fn is_modal_first_button(modal_area: Rect, col: u16, width: u16) -> bool {
    let inner_x = modal_area.x + 1;
    col >= inner_x && col < inner_x + width
}

fn picker_area(frame_area: Rect) -> Rect {
    let w = (frame_area.width as f32 * 0.6) as u16;
    let h = (frame_area.height as f32 * 0.5) as u16;
    let x = (frame_area.width.saturating_sub(w)) / 2;
    let y = (frame_area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

fn move_list_selection(list: &mut ListState, len: usize, delta: isize) {
    if len == 0 {
        list.select(None);
        return;
    }

    let current = list.selected().unwrap_or(0) as isize;
    let next = (current + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
    list.select(Some(next));
}

fn move_sidebar_to_workspace(
    list: &mut ListState,
    items: &[SidebarItem],
    delta: isize,
) {
    if items.is_empty() {
        list.select(None);
        return;
    }
    let current = list.selected().unwrap_or(0) as isize;
    let step = if delta > 0 { 1isize } else { -1 };
    let len = items.len() as isize;
    let mut pos = current + step;
    while pos >= 0 && pos < len {
        if matches!(items[pos as usize], SidebarItem::Workspace(_, _)) {
            list.select(Some(pos as usize));
            return;
        }
        pos += step;
    }
}

fn menu_action_at_column(col: u16) -> Option<Action> {
    const MENU_ITEMS: &[(u16, u16, Action)] = &[
        (1, 5, Action::NewWorkspace),
        (8, 5, Action::NewTab),
        (15, 8, Action::DeleteWorkspace),
        (25, 6, Action::ShowHelp),
        (33, 6, Action::Quit),
    ];

    MENU_ITEMS
        .iter()
        .find(|(start, width, _)| col >= *start && col < *start + *width)
        .map(|(_, _, action)| action.clone())
}

fn key_to_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
    use crossterm::event::{KeyCode, KeyModifiers};

    let mods = key.modifiers;
    match key.code {
        KeyCode::Char(c) => {
            if mods.contains(KeyModifiers::CONTROL) {
                let byte = (c.to_ascii_lowercase() as u8).wrapping_sub(b'a').wrapping_add(1);
                Some(vec![byte])
            } else {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                Some(s.as_bytes().to_vec())
            }
        }
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::F(n) => {
            let seq = match n {
                1 => "\x1bOP",
                2 => "\x1bOQ",
                3 => "\x1bOR",
                4 => "\x1bOS",
                5 => "\x1b[15~",
                6 => "\x1b[17~",
                7 => "\x1b[18~",
                8 => "\x1b[19~",
                9 => "\x1b[20~",
                10 => "\x1b[21~",
                11 => "\x1b[23~",
                12 => "\x1b[24~",
                _ => return None,
            };
            Some(seq.as_bytes().to_vec())
        }
        _ => None,
    }
}

fn terminal_content_rect(terminal: Rect) -> Rect {
    Rect {
        x: terminal.x + 1,
        y: terminal.y + 2,
        width: terminal.width.saturating_sub(2),
        height: terminal.height.saturating_sub(3),
    }
}

fn tab_program_for_new(command: &str) -> String {
    if let Some(path) = command.strip_prefix("diff ") {
        let escaped = path.replace('\'', "'\\''");
        return format!(
            "(git diff --color=always -- '{escaped}' 2>/dev/null; \
             if [ -f '{escaped}' ] && ! git ls-files --error-unmatch '{escaped}' >/dev/null 2>&1; then \
             git diff --no-index --color=always /dev/null '{escaped}' 2>/dev/null; fi) | less -R"
        );
    }
    match command {
        "shell" => {
            if Path::new("/bin/zsh").exists() {
                "/bin/zsh".to_string()
            } else {
                "/bin/sh".to_string()
            }
        }
        _ => command.to_string(),
    }
}

fn tab_program_for_resume(command: &str) -> String {
    if command.starts_with("diff ") {
        return tab_program_for_new(command);
    }
    match command {
        "shell" => tab_program_for_new(command),
        "opencode" => "opencode -c".to_string(),
        _ => command.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Project;
    use git2::Repository;
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

    #[test]
    fn menu_click_targets_match_expected_ranges() {
        assert_eq!(menu_action_at_column(1), Some(Action::NewWorkspace));
        assert_eq!(menu_action_at_column(8), Some(Action::NewTab));
        assert_eq!(menu_action_at_column(15), Some(Action::DeleteWorkspace));
        assert_eq!(menu_action_at_column(25), Some(Action::ShowHelp));
        assert_eq!(menu_action_at_column(33), Some(Action::Quit));
        assert_eq!(menu_action_at_column(40), None);
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
