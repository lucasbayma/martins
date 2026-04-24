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
use crate::ui::modal::{DeleteForm, Modal, NewWorkspaceForm, RemoveProjectForm};
use std::path::Path;

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
    app.active_workspace_idx = app.global_state.projects[idx].active().next().map(|_| 0);
    app.active_tab = 0;
    app.preview_lines = None;
    app.right_list.select(None);
    app.refresh_diff().await;
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
    app.save_state();
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
    app.save_state();

    let _ = std::fs::remove_dir_all(&worktree_path);
}

pub fn delete_archived_workspace(app: &mut App, project_idx: usize, archived_idx: usize) {
    let Some(project) = app.global_state.projects.get(project_idx) else { return };
    let Some(ws) = project.archived().nth(archived_idx) else { return };
    let ws_name = ws.name.clone();
    let worktree_path = ws.worktree_path.clone();

    if let Some(project) = app.global_state.projects.get_mut(project_idx) {
        project.delete_workspace(&ws_name);
    }

    let _ = std::fs::remove_dir_all(&worktree_path);
    app.save_state();
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
        app.active_workspace_idx = None;
        app.active_tab = 0;
        app.modified_files.clear();
        app.right_list.select(None);
        app.preview_lines = None;
        app.watcher = None;
    }
    app.save_state();
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
    app.active_workspace_idx = active_count.checked_sub(1);
    app.active_tab = 0;
    app.save_state();

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
        app.active_tab = workspace.tabs.len() - 1;
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
    app.save_state();
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
    app.save_state();
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
