//! File system watcher with debouncing and noise filtering.
#![allow(dead_code)]

use anyhow::Result;
use notify_debouncer_mini::notify::RecursiveMode;
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum FsEvent {
    Changed(PathBuf),
    Removed(PathBuf),
}

/// Directories to filter out from events.
const NOISE_DIRS: &[&str] = &[
    "/.git/",
    "/target/",
    "/node_modules/",
    "/.martins/",
    "/dist/",
    "/build/",
    "/.next/",
    "/.venv/",
];

fn is_noise(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    NOISE_DIRS
        .iter()
        .any(|noise| path_str.contains(noise) || path_str.ends_with(&noise[..noise.len() - 1]))
}

pub struct Watcher {
    debouncer: Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>,
    events_rx: mpsc::UnboundedReceiver<FsEvent>,
}

impl Watcher {
    pub fn new() -> Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel::<FsEvent>();
        let tx = Arc::new(tx);

        let debouncer = new_debouncer(
            Duration::from_millis(750),
            move |result: DebounceEventResult| {
                let events = match result {
                    Ok(events) => events,
                    Err(_) => return,
                };
                for event in events {
                    let path = event.path;
                    if is_noise(&path) {
                        continue;
                    }
                    // DebouncedEventKind is Any/AnyContinuous — check existence
                    // to distinguish changed vs removed.
                    let fs_event = if path.exists() {
                        FsEvent::Changed(path)
                    } else {
                        FsEvent::Removed(path)
                    };
                    let _ = tx.send(fs_event);
                }
            },
        )?;

        Ok(Self {
            debouncer,
            events_rx: rx,
        })
    }

    pub fn watch(&mut self, path: &Path) -> Result<()> {
        self.debouncer
            .watcher()
            .watch(path, RecursiveMode::Recursive)?;
        Ok(())
    }

    pub fn unwatch(&mut self, path: &Path) -> Result<()> {
        self.debouncer.watcher().unwatch(path)?;
        Ok(())
    }

    pub async fn next_event(&mut self) -> Option<FsEvent> {
        self.events_rx.recv().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::time::timeout;

    #[tokio::test]
    async fn detect_change() {
        let tmp = TempDir::new().unwrap();
        let mut watcher = Watcher::new().unwrap();
        watcher.watch(tmp.path()).unwrap();

        // Write a file
        std::thread::sleep(Duration::from_millis(100)); // let watcher settle
        std::fs::write(tmp.path().join("test.txt"), b"hello").unwrap();

        // Should receive event within 1500ms (debounce 750ms + margin)
        let event = timeout(Duration::from_millis(1500), watcher.next_event()).await;
        assert!(event.is_ok(), "timed out waiting for event");
        assert!(event.unwrap().is_some());
    }

    #[tokio::test]
    async fn filter_noise() {
        let tmp = TempDir::new().unwrap();
        let mut watcher = Watcher::new().unwrap();
        watcher.watch(tmp.path()).unwrap();

        std::thread::sleep(Duration::from_millis(100));

        // Write to .git/ and target/ — should be filtered
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), b"ref: refs/heads/main").unwrap();

        let target_dir = tmp.path().join("target");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("foo"), b"bar").unwrap();

        // Should NOT receive any events within 2s
        let event = timeout(Duration::from_millis(2000), watcher.next_event()).await;
        assert!(event.is_err(), "should not receive events for noise dirs");
    }

    #[tokio::test]
    async fn debounce_rapid() {
        let tmp = TempDir::new().unwrap();
        let mut watcher = Watcher::new().unwrap();
        watcher.watch(tmp.path()).unwrap();

        std::thread::sleep(Duration::from_millis(100));

        // Write 5 times rapidly
        for i in 0..5 {
            std::fs::write(tmp.path().join("rapid.txt"), format!("write {}", i)).unwrap();
            std::thread::sleep(Duration::from_millis(50));
        }

        // Should receive at most 2 events (debounced)
        let mut count = 0;
        let deadline = std::time::Instant::now() + Duration::from_millis(2000);
        while std::time::Instant::now() < deadline {
            let remaining = deadline - std::time::Instant::now();
            let event = timeout(remaining, watcher.next_event()).await;
            match event {
                Ok(Some(_)) => count += 1,
                _ => break,
            }
        }
        assert!(
            count <= 2,
            "expected at most 2 debounced events, got {}",
            count
        );
        assert!(count >= 1, "expected at least 1 event");
    }
}
