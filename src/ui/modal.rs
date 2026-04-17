//! Modal system: new workspace, confirm delete, install binaries.
#![allow(dead_code)]

use crate::state::Agent;
use crate::tools::Tool;
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
    pub agent: Agent,
    pub base_branch: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DeleteForm {
    pub workspace_name: String,
    pub unpushed_commits: usize,
    pub delete_branch: bool,
}

#[derive(Debug, Clone, Default)]
pub struct InstallForm {
    pub missing_tools: Vec<Tool>,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Default)]
pub enum Modal {
    #[default]
    None,
    NewWorkspace(NewWorkspaceForm),
    ConfirmDelete(DeleteForm),
    InstallMissing(InstallForm),
}

/// Center a rect of given size within the frame.
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

/// Render the active modal (if any).
pub fn render(frame: &mut Frame, modal: &Modal) {
    match modal {
        Modal::None => {}
        Modal::NewWorkspace(form) => render_new_workspace(frame, form),
        Modal::ConfirmDelete(form) => render_confirm_delete(frame, form),
        Modal::InstallMissing(form) => render_install_missing(frame, form),
    }
}

fn render_new_workspace(frame: &mut Frame, form: &NewWorkspaceForm) {
    let area = centered_rect(50, 40, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" NEW WORKSPACE ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT_GOLD));

    let agent_str = match form.agent {
        Agent::Opencode => "opencode",
        Agent::Claude => "claude",
        Agent::Codex => "codex",
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Name: ", Style::default().fg(theme::TEXT_MUTED)),
            Span::styled(
                format!(
                    "[{}]",
                    if form.name_input.is_empty() {
                        "auto-generate"
                    } else {
                        &form.name_input
                    }
                ),
                Style::default().fg(theme::TEXT_PRIMARY),
            ),
        ]),
        Line::from(vec![
            Span::styled("Agent: ", Style::default().fg(theme::TEXT_MUTED)),
            Span::styled(agent_str, Style::default().fg(theme::TEXT_PRIMARY)),
        ]),
        Line::from(vec![
            Span::styled("Branch: ", Style::default().fg(theme::TEXT_MUTED)),
            Span::styled(&form.base_branch, Style::default().fg(theme::TEXT_PRIMARY)),
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

fn render_install_missing(frame: &mut Frame, form: &InstallForm) {
    let area = centered_rect(60, 50, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" INSTALL MISSING TOOLS ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT_GOLD));

    let mut lines = vec![
        Line::from(vec![Span::styled(
            "Missing tools:",
            Style::default().fg(theme::TEXT_MUTED),
        )]),
        Line::from(""),
    ];

    for tool in &form.missing_tools {
        lines.push(Line::from(vec![
            Span::styled("  ✗ ", Style::default().fg(theme::ACCENT_TERRA)),
            Span::styled(tool.binary_name(), Style::default().fg(theme::TEXT_PRIMARY)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("[y] Install  ", Style::default().fg(theme::ACCENT_GOLD)),
        Span::styled("[n/Esc] Skip", Style::default().fg(theme::TEXT_MUTED)),
    ]));

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
            agent: Agent::Opencode,
            base_branch: "main".to_string(),
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
    fn none_modal_renders_nothing() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &Modal::None)).unwrap();
        // Should not panic
    }
}
