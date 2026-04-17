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
mod tools;
mod ui;
mod watcher;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let repo_root = crate::git::repo::discover(&cwd).unwrap_or_else(|_| cwd.clone());

    let log_dir = repo_root.join(".martins").join("logs");
    let _ = logging::init_logging(&log_dir);
    logging::install_panic_hook();

    let _ = config::ensure_gitignore(&repo_root);

    let mut terminal = ratatui::init();
    let result = match app::App::new(repo_root).await {
        Ok(mut app) => app.run(&mut terminal).await,
        Err(error) => Err(error),
    };

    ratatui::restore();
    result
}
