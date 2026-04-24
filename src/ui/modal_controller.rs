//! Modal dialog key + click dispatch.
//!
//! Extracted from src/app.rs as part of the architectural split (Phase 1).
//! Owns the state machine that drives modal input routing; geometry helpers
//! for modal button hit-testing live here since they have no other consumer.

use crate::app::App;
use crate::ui::modal::{self, Modal};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;

pub async fn handle_modal_key(app: &mut App, key: KeyEvent) {
    let modal = std::mem::take(&mut app.modal);
    match modal {
        Modal::None => {}
        Modal::NewWorkspace(mut form) => match key.code {
            KeyCode::Esc => app.modal = Modal::None,
            KeyCode::Enter => {
                app.queue_workspace_creation(&form);
            }
            KeyCode::Backspace => {
                form.name_input.pop();
                form.error = None;
                app.modal = Modal::NewWorkspace(form);
            }
            KeyCode::Char(c) => {
                form.name_input.push(c);
                form.error = None;
                app.modal = Modal::NewWorkspace(form);
            }
            _ => app.modal = Modal::NewWorkspace(form),
        },
        Modal::AddProject(mut form) => match key.code {
            KeyCode::Esc => app.modal = Modal::None,
            KeyCode::Enter => {
                if let Some(entry) = form.selected_entry().cloned() {
                    if entry.is_git_repo {
                        let path = entry.path.to_string_lossy().to_string();
                        match app.add_project_from_path(path).await {
                            Ok(()) => app.modal = Modal::None,
                            Err(error) => {
                                form.error = Some(error);
                                app.modal = Modal::AddProject(form);
                            }
                        }
                    } else {
                        form.navigate_into(form.selected);
                        app.modal = Modal::AddProject(form);
                    }
                } else {
                    form.navigate_up();
                    app.modal = Modal::AddProject(form);
                }
            }
            KeyCode::Backspace => {
                form.navigate_up();
                app.modal = Modal::AddProject(form);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                form.move_selection(1);
                app.modal = Modal::AddProject(form);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                form.move_selection(-1);
                app.modal = Modal::AddProject(form);
            }
            _ => app.modal = Modal::AddProject(form),
        },
        Modal::ConfirmDelete(form) => match key.code {
            KeyCode::Esc => app.modal = Modal::None,
            KeyCode::Enter => {
                app.confirm_delete_workspace(&form);
                app.modal = Modal::None;
            }
            _ => app.modal = Modal::ConfirmDelete(form),
        },
        Modal::ConfirmQuit => match key.code {
            KeyCode::Esc => app.modal = Modal::None,
            KeyCode::Enter => {
                app.modal = Modal::None;
                app.should_quit = true;
            }
            _ => app.modal = Modal::ConfirmQuit,
        },
        Modal::ConfirmArchive(form) => match key.code {
            KeyCode::Esc => app.modal = Modal::None,
            KeyCode::Enter => {
                if let Some(project) = app.active_project_mut() {
                    project.archive(&form.workspace_name);
                }
                app.modal = Modal::None;
                app.refresh_active_workspace_after_change();
                app.save_state();
            }
            _ => app.modal = Modal::ConfirmArchive(form),
        },
        Modal::ConfirmRemoveProject(form) => match key.code {
            KeyCode::Esc => app.modal = Modal::None,
            KeyCode::Enter => {
                app.confirm_remove_project(&form).await;
                app.modal = Modal::None;
            }
            _ => app.modal = Modal::ConfirmRemoveProject(form),
        },
        Modal::CommandArgs(mut form) => match key.code {
            KeyCode::Esc => app.modal = Modal::None,
            KeyCode::Enter => {
                let command = if form.args_input.trim().is_empty() {
                    form.agent.clone()
                } else {
                    format!("{} {}", form.agent, form.args_input.trim())
                };
                if let Err(error) = app.create_tab(command).await {
                    tracing::error!("failed to create tab: {error}");
                }
            }
            KeyCode::Backspace => {
                form.args_input.pop();
                app.modal = Modal::CommandArgs(form);
            }
            KeyCode::Char(c) => {
                form.args_input.push(c);
                app.modal = Modal::CommandArgs(form);
            }
            _ => app.modal = Modal::CommandArgs(form),
        },
        Modal::Help => {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')) {
                app.modal = Modal::None;
            } else {
                app.modal = Modal::Help;
            }
        }
        Modal::Loading(_) => {}
    }
}

pub async fn handle_modal_click(app: &mut App, col: u16, row: u16) -> bool {
    let frame_area = app.last_frame_area;
    if frame_area.width == 0 || frame_area.height == 0 {
        return false;
    }

    match app.modal.clone() {
        Modal::AddProject(form) => {
            let modal_area = modal::centered_rect(60, 70, frame_area);
            if !rect_contains(modal_area, col, row) {
                app.modal = Modal::None;
                return true;
            }

            let inner_y = modal_area.y + 1;
            let inner_height = modal_area.height.saturating_sub(2);

            let footer_height: u16 = if form.error.is_some() { 3 } else { 2 };
            let list_y = inner_y + 2;
            let list_height = inner_height.saturating_sub(2 + footer_height) as usize;

            if row < list_y || row >= list_y + list_height as u16 {
                return true;
            }

            let click_row = (row - list_y) as usize;

            let scroll_offset = if form.selected >= list_height {
                form.selected - list_height + 1
            } else {
                0
            };

            let has_parent_row = scroll_offset == 0;
            if has_parent_row && click_row == 0 {
                let mut form = form.clone();
                form.navigate_up();
                app.modal = Modal::AddProject(form);
                return true;
            }

            let entry_offset = if has_parent_row { click_row - 1 } else { click_row };
            let entry_idx = scroll_offset + entry_offset;

            let entry = form.entries.get(entry_idx).cloned();
            let mut form = form.clone();

            if let Some(entry) = entry {
                if entry.is_git_repo {
                    let path = entry.path.to_string_lossy().to_string();
                    match app.add_project_from_path(path).await {
                        Ok(()) => app.modal = Modal::None,
                        Err(error) => {
                            form.error = Some(error);
                            app.modal = Modal::AddProject(form);
                        }
                    }
                } else {
                    form.navigate_into(entry_idx);
                    app.modal = Modal::AddProject(form);
                }
            } else {
                form.selected = entry_idx.min(form.entries.len().saturating_sub(1));
                app.modal = Modal::AddProject(form);
            }

            true
        }
        Modal::ConfirmQuit => {
            let modal_area = modal::centered_rect(40, 30, frame_area);
            if !rect_contains(modal_area, col, row) {
                app.modal = Modal::None;
                return true;
            }

            if row == modal_button_row_y(modal_area) {
                if is_modal_first_button(modal_area, col, 14) {
                    app.modal = Modal::None;
                    app.should_quit = true;
                } else {
                    app.modal = Modal::None;
                }
            }
            true
        }
        Modal::ConfirmArchive(form) => {
            let modal_area = modal::centered_rect(50, 30, frame_area);
            if !rect_contains(modal_area, col, row) {
                app.modal = Modal::None;
                return true;
            }

            if row == modal_button_row_y(modal_area) {
                if is_modal_first_button(modal_area, col, 17) {
                    if let Some(project) = app.active_project_mut() {
                        project.archive(&form.workspace_name);
                    }
                    app.refresh_active_workspace_after_change();
                    app.save_state();
                }
                app.modal = Modal::None;
            }
            true
        }
        Modal::ConfirmDelete(form) => {
            let modal_area = modal::centered_rect(50, 40, frame_area);
            if !rect_contains(modal_area, col, row) {
                app.modal = Modal::None;
                return true;
            }

            if row == modal_button_row_y(modal_area) {
                if is_modal_first_button(modal_area, col, 12) {
                    app.confirm_delete_workspace(&form);
                }
                app.modal = Modal::None;
            }
            true
        }
        Modal::ConfirmRemoveProject(form) => {
            let modal_area = modal::centered_rect(50, 35, frame_area);
            if !rect_contains(modal_area, col, row) {
                app.modal = Modal::None;
                return true;
            }

            if row == modal_button_row_y(modal_area) {
                if is_modal_first_button(modal_area, col, 16) {
                    app.confirm_remove_project(&form).await;
                }
                app.modal = Modal::None;
            }
            true
        }
        Modal::Help => {
            let modal_area = modal::centered_rect(70, 80, frame_area);
            app.modal = Modal::None;
            let _ = rect_contains(modal_area, col, row);
            true
        }
        Modal::NewWorkspace(form) => {
            let modal_area = modal::centered_rect(50, 30, frame_area);
            if !rect_contains(modal_area, col, row) {
                app.modal = Modal::None;
                return true;
            }

            if row == modal_button_row_y(modal_area) {
                if is_modal_first_button(modal_area, col, 12) {
                    app.queue_workspace_creation(&form);
                } else {
                    app.modal = Modal::None;
                }
            } else {
                app.modal = Modal::NewWorkspace(form);
            }
            true
        }
        Modal::CommandArgs(form) => {
            let modal_area = modal::centered_rect(50, 30, frame_area);
            if !rect_contains(modal_area, col, row) {
                app.modal = Modal::None;
                return true;
            }

            if row == modal_button_row_y(modal_area) {
                if is_modal_first_button(modal_area, col, 12) {
                    let command = if form.args_input.trim().is_empty() {
                        form.agent.clone()
                    } else {
                        format!("{} {}", form.agent, form.args_input.trim())
                    };
                    if let Err(error) = app.create_tab(command).await {
                        tracing::error!("failed to create tab: {error}");
                    }
                } else {
                    app.modal = Modal::None;
                }
            } else {
                app.modal = Modal::CommandArgs(form);
            }
            true
        }
        _ => false,
    }
}

pub fn modal_button_row_y(modal_area: Rect) -> u16 {
    modal_area.y + modal_area.height.saturating_sub(2)
}

pub fn is_modal_first_button(modal_area: Rect, col: u16, width: u16) -> bool {
    let inner_x = modal_area.x + 1;
    col >= inner_x && col < inner_x + width
}

fn rect_contains(rect: Rect, col: u16, row: u16) -> bool {
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}
