//! Responsive 3-pane layout computation.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// User-controlled sidebar visibility toggles.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct LayoutState {
    pub show_left: bool,
    pub show_right: bool,
}

#[allow(dead_code)]
impl LayoutState {
    pub fn new() -> Self {
        Self {
            show_left: true,
            show_right: true,
        }
    }

    pub fn toggle_left(&mut self) {
        self.show_left = !self.show_left;
    }

    pub fn toggle_right(&mut self) {
        self.show_right = !self.show_right;
    }
}

/// Computed pane rectangles for a single frame.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PaneRects {
    pub left: Option<Rect>,
    pub terminal: Rect,
    pub right: Option<Rect>,
    pub status_bar: Rect,
}

/// Compute pane layout based on frame size and user toggles.
/// Returns None for sidebars that are hidden (by breakpoint or toggle).
#[allow(dead_code)]
pub fn compute(frame_size: Rect, state: &LayoutState) -> PaneRects {
    let w = frame_size.width;
    let _h = frame_size.height;

    // Determine which sidebars are visible
    let show_left = w >= 100 && state.show_left;
    let show_right = w >= 120 && state.show_right;

    // Split frame into content + status bar (1 row)
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame_size);

    let content_area = vertical[0];
    let status_bar = vertical[1];

    // Sidebar width: min(30, max(20, 20% of frame))
    let sidebar_w = ((w as f32 * 0.20) as u16).clamp(20, 30);

    if show_left && show_right {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(sidebar_w),
                Constraint::Min(1),
                Constraint::Length(sidebar_w),
            ])
            .split(content_area);
        PaneRects {
            left: Some(chunks[0]),
            terminal: chunks[1],
            right: Some(chunks[2]),
            status_bar,
        }
    } else if show_left {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(sidebar_w), Constraint::Min(1)])
            .split(content_area);
        PaneRects {
            left: Some(chunks[0]),
            terminal: chunks[1],
            right: None,
            status_bar,
        }
    } else {
        PaneRects {
            left: None,
            terminal: content_area,
            right: None,
            status_bar,
        }
    }
}

/// Returns true if the frame is too small to render (< 80×24).
#[allow(dead_code)]
pub fn is_too_small(frame_size: Rect) -> bool {
    frame_size.width < 80 || frame_size.height < 24
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: u16, h: u16) -> Rect {
        Rect::new(0, 0, w, h)
    }

    #[test]
    fn full_layout_200x60() {
        let state = LayoutState::new();
        let panes = compute(rect(200, 60), &state);
        assert!(panes.left.is_some());
        assert!(panes.right.is_some());
        assert!(panes.terminal.width > 0);
        assert_eq!(panes.status_bar.height, 1);
    }

    #[test]
    fn medium_layout_110x40() {
        let state = LayoutState::new();
        let panes = compute(rect(110, 40), &state);
        assert!(panes.left.is_some());
        assert!(panes.right.is_none()); // right hidden at <120
        assert!(panes.terminal.width > 0);
    }

    #[test]
    fn narrow_layout_85x30() {
        let state = LayoutState::new();
        let panes = compute(rect(85, 30), &state);
        assert!(panes.left.is_none()); // both hidden at <100
        assert!(panes.right.is_none());
        assert!(panes.terminal.width > 0);
    }

    #[test]
    fn too_small_70x24() {
        assert!(is_too_small(rect(70, 24)));
        assert!(!is_too_small(rect(80, 24)));
        assert!(!is_too_small(rect(200, 60)));
    }

    #[test]
    fn user_toggle_right() {
        let mut state = LayoutState::new();
        // At 200 wide, right is visible
        let panes = compute(rect(200, 60), &state);
        assert!(panes.right.is_some());
        // Toggle right off
        state.toggle_right();
        let panes = compute(rect(200, 60), &state);
        assert!(panes.right.is_none());
        // Toggle back on
        state.toggle_right();
        let panes = compute(rect(200, 60), &state);
        assert!(panes.right.is_some());
    }
}
