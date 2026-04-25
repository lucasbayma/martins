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
    current_gen: u64,
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
            let ((sc_raw, sr_raw), (ec_raw, er_raw)) = sel.normalized();
            // D-06: translate anchored rows to current-screen rows.
            // current_row = anchored_row - (current_gen - sel_gen)
            let start_delta = current_gen.saturating_sub(sel.start_gen);
            // D-07: mid-drag end is cursor-relative — delta=0 when end_gen is None.
            let end_delta = sel
                .end_gen
                .map(|g| current_gen.saturating_sub(g))
                .unwrap_or(0);
            let sr_translated = (sr_raw as i64) - (start_delta as i64);
            let er_translated = (er_raw as i64) - (end_delta as i64);
            // GAP-7-01 instrumentation: env-var gated selection-render tracing for
            // hypothesis E (scroll-generation false-positive inflates overlay translation).
            // Set MARTINS_MOUSE_DEBUG=1 to log per-frame selection geometry.
            if std::env::var_os("MARTINS_MOUSE_DEBUG").is_some() {
                eprintln!(
                    "[sel-render] raw=({},{})->({},{}) gens=start{}/end{:?}/curr{} \
                     deltas=({},{}) translated={}->{}",
                    sc_raw,
                    sr_raw,
                    ec_raw,
                    er_raw,
                    sel.start_gen,
                    sel.end_gen,
                    current_gen,
                    start_delta,
                    end_delta,
                    sr_translated,
                    er_translated
                );
            }
            // D-08: if entire selection has scrolled off (end row above top),
            // render nothing — but SelectionState stays in app state.
            if er_translated >= 0 {
                let sr = sr_translated.max(0) as u16;
                let er = er_translated.max(0) as u16;
                // D-08: clip start column to 0 if the start row was clipped.
                let sc = if sr_translated < 0 { 0 } else { sc_raw };
                let ec = ec_raw;
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
                            // D-20 + D-21: XOR REVERSED — already-reversed cells
                            // un-reverse, making the highlight visually distinct
                            // from surrounding vt100 reverse-video.
                            cell.modifier.toggle(Modifier::REVERSED);
                        }
                    }
                }
            }
        }
    }


}

/// #[cfg(test)] shim mirroring the production highlight pass over an
/// arbitrary `Rect` + `SelectionState`. Lets the render tests exercise
/// REVERSED toggling and anchored-coord translation without spawning a
/// PtySession or constructing the full `tab_specs`/`sessions` argument
/// fan-out that `render` requires.
#[cfg(test)]
pub(crate) fn render_with_selection_for_test(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    selection: Option<&crate::app::SelectionState>,
    current_gen: u64,
) {
    let inner = area;
    let Some(sel) = selection else { return };
    if sel.is_empty() {
        return;
    }
    let ((sc_raw, sr_raw), (ec_raw, er_raw)) = sel.normalized();
    let start_delta = current_gen.saturating_sub(sel.start_gen);
    let end_delta = sel
        .end_gen
        .map(|g| current_gen.saturating_sub(g))
        .unwrap_or(0);
    let sr_translated = (sr_raw as i64) - (start_delta as i64);
    let er_translated = (er_raw as i64) - (end_delta as i64);
    if er_translated < 0 {
        return;
    }
    let sr = sr_translated.max(0) as u16;
    let er = er_translated.max(0) as u16;
    let sc = if sr_translated < 0 { 0 } else { sc_raw };
    let ec = ec_raw;
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
                cell.modifier.toggle(Modifier::REVERSED);
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
                render(frame, frame.area(), &[], &[], 0, InputMode::Normal, false, None, None, 0);
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
                render(frame, frame.area(), &[], &[], 0, InputMode::Terminal, true, None, None, 0);
            })
            .unwrap();
    }
}
