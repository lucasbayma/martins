//! Terminal pane: tui-term PseudoTerminal widget + tab bar.

#![allow(dead_code)]

use crate::app::SelectionState;

pub fn tab_label(command: &str) -> String {
    if let Some(path) = command.strip_prefix("diff ") {
        let filename = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);
        format!("diff:{filename}")
    } else {
        command.to_string()
    }
}
use crate::keys::InputMode;
use crate::pty::session::PtySession;
use crate::state::TabSpec;
use crate::ui::theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use tui_term::widget::PseudoTerminal;

/// Render the terminal pane with tab bar.
pub struct WorkspaceInfo {
    pub name: String,
    pub path: String,
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    sessions: &[(u32, &PtySession)],
    tab_specs: &[TabSpec],
    active_tab: usize,
    mode: InputMode,
    focused: bool,
    workspace_info: Option<&WorkspaceInfo>,
    selection: Option<&SelectionState>,
) {
    if tab_specs.is_empty() {
        let block = Block::default()
            .title(" Terminal ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER_MUTED));

        let lines = if let Some(info) = workspace_info {
            vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Workspace: ", Style::default().fg(theme::TEXT_MUTED)),
                    Span::styled(
                        &info.name,
                        Style::default().fg(theme::TEXT_PRIMARY).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  Path:      ", Style::default().fg(theme::TEXT_MUTED)),
                    Span::styled(&info.path, Style::default().fg(theme::TEXT_SECONDARY)),
                ]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "  Press 't' or click [+] to open a tab",
                    Style::default().fg(theme::ACCENT_GOLD),
                )]),
            ]
        } else {
            vec![Line::from(vec![Span::styled(
                "  No active workspace. Press 'n' to create one.",
                Style::default().fg(theme::TEXT_MUTED),
            )])]
        };

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    let tab_area = chunks[0];
    let term_area = chunks[1];

    let tab_spans: Vec<Span<'_>> = tab_specs
        .iter()
        .enumerate()
        .flat_map(|(index, tab_spec)| {
            let color = if index == active_tab {
                match mode {
                    InputMode::Normal => theme::ACCENT_GOLD,
                    InputMode::Terminal => theme::ACCENT_SAGE,
                }
            } else {
                theme::TEXT_MUTED
            };
            let base_style = if index == active_tab {
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color)
            };

            let label = tab_label(&tab_spec.command);
            [
                Span::styled(format!(" {label}"), base_style),
                Span::styled(" ✕ ", Style::default().fg(theme::TEXT_DIM)),
            ]
        })
        .chain(std::iter::once(Span::styled(
            " [+] ",
            Style::default().fg(theme::ACCENT_GOLD),
        )))
        .collect();

    let tab_line =
        Paragraph::new(Line::from(tab_spans)).style(Style::default().bg(theme::BG_SURFACE));
    frame.render_widget(tab_line, tab_area);

    let border_color = if focused {
        match mode {
            InputMode::Normal => theme::ACCENT_GOLD,
            InputMode::Terminal => theme::ACCENT_SAGE,
        }
    } else {
        theme::BORDER_MUTED
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(term_area);

    frame.render_widget(block, term_area);

    if let Some((_, session)) = sessions.get(active_tab) {
        let parser_guard = session.parser.try_read().or_else(|_| {
            std::thread::sleep(std::time::Duration::from_micros(500));
            session.parser.try_read()
        });
        if let Ok(parser) = parser_guard {
            let pseudo_terminal = PseudoTerminal::new(parser.screen());
            frame.render_widget(pseudo_terminal, inner);
        }
    }

    if let Some(sel) = selection {
        if !sel.is_empty() {
            let ((sc, sr), (ec, er)) = sel.normalized();
            let buf = frame.buffer_mut();
            for row in sr..=er {
                if row >= inner.height {
                    break;
                }
                let c_start = if row == sr { sc } else { 0 };
                let c_end = if row == er { ec } else { inner.width.saturating_sub(1) };
                for col in c_start..=c_end {
                    if col >= inner.width {
                        break;
                    }
                    if let Some(cell) = buf.cell_mut((inner.x + col, inner.y + row)) {
                        cell.set_bg(theme::ACCENT_GOLD);
                        cell.set_fg(theme::BG_SURFACE);
                    }
                }
            }
        }
    }


}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn renders_empty_pane() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &[], &[], 0, InputMode::Normal, false, None, None);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        assert!(content.contains("No active workspace") || content.contains("Terminal"));
    }

    #[test]
    fn mode_border_color_changes() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &[], &[], 0, InputMode::Terminal, true, None, None);
            })
            .unwrap();
    }
}
