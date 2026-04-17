mod agents;
mod app;
mod config;
mod editor;
mod error;
mod git;
mod keys;
mod mpb;
mod pty;
mod state;
mod tools;
mod ui;
mod watcher;

fn main() {
    println!("martins {}", env!("CARGO_PKG_VERSION"));
}
