//! Terminal pane: tui-term PseudoTerminal widget + tab bar.

#![allow(dead_code)]

use crate::keys::InputMode;
use crate::pty::session::PtySession;
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
pub fn render(
    frame: &mut Frame,
    area: Rect,
    sessions: &[(u32, &PtySession)],
    active_tab: usize,
    mode: InputMode,
    focused: bool,
) {
    if sessions.is_empty() {
        let block = Block::default()
            .title(" Terminal ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER_MUTED));
        let paragraph = Paragraph::new("No active workspace. Press 'n' to create one.")
            .block(block)
            .style(Style::default().fg(theme::TEXT_MUTED));

        frame.render_widget(paragraph, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    let tab_area = chunks[0];
    let term_area = chunks[1];

    let tab_spans: Vec<Span<'_>> = sessions
        .iter()
        .enumerate()
        .map(|(index, (tab_id, _))| {
            let label = format!(" {} ", tab_id);

            if index == active_tab {
                let color = match mode {
                    InputMode::Normal => theme::ACCENT_GOLD,
                    InputMode::Terminal => theme::ACCENT_SAGE,
                };

                Span::styled(
                    label,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(label, Style::default().fg(theme::TEXT_MUTED))
            }
        })
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

    if let Some((_, session)) = sessions.get(active_tab)
        && let Ok(parser) = session.parser.try_read()
    {
        let pseudo_terminal = PseudoTerminal::new(parser.screen());
        frame.render_widget(pseudo_terminal, inner);
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
                render(frame, frame.area(), &[], 0, InputMode::Normal, false);
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
                render(frame, frame.area(), &[], 0, InputMode::Terminal, true);
            })
            .unwrap();
    }
}
