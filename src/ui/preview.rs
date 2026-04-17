//! File preview overlay using bat for syntax highlighting.

#![allow(dead_code)]

use crate::ui::theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::path::Path;
use std::process::Command;

/// Run `bat` on a file and return output lines.
/// Falls back to plain file reading if bat is not available.
pub fn bat_preview(path: &Path, max_lines: usize) -> Vec<String> {
    let bat_result = Command::new("bat")
        .args(["--color=never", "--style=numbers,changes", "--paging=never"])
        .arg(path)
        .output();

    let output = match bat_result {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        _ => std::fs::read_to_string(path).unwrap_or_else(|_| "(binary file)".to_string()),
    };

    output
        .lines()
        .take(max_lines)
        .map(|line| line.to_string())
        .collect()
}

/// Render a file preview overlay.
pub fn render_preview(frame: &mut Frame<'_>, path: &Path, lines: &[String]) {
    let r = frame.area();
    let w = (r.width as f32 * 0.80) as u16;
    let h = (r.height as f32 * 0.80) as u16;
    let x = (r.width.saturating_sub(w)) / 2;
    let y = (r.height.saturating_sub(h)) / 2;
    let area = Rect::new(x, y, w, h);

    frame.render_widget(Clear, area);

    let title = format!(" {} ", path.display());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT_GOLD));

    let content: Vec<Line<'_>> = if lines.is_empty() {
        vec![Line::from(Span::styled(
            "(empty file)",
            Style::default().fg(theme::TEXT_MUTED),
        ))]
    } else {
        lines
            .iter()
            .map(|line| Line::from(Span::raw(line.clone())))
            .collect()
    };

    let paragraph = Paragraph::new(content)
        .block(block)
        .style(Style::default().fg(theme::TEXT_SECONDARY));

    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn preview_text_file() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "line 1").unwrap();
        writeln!(file, "line 2").unwrap();
        writeln!(file, "line 3").unwrap();

        let lines = bat_preview(file.path(), 100);
        assert!(!lines.is_empty());
        let joined = lines.join("\n");
        assert!(joined.contains("line 1") || joined.contains('1'));
    }

    #[test]
    fn preview_missing_file() {
        let _lines = bat_preview(Path::new("/nonexistent/file.rs"), 100);
    }

    #[test]
    fn render_preview_no_panic() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let lines = vec!["fn main() {}".to_string(), "// comment".to_string()];

        terminal
            .draw(|frame| {
                render_preview(frame, Path::new("src/main.rs"), &lines);
            })
            .unwrap();
    }
}
