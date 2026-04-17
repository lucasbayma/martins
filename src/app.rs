//! Application state and main event loop.

use crate::git::diff;
use crate::git::repo;
use crate::keys::{Action, EscapeDetector, InputMode, Keymap};
use crate::pty::manager::PtyManager;
use crate::state::{Agent, AppState};
use crate::ui::layout::{self, LayoutState};
use crate::ui::modal::{self, Modal, NewWorkspaceForm};
use crate::ui::picker::{self, Picker, PickerKind, PickerOutcome};
use crate::ui::preview;
use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind};
use futures::StreamExt;
use ratatui::{
    DefaultTerminal, Frame,
    layout::Rect,
    widgets::{ListState, Paragraph},
};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::interval;

pub struct App {
    pub repo_root: PathBuf,
    pub repo_name: String,
    pub state: AppState,
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
    pub escape_detector: EscapeDetector,
    pub keymap: Keymap,
    pub should_quit: bool,
    pub base_branch: String,
}

impl App {
    pub async fn new(repo_root: PathBuf) -> Result<Self> {
        let repo_name = repo_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("repo")
            .to_string();

        let state = AppState::load(&repo_root).unwrap_or_default();
        let active_workspace_idx = state.active().next().map(|_| 0);

        let base_branch = repo::current_branch_async(repo_root.clone())
            .await
            .unwrap_or_else(|_| "main".to_string());

        let mut left_list = ListState::default();
        if active_workspace_idx.is_some() {
            left_list.select(Some(1));
        }

        Ok(Self {
            repo_root,
            repo_name,
            state,
            layout: LayoutState::new(),
            mode: InputMode::Normal,
            modal: Modal::None,
            picker: None,
            preview_lines: None,
            left_list,
            right_list: ListState::default(),
            pty_manager: PtyManager::new(),
            active_workspace_idx,
            active_tab: 0,
            modified_files: Vec::new(),
            escape_detector: EscapeDetector::new(),
            keymap: Keymap::default_keymap(),
            should_quit: false,
            base_branch,
        })
    }

    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let mut events = EventStream::new();
        let mut refresh_tick = interval(Duration::from_secs(5));

        self.refresh_diff().await;

        loop {
            terminal.draw(|frame| self.draw(frame))?;

            if self.should_quit {
                break;
            }

            tokio::select! {
                Some(Ok(event)) = events.next() => {
                    self.handle_event(event).await;
                }
                _ = refresh_tick.tick() => {
                    self.refresh_diff().await;
                }
            }
        }

        Ok(())
    }

    async fn refresh_diff(&mut self) {
        if let Ok(files) =
            diff::modified_files(self.repo_root.clone(), self.base_branch.clone()).await
        {
            self.modified_files = files;

            if self.modified_files.is_empty() {
                self.right_list.select(None);
            } else if self.right_list.selected().is_none() {
                self.right_list.select(Some(0));
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();

        if layout::is_too_small(area) {
            let message = Paragraph::new("Terminal too small (min 80×24)")
                .style(ratatui::style::Style::default().fg(crate::ui::theme::ACCENT_TERRA));
            frame.render_widget(message, area);
            return;
        }

        let panes = layout::compute(area, &self.layout);

        if let Some(left_rect) = panes.left {
            crate::ui::sidebar_left::render(
                frame,
                left_rect,
                &self.state,
                &mut self.left_list,
                matches!(self.mode, InputMode::Normal),
                &self.repo_name,
            );
        }

        if let Some(right_rect) = panes.right {
            crate::ui::sidebar_right::render(
                frame,
                right_rect,
                &self.modified_files,
                &mut self.right_list,
                false,
                &self.base_branch,
            );
        }

        let active_sessions = self
            .active_workspace_idx
            .and_then(|index| self.state.active().nth(index))
            .map(|workspace| {
                workspace
                    .tabs
                    .iter()
                    .filter_map(|tab| {
                        self.pty_manager
                            .get_session(&workspace.name, tab.id)
                            .map(|session| (tab.id, session))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let active_tab = if active_sessions.is_empty() {
            0
        } else {
            self.active_tab.min(active_sessions.len() - 1)
        };

        crate::ui::terminal::render(
            frame,
            panes.terminal,
            &active_sessions,
            active_tab,
            self.mode,
            true,
        );

        self.draw_status_bar(frame, panes.status_bar);
        modal::render(frame, &self.modal);

        if let Some(picker) = &self.picker {
            picker::render(frame, picker);
        }

        if let Some((path, lines)) = &self.preview_lines {
            preview::render_preview(frame, path, lines);
        }
    }

    fn draw_status_bar(&self, frame: &mut Frame, area: Rect) {
        let mode_label = match self.mode {
            InputMode::Normal => " NORMAL ",
            InputMode::Terminal => " TERMINAL ",
        };
        let mode_color = match self.mode {
            InputMode::Normal => crate::ui::theme::ACCENT_GOLD,
            InputMode::Terminal => crate::ui::theme::ACCENT_SAGE,
        };

        let status = Paragraph::new(ratatui::text::Line::from(vec![
            ratatui::text::Span::styled(
                mode_label,
                ratatui::style::Style::default()
                    .fg(mode_color)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            ratatui::text::Span::styled(
                format!("  {}  ", self.repo_name),
                ratatui::style::Style::default().fg(crate::ui::theme::TEXT_MUTED),
            ),
            ratatui::text::Span::styled(
                format!("  {} changes", self.modified_files.len()),
                ratatui::style::Style::default().fg(crate::ui::theme::TEXT_DIM),
            ),
        ]));

        frame.render_widget(status, area);
    }

    async fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key).await,
            Event::Resize(_, _) => {}
            _ => {}
        }
    }

    async fn handle_key(&mut self, key: KeyEvent) {
        if let Some(picker) = &mut self.picker {
            let kind = picker.kind.clone();
            match picker.on_key(key) {
                PickerOutcome::Cancelled => self.picker = None,
                PickerOutcome::Selected(index) => {
                    self.picker = None;
                    if kind == PickerKind::Workspaces {
                        self.select_active_workspace(index);
                    }
                }
                PickerOutcome::Continue => {}
            }
            return;
        }

        if !matches!(self.modal, Modal::None) {
            match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter => self.modal = Modal::None,
                _ => {}
            }
            return;
        }

        if self.mode == InputMode::Terminal {
            if let Some(Action::ExitTerminalMode) =
                crate::keys::resolve_terminal(&mut self.escape_detector, &key)
            {
                self.mode = InputMode::Normal;
            }
            return;
        }

        if let Some(action) = self.keymap.resolve_normal(&key).cloned() {
            self.dispatch_action(action).await;
        }
    }

    async fn dispatch_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::NextItem => {
                let len = self.state.active().count();
                if len > 0 {
                    let current = self.active_workspace_idx.unwrap_or(0);
                    self.select_active_workspace((current + 1).min(len - 1));
                }
            }
            Action::PrevItem => {
                if let Some(current) = self.active_workspace_idx {
                    self.select_active_workspace(current.saturating_sub(1));
                }
            }
            Action::EnterTerminalMode => self.mode = InputMode::Terminal,
            Action::ToggleSidebarLeft => self.layout.toggle_left(),
            Action::ToggleSidebarRight => self.layout.toggle_right(),
            Action::OpenFuzzy => {
                let items: Vec<String> = self
                    .state
                    .active()
                    .map(|workspace| workspace.name.clone())
                    .collect();
                self.picker = Some(Picker::new(items, PickerKind::Workspaces));
            }
            Action::NewWorkspace => {
                self.modal = Modal::NewWorkspace(NewWorkspaceForm {
                    name_input: String::new(),
                    agent: Agent::Opencode,
                    base_branch: self.base_branch.clone(),
                    error: None,
                });
            }
            Action::ArchiveWorkspace => {
                if let Some(index) = self.active_workspace_idx {
                    let active: Vec<_> = self
                        .state
                        .active()
                        .map(|workspace| workspace.name.clone())
                        .collect();

                    if let Some(name) = active.get(index) {
                        self.state.archive(name);
                        self.save_state();

                        let remaining = self.state.active().count();
                        if remaining == 0 {
                            self.active_workspace_idx = None;
                            self.left_list.select(None);
                        } else {
                            self.select_active_workspace(index.min(remaining - 1));
                        }
                    }
                }
            }
            Action::Preview => {
                if let Some(index) = self.right_list.selected()
                    && let Some(entry) = self.modified_files.get(index)
                {
                    let full_path = self.repo_root.join(&entry.path);
                    let lines = preview::bat_preview(&full_path, 200);
                    self.preview_lines = Some((full_path, lines));
                }
            }
            _ => {}
        }
    }

    fn select_active_workspace(&mut self, index: usize) {
        self.active_workspace_idx = Some(index);
        self.left_list.select(Some(index + 1));
    }

    fn save_state(&self) {
        if let Err(error) = self.state.save(&self.repo_root) {
            tracing::error!("failed to save state: {error}");
        }
    }
}
