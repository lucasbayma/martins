//! Config path resolution with XDG fallback.

#![allow(dead_code)]

use directories::ProjectDirs;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub enum GitignoreAction {
    NoChange,
    Appended,
    Created,
}

/// Returns `{repo_root}/.martins/state.json`
#[allow(dead_code)]
pub fn repo_state_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".martins").join("state.json")
}

/// Returns `{repo_root}/.martins/state.json` if writable,
/// otherwise XDG data dir fallback.
pub fn repo_state_path_with_fallback(repo_root: &Path) -> PathBuf {
    let martins_dir = repo_root.join(".martins");
    if is_writable(repo_root) {
        martins_dir.join("state.json")
    } else {
        tracing::warn!("repo .martins/ not writable, using XDG fallback");
        let hash = hash_repo_path(repo_root);
        if let Some(dirs) = ProjectDirs::from("", "", "martins") {
            dirs.data_dir().join(&hash).join("state.json")
        } else {
            // Last resort: tmp
            std::env::temp_dir()
                .join("martins")
                .join(&hash)
                .join("state.json")
        }
    }
}

/// Test writability by probing `.martins/.write_probe`.
pub fn is_writable(repo_root: &Path) -> bool {
    let probe = repo_root.join(".martins").join(".write_probe");
    // Try to create dirs and write probe
    if std::fs::create_dir_all(repo_root.join(".martins")).is_err() {
        return false;
    }
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// SHA-256 of the path string, truncated to 12 hex chars.
pub fn hash_repo_path(p: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(p.to_string_lossy().as_bytes());
    let result = hasher.finalize();
    let mut hex = String::with_capacity(12);
    for b in &result[..6] {
        write!(&mut hex, "{:02x}", b).unwrap();
    }
    hex
}

/// Ensure `.martins/` is in `.gitignore`.
pub fn ensure_gitignore(repo_root: &Path) -> std::io::Result<GitignoreAction> {
    let gitignore = repo_root.join(".gitignore");
    let entry = ".martins/";

    if !gitignore.exists() {
        std::fs::write(&gitignore, format!("{}\n", entry))?;
        return Ok(GitignoreAction::Created);
    }

    let content = std::fs::read_to_string(&gitignore)?;
    // Check each line for .martins/ or /.martins/
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == ".martins/" || trimmed == "/.martins/" || trimmed == ".martins" {
            return Ok(GitignoreAction::NoChange);
        }
    }

    // Need to append
    let append = if content.ends_with('\n') {
        format!("{}\n", entry)
    } else {
        format!("\n{}\n", entry)
    };
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().append(true).open(&gitignore)?;
    file.write_all(append.as_bytes())?;
    Ok(GitignoreAction::Appended)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn writable_repo_uses_local() {
        let tmp = TempDir::new().unwrap();
        let path = repo_state_path_with_fallback(tmp.path());
        assert!(path.starts_with(tmp.path()));
        assert!(path.ends_with("state.json"));
    }

    #[test]
    fn hash_is_deterministic() {
        let p = Path::new("/some/repo/path");
        let h1 = hash_repo_path(p);
        let h2 = hash_repo_path(p);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 12);
        // All hex
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn ensure_gitignore_create() {
        let tmp = TempDir::new().unwrap();
        let action = ensure_gitignore(tmp.path()).unwrap();
        assert!(matches!(action, GitignoreAction::Created));
        let content = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains(".martins/"));
    }

    #[test]
    fn ensure_gitignore_noop() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".gitignore"), ".martins/\n").unwrap();
        let action = ensure_gitignore(tmp.path()).unwrap();
        assert!(matches!(action, GitignoreAction::NoChange));
    }

    #[test]
    fn ensure_gitignore_append() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".gitignore"), "target/\n").unwrap();
        let action = ensure_gitignore(tmp.path()).unwrap();
        assert!(matches!(action, GitignoreAction::Appended));
        let content = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains("target/"));
        assert!(content.contains(".martins/"));
    }
}
