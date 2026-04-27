//! Event routing and action dispatch.
//!
//! Extracted from src/app.rs as part of the architectural split (Phase 1).
//! Owns the translation from crossterm events + keymap actions into
//! App state mutations. All functions take `&mut App` (or `&App` when
//! read-only) — the App struct remains the single source of state.

use crate::app::{App, SelectionState, SidebarItem, TabClick};
use crate::keys::{Action, InputMode};
use crate::ui::modal::{AddProjectForm, CommandArgsForm, Modal, NewWorkspaceForm};
use crate::ui::picker::{Picker, PickerKind, PickerOutcome};
use crate::ui::preview;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;

/// Encode a crossterm MouseEvent into an SGR (1006) byte sequence for
/// forwarding into the wrapped tmux PTY. Coords are inner-pane-relative AND
/// 1-based per xterm convention. Returns None for events that should not be
/// forwarded (Moved without button, ScrollLeft/ScrollRight, non-Left buttons,
/// and any other variant out of Phase 7 scope per CONTEXT.md D-04 / Deferred).
///
/// Per 07-RESEARCH.md §SGR Mouse Encoding: button bits {0=left} | motion(32) for Drag,
/// modifier bits {SHIFT=4, ALT=8, CONTROL=16}, trailing byte 'M' for press/motion and
/// 'm' for release.
///
/// Mirrors the existing inline encodes at events.rs:195 (scroll wheel) and
/// events.rs:256 (sidebar click forward) — single source of truth for SGR.
pub(crate) fn encode_sgr_mouse(
    kind: MouseEventKind,
    modifiers: KeyModifiers,
    local_col: u16,
    local_row: u16,
) -> Option<Vec<u8>> {
    let (button_base, trailing) = match kind {
        MouseEventKind::Down(MouseButton::Left)  => (0u8, 'M'),
        MouseEventKind::Drag(MouseButton::Left)  => (32u8, 'M'),
        MouseEventKind::Up(MouseButton::Left)    => (0u8, 'm'),
        MouseEventKind::ScrollUp                 => (64u8, 'M'),
        MouseEventKind::ScrollDown               => (65u8, 'M'),
        _ => return None,
    };
    let mut cb = button_base;
    if modifiers.contains(KeyModifiers::SHIFT)   { cb += 4; }
    if modifiers.contains(KeyModifiers::ALT)     { cb += 8; }
    if modifiers.contains(KeyModifiers::CONTROL) { cb += 16; }
    let col = local_col + 1;
    let row = local_row + 1;
    Some(format!("\x1b[<{cb};{col};{row}{trailing}").into_bytes())
}

pub async fn handle_event(app: &mut App, event: Event) {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(app, key).await,
        Event::Mouse(mouse) => handle_mouse(app, mouse).await,
        Event::Paste(text) => {
            if app.mode == InputMode::Terminal {
                let mut buf = Vec::with_capacity(text.len() + 12);
                buf.extend_from_slice(b"\x1b[200~");
                buf.extend_from_slice(text.as_bytes());
                buf.extend_from_slice(b"\x1b[201~");
                app.write_active_tab_input(&buf);
            }
        }
        Event::Resize(_, _) => {}
        _ => {}
    }
}

pub async fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    let in_terminal = app.last_panes.as_ref().is_some_and(|p| {
        let inner = terminal_content_rect(p.terminal);
        rect_contains(inner, mouse.column, mouse.row)
    });

    // GAP-7-01 instrumentation: env-var gated mouse-event tracing for hypothesis D
    // (drag-exits-inner-rect-drops-events). Set MARTINS_MOUSE_DEBUG=1 and run with
    // 2>/tmp/martins-debug.log. Each forwarded/dropped event logs (kind, raw col/row,
    // inner rect, in_terminal flag, delegate decision). Cheap when unset.
    if std::env::var_os("MARTINS_MOUSE_DEBUG").is_some() {
        let inner = app
            .last_panes
            .as_ref()
            .map(|p| terminal_content_rect(p.terminal))
            .unwrap_or(Rect::default());
        let delegate = app.active_session_delegates_to_tmux();
        eprintln!(
            "[mouse] kind={:?} raw=({},{}) inner=({},{},{}x{}) in_term={} delegate={}",
            mouse.kind,
            mouse.column,
            mouse.row,
            inner.x,
            inner.y,
            inner.width,
            inner.height,
            in_terminal,
            delegate
        );
    }

    // Phase 7 (D-07/D-08): conditional intercept of Left-button events when
    // the inner program has not requested mouse mode AND we're not on alternate
    // screen. Forward as raw SGR (1006) bytes into the wrapped tmux PTY, which
    // owns visual feedback via its native copy-mode-vi bindings (defaults +
    // ensure_config 3-line overrides from Plan 07-02).
    //
    // Gates per RESEARCH §Pitfall #1: in-terminal AND no modal AND no picker —
    // otherwise modal/picker clicks would leak as tmux clipboard clicks.
    //
    // WR-02 (Phase 07 review): the delegation decision is *latched per gesture*
    // — once a `Down(Left)` is forwarded, the matching `Drag/Up(Left)` events
    // continue forwarding even if the inner program toggles DECSET 1000h /
    // 1049h mid-gesture. Without the latch, tmux would never receive the
    // matching Up, leaving its button-state machine stuck and
    // `tmux_in_copy_mode` stuck `true` until the next legitimate forwarded Up.
    let live_delegating = app.active_session_delegates_to_tmux();
    let gesture_latched = app.tmux_gesture_delegating();
    let delegating = live_delegating || gesture_latched;
    if in_terminal
        && matches!(app.modal, Modal::None)
        && app.picker.is_none()
        && delegating
    {
        let inner = terminal_content_rect(app.last_panes.as_ref().unwrap().terminal);
        let local_col = mouse.column.saturating_sub(inner.x);
        let local_row = mouse.row.saturating_sub(inner.y);
        let forwarded = matches!(
            mouse.kind,
            MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left)
        );
        // WR-02: a Down only opens a forwarded gesture when delegation is live.
        // If we entered this block solely on the `gesture_latched` path, a fresh
        // Down(Left) with no live delegation would mean the prior gesture's Up
        // was lost — defensively close that gesture rather than starting a new
        // one. Drag/Up always honor the latch (that is the whole point).
        let open_new_gesture = matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
            && live_delegating;
        let continue_gesture = matches!(
            mouse.kind,
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
        ) && (live_delegating || gesture_latched);
        if forwarded && (open_new_gesture || continue_gesture) {
            // Pitfall #2: clear any stale overlay selection from a prior
            // mouse-app session (e.g. user just quit vim). Otherwise the
            // residual REVERSED highlight would render alongside tmux's own.
            if app.selection.is_some() {
                app.clear_selection();
            }
            if let Some(bytes) =
                encode_sgr_mouse(mouse.kind, mouse.modifiers, local_col, local_row)
            {
                app.write_active_tab_input(&bytes);
            }
            // State machine for tmux_in_copy_mode (RESEARCH §State Source — Option a):
            // Down  → set true   (tmux will enter copy-mode on the drag-or-click)
            // Drag  → set drag_seen true
            // Up    → if drag_seen was set, leave in_copy_mode true (selection now
            //         shown by tmux); else set in_copy_mode false (single click,
            //         no copy-mode entered).
            //
            // WR-02: latch on Down, release on Up so the gesture is atomic.
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    app.tmux_in_copy_mode_set(true);
                    app.tmux_gesture_delegating_set(true);
                }
                MouseEventKind::Drag(MouseButton::Left) => app.tmux_drag_seen_set(true),
                MouseEventKind::Up(MouseButton::Left) => {
                    if !app.tmux_drag_seen_take() {
                        app.tmux_in_copy_mode_set(false);
                    }
                    app.tmux_gesture_delegating_set(false);
                }
                _ => {}
            }
            // Do NOT mark_dirty — tmux's PTY output triggers redraw through the
            // existing drain → output_notify → mark_dirty path.
            return;
        }
        // Non-forwarded variants (Moved, scroll, etc.) fall through to the
        // existing match below — overlay path will route ScrollUp/Down via its
        // own SGR encode at events.rs:195-196.
    }

    if in_terminal {
        match mouse.kind {
            MouseEventKind::Drag(MouseButton::Left) => {
                let inner = terminal_content_rect(app.last_panes.as_ref().unwrap().terminal);
                let col = mouse.column.saturating_sub(inner.x).min(inner.width.saturating_sub(1));
                let row = mouse.row.saturating_sub(inner.y).min(inner.height.saturating_sub(1));
                let current_gen = app.active_scroll_generation();
                if let Some(sel) = &mut app.selection {
                    // Mid-drag extension: only the live cursor endpoint moves.
                    // end_gen stays None and text stays None until Up (D-07).
                    sel.end_col = col;
                    sel.end_row = row;
                } else {
                    app.selection = Some(SelectionState {
                        start_col: col,
                        start_row: row,
                        start_gen: current_gen,
                        end_col: col,
                        end_row: row,
                        end_gen: None,
                        dragging: true,
                        text: None,
                    });
                }
                app.mark_dirty();
                return;
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(mut sel) = app.selection.take() {
                    if sel.is_empty() {
                        // Empty selection — leave app.selection as None (D-04 inverse).
                        app.mark_dirty();
                        return;
                    }
                    sel.dragging = false;
                    sel.end_gen = Some(app.active_scroll_generation());
                    // MINOR-01: only store snapshot when materialization
                    // actually returned text. `materialize_selection_text`
                    // returns `String::new()` on parser try_read contention
                    // OR when the selection's visible content is genuinely
                    // empty. Storing `Some("")` would defeat the live
                    // re-materialization fallback in
                    // `copy_selection_to_clipboard` (which uses
                    // `unwrap_or_else` — a `Some("")` short-circuits the
                    // fallback and `pbcopy` writes nothing).
                    let text = app.materialize_selection_text(&sel);
                    if !text.is_empty() {
                        sel.text = Some(text);
                    }
                    app.selection = Some(sel);
                    app.copy_selection_to_clipboard();
                    app.mark_dirty();
                    return;
                }
            }
            _ => {}
        }
    }

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // D-19: Shift+click takes precedence — extends end of existing
            // selection. No-op if no selection active.
            if mouse.modifiers.contains(KeyModifiers::SHIFT) {
                if app.selection.is_some() && in_terminal {
                    let inner = terminal_content_rect(app.last_panes.as_ref().unwrap().terminal);
                    let col = mouse
                        .column
                        .saturating_sub(inner.x)
                        .min(inner.width.saturating_sub(1));
                    let row = mouse
                        .row
                        .saturating_sub(inner.y)
                        .min(inner.height.saturating_sub(1));
                    app.extend_selection_to(row, col);
                    return;
                }
                // No selection + shift+click = no-op (D-19).
                return;
            }
            // D-12 / D-13: any plain Down(Left) clears the active selection.
            // D-23: dirty-mark in the same scope as the mutation.
            if app.selection.is_some() {
                app.selection = None;
                app.mark_dirty();
            }
            // D-16: maintain a 300ms click cluster counter. Reset to 1 if
            // the click lands at a different row OR exceeds the threshold.
            // Click-counter logic only meaningful inside the terminal pane.
            if in_terminal {
                let inner = terminal_content_rect(app.last_panes.as_ref().unwrap().terminal);
                let inner_col = mouse
                    .column
                    .saturating_sub(inner.x)
                    .min(inner.width.saturating_sub(1));
                let inner_row = mouse
                    .row
                    .saturating_sub(inner.y)
                    .min(inner.height.saturating_sub(1));
                let now = std::time::Instant::now();
                let within_threshold = app
                    .last_click_at
                    .is_some_and(|t| now.duration_since(t) < std::time::Duration::from_millis(300));
                let same_row = mouse.row == app.last_click_row;
                if within_threshold && same_row {
                    app.last_click_count = app.last_click_count.saturating_add(1);
                } else {
                    app.last_click_count = 1;
                }
                app.last_click_at = Some(now);
                app.last_click_row = mouse.row;

                // D-15: dispatch on click count.
                match app.last_click_count {
                    2 => {
                        app.select_word_at(inner_row, inner_col);
                        return;
                    }
                    3 => {
                        app.select_line_at(inner_row);
                        return;
                    }
                    _ => {}
                }
            } else {
                // Outside terminal: still maintain reset semantics so a
                // subsequent in-terminal click starts fresh.
                app.last_click_count = 1;
                app.last_click_at = Some(std::time::Instant::now());
                app.last_click_row = mouse.row;
            }
            handle_click(app, mouse.column, mouse.row).await;
        }
        MouseEventKind::ScrollUp => handle_scroll(app, mouse.column, mouse.row, -1),
        MouseEventKind::ScrollDown => handle_scroll(app, mouse.column, mouse.row, 1),
        _ => {}
    }
}

pub fn handle_scroll(app: &mut App, col: u16, row: u16, delta: isize) {
    if let Modal::AddProject(ref mut form) = app.modal {
        form.move_selection(delta);
        return;
    }

    let Some(panes) = &app.last_panes else { return };

    if rect_contains(panes.terminal, col, row) {
        let inner = terminal_content_rect(panes.terminal);
        let local_col = col.saturating_sub(inner.x).saturating_add(1).max(1);
        let local_row = row.saturating_sub(inner.y).saturating_add(1).max(1);
        let button: u8 = if delta < 0 { 64 } else { 65 };
        let seq = format!("\x1b[<{button};{local_col};{local_row}M");
        app.write_active_tab_input(seq.as_bytes());
        return;
    }

    if let Some(right) = panes.right
        && rect_contains(right, col, row)
    {
        move_list_selection(&mut app.right_list, app.modified_files.len(), delta);
        return;
    }

    if let Some(left) = panes.left
        && rect_contains(left, col, row)
    {
        move_sidebar_to_workspace(&mut app.left_list, &app.sidebar_items, delta);
        return;
    }

    move_sidebar_to_workspace(&mut app.left_list, &app.sidebar_items, delta);
}

pub async fn handle_click(app: &mut App, col: u16, row: u16) {
    if handle_picker_click(app, col, row).await {
        return;
    }

    if crate::ui::modal_controller::handle_modal_click(app, col, row).await {
        return;
    }

    let Some(panes) = app.last_panes.clone() else { return };

    if !rect_contains(panes.terminal, col, row) {
        app.mode = InputMode::Normal;
    }

    if rect_contains(panes.terminal, col, row) {
        if row == panes.terminal.y {
            if let Some(click) = app.tab_at_column(panes.terminal, col) {
                match click {
                    TabClick::Select(idx) => dispatch_action(app, Action::ClickTab(idx)).await,
                    TabClick::Close(idx) => {
                        // D-22: tab-close click first focuses the tab
                        // (selection invariant: anchored gen is per-session,
                        // so the about-to-be-closed tab's selection is
                        // meaningless), then dispatches the close.
                        app.set_active_tab(idx);
                        dispatch_action(app, Action::CloseTab).await;
                    }
                    TabClick::Add => dispatch_action(app, Action::NewTab).await,
                }
                return;
            }
        }

        let inner = terminal_content_rect(panes.terminal);
        if rect_contains(inner, col, row) {
            let local_col = col.saturating_sub(inner.x) + 1;
            let local_row = row.saturating_sub(inner.y) + 1;
            let press = format!("\x1b[<0;{local_col};{local_row}M");
            let release = format!("\x1b[<0;{local_col};{local_row}m");
            app.write_active_tab_input(press.as_bytes());
            app.write_active_tab_input(release.as_bytes());
        }

        app.mode = InputMode::Terminal;
        return;
    }

    if let Some(left) = panes.left
        && rect_contains(left, col, row)
        && row > left.y
        && row < left.y + left.height - 1
    {
        let local_row = (row - left.y - 1) as usize;
        if let Some(item) = app.sidebar_items.get(local_row).cloned() {
            app.left_list.select(Some(local_row));
            match item {
                SidebarItem::RemoveProject(idx) => {
                    let delete_zone_start = left.x + left.width.saturating_sub(4);
                    if col >= delete_zone_start {
                        if let Some(project) = app.global_state.projects.get(idx) {
                            app.modal = Modal::ConfirmRemoveProject(crate::ui::modal::RemoveProjectForm {
                                project_name: project.name.clone(),
                                project_id: project.id.clone(),
                            });
                        }
                    } else {
                        dispatch_action(app, Action::ClickProject(idx)).await;
                    }
                }
                SidebarItem::Workspace(project_idx, workspace_idx) => {
                    let delete_zone_start = left.x + left.width.saturating_sub(4);
                    if col >= delete_zone_start {
                        if app.active_project_idx != Some(project_idx) {
                            crate::workspace::switch_project(app, project_idx).await;
                        }
                        app.select_active_workspace(workspace_idx);
                        crate::workspace::archive_active_workspace(app);
                    } else {
                        dispatch_action(app, Action::ClickWorkspace(project_idx, workspace_idx)).await;
                    }
                }
                SidebarItem::ArchivedHeader(project_idx) => {
                    if let Some(project) = app.global_state.projects.get(project_idx) {
                        let id = project.id.clone();
                        if !app.archived_expanded.remove(&id) {
                            app.archived_expanded.insert(id);
                        }
                    }
                }
                SidebarItem::ArchivedWorkspace(project_idx, archived_idx) => {
                    let delete_zone_start = left.x + left.width.saturating_sub(4);
                    if col >= delete_zone_start {
                        crate::workspace::delete_archived_workspace(app, project_idx, archived_idx);
                    }
                }
                SidebarItem::AddProject => dispatch_action(app, Action::AddProject).await,
                SidebarItem::NewWorkspace(project_idx) => {
                    dispatch_action(app, Action::ClickProject(project_idx)).await;
                    dispatch_action(app, Action::NewWorkspace).await;
                }
            }
        }
        return;
    }

    if let Some(right) = panes.right
        && rect_contains(right, col, row)
        && row > right.y
        && row < right.y + right.height - 1
    {
        let local_row = (row - right.y - 1) as usize;
        let offset = app.right_list.offset();
        let absolute_idx = offset + local_row;
        if absolute_idx < app.modified_files.len() {
            dispatch_action(app, Action::ClickFile(absolute_idx)).await;
        }
        return;
    }

    if rect_contains(panes.menu_bar, col, row) {
        let local_col = col.saturating_sub(panes.menu_bar.x);
        if let Some(action) = menu_action_at_column(local_col) {
            dispatch_action(app, action).await;
        }
        return;
    }

    if rect_contains(panes.status_bar, col, row) {
        let quit_label_len = " [Quit] ".len() as u16;
        let quit_start = panes.status_bar.x + panes.status_bar.width - quit_label_len;
        if col >= quit_start {
            dispatch_action(app, Action::Quit).await;
        }
    }
}

pub async fn handle_key(app: &mut App, key: KeyEvent) {
    if let KeyCode::F(n) = key.code {
        if (1..=9).contains(&n) {
            let tab_count = app
                .active_workspace()
                .map(|ws| ws.tabs.len())
                .unwrap_or(0);
            if tab_count > 0 {
                // D-22: tab switch via F1..F9 number-key clears any
                // active selection (per-session anchored gen).
                app.set_active_tab((n as usize - 1).min(tab_count - 1));
                app.mode = InputMode::Terminal;
            }
            return;
        }
    }

    if let Some(picker) = &mut app.picker {
        let outcome = picker.on_key(key);
        apply_picker_outcome(app, outcome).await;
        return;
    }

    if matches!(app.modal, Modal::Loading(_)) {
        return;
    }

    if !matches!(app.modal, Modal::None) {
        crate::ui::modal_controller::handle_modal_key(app, key).await;
        return;
    }

    // D-02, D-03 (Phase 6) + D-10, D-11 (Phase 7): cmd+c precedence chain.
    //   Tier 1 — overlay selection → copy snapshot text via existing path.
    //   Tier 2 — delegating session with non-empty tmux paste-buffer → spawn
    //            `tmux save-buffer - -t <session> | pbcopy` off-thread.
    //   Tier 3 — Terminal mode without either → SIGINT (0x03) to PTY.
    if key.code == KeyCode::Char('c')
        && key.modifiers.contains(KeyModifiers::SUPER)
    {
        // Tier 1 (Phase 6, unchanged): overlay selection.
        if let Some(sel) = &app.selection {
            if !sel.is_empty() {
                app.copy_selection_to_clipboard();
                // D-04: do NOT clear selection after copy.
                return;
            }
        }
        // Tier 2 (Phase 7 D-10): tmux paste-buffer when delegating.
        // Spawn off-thread to keep cmd+c sub-50ms even on cold tmux client.
        // save_buffer_to_pbcopy returns false silently when no buffer present
        // (RESEARCH §Subprocess Behavior); the user-visible effect of "no buffer"
        // is identical to "buffer was empty" — pbcopy receives nothing and the
        // existing clipboard contents are preserved.
        if app.active_session_delegates_to_tmux() {
            if let Some(session_name) = app.active_tmux_session_name() {
                tokio::task::spawn_blocking(move || {
                    crate::tmux::save_buffer_to_pbcopy(&session_name);
                });
                return;
            }
        }
        // Tier 3 (Phase 6, unchanged): SIGINT in Terminal mode.
        if app.mode == InputMode::Terminal {
            app.write_active_tab_input(&[0x03]);
            return;
        }
        // Normal mode + no selection + not delegating — fall through to keymap
        // (ctrl+c → Quit path unchanged).
    }

    // D-14 (Phase 6) + D-14 (Phase 7): Esc precedence chain.
    //   Tier 1 — overlay selection active → clear locally (Phase 6, unchanged).
    //   Tier 2 — delegating + tmux in copy-mode → forward `\x1b` byte to PTY.
    //   Tier 3 — neither → fall through to existing PTY/keymap path (unchanged).
    if key.code == KeyCode::Esc && key.modifiers == KeyModifiers::NONE {
        // Tier 1: overlay selection clear.
        if app.selection.is_some() {
            app.selection = None;
            app.mark_dirty();
            return;
        }
        // Tier 2 (Phase 7): forward to delegating tmux in copy-mode.
        // Tmux's vi-mode default Esc = clear-selection; our Plan 07-02 ensure_config
        // override binds Escape → cancel for single-press exit (RESEARCH §Tmux
        // Defaults — CRITICAL Esc asymmetry).
        if app.active_session_delegates_to_tmux() && app.tmux_in_copy_mode() {
            app.write_active_tab_input(&[0x1b]);
            // Locally clear the flag — tmux's `cancel` exits copy-mode, no
            // observable round-trip needed.
            app.tmux_in_copy_mode_set(false);
            return;
        }
        // Tier 3: fall through to existing path (PTY forward in Terminal mode,
        // or keymap in Normal mode — both unchanged).
    }

    if app.mode == InputMode::Terminal {
        if key.code == KeyCode::Char('b')
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            app.mode = InputMode::Normal;
            return;
        }
        app.forward_key_to_pty(&key);
        return;
    }

    if let Some(action) = app.keymap.resolve_normal(&key).cloned() {
        dispatch_action(app, action).await;
    }
}

pub async fn handle_picker_click(app: &mut App, col: u16, row: u16) -> bool {
    let Some(picker) = &app.picker else { return false };

    let frame_area = app.last_frame_area;
    if frame_area.width == 0 || frame_area.height == 0 {
        return false;
    }

    let picker_rect = picker_area(frame_area);
    if !rect_contains(picker_rect, col, row) {
        app.picker = None;
        return true;
    }

    let list_y = picker_rect.y + 3;
    let list_height = picker_rect.height.saturating_sub(4);
    if row >= list_y && row < list_y + list_height {
        let click_idx = (row - list_y) as usize;
        if let Some(&item_idx) = picker.filtered.get(click_idx) {
            apply_picker_outcome(app, PickerOutcome::Selected(item_idx)).await;
            return true;
        }
    }

    true
}

pub async fn apply_picker_outcome(app: &mut App, outcome: PickerOutcome) {
    match outcome {
        PickerOutcome::Cancelled => app.picker = None,
        PickerOutcome::Selected(index) => {
            let kind = app.picker.as_ref().map(|picker| picker.kind.clone());
            let picked_item = app
                .picker
                .as_ref()
                .and_then(|p| p.items.get(index).cloned());
            app.picker = None;
            match kind {
                Some(PickerKind::Workspaces) => app.select_active_workspace(index),
                Some(PickerKind::NewTab) => {
                    if let Some(command) = picked_item {
                        if command == "shell" {
                            if let Err(error) = crate::workspace::create_tab(app, "shell".to_string()).await {
                                tracing::error!("failed to create tab: {error}");
                            }
                        } else {
                            app.modal = Modal::CommandArgs(CommandArgsForm {
                                agent: command,
                                args_input: String::new(),
                            });
                        }
                    }
                }
                Some(PickerKind::ModifiedFiles) | None => {}
            }
        }
        PickerOutcome::Continue => {}
    }
}

pub async fn dispatch_action(app: &mut App, action: Action) {
    match action {
        Action::Quit => app.modal = Modal::ConfirmQuit,
        Action::NextItem => {
            move_sidebar_to_workspace(&mut app.left_list, &app.sidebar_items, 1);
            if let Some(idx) = app.left_list.selected() {
                activate_sidebar_item(app, idx).await;
            }
        }
        Action::PrevItem => {
            move_sidebar_to_workspace(&mut app.left_list, &app.sidebar_items, -1);
            if let Some(idx) = app.left_list.selected() {
                activate_sidebar_item(app, idx).await;
            }
        }
        Action::EnterSelected => {
            let has_tabs = app
                .active_workspace()
                .map(|ws| !ws.tabs.is_empty())
                .unwrap_or(false);
            if has_tabs {
                app.mode = InputMode::Terminal;
            } else if app.active_workspace().is_some() {
                app.open_new_tab_picker();
            }
        }
        Action::EnterTerminalMode | Action::FocusTerminal => app.mode = InputMode::Terminal,
        Action::ToggleSidebarLeft => app.layout.toggle_left(),
        Action::ToggleSidebarRight => app.layout.toggle_right(),
        Action::OpenFuzzy => {
            let items: Vec<String> = app
                .active_project()
                .map(|project| project.active().map(|workspace| workspace.name.clone()).collect())
                .unwrap_or_default();
            app.picker = Some(Picker::new(items, PickerKind::Workspaces));
        }
        Action::NewTab => {
            app.open_new_tab_picker();
        }
        Action::CloseTab => {
            let Some(project) = app.active_project() else {
                return;
            };
            let Some(workspace) = app.active_workspace() else {
                return;
            };
            let Some(tab) = workspace.tabs.get(app.active_tab).cloned() else {
                return;
            };

            let project_id = project.id.clone();
            let ws_name = workspace.name.clone();
            let tmux_name = crate::tmux::tab_session_name(&project_id, &ws_name, tab.id);
            let current_active_tab = app.active_tab;

            crate::tmux::kill_session(&tmux_name);
            app.pty_manager.close_tab(&project_id, &ws_name, tab.id);

            let new_active_tab = if let Some(project) = app.active_project_mut()
                && let Some(workspace) = project.workspaces.iter_mut().find(|workspace| workspace.name == ws_name)
            {
                workspace.tabs.retain(|existing| existing.id != tab.id);
                if workspace.tabs.is_empty() {
                    Some(0)
                } else {
                    Some(current_active_tab.min(workspace.tabs.len() - 1))
                }
            } else {
                None
            };
            if let Some(idx) = new_active_tab {
                // D-22: closing a tab implicitly switches to a different
                // session — the killed tab's selection is gone with it.
                app.set_active_tab(idx);
            }

            app.save_state_spawn();
        }
        Action::SwitchTab(n) => {
            let Some(workspace) = app.active_workspace() else {
                return;
            };
            if workspace.tabs.is_empty() {
                return;
            }
            let new_idx = (n as usize - 1).min(workspace.tabs.len() - 1);
            // D-22: keymap-driven tab switch clears any active selection.
            app.set_active_tab(new_idx);
            app.mode = InputMode::Terminal;
        }
        Action::NewWorkspace | Action::NewWorkspaceAuto => {
            if app.active_project().is_some() {
                app.modal = Modal::NewWorkspace(NewWorkspaceForm::default());
            } else {
                app.modal = Modal::AddProject(AddProjectForm::default());
            }
        }
        Action::AddProject => {
            app.modal = Modal::AddProject(AddProjectForm::default());
        }
        Action::ShowHelp => {
            app.modal = Modal::Help;
        }
        Action::ArchiveWorkspace => {
            if app.global_state.projects.is_empty() {
                app.modal = Modal::AddProject(AddProjectForm::default());
                return;
            }
            crate::workspace::archive_active_workspace(app);
        }
        Action::Preview => {
            if let (Some(project), Some(index)) = (app.active_project(), app.right_list.selected())
                && let Some(entry) = app.modified_files.get(index)
            {
                let full_path = project.repo_root.join(&entry.path);
                let lines = preview::bat_preview(&full_path, 200);
                app.preview_lines = Some((full_path, lines));
            }
        }
        Action::UnarchiveWorkspace => {}
        Action::DeleteWorkspace => {
            if let Some(idx) = app.active_workspace_idx {
                let name = app
                    .active_project()
                    .and_then(|project| project.active().nth(idx))
                    .map(|workspace| workspace.name.clone());
                if let Some(name) = name {
                    app.modal = Modal::ConfirmDelete(crate::ui::modal::DeleteForm {
                        workspace_name: name,
                        unpushed_commits: 0,
                        delete_branch: false,
                    });
                }
            }
        }
        Action::ClickProject(idx) => {
            if app.active_project_idx == Some(idx) {
                if let Some(project) = app.global_state.projects.get_mut(idx) {
                    project.expanded = !project.expanded;
                }
                app.save_state_spawn();
            } else {
                crate::workspace::switch_project(app, idx).await;
                if let Some(project) = app.global_state.projects.get_mut(idx) {
                    project.expanded = true;
                }
                app.save_state_spawn();
            }
        }
        Action::ClickWorkspace(project_idx, workspace_idx) => {
            if app.active_project_idx != Some(project_idx) {
                crate::workspace::switch_project(app, project_idx).await;
            }
            app.select_active_workspace(workspace_idx);
            app.refresh_diff_spawn();
            let has_tabs = app
                .active_workspace()
                .map(|ws| !ws.tabs.is_empty())
                .unwrap_or(false);
            if has_tabs {
                app.mode = InputMode::Terminal;
            } else {
                app.open_new_tab_picker();
            }
        }
        Action::ClickTab(idx) => {
            // D-22: tab pick from the right-list / dispatch_action route
            // clears any active selection.
            app.set_active_tab(idx);
            app.mode = InputMode::Terminal;
        }
        Action::ClickFile(idx) => {
            app.right_list.select(Some(idx));
            if let Some(entry) = app.modified_files.get(idx).cloned() {
                let path = entry.path.to_string_lossy().to_string();
                if let Err(error) = crate::workspace::create_tab(app, format!("diff {}", path)).await {
                    tracing::warn!("failed to open diff tab: {error}");
                }
            }
        }
        Action::ToggleProjectExpand(idx) => {
            if let Some(project) = app.global_state.projects.get_mut(idx) {
                project.expanded = !project.expanded;
            }
            app.save_state_spawn();
        }
        _ => {}
    }
}

pub async fn activate_sidebar_item(app: &mut App, index: usize) {
    let Some(item) = app.sidebar_items.get(index).cloned() else { return };
    match item {
        SidebarItem::RemoveProject(project_idx) => {
            if app.active_project_idx != Some(project_idx) {
                crate::workspace::switch_project(app, project_idx).await;
            }
        }
        SidebarItem::Workspace(project_idx, workspace_idx) => {
            if app.active_project_idx != Some(project_idx) {
                crate::workspace::switch_project(app, project_idx).await;
            }
            app.select_active_workspace(workspace_idx);
            app.refresh_diff_spawn();
        }
        _ => {}
    }
}

pub(crate) fn rect_contains(rect: Rect, col: u16, row: u16) -> bool {
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

pub(crate) fn terminal_content_rect(terminal: Rect) -> Rect {
    Rect {
        x: terminal.x + 1,
        y: terminal.y + 2,
        width: terminal.width.saturating_sub(2),
        height: terminal.height.saturating_sub(3),
    }
}

fn picker_area(frame_area: Rect) -> Rect {
    let w = (frame_area.width as f32 * 0.6) as u16;
    let h = (frame_area.height as f32 * 0.5) as u16;
    let x = (frame_area.width.saturating_sub(w)) / 2;
    let y = (frame_area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

fn move_list_selection(list: &mut ListState, len: usize, delta: isize) {
    if len == 0 {
        list.select(None);
        return;
    }

    let current = list.selected().unwrap_or(0) as isize;
    let next = (current + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
    list.select(Some(next));
}

fn move_sidebar_to_workspace(
    list: &mut ListState,
    items: &[SidebarItem],
    delta: isize,
) {
    if items.is_empty() {
        list.select(None);
        return;
    }
    let current = list.selected().unwrap_or(0) as isize;
    let step = if delta > 0 { 1isize } else { -1 };
    let len = items.len() as isize;
    let mut pos = current + step;
    while pos >= 0 && pos < len {
        if matches!(items[pos as usize], SidebarItem::Workspace(_, _)) {
            list.select(Some(pos as usize));
            return;
        }
        pos += step;
    }
}

pub(crate) fn menu_action_at_column(col: u16) -> Option<Action> {
    const MENU_ITEMS: &[(u16, u16, Action)] = &[
        (1, 5, Action::NewWorkspace),
        (8, 5, Action::NewTab),
        (15, 8, Action::DeleteWorkspace),
        (25, 6, Action::ShowHelp),
        (33, 6, Action::Quit),
    ];

    MENU_ITEMS
        .iter()
        .find(|(start, width, _)| col >= *start && col < *start + *width)
        .map(|(_, _, action)| action.clone())
}

pub(crate) fn key_to_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
    use crossterm::event::{KeyCode, KeyModifiers};

    let mods = key.modifiers;
    match key.code {
        KeyCode::Char(c) => {
            if mods.contains(KeyModifiers::CONTROL) {
                let byte = (c.to_ascii_lowercase() as u8).wrapping_sub(b'a').wrapping_add(1);
                Some(vec![byte])
            } else {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                Some(s.as_bytes().to_vec())
            }
        }
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::F(n) => {
            let seq = match n {
                1 => "\x1bOP",
                2 => "\x1bOQ",
                3 => "\x1bOR",
                4 => "\x1bOS",
                5 => "\x1b[15~",
                6 => "\x1b[17~",
                7 => "\x1b[18~",
                8 => "\x1b[19~",
                9 => "\x1b[20~",
                10 => "\x1b[21~",
                11 => "\x1b[23~",
                12 => "\x1b[24~",
                _ => return None,
            };
            Some(seq.as_bytes().to_vec())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_click_targets_match_expected_ranges() {
        assert_eq!(menu_action_at_column(1), Some(Action::NewWorkspace));
        assert_eq!(menu_action_at_column(8), Some(Action::NewTab));
        assert_eq!(menu_action_at_column(15), Some(Action::DeleteWorkspace));
        assert_eq!(menu_action_at_column(25), Some(Action::ShowHelp));
        assert_eq!(menu_action_at_column(33), Some(Action::Quit));
        assert_eq!(menu_action_at_column(40), None);
    }
}
