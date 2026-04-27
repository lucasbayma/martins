//! Workspace and project lifecycle: create, archive, delete, switch, add.
//!
//! Extracted from src/app.rs as part of the architectural split (Phase 1).
//! All functions take `&mut App` (or `&App` when read-only) and coordinate
//! state mutation with git worktree + tmux subprocess lifecycle. Preserves
//! the original call order — state mutation order matters for crash
//! recovery (see CONCERNS.md "Workspace creation is distributed").

use crate::app::App;
use crate::git::{repo, worktree};
use crate::keys::InputMode;
use crate::state::{Agent, TabSpec, WorkspaceStatus};
use crate::ui::layout;
use crate::ui::modal::{DeleteForm, Modal, NewWorkspaceForm, RemoveProjectForm};
use ratatui::layout::Rect;
use std::path::Path;

/// Reattach tmux sessions for all active workspaces on startup.
///
/// Called from App::new after the App struct is constructed. Spawns any
/// tmux sessions that have gone missing since last run, resizes existing
/// ones to match the current terminal geometry, then attaches the
/// PtyManager to each via `tmux attach-session`.
pub(crate) fn reattach_tmux_sessions(app: &mut App) {
    if !crate::tmux::is_available() {
        return;
    }

    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let frame_rect = Rect::new(0, 0, term_cols, term_rows);
    let panes = layout::compute(frame_rect, &app.layout);
    let rows = panes.terminal.height.saturating_sub(3);
    let cols = panes.terminal.width.saturating_sub(2);
    app.last_pty_size = (rows, cols);

    let existing_sessions = std::process::Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();

    let mut created_sessions = Vec::new();

    for project in &app.global_state.projects {
        for workspace in project
            .workspaces
            .iter()
            .filter(|w| !matches!(w.status, crate::state::WorkspaceStatus::Archived))
        {
            for tab in &workspace.tabs {
                let tmux_name =
                    crate::tmux::tab_session_name(&project.id, &workspace.name, tab.id);
                if !existing_sessions.contains(&tmux_name) {
                    let program = tab_program_for_resume(&tab.command);
                    let _ = crate::tmux::new_session(
                        &tmux_name,
                        &workspace.worktree_path,
                        &program,
                        cols,
                        rows,
                    );
                    created_sessions.push(tmux_name);
                }
            }
        }
    }

    if !created_sessions.is_empty() {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    for project in &app.global_state.projects {
        for workspace in project.active() {
            for tab in &workspace.tabs {
                let tmux_name =
                    crate::tmux::tab_session_name(&project.id, &workspace.name, tab.id);

                crate::tmux::enforce_session_options(&tmux_name);
                crate::tmux::resize_session(&tmux_name, cols, rows);

                if tab.command != "shell" {
                    let current = crate::tmux::pane_command(&tmux_name);
                    let is_shell = current.as_deref().is_none_or(|cmd| {
                        matches!(cmd, "bash" | "zsh" | "sh" | "fish" | "dash")
                    });
                    if is_shell {
                        let program = tab_program_for_resume(&tab.command);
                        crate::tmux::send_key(&tmux_name, &program);
                        crate::tmux::send_key(&tmux_name, "Enter");
                    }
                }

                let _ = app.pty_manager.spawn_tab(
                    project.id.clone(),
                    workspace.name.clone(),
                    tab.id,
                    workspace.worktree_path.clone(),
                    "tmux",
                    &["attach-session", "-t", &tmux_name],
                    rows,
                    cols,
                );
            }
        }
    }
}

pub async fn switch_project(app: &mut App, idx: usize) {
    if idx >= app.global_state.projects.len() {
        return;
    }

    let old_repo_root = app.active_project().map(|project| project.repo_root.clone());
    let new_repo_root = app.global_state.projects[idx].repo_root.clone();
    let new_project_id = app.global_state.projects[idx].id.clone();

    if let Some(watcher) = &mut app.watcher {
        if let Some(old_repo_root) = old_repo_root {
            let _ = watcher.unwatch(&old_repo_root);
        }
        let _ = watcher.watch(&new_repo_root);
    } else if let Ok(mut watcher) = crate::watcher::Watcher::new() {
        let _ = watcher.watch(&new_repo_root);
        app.watcher = Some(watcher);
    }

    app.active_project_idx = Some(idx);
    app.global_state.active_project_id = Some(new_project_id);
    // D-22: switching projects leaves the previous session's selection
    // meaningless. Clear BEFORE the index change so the next render
    // never observes a stale selection against the new active session.
    app.clear_selection();
    app.active_workspace_idx = app.global_state.projects[idx].active().next().map(|_| 0);
    app.set_active_tab(0);
    app.preview_lines = None;
    app.right_list.select(None);
    app.refresh_diff_spawn();
}

pub fn queue_workspace_creation(app: &mut App, form: &NewWorkspaceForm) {
    let name = (!form.name_input.is_empty()).then(|| form.name_input.clone());
    app.modal = Modal::Loading("Creating workspace...".to_string());
    app.pending_workspace = Some(name);
}

pub fn confirm_delete_workspace(app: &mut App, form: &DeleteForm) {
    let name = form.workspace_name.clone();
    if let Some(project) = app.active_project_mut() {
        project.remove(&name);
    }
    app.refresh_active_workspace_after_change();
    app.save_state_spawn();
}

pub fn archive_active_workspace(app: &mut App) {
    let Some(ws) = app.active_workspace() else { return };
    let ws_name = ws.name.clone();
    let worktree_path = ws.worktree_path.clone();
    let tab_ids: Vec<u32> = ws.tabs.iter().map(|t| t.id).collect();
    let Some(project) = app.active_project() else { return };
    let project_id = project.id.clone();

    for tab_id in &tab_ids {
        let tmux_name = crate::tmux::tab_session_name(&project_id, &ws_name, *tab_id);
        crate::tmux::kill_session(&tmux_name);
        app.pty_manager.close_tab(&project_id, &ws_name, *tab_id);
    }

    if let Some(project) = app.active_project_mut() {
        project.archive(&ws_name);
    }
    app.refresh_active_workspace_after_change();
    app.save_state_spawn();

    // BG-05 success criterion #4 (archive feels instant): worktree
    // cleanup can take hundreds of ms on node_modules/target-heavy
    // repos. Spawn on a blocking worker — fire-and-forget. By this
    // point the workspace is already unlinked from global_state, so
    // a failed cleanup leaves only a filesystem remnant (not a
    // correctness issue). See RESEARCH §17 Q3.
    let worktree = worktree_path.clone();
    tokio::task::spawn_blocking(move || {
        let _ = std::fs::remove_dir_all(&worktree);
    });
}

pub fn delete_archived_workspace(app: &mut App, project_idx: usize, archived_idx: usize) {
    let Some(project) = app.global_state.projects.get(project_idx) else { return };
    let Some(ws) = project.archived().nth(archived_idx) else { return };
    let ws_name = ws.name.clone();
    let worktree_path = ws.worktree_path.clone();

    if let Some(project) = app.global_state.projects.get_mut(project_idx) {
        project.delete_workspace(&ws_name);
    }

    // BG-05 success criterion #4: same rationale as archive_active_workspace.
    // Worktree cleanup moves off the event-loop thread; fire-and-forget
    // because the workspace is already unlinked from global_state.
    let worktree = worktree_path.clone();
    tokio::task::spawn_blocking(move || {
        let _ = std::fs::remove_dir_all(&worktree);
    });
    app.save_state_spawn();
}

pub async fn confirm_remove_project(app: &mut App, form: &RemoveProjectForm) {
    app.global_state.remove_project(&form.project_id);
    app.active_project_idx = app
        .global_state
        .active_project_id
        .as_ref()
        .and_then(|id| app.global_state.projects.iter().position(|project| &project.id == id));
    if let Some(idx) = app.active_project_idx {
        switch_project(app, idx).await;
    } else {
        // D-22: removing the last project drops every session — selection
        // anchored to a now-deleted PtySession is meaningless.
        app.clear_selection();
        app.active_workspace_idx = None;
        app.set_active_tab(0);
        app.modified_files.clear();
        app.right_list.select(None);
        app.preview_lines = None;
        app.watcher = None;
    }
    app.save_state_spawn();
}

pub async fn create_workspace(app: &mut App, name: Option<String>) -> Result<(), String> {
    let project = app
        .active_project_mut()
        .ok_or_else(|| "no active project".to_string())?;

    if let Some(ref n) = name {
        if project.workspaces.iter().any(|w| w.name == *n) {
            return Err(format!("workspace '{}' already exists", n));
        }
    }

    let repo_root = project.repo_root.clone();
    let base_branch = project.base_branch.clone();

    let ws_name = crate::agents::create_workspace_entry(project, name, Agent::default())
        .map_err(|error| error.to_string())?;

    let wt_path = worktree::create(
        repo_root,
        ws_name.clone(),
        base_branch,
    )
    .await;

    if let Err(e) = &wt_path {
        if let Some(project) = app.active_project_mut() {
            project.remove(&ws_name);
        }
        return Err(e.to_string());
    }
    let wt_path = wt_path.unwrap();

    let ws = app
        .active_project_mut()
        .and_then(|p| p.workspaces.iter_mut().find(|w| w.name == ws_name))
        .ok_or_else(|| "workspace disappeared".to_string())?;
    ws.worktree_path = wt_path;
    ws.status = WorkspaceStatus::Active;

    let active_count = app
        .active_project()
        .map(|p| p.active().count())
        .unwrap_or(0);
    // D-22: a fresh workspace has its own session — drop any prior
    // selection before advancing the workspace index.
    app.clear_selection();
    app.active_workspace_idx = active_count.checked_sub(1);
    app.set_active_tab(0);
    app.save_state_spawn();

    let _ = create_tab(app, "shell".to_string()).await;
    Ok(())
}

pub async fn create_tab(app: &mut App, command: String) -> Result<(), String> {
    let Some(project) = app.active_project() else {
        return Err("no active project".to_string());
    };
    let Some(workspace) = app.active_workspace() else {
        return Err("no active workspace".to_string());
    };

    let project_id = project.id.clone();
    let ws_name = workspace.name.clone();
    let worktree_path = workspace.worktree_path.clone();
    let next_id = workspace.tabs.iter().map(|tab| tab.id).max().map_or(0, |id| id + 1);
    let (rows, cols) = app.last_pty_size;
    let tmux_name = crate::tmux::tab_session_name(&project_id, &ws_name, next_id);
    let program = tab_program_for_new(&command);

    let tmux_name_c = tmux_name.clone();
    let worktree_c = worktree_path.clone();
    let program_c = program.clone();
    tokio::task::spawn_blocking(move || {
        crate::tmux::new_session(&tmux_name_c, &worktree_c, &program_c, cols, rows)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    if let Some(project) = app.active_project_mut()
        && let Some(workspace) = project.workspaces.iter_mut().find(|workspace| workspace.name == ws_name)
    {
        workspace.tabs.push(TabSpec {
            id: next_id,
            command: command.clone(),
        });
        let new_idx = workspace.tabs.len() - 1;
        app.set_active_tab(new_idx);
    }

    app.pty_manager
        .spawn_tab(
            project_id,
            ws_name,
            next_id,
            worktree_path,
            "tmux",
            &["attach-session", "-t", &tmux_name],
            rows,
            cols,
        )
        .map_err(|error| error.to_string())?;

    app.mode = InputMode::Terminal;
    app.save_state_spawn();
    Ok(())
}

pub async fn add_project_from_path(app: &mut App, path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("path is required".to_string());
    }

    let repo_root = repo::discover(Path::new(trimmed)).map_err(|error| error.to_string())?;
    let base_branch = repo::current_branch_async(repo_root.clone())
        .await
        .unwrap_or_else(|_| "main".to_string());
    let project_id = app.global_state.ensure_project(&repo_root, base_branch);
    let _ = crate::config::ensure_gitignore(&repo_root);
    app.global_state.active_project_id = Some(project_id.clone());
    app.active_project_idx = app
        .global_state
        .projects
        .iter()
        .position(|project| project.id == project_id);
    app.active_workspace_idx = app.active_project().and_then(|project| project.active().next().map(|_| 0));
    app.save_state_spawn();
    if let Some(idx) = app.active_project_idx {
        switch_project(app, idx).await;
    }
    Ok(())
}

pub(crate) fn tab_program_for_new(command: &str) -> String {
    if let Some(path) = command.strip_prefix("diff ") {
        let escaped = path.replace('\'', "'\\''");
        return format!(
            "(git diff --color=always -- '{escaped}' 2>/dev/null; \
             if [ -f '{escaped}' ] && ! git ls-files --error-unmatch '{escaped}' >/dev/null 2>&1; then \
             git diff --no-index --color=always /dev/null '{escaped}' 2>/dev/null; fi) | less -R"
        );
    }
    match command {
        "shell" => {
            if Path::new("/bin/zsh").exists() {
                "/bin/zsh".to_string()
            } else {
                "/bin/sh".to_string()
            }
        }
        _ => command.to_string(),
    }
}

pub(crate) fn tab_program_for_resume(command: &str) -> String {
    if command.starts_with("diff ") {
        return tab_program_for_new(command);
    }
    match command {
        "shell" => tab_program_for_new(command),
        "opencode" => "opencode -c".to_string(),
        _ => command.to_string(),
    }
}
