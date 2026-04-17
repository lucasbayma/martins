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
    let repo_root = match crate::git::repo::discover(&cwd) {
        Ok(root) => root,
        Err(_) => {
            eprintln!("error: martins must be run from inside a git repository");
            eprintln!("hint: run `git init` to create a new repository here");
            std::process::exit(2);
        }
    };

    let log_dir = repo_root.join(".martins").join("logs");
    let _ = logging::init_logging(&log_dir);
    logging::install_panic_hook();

    let _ = config::ensure_gitignore(&repo_root);

    let mut terminal = ratatui::init();
    let result = match app::App::new(repo_root).await {
        Ok(mut app) => {
            let missing = crate::tools::preflight();
            if !missing.tools.is_empty() {
                app.modal =
                    crate::ui::modal::Modal::InstallMissing(crate::ui::modal::InstallForm {
                        missing_tools: missing.tools,
                        confirmed: false,
                    });
            }
            app.run(&mut terminal).await
        }
        Err(error) => Err(error),
    };

    ratatui::restore();
    result
}
