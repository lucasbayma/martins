mod agents;
mod app;
mod config;
mod editor;
mod error;
mod git;
mod keys;
mod logging;
pub mod mpb;
mod pty;
mod state;
mod tmux;
mod tools;
mod ui;
mod watcher;

use anyhow::Result;
use crossterm::{event::{DisableMouseCapture, EnableMouseCapture}, execute};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let log_dir = config::global_log_dir();
    let _ = logging::init_logging(&log_dir);
    logging::install_panic_hook();

    let state_path = config::global_state_path();
    let mut global_state = state::GlobalState::load(&state_path).unwrap_or_default();

    let cwd = std::env::current_dir()?;
    if let Ok(repo_root) = crate::git::repo::discover(&cwd) {
        let base_branch = crate::git::repo::current_branch_async(repo_root.clone())
            .await
            .unwrap_or_else(|_| "main".to_string());
        let project_id = global_state.ensure_project(&repo_root, base_branch);
        let _ = config::ensure_gitignore(&repo_root);
        if global_state.active_project_id.is_none() {
            global_state.active_project_id = Some(project_id);
        }
    }

    if let Some(path) = std::env::args().nth(1) {
        let repo_root = crate::git::repo::discover(&PathBuf::from(path))?;
        let base_branch = crate::git::repo::current_branch_async(repo_root.clone())
            .await
            .unwrap_or_else(|_| "main".to_string());
        let project_id = global_state.ensure_project(&repo_root, base_branch);
        let _ = config::ensure_gitignore(&repo_root);
        global_state.active_project_id = Some(project_id);
    }

    let mut terminal = ratatui::init();
    execute!(std::io::stdout(), EnableMouseCapture)?;

    let result = match app::App::new(global_state, state_path).await {
        Ok(mut app) => app.run(&mut terminal).await,
        Err(error) => Err(error),
    };

    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}
