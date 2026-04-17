//! Left sidebar: workspace list with status icons.

use crate::state::{AppState, WorkspaceStatus};
use crate::ui::theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};

/// Render the left sidebar.
#[allow(dead_code)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    list_state: &mut ListState,
    focused: bool,
    repo_name: &str,
) {
    let border_style = if focused {
        Style::default().fg(theme::ACCENT_GOLD)
    } else {
        Style::default().fg(theme::BORDER_MUTED)
    };

    let block = Block::default()
        .title(format!(" {} ", repo_name))
        .borders(Borders::ALL)
        .border_style(border_style);

    let _inner = block.inner(area);

    // Build list items
    let mut items: Vec<ListItem> = Vec::new();

    // Section header
    items.push(ListItem::new(Line::from(vec![Span::styled(
        "WORKSPACES",
        Style::default()
            .fg(theme::TEXT_MUTED)
            .add_modifier(Modifier::BOLD),
    )])));

    // Active/inactive/exited workspaces
    let active_workspaces: Vec<_> = state.active().collect();

    if active_workspaces.is_empty() && state.archived().count() == 0 {
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "No workspaces. Press 'n' to create one.",
            Style::default().fg(theme::TEXT_MUTED),
        )])));
    } else {
        for ws in &active_workspaces {
            let (icon, icon_style) = match &ws.status {
                WorkspaceStatus::Active => ("●", Style::default().fg(theme::ACCENT_SAGE)),
                WorkspaceStatus::Inactive => ("○", Style::default().fg(theme::TEXT_MUTED)),
                WorkspaceStatus::Exited(code) => {
                    let _ = code;
                    ("◐", Style::default().fg(theme::ACCENT_TERRA))
                }
                WorkspaceStatus::Archived => ("⋯", Style::default().fg(theme::TEXT_DIM)),
            };

            let name_style = Style::default().fg(theme::TEXT_PRIMARY);
            items.push(ListItem::new(Line::from(vec![
                Span::styled(icon, icon_style),
                Span::raw(" "),
                Span::styled(ws.name.clone(), name_style),
            ])));
        }

        // Archived section
        let archived: Vec<_> = state.archived().collect();
        if !archived.is_empty() {
            items.push(ListItem::new(Line::from(vec![Span::styled(
                format!("▼ ARCHIVED  {}", archived.len()),
                Style::default().fg(theme::TEXT_DIM),
            )])));
            for ws in &archived {
                items.push(ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled("⋯ ", Style::default().fg(theme::TEXT_DIM)),
                    Span::styled(ws.name.clone(), Style::default().fg(theme::TEXT_MUTED)),
                ])));
            }
        }
    }

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(theme::BG_SELECTED)
            .add_modifier(Modifier::BOLD),
    );

    frame.render_stateful_widget(list, area, list_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Agent, AppState, Workspace, WorkspaceStatus};
    use ratatui::{Terminal, backend::TestBackend};
    use std::path::PathBuf;

    fn make_workspace(name: &str, status: WorkspaceStatus) -> Workspace {
        Workspace {
            name: name.to_string(),
            worktree_path: PathBuf::from(format!("/tmp/{}", name)),
            base_branch: "main".to_string(),
            agent: Agent::Opencode,
            status,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            tabs: vec![],
        }
    }

    #[test]
    fn renders_without_panic() {
        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = AppState::default();
        state.add_workspace(make_workspace("caetano", WorkspaceStatus::Active));
        state.add_workspace(make_workspace("gil", WorkspaceStatus::Inactive));
        state.add_workspace(make_workspace("elis", WorkspaceStatus::Exited(42)));
        state.archive("elis");

        let mut list_state = ListState::default();
        terminal
            .draw(|f| {
                render(f, f.area(), &state, &mut list_state, true, "myrepo");
            })
            .unwrap();
    }

    #[test]
    fn empty_state_renders() {
        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = AppState::default();
        let mut list_state = ListState::default();
        terminal
            .draw(|f| {
                render(f, f.area(), &state, &mut list_state, false, "myrepo");
            })
            .unwrap();
        // Verify it rendered something (no panic)
        let buf = terminal.backend().buffer().clone();
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("WORKSPACES") || content.contains("No workspaces"));
    }
}
