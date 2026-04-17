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

fn main() {
    let _ = logging::init_logging(std::path::Path::new("logs"));
    logging::install_panic_hook();
    tracing::info!("martins {}", env!("CARGO_PKG_VERSION"));
}
