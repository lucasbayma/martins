//! Left sidebar: project/workspace tree.

use crate::app::SidebarItem;
use crate::state::GlobalState;
use crate::ui::theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};

#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    state: &GlobalState,
    active_project_idx: Option<usize>,
    _active_workspace_idx: Option<usize>,
    list_state: &mut ListState,
    focused: bool,
    working_map: &std::collections::HashMap<(String, String), bool>,
) -> Vec<SidebarItem> {
    let border_style = if focused {
        Style::default().fg(theme::ACCENT_GOLD)
    } else {
        Style::default().fg(theme::BORDER_MUTED)
    };

    let block = Block::default()
        .title(" Projects ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let mut items = Vec::new();
    let mut sidebar_items = Vec::new();

    if state.projects.is_empty() {
        list_state.select(Some(0));
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "No projects. Press 'a' to add one.",
            Style::default().fg(theme::TEXT_MUTED),
        )])));
        sidebar_items.push(SidebarItem::AddProject);
    } else {
        for (project_idx, project) in state.projects.iter().enumerate() {
            let is_active_project = active_project_idx == Some(project_idx);
            let arrow = if project.expanded { "▼" } else { "▶" };
            let label = if project.expanded {
                format!("{} {}", arrow, project.name)
            } else {
                format!("{} {} ({})", arrow, project.name, project.active().count())
            };
            let row_width = area.width.saturating_sub(2) as usize;
            let padding = " ".repeat(row_width.saturating_sub(label.chars().count() + 2));
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    label,
                    Style::default()
                        .fg(theme::TEXT_PRIMARY)
                        .add_modifier(if is_active_project {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::raw(padding),
                Span::styled("✕", Style::default().fg(theme::ACCENT_TERRA)),
            ])));
            sidebar_items.push(SidebarItem::RemoveProject(project_idx));

            if project.expanded {
                for (workspace_idx, ws) in project.active().enumerate() {
                    let key = (project.id.clone(), ws.name.clone());
                    let is_working = working_map.get(&key).copied().unwrap_or(false);
                    let (state_icon, state_style) = if is_working {
                        ("⚡", Style::default().fg(theme::ACCENT_GOLD))
                    } else {
                        ("✓", Style::default().fg(theme::ACCENT_SAGE))
                    };

                    let inner_w = area.width.saturating_sub(2) as usize;
                    let left_len = 2 + 1 + 1 + ws.name.len();
                    let pad = inner_w.saturating_sub(left_len + 1);
                    items.push(ListItem::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(state_icon, state_style),
                        Span::raw(" "),
                        Span::styled(ws.name.clone(), Style::default().fg(theme::TEXT_PRIMARY)),
                        Span::raw(" ".repeat(pad)),
                        Span::styled("✕", Style::default().fg(theme::ACCENT_TERRA)),
                    ])));
                    sidebar_items.push(SidebarItem::Workspace(project_idx, workspace_idx));
                }

                items.push(ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        "+ new workspace",
                        Style::default().fg(theme::ACCENT_GOLD),
                    ),
                ])));
                sidebar_items.push(SidebarItem::NewWorkspace(project_idx));
            }
        }

        items.push(ListItem::new(Line::from(vec![Span::styled(
            "──────────",
            Style::default().fg(theme::TEXT_DIM),
        )])));
        sidebar_items.push(SidebarItem::AddProject);
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "+ Add Project",
            Style::default().fg(theme::ACCENT_GOLD),
        )])));
        sidebar_items.push(SidebarItem::AddProject);
    }

    if list_state.selected().is_none() && !items.is_empty() {
        list_state.select(Some(0));
    }

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(theme::BG_SELECTED)
            .add_modifier(Modifier::BOLD),
    );

    frame.render_stateful_widget(list, area, list_state);
    sidebar_items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Agent, GlobalState, Project, Workspace, WorkspaceStatus};
    use ratatui::{Terminal, backend::TestBackend};
    use std::path::PathBuf;

    fn make_workspace(name: &str, status: WorkspaceStatus) -> Workspace {
        Workspace {
            name: name.to_string(),
            worktree_path: PathBuf::from(format!("/tmp/{name}")),
            base_branch: "main".to_string(),
            agent: Agent::Opencode,
            status,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            tabs: vec![],
        }
    }

    fn make_project(name: &str, expanded: bool) -> Project {
        let mut project = Project::new(PathBuf::from(format!("/tmp/{name}")), "main".to_string());
        project.name = name.to_string();
        project.expanded = expanded;
        project.add_workspace(make_workspace("caetano", WorkspaceStatus::Active));
        project.add_workspace(make_workspace("gil", WorkspaceStatus::Inactive));
        project
    }

    #[test]
    fn renders_project_tree() {
        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = GlobalState::default();
        state.projects.push(make_project("martins", true));
        state.projects.push(make_project("api-server", false));
        let mut list_state = ListState::default();

        terminal
            .draw(|f| {
                let items = render(
                    f,
                    f.area(),
                    &state,
                    Some(0),
                    Some(0),
                    &mut list_state,
                    true,
                    &std::collections::HashMap::new(),
                );
                assert!(!items.is_empty());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("martins"));
        assert!(content.contains("api-server"));
        assert!(!content.contains("[opencode]"));
        assert!(content.contains("✕"));
    }

    #[test]
    fn empty_state_renders() {
        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = GlobalState::default();
        let mut list_state = ListState::default();
        terminal
            .draw(|f| {
                render(f, f.area(), &state, None, None, &mut list_state, false, &std::collections::HashMap::new());
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("No projects") || content.contains("Add Project"));
    }
}
