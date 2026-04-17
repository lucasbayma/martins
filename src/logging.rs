//! Tracing subscriber setup with file rotation and panic hook.

use std::path::Path;

use anyhow::Result;
use tracing_appender::rolling;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize file-only tracing subscriber.
/// Logs go to `{log_dir}/martins-YYYY-MM-DD.log`.
/// Console output is suppressed (would corrupt TUI).
pub fn init_logging(log_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(log_dir)?;

    let file_appender = rolling::daily(log_dir, "martins");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    Box::leak(Box::new(guard));

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if cfg!(debug_assertions) {
            EnvFilter::new("debug")
        } else {
            EnvFilter::new("info")
        }
    });

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .try_init();

    tracing::info!("starting martins v{}", env!("CARGO_PKG_VERSION"));
    Ok(())
}

/// Install a panic hook that logs the panic message before unwinding.
/// Also attempts to restore terminal raw mode (best-effort).
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {}", info);
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stderr(), crossterm::terminal::LeaveAlternateScreen);
        default_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn init_creates_rolling_file() {
        let tmp = TempDir::new().unwrap();
        init_logging(tmp.path()).unwrap();
        tracing::info!("starting martins v0.1.0");
        std::thread::sleep(std::time::Duration::from_millis(50));

        let entries: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(!entries.is_empty(), "expected at least one log file");
        let log_file = &entries[0];
        let name = log_file.file_name();
        let name_str = name.to_string_lossy();
        assert!(
            name_str.starts_with("martins"),
            "log file should start with 'martins', got: {name_str}"
        );
    }

    #[test]
    fn panic_hook_installs() {
        install_panic_hook();
        std::panic::set_hook(Box::new(|_| {}));
    }
}
