//! Modal system: forms and confirmations.
#![allow(dead_code)]

use crate::state::Agent;
use crate::ui::theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

#[derive(Debug, Clone, Default)]
pub struct NewWorkspaceForm {
    pub name_input: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FolderEntry {
    pub name: String,
    pub path: std::path::PathBuf,
    pub is_git_repo: bool,
}

#[derive(Debug, Clone)]
pub struct AddProjectForm {
    pub current_dir: std::path::PathBuf,
    pub entries: Vec<FolderEntry>,
    pub selected: usize,
    pub error: Option<String>,
}

impl Default for AddProjectForm {
    fn default() -> Self {
        let home = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/"));
        let mut form = Self {
            current_dir: home,
            entries: Vec::new(),
            selected: 0,
            error: None,
        };
        form.refresh();
        form
    }
}

impl AddProjectForm {
    pub fn refresh(&mut self) {
        self.entries.clear();
        self.selected = 0;
        self.error = None;

        let Ok(read_dir) = std::fs::read_dir(&self.current_dir) else {
            self.error = Some("cannot read directory".to_string());
            return;
        };

        let mut dirs: Vec<FolderEntry> = read_dir
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .map(|e| {
                let path = e.path();
                let is_git_repo = path.join(".git").exists();
                FolderEntry {
                    name: e.file_name().to_string_lossy().to_string(),
                    path,
                    is_git_repo,
                }
            })
            .collect();

        dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.entries = dirs;
    }

    pub fn navigate_into(&mut self, idx: usize) {
        if let Some(entry) = self.entries.get(idx) {
            self.current_dir = entry.path.clone();
            self.refresh();
        }
    }

    pub fn navigate_up(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            self.current_dir = parent.to_path_buf();
            self.refresh();
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let current = self.selected as isize;
        let next = (current + delta).clamp(0, self.entries.len() as isize - 1) as usize;
        self.selected = next;
    }

    pub fn selected_entry(&self) -> Option<&FolderEntry> {
        self.entries.get(self.selected)
    }
}

#[derive(Debug, Clone, Default)]
pub struct DeleteForm {
    pub workspace_name: String,
    pub unpushed_commits: usize,
    pub delete_branch: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RemoveProjectForm {
    pub project_name: String,
    pub project_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct ArchiveForm {
    pub workspace_name: String,
}

#[derive(Debug, Clone, Default)]
pub enum Modal {
    #[default]
    None,
    NewWorkspace(NewWorkspaceForm),
    ConfirmQuit,
    ConfirmDelete(DeleteForm),
    ConfirmArchive(ArchiveForm),
    ConfirmRemoveProject(RemoveProjectForm),
    AddProject(AddProjectForm),
    Help,
    Loading(String),
}

pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub fn render(frame: &mut Frame, modal: &Modal) {
    match modal {
        Modal::None => {}
        Modal::NewWorkspace(form) => render_new_workspace(frame, form),
        Modal::ConfirmQuit => render_confirm_quit(frame),
        Modal::ConfirmDelete(form) => render_confirm_delete(frame, form),
        Modal::ConfirmArchive(form) => render_confirm_archive(frame, form),
        Modal::ConfirmRemoveProject(form) => render_confirm_remove_project(frame, form),
        Modal::AddProject(form) => render_add_project(frame, form),
        Modal::Help => render_help(frame),
        Modal::Loading(msg) => render_loading(frame, msg),
    }
}

fn render_loading(frame: &mut Frame, message: &str) {
    let area = centered_rect(40, 20, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT_GOLD));

    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("  {message}"),
            Style::default()
                .fg(theme::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        )]),
    ];

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, area);
}

fn agent_label(agent: &Agent) -> &'static str {
    match agent {
        Agent::Opencode => "opencode",
        Agent::Claude => "claude",
        Agent::Codex => "codex",
    }
}

fn render_new_workspace(frame: &mut Frame, form: &NewWorkspaceForm) {
    let area = centered_rect(50, 30, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" NEW WORKSPACE ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT_GOLD));

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Name: ", Style::default().fg(theme::TEXT_MUTED)),
            Span::styled(
                if form.name_input.is_empty() {
                    "[auto-generate]".to_string()
                } else {
                    form.name_input.clone()
                },
                Style::default().fg(theme::TEXT_PRIMARY),
            ),
        ]),
        Line::from(""),
    ];

    if let Some(err) = &form.error {
        lines.push(Line::from(vec![Span::styled(
            err.clone(),
            Style::default()
                .fg(theme::ACCENT_TERRA)
                .add_modifier(Modifier::BOLD),
        )]));
    }

    lines.push(Line::from(vec![
        Span::styled("[↵] Create  ", Style::default().fg(theme::ACCENT_GOLD)),
        Span::styled("[Esc] Cancel", Style::default().fg(theme::TEXT_MUTED)),
    ]));

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, area);
}

fn render_add_project(frame: &mut Frame, form: &AddProjectForm) {
    let area = centered_rect(60, 70, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" OPEN PROJECT ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT_GOLD));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 4 || inner.width < 10 {
        return;
    }

    let path_display = form.current_dir.to_string_lossy();
    let max_path_w = inner.width.saturating_sub(2) as usize;
    let truncated_path = if path_display.len() > max_path_w {
        format!("...{}", &path_display[path_display.len() - max_path_w + 3..])
    } else {
        path_display.to_string()
    };

    let header = Line::from(vec![Span::styled(
        truncated_path,
        Style::default()
            .fg(theme::ACCENT_GOLD)
            .add_modifier(Modifier::BOLD),
    )]);
    frame.render_widget(Paragraph::new(header), Rect { y: inner.y, height: 1, ..inner });

    let separator = Line::from(vec![Span::styled(
        "─".repeat(inner.width as usize),
        Style::default().fg(theme::BORDER_MUTED),
    )]);
    frame.render_widget(Paragraph::new(separator), Rect { y: inner.y + 1, height: 1, ..inner });

    let footer_height: u16 = if form.error.is_some() { 3 } else { 2 };
    let list_height = inner.height.saturating_sub(2 + footer_height) as usize;
    let list_y = inner.y + 2;

    let scroll_offset = if form.selected >= list_height {
        form.selected - list_height + 1
    } else {
        0
    };

    let parent_line = Line::from(vec![
        Span::styled("  ↑ ", Style::default().fg(theme::TEXT_MUTED)),
        Span::styled("..", Style::default().fg(theme::TEXT_SECONDARY)),
    ]);
    let is_parent_selected = form.entries.is_empty() && form.selected == 0;
    let parent_style = if is_parent_selected {
        Style::default().bg(theme::BG_SELECTED)
    } else {
        Style::default()
    };

    if scroll_offset == 0 {
        frame.render_widget(
            Paragraph::new(parent_line).style(parent_style),
            Rect { x: inner.x, y: list_y, width: inner.width, height: 1 },
        );
    }

    let entry_start_row = if scroll_offset == 0 { 1usize } else { 0 };
    let visible_slots = list_height.saturating_sub(if scroll_offset == 0 { 1 } else { 0 });

    for (vis_idx, entry_idx) in (scroll_offset..form.entries.len())
        .take(visible_slots)
        .enumerate()
    {
        let entry = &form.entries[entry_idx];
        let is_selected = entry_idx == form.selected;
        let row = list_y + (entry_start_row + vis_idx) as u16;

        if row >= list_y + list_height as u16 {
            break;
        }

        let (icon, icon_style) = if entry.is_git_repo {
            ("● ", Style::default().fg(theme::ACCENT_SAGE))
        } else {
            ("  ", Style::default().fg(theme::TEXT_DIM))
        };

        let name_style = if entry.is_git_repo {
            Style::default().fg(theme::TEXT_PRIMARY)
        } else {
            Style::default().fg(theme::TEXT_SECONDARY)
        };

        let mut line_spans = vec![
            Span::styled("  ", Style::default()),
            Span::styled(icon, icon_style),
            Span::styled(format!("{}/", entry.name), name_style),
        ];

        if entry.is_git_repo {
            line_spans.push(Span::styled(" git", Style::default().fg(theme::TEXT_DIM)));
        }

        let bg = if is_selected {
            Style::default().bg(theme::BG_SELECTED)
        } else {
            Style::default()
        };

        frame.render_widget(
            Paragraph::new(Line::from(line_spans)).style(bg),
            Rect { x: inner.x, y: row, width: inner.width, height: 1 },
        );
    }

    let footer_y = inner.y + inner.height - footer_height;

    if let Some(err) = &form.error {
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                err.clone(),
                Style::default().fg(theme::ACCENT_TERRA).add_modifier(Modifier::BOLD),
            )])),
            Rect { x: inner.x, y: footer_y, width: inner.width, height: 1 },
        );
    }

    let hint_y = inner.y + inner.height - 1;
    let hints = Line::from(vec![
        Span::styled("[↵] Open/Enter  ", Style::default().fg(theme::ACCENT_GOLD)),
        Span::styled("[⌫] Back  ", Style::default().fg(theme::TEXT_MUTED)),
        Span::styled("[Esc] Cancel", Style::default().fg(theme::TEXT_MUTED)),
    ]);
    frame.render_widget(Paragraph::new(hints), Rect { x: inner.x, y: hint_y, width: inner.width, height: 1 });
}

fn render_confirm_delete(frame: &mut Frame, form: &DeleteForm) {
    let area = centered_rect(50, 40, frame.area());
    frame.render_widget(Clear, area);

    let border_style = if form.unpushed_commits > 0 {
        Style::default().fg(theme::ACCENT_TERRA)
    } else {
        Style::default().fg(theme::ACCENT_GOLD)
    };

    let block = Block::default()
        .title(" CONFIRM DELETE ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(border_style);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Delete workspace: ", Style::default().fg(theme::TEXT_MUTED)),
            Span::styled(
                &form.workspace_name,
                Style::default()
                    .fg(theme::TEXT_PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
    ];

    if form.unpushed_commits > 0 {
        lines.push(Line::from(vec![Span::styled(
            format!(
                "⚠ WARNING: {} unpushed commits on this branch will be permanently lost.",
                form.unpushed_commits
            ),
            Style::default()
                .fg(theme::ACCENT_TERRA)
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(vec![
        Span::styled("[↵] Delete  ", Style::default().fg(theme::ACCENT_TERRA)),
        Span::styled("[Esc] Cancel", Style::default().fg(theme::TEXT_MUTED)),
    ]));

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, area);
}

fn render_confirm_archive(frame: &mut Frame, form: &ArchiveForm) {
    let area = centered_rect(50, 30, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" CONFIRM ARCHIVE ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT_GOLD));

    let lines = vec![
        Line::from(vec![
            Span::styled("Archive workspace: ", Style::default().fg(theme::TEXT_MUTED)),
            Span::styled(
                &form.workspace_name,
                Style::default()
                    .fg(theme::TEXT_PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Enter] Archive  ", Style::default().fg(theme::ACCENT_GOLD)),
            Span::styled("[Esc] Cancel", Style::default().fg(theme::TEXT_MUTED)),
        ]),
    ];

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, area);
}

fn render_confirm_quit(frame: &mut Frame) {
    let area = centered_rect(40, 30, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" CONFIRM QUIT ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT_GOLD));

    let lines = vec![
        Line::from(vec![Span::styled(
            "Are you sure you want to quit?",
            Style::default().fg(theme::TEXT_PRIMARY),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Enter] Quit  ", Style::default().fg(theme::ACCENT_TERRA)),
            Span::styled("[Esc] Cancel", Style::default().fg(theme::TEXT_MUTED)),
        ]),
    ];

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, area);
}

fn render_confirm_remove_project(frame: &mut Frame, form: &RemoveProjectForm) {
    let area = centered_rect(50, 35, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" REMOVE PROJECT ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT_GOLD));

    let lines = vec![
        Line::from(vec![
            Span::styled("Remove project: ", Style::default().fg(theme::TEXT_MUTED)),
            Span::styled(
                &form.project_name,
                Style::default()
                    .fg(theme::TEXT_PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Enter] Remove  ", Style::default().fg(theme::ACCENT_TERRA)),
            Span::styled("[Esc] Cancel", Style::default().fg(theme::TEXT_MUTED)),
        ]),
    ];

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, area);
}

fn render_help(frame: &mut Frame) {
    fn section(title: &str) -> Line<'static> {
        Line::from(vec![Span::styled(
            title.to_string(),
            Style::default()
                .fg(theme::ACCENT_GOLD)
                .add_modifier(Modifier::BOLD),
        )])
    }

    fn shortcut(keys: &str, description: &str) -> Line<'static> {
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{keys:<12}"),
                Style::default().fg(theme::TEXT_PRIMARY),
            ),
            Span::styled(description.to_string(), Style::default().fg(theme::TEXT_MUTED)),
        ])
    }

    let area = centered_rect(70, 80, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" SHORTCUTS ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT_GOLD));

    let lines = vec![
        Line::from(""),
        section("Navigation"),
        shortcut("j/k  ↑/↓", "Move selection"),
        shortcut("1-9", "Switch tab (Normal mode)"),
            shortcut("F1-F9", "Switch tab (any mode)"),
        shortcut("Ctrl+B", "Switch to sidebar"),
        Line::from(""),
        section("Workspace"),
        shortcut("n", "New workspace"),
        shortcut("d", "Delete workspace"),
        shortcut("a", "Archive workspace"),
        Line::from(""),
        section("Tabs"),
        shortcut("t", "New tab (agent/shell)"),
        shortcut("T", "Close current tab"),
        Line::from(""),
        section("Project"),
        shortcut("+", "Add Project (via sidebar)"),
        Line::from(""),
        section("View"),
        shortcut("[", "Toggle left sidebar (Normal mode)"),
        shortcut("]", "Toggle right sidebar (Normal mode)"),
        shortcut("/", "Fuzzy search workspaces"),
        shortcut("p", "Preview file"),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Esc]", Style::default().fg(theme::TEXT_PRIMARY)),
            Span::styled(" Close", Style::default().fg(theme::TEXT_MUTED)),
        ]),
    ];

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn new_workspace_modal_renders() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let modal = Modal::NewWorkspace(NewWorkspaceForm {
            name_input: "caetano".to_string(),
            error: None,
        });
        terminal.draw(|f| render(f, &modal)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(
            content.contains("NEW WORKSPACE")
                || content.contains("caetano")
                || content.contains("opencode")
        );
    }

    #[test]
    fn add_project_modal_renders_error() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut form = AddProjectForm {
            current_dir: std::path::PathBuf::from("/tmp"),
            error: Some("not a git repository".to_string()),
            ..Default::default()
        };
        form.refresh();
        let modal = Modal::AddProject(form);
        terminal.draw(|f| render(f, &modal)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("OPEN PROJECT") || content.contains("not a git repository"));
    }

    #[test]
    fn delete_modal_with_unpushed_warning() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let modal = Modal::ConfirmDelete(DeleteForm {
            workspace_name: "gil".to_string(),
            unpushed_commits: 5,
            delete_branch: false,
        });
        terminal.draw(|f| render(f, &modal)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("WARNING") || content.contains("5") || content.contains("gil"));
    }

    #[test]
    fn confirm_quit_modal_renders() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &Modal::ConfirmQuit)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("CONFIRM QUIT"));
        assert!(content.contains("Are you sure you want to quit?"));
    }

    #[test]
    fn remove_project_modal_renders() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let modal = Modal::ConfirmRemoveProject(RemoveProjectForm {
            project_name: "martins".to_string(),
            project_id: "abc123".to_string(),
        });
        terminal.draw(|f| render(f, &modal)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("REMOVE PROJECT"));
        assert!(content.contains("martins"));
    }

    #[test]
    fn none_modal_renders_nothing() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &Modal::None)).unwrap();
    }

    #[test]
    fn help_modal_renders_shortcuts() {
        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &Modal::Help)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("SHORTCUTS"));
        assert!(content.contains("Navigation"));
        assert!(content.contains("Delete workspace"));
    }
}
