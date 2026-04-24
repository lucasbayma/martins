//! Draw orchestration for the TUI frame.
//!
//! Extracted from src/app.rs as part of the architectural split (Phase 1).
//! Pure read-only composition of UI widgets; mutates only ListState widgets
//! that ratatui hands back to the next frame.

use crate::keys::InputMode;
use crate::ui::{layout, modal, picker, preview, sidebar_left, sidebar_right, terminal, theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;

pub fn draw(app: &mut crate::app::App, frame: &mut Frame) {
    let area = frame.area();
    app.last_frame_area = area;

    if layout::is_too_small(area) {
        let message = Paragraph::new("Terminal too small (min 80×24)")
            .style(ratatui::style::Style::default().fg(theme::ACCENT_TERRA));
        frame.render_widget(message, area);
        return;
    }

    let panes = layout::compute(area, &app.layout);
    app.last_panes = Some(panes.clone());

    if let Some(left_rect) = panes.left {
        let working_map = app.build_working_map();
        app.sidebar_items = sidebar_left::render(
            frame,
            left_rect,
            &app.global_state,
            app.active_project_idx,
            app.active_workspace_idx,
            &mut app.left_list,
            matches!(app.mode, InputMode::Normal),
            &working_map,
            &app.archived_expanded,
        );
    } else {
        app.sidebar_items.clear();
        app.left_list.select(None);
    }

    if let Some(right_rect) = panes.right {
        let base_branch = app
            .active_project()
            .map(|project| project.base_branch.clone())
            .unwrap_or_else(|| "main".to_string());
        sidebar_right::render(
            frame,
            right_rect,
            &app.modified_files,
            &mut app.right_list,
            false,
            &base_branch,
        );
    }

    let active_sessions = app.active_sessions();
    let active_tab = if active_sessions.is_empty() {
        0
    } else {
        app.active_tab.min(active_sessions.len() - 1)
    };

    let ws_info = app.active_workspace().map(|ws| terminal::WorkspaceInfo {
        name: ws.name.clone(),
        path: ws.worktree_path.to_string_lossy().to_string(),
    });

    terminal::render(
        frame,
        panes.terminal,
        &active_sessions,
        app.active_workspace().map(|workspace| workspace.tabs.as_slice()).unwrap_or(&[]),
        active_tab,
        app.mode,
        true,
        ws_info.as_ref(),
        app.selection.as_ref(),
    );

    crate::ui::draw::status_bar(app, frame, panes.status_bar);
    crate::ui::draw::menu_bar(frame, panes.menu_bar);
    modal::render(frame, &app.modal);

    if let Some(picker) = &app.picker {
        picker::render(frame, picker);
    }

    if let Some((path, lines)) = &app.preview_lines {
        preview::render_preview(frame, path, lines);
    }
}

pub fn status_bar(app: &crate::app::App, frame: &mut Frame, area: Rect) {
    let mode_label = match app.mode {
        InputMode::Normal => " NORMAL ",
        InputMode::Terminal => " TERMINAL ",
    };
    let mode_color = match app.mode {
        InputMode::Normal => theme::ACCENT_GOLD,
        InputMode::Terminal => theme::ACCENT_SAGE,
    };
    let project_name = app
        .active_project()
        .map(|project| project.name.as_str())
        .unwrap_or("no project");
    let workspace_name = app
        .active_workspace()
        .map(|workspace| workspace.name.as_str())
        .unwrap_or("-");

    let quit_label = " [Quit] ";
    let left_spans = vec![
        ratatui::text::Span::styled(
            mode_label,
            ratatui::style::Style::default()
                .fg(mode_color)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ),
        ratatui::text::Span::styled(
            format!("  {} > {}  ", project_name, workspace_name),
            ratatui::style::Style::default().fg(theme::TEXT_MUTED),
        ),
        ratatui::text::Span::styled(
            format!("{} changes", app.modified_files.len()),
            ratatui::style::Style::default().fg(theme::TEXT_DIM),
        ),
    ];

    let left_width: u16 = left_spans.iter().map(|s| s.content.len() as u16).sum();
    let gap = (area.width).saturating_sub(left_width + quit_label.len() as u16);

    let mut spans = left_spans;
    spans.push(ratatui::text::Span::styled(
        " ".repeat(gap as usize),
        ratatui::style::Style::default(),
    ));
    spans.push(ratatui::text::Span::styled(
        quit_label,
        ratatui::style::Style::default()
            .fg(theme::ACCENT_TERRA)
            .add_modifier(ratatui::style::Modifier::BOLD),
    ));

    let status = Paragraph::new(ratatui::text::Line::from(spans));
    frame.render_widget(status, area);
}

pub fn menu_bar(frame: &mut Frame, area: Rect) {
    fn item(key: &str, label: &str) -> Vec<ratatui::text::Span<'static>> {
        vec![
            ratatui::text::Span::styled(
                key.to_string(),
                ratatui::style::Style::default()
                    .fg(theme::ACCENT_GOLD)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            ratatui::text::Span::styled(
                format!(" {label}"),
                ratatui::style::Style::default().fg(theme::TEXT_MUTED),
            ),
        ]
    }

    let mut spans = vec![ratatui::text::Span::raw(" ")];
    for (idx, (key, label)) in [("n", "New"), ("t", "Tab"), ("d", "Delete"), ("?", "Help"), ("q", "Quit")]
        .into_iter()
        .enumerate()
    {
        if idx > 0 {
            spans.push(ratatui::text::Span::raw("  "));
        }
        spans.extend(item(key, label));
    }

    let menu = Paragraph::new(ratatui::text::Line::from(spans));
    frame.render_widget(menu, area);
}
