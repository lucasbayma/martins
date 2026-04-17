#![allow(dead_code)]
//! Right sidebar: modified files list with status icons.

use crate::git::diff::{FileEntry, FileStatus};
use crate::ui::theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};

/// Truncate a path string to fit within `max_width`, prefixing with "..." if needed.
fn truncate_path(path: &str, max_width: usize) -> String {
    if path.len() <= max_width {
        path.to_string()
    } else {
        let tail = &path[path.len().saturating_sub(max_width.saturating_sub(3))..];
        format!("...{}", tail)
    }
}

/// Render the right sidebar with modified files.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    files: &[FileEntry],
    list_state: &mut ListState,
    focused: bool,
    base_branch: &str,
) {
    let border_style = if focused {
        Style::default().fg(theme::ACCENT_GOLD)
    } else {
        Style::default().fg(theme::BORDER_MUTED)
    };

    let title = format!(" Changes {} ", files.len());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    // Available width for path (area - borders - icon - space)
    let path_width = (area.width as usize).saturating_sub(5);

    let items: Vec<ListItem> = if files.is_empty() {
        vec![ListItem::new(Line::from(vec![Span::styled(
            "No changes.",
            Style::default().fg(theme::TEXT_MUTED),
        )]))]
    } else {
        files
            .iter()
            .map(|entry| {
                let (icon, icon_style) = match entry.status {
                    FileStatus::Modified => ("M", Style::default().fg(theme::ACCENT_GOLD)),
                    FileStatus::Added => ("A", Style::default().fg(theme::ACCENT_SAGE)),
                    FileStatus::Deleted => ("D", Style::default().fg(theme::ACCENT_TERRA)),
                    FileStatus::Renamed => ("R", Style::default().fg(theme::TEXT_SECONDARY)),
                    FileStatus::Untracked => ("?", Style::default().fg(theme::TEXT_MUTED)),
                };

                let path_str = entry.path.to_string_lossy();
                let truncated = truncate_path(&path_str, path_width);

                ListItem::new(Line::from(vec![
                    Span::styled(icon, icon_style),
                    Span::raw(" "),
                    Span::styled(truncated, Style::default().fg(theme::TEXT_SECONDARY)),
                ]))
            })
            .collect()
    };

    // Add base branch indicator as subtitle
    let _ = base_branch; // used in title in full impl

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
    use crate::git::diff::{FileEntry, FileStatus};
    use ratatui::{Terminal, backend::TestBackend};
    use std::path::PathBuf;

    fn make_entry(path: &str, status: FileStatus) -> FileEntry {
        FileEntry {
            path: PathBuf::from(path),
            status,
        }
    }

    #[test]
    fn renders_files() {
        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let files = vec![
            make_entry("src/main.rs", FileStatus::Modified),
            make_entry("src/new.rs", FileStatus::Added),
            make_entry("old.rs", FileStatus::Deleted),
            make_entry("untracked.txt", FileStatus::Untracked),
        ];
        let mut list_state = ListState::default();
        terminal
            .draw(|f| {
                render(f, f.area(), &files, &mut list_state, true, "main");
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains('M') || content.contains('A'));
    }

    #[test]
    fn empty_state() {
        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut list_state = ListState::default();
        terminal
            .draw(|f| {
                render(f, f.area(), &[], &mut list_state, false, "main");
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("No changes"));
    }

    #[test]
    fn long_path_truncated() {
        let long = "very/deeply/nested/directory/structure/file.rs";
        let truncated = truncate_path(long, 20);
        assert!(truncated.len() <= 20);
        assert!(truncated.starts_with("..."));
    }
}
