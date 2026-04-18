//! Input mode management and keymap registry.

#![allow(dead_code)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    FocusLeft,
    FocusRight,
    FocusTerminal,
    NextItem,
    PrevItem,
    EnterSelected,
    NewWorkspace,
    NewWorkspaceAuto,
    ArchiveWorkspace,
    UnarchiveWorkspace,
    DeleteWorkspace,
    NewTab,
    CloseTab,
    SwitchTab(u8),
    Quit,
    ToggleSidebarLeft,
    ToggleSidebarRight,
    OpenFuzzy,
    EnterTerminalMode,
    ExitTerminalMode,
    ShowHelp,
    Preview,
    Edit,
    AddProject,
    ClickProject(usize),
    ClickWorkspace(usize, usize),
    ClickTab(usize),
    ClickFile(usize),
    ToggleProjectExpand(usize),
}

pub struct Keymap {
    normal: HashMap<KeyEvent, Action>,
}

impl Keymap {
    pub fn default_keymap() -> Self {
        let mut normal = HashMap::new();

        // Navigation
        normal.insert(
            key(KeyCode::Char('j'), KeyModifiers::NONE),
            Action::NextItem,
        );
        normal.insert(
            key(KeyCode::Char('k'), KeyModifiers::NONE),
            Action::PrevItem,
        );
        normal.insert(key(KeyCode::Down, KeyModifiers::NONE), Action::NextItem);
        normal.insert(key(KeyCode::Up, KeyModifiers::NONE), Action::PrevItem);
        normal.insert(key(KeyCode::Tab, KeyModifiers::NONE), Action::FocusTerminal);
        normal.insert(
            key(KeyCode::Char('h'), KeyModifiers::NONE),
            Action::FocusLeft,
        );
        normal.insert(
            key(KeyCode::Char('l'), KeyModifiers::NONE),
            Action::FocusRight,
        );
        normal.insert(
            key(KeyCode::Enter, KeyModifiers::NONE),
            Action::EnterSelected,
        );

        // Workspace ops
        normal.insert(
            key(KeyCode::Char('n'), KeyModifiers::NONE),
            Action::NewWorkspace,
        );
        normal.insert(
            key(KeyCode::Char('N'), KeyModifiers::SHIFT),
            Action::NewWorkspaceAuto,
        );
        normal.insert(
            key(KeyCode::Char('a'), KeyModifiers::NONE),
            Action::ArchiveWorkspace,
        );
        normal.insert(
            key(KeyCode::Char('u'), KeyModifiers::NONE),
            Action::UnarchiveWorkspace,
        );
        normal.insert(
            key(KeyCode::Char('d'), KeyModifiers::NONE),
            Action::DeleteWorkspace,
        );

        // Tab ops
        normal.insert(key(KeyCode::Char('t'), KeyModifiers::NONE), Action::NewTab);
        normal.insert(
            key(KeyCode::Char('T'), KeyModifiers::SHIFT),
            Action::CloseTab,
        );
        for i in 1u8..=9 {
            normal.insert(
                key(
                    KeyCode::Char(char::from_digit(i as u32, 10).unwrap()),
                    KeyModifiers::NONE,
                ),
                Action::SwitchTab(i),
            );
        }

        // App ops
        normal.insert(key(KeyCode::Char('q'), KeyModifiers::NONE), Action::Quit);
        normal.insert(key(KeyCode::Char('c'), KeyModifiers::CONTROL), Action::Quit);
        // Ctrl+Q
        normal.insert(
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
            Action::Quit,
        );
        normal.insert(
            key(KeyCode::Char('/'), KeyModifiers::NONE),
            Action::OpenFuzzy,
        );
        normal.insert(
            key(KeyCode::Char('?'), KeyModifiers::SHIFT),
            Action::ShowHelp,
        );
        normal.insert(key(KeyCode::Char('p'), KeyModifiers::NONE), Action::Preview);
        normal.insert(key(KeyCode::Char('e'), KeyModifiers::NONE), Action::Edit);

        // Sidebar toggles
        normal.insert(
            KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
            Action::ToggleSidebarLeft,
        );
        normal.insert(
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
            Action::ToggleSidebarRight,
        );

        // Enter terminal mode
        normal.insert(
            key(KeyCode::Char('i'), KeyModifiers::NONE),
            Action::EnterTerminalMode,
        );

        Self { normal }
    }

    /// Resolve a key event in Normal mode.
    pub fn resolve_normal(&self, key: &KeyEvent) -> Option<&Action> {
        self.normal.get(key)
    }
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

/// State machine for double-Esc detection in Terminal mode.
/// Returns Some(ExitTerminalMode) on second Esc within 300ms.
pub struct EscapeDetector {
    first_esc_at: Option<Instant>,
}

impl EscapeDetector {
    pub fn new() -> Self {
        Self { first_esc_at: None }
    }

    /// Process a key event in Terminal mode.
    /// Returns Some(Action::ExitTerminalMode) if double-Esc detected.
    /// Returns None for all other keys (they should be forwarded to PTY).
    pub fn process(&mut self, event: &KeyEvent) -> Option<Action> {
        let is_esc = event.code == KeyCode::Esc && event.modifiers == KeyModifiers::NONE;
        let is_ctrl_b =
            event.code == KeyCode::Char('b') && event.modifiers == KeyModifiers::CONTROL;

        if is_ctrl_b {
            self.first_esc_at = None;
            return Some(Action::ExitTerminalMode);
        }

        if is_esc {
            if let Some(first) = self.first_esc_at {
                if first.elapsed() < Duration::from_millis(300) {
                    self.first_esc_at = None;
                    return Some(Action::ExitTerminalMode);
                }
            }
            self.first_esc_at = Some(Instant::now());
            return None;
        }

        // Non-Esc key: reset state, forward to PTY
        self.first_esc_at = None;
        None
    }
}

impl Default for EscapeDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve a key in Terminal mode using EscapeDetector.
/// Returns Some(Action) only for mode-exit keys; None means forward to PTY.
pub fn resolve_terminal(detector: &mut EscapeDetector, event: &KeyEvent) -> Option<Action> {
    detector.process(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn normal_mode_mappings() {
        let km = Keymap::default_keymap();
        assert_eq!(
            km.resolve_normal(&make_key(KeyCode::Char('j'), KeyModifiers::NONE)),
            Some(&Action::NextItem)
        );
        assert_eq!(
            km.resolve_normal(&make_key(KeyCode::Char('k'), KeyModifiers::NONE)),
            Some(&Action::PrevItem)
        );
        assert_eq!(
            km.resolve_normal(&make_key(KeyCode::Char('n'), KeyModifiers::NONE)),
            Some(&Action::NewWorkspace)
        );
        assert_eq!(
            km.resolve_normal(&make_key(KeyCode::Char('q'), KeyModifiers::NONE)),
            Some(&Action::Quit)
        );
        assert_eq!(
            km.resolve_normal(&make_key(KeyCode::Char('i'), KeyModifiers::NONE)),
            Some(&Action::EnterTerminalMode)
        );
    }

    #[test]
    fn terminal_mode_forwards() {
        let mut det = EscapeDetector::new();
        // Regular keys return None (forward to PTY)
        assert_eq!(
            resolve_terminal(&mut det, &make_key(KeyCode::Char('a'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            resolve_terminal(&mut det, &make_key(KeyCode::Char('j'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            resolve_terminal(&mut det, &make_key(KeyCode::Enter, KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            resolve_terminal(&mut det, &make_key(KeyCode::Backspace, KeyModifiers::NONE)),
            None
        );
        // Ctrl+C returns None (forward to PTY)
        assert_eq!(
            resolve_terminal(
                &mut det,
                &make_key(KeyCode::Char('c'), KeyModifiers::CONTROL)
            ),
            None
        );
    }

    #[test]
    fn ctrl_b_exits_terminal() {
        let mut det = EscapeDetector::new();
        let result = resolve_terminal(
            &mut det,
            &make_key(KeyCode::Char('b'), KeyModifiers::CONTROL),
        );
        assert_eq!(result, Some(Action::ExitTerminalMode));
    }

    #[test]
    fn double_esc_exits_within_300ms() {
        let mut det = EscapeDetector::new();
        // First Esc: None (buffered)
        let r1 = resolve_terminal(&mut det, &make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(r1, None);
        // Second Esc immediately: ExitTerminalMode
        let r2 = resolve_terminal(&mut det, &make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(r2, Some(Action::ExitTerminalMode));
    }

    #[test]
    fn single_esc_does_not_exit() {
        let mut det = EscapeDetector::new();
        let r = resolve_terminal(&mut det, &make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(r, None);
        // After 400ms, state resets
        std::thread::sleep(std::time::Duration::from_millis(400));
        let r2 = resolve_terminal(&mut det, &make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(r2, None);
    }

    #[test]
    fn ctrl_q_quits() {
        let km = Keymap::default_keymap();
        let result = km.resolve_normal(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
        assert_eq!(result, Some(&Action::Quit));
    }

    #[test]
    fn number_keys_switch_tabs() {
        let km = Keymap::default_keymap();
        let result = km.resolve_normal(&make_key(KeyCode::Char('3'), KeyModifiers::NONE));
        assert_eq!(result, Some(&Action::SwitchTab(3)));
    }
}
