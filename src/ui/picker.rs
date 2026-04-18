#![allow(dead_code)]

//! Fuzzy picker overlay for workspaces and modified files.

use crate::ui::theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nucleo_matcher::{
    Config, Matcher,
    pattern::{CaseMatching, Normalization, Pattern},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerKind {
    Workspaces,
    ModifiedFiles,
    NewTab,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerOutcome {
    Selected(usize),
    Cancelled,
    Continue,
}

pub struct Picker {
    pub input: String,
    pub items: Vec<String>,
    pub filtered: Vec<usize>, // indices into items
    pub kind: PickerKind,
    pub selected: usize,
}

impl Picker {
    pub fn new(items: Vec<String>, kind: PickerKind) -> Self {
        let filtered: Vec<usize> = (0..items.len()).collect();
        Self {
            input: String::new(),
            items,
            filtered,
            kind,
            selected: 0,
        }
    }

    /// Re-rank items using nucleo-matcher.
    pub fn update_filter(&mut self) {
        if self.input.is_empty() {
            self.filtered = (0..self.items.len()).collect();
            self.selected = 0;
            return;
        }

        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(&self.input, CaseMatching::Ignore, Normalization::Smart);

        let mut scored: Vec<(usize, u32)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let mut buf = Vec::new();
                let score =
                    pattern.score(nucleo_matcher::Utf32Str::new(item, &mut buf), &mut matcher);
                score.map(|s| (i, s))
            })
            .collect();

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        self.filtered = scored.into_iter().map(|(i, _)| i).take(20).collect();
        self.selected = 0;
    }

    pub fn on_key(&mut self, key: KeyEvent) -> PickerOutcome {
        match key.code {
            KeyCode::Esc => PickerOutcome::Cancelled,
            KeyCode::Enter => {
                if let Some(&idx) = self.filtered.get(self.selected) {
                    PickerOutcome::Selected(idx)
                } else {
                    PickerOutcome::Cancelled
                }
            }
            KeyCode::Down | KeyCode::Char('j') if key.modifiers == KeyModifiers::NONE => {
                if self.selected + 1 < self.filtered.len() {
                    self.selected += 1;
                }
                PickerOutcome::Continue
            }
            KeyCode::Up | KeyCode::Char('k') if key.modifiers == KeyModifiers::NONE => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                PickerOutcome::Continue
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.update_filter();
                PickerOutcome::Continue
            }
            KeyCode::Char(c) => {
                self.input.push(c);
                self.update_filter();
                PickerOutcome::Continue
            }
            _ => PickerOutcome::Continue,
        }
    }
}

/// Render the fuzzy picker overlay.
pub fn render(frame: &mut Frame, picker: &Picker) {
    // 60% width, 50% height, centered
    let area = {
        let r = frame.area();
        let w = (r.width as f32 * 0.6) as u16;
        let h = (r.height as f32 * 0.5) as u16;
        let x = (r.width.saturating_sub(w)) / 2;
        let y = (r.height.saturating_sub(h)) / 2;
        Rect::new(x, y, w, h)
    };

    frame.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // input
            Constraint::Min(1),    // list
            Constraint::Length(1), // footer
        ])
        .split(area);

    // Input box
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT_GOLD));
    let input_para = Paragraph::new(format!("> {}", picker.input))
        .block(input_block)
        .style(Style::default().fg(theme::TEXT_PRIMARY));
    frame.render_widget(input_para, chunks[0]);

    // Results list
    let kind_label = match picker.kind {
        PickerKind::Workspaces => "WS  ",
        PickerKind::ModifiedFiles => "FILE",
        PickerKind::NewTab => "TAB ",
    };

    let items: Vec<ListItem> = picker
        .filtered
        .iter()
        .enumerate()
        .map(|(display_idx, &item_idx)| {
            let name = &picker.items[item_idx];
            let style = if display_idx == picker.selected {
                Style::default()
                    .bg(theme::BG_SELECTED)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT_SECONDARY)
            };
            ListItem::new(Line::from(vec![
                Span::styled(kind_label, Style::default().fg(theme::TEXT_MUTED)),
                Span::raw(" "),
                Span::styled(name.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT)
            .border_style(Style::default().fg(theme::BORDER_MUTED)),
    );
    frame.render_widget(list, chunks[1]);

    // Footer
    let footer = Paragraph::new(format!(
        "{} of {} matches    ↑↓ ↵ open",
        picker.filtered.len(),
        picker.items.len()
    ))
    .style(Style::default().fg(theme::TEXT_MUTED));
    frame.render_widget(footer, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use ratatui::{Terminal, backend::TestBackend};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn fuzzy_filter() {
        let items: Vec<String> = vec![
            "caetano".to_string(),
            "gil".to_string(),
            "elis".to_string(),
            "chico".to_string(),
            "caetano-2".to_string(),
        ];
        let mut picker = Picker::new(items, PickerKind::Workspaces);
        picker.input = "cae".to_string();
        picker.update_filter();
        assert!(!picker.filtered.is_empty());
        // caetano should be in results
        let names: Vec<&str> = picker
            .filtered
            .iter()
            .map(|&i| picker.items[i].as_str())
            .collect();
        assert!(names.iter().any(|n| n.contains("caetano")));
    }

    #[test]
    fn navigate_and_select() {
        let items: Vec<String> = (0..5).map(|i| format!("item-{}", i)).collect();
        let mut picker = Picker::new(items, PickerKind::Workspaces);
        // Navigate down 3 times
        picker.on_key(key(KeyCode::Down));
        picker.on_key(key(KeyCode::Down));
        picker.on_key(key(KeyCode::Down));
        assert_eq!(picker.selected, 3);
        // Select
        let outcome = picker.on_key(key(KeyCode::Enter));
        assert_eq!(outcome, PickerOutcome::Selected(3));
    }

    #[test]
    fn esc_cancels() {
        let mut picker = Picker::new(vec!["a".to_string()], PickerKind::Workspaces);
        let outcome = picker.on_key(key(KeyCode::Esc));
        assert_eq!(outcome, PickerOutcome::Cancelled);
    }

    #[test]
    fn renders_without_panic() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let picker = Picker::new(
            vec!["caetano".to_string(), "gil".to_string()],
            PickerKind::Workspaces,
        );
        terminal.draw(|f| render(f, &picker)).unwrap();
    }
}
