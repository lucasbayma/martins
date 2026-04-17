//! Git repository discovery and branch operations.

#![allow(dead_code)]

use anyhow::{Context, Result};
use git2::Repository;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("not a git repository: {0}")]
    NotARepository(PathBuf),
    #[error("bare repository not supported")]
    BareRepository,
    #[error("git2 error: {0}")]
    Git2(#[from] git2::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Discover the root of the git repository containing `start`.
pub fn discover(start: &Path) -> Result<PathBuf, GitError> {
    let repo =
        Repository::discover(start).map_err(|_| GitError::NotARepository(start.to_path_buf()))?;
    let workdir = repo.workdir().ok_or(GitError::BareRepository)?;
    Ok(workdir.to_path_buf())
}

/// Open a repository at `path`.
pub fn open(path: &Path) -> Result<Repository, GitError> {
    Ok(Repository::open(path)?)
}

/// Get the current branch name (or short commit hash if detached HEAD).
pub fn current_branch(repo: &Repository) -> Result<String, GitError> {
    let head = repo.head()?;
    if head.is_branch() {
        Ok(head.shorthand().unwrap_or("HEAD").to_string())
    } else {
        // Detached HEAD: return 8-char hash
        let oid = head
            .target()
            .ok_or_else(|| git2::Error::from_str("HEAD has no target"))?;
        Ok(oid.to_string()[..8].to_string())
    }
}

/// Async wrapper: get current branch name using spawn_blocking.
pub async fn current_branch_async(path: PathBuf) -> Result<String> {
    tokio::task::spawn_blocking(move || {
        let repo = Repository::open(&path)
            .with_context(|| format!("failed to open repo at {}", path.display()))?;
        current_branch(&repo).map_err(anyhow::Error::from)
    })
    .await?
}

/// Returns true if the repository is bare.
pub fn is_bare(repo: &Repository) -> bool {
    repo.is_bare()
}

/// Given a worktree path, resolve to the main repository root.
/// For linked worktrees, `repo.path()` points at `<main>/.git/worktrees/<name>/`.
pub fn main_repo_root(worktree_path: &Path) -> Result<PathBuf, GitError> {
    let repo = Repository::open(worktree_path)?;
    if repo.is_worktree() {
        let root = repo
            .path()
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or_else(|| git2::Error::from_str("cannot determine main repo root"))?;
        Ok(root.to_path_buf())
    } else {
        let root = repo
            .path()
            .parent()
            .ok_or_else(|| git2::Error::from_str("cannot determine main repo root"))?;
        Ok(root.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_repo(dir: &Path) -> Repository {
        let repo = Repository::init(dir).unwrap();
        // Create initial commit so HEAD is valid
        let sig = git2::Signature::now("test", "test@example.com").unwrap();
        let tree_id = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        drop(tree);
        repo
    }

    #[test]
    fn discover_nested() {
        let tmp = TempDir::new().unwrap();
        init_repo(tmp.path());
        let nested = tmp.path().join("a/b/c");
        std::fs::create_dir_all(&nested).unwrap();
        let root = discover(&nested).unwrap();
        // The discovered root should be the TempDir path (canonicalized)
        assert_eq!(
            root.canonicalize().unwrap(),
            tmp.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn discover_non_repo() {
        let tmp = TempDir::new().unwrap();
        // No git init — should fail
        let result = discover(tmp.path());
        assert!(matches!(result, Err(GitError::NotARepository(_))));
    }

    #[test]
    fn current_branch_normal() {
        let tmp = TempDir::new().unwrap();
        let repo = init_repo(tmp.path());
        let branch = current_branch(&repo).unwrap();
        // Default branch is "master" or "main" depending on git config
        assert!(!branch.is_empty());
        assert!(branch == "master" || branch == "main" || branch.len() == 8);
    }

    #[test]
    fn detached_head_returns_hash() {
        let tmp = TempDir::new().unwrap();
        let repo = init_repo(tmp.path());
        // Get the HEAD commit OID
        let head_oid = repo.head().unwrap().target().unwrap();
        // Detach HEAD
        repo.set_head_detached(head_oid).unwrap();
        let branch = current_branch(&repo).unwrap();
        assert_eq!(branch.len(), 8, "detached HEAD should return 8-char hash");
        // Should be valid hex
        assert!(branch.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn is_bare_false_for_normal() {
        let tmp = TempDir::new().unwrap();
        let repo = init_repo(tmp.path());
        assert!(!is_bare(&repo));
    }

    #[test]
    fn worktree_main_repo() {
        let tmp = TempDir::new().unwrap();
        let main_path = tmp.path().join("main-repo");
        std::fs::create_dir_all(&main_path).unwrap();
        let repo = init_repo(&main_path);

        // Create a worktree
        let wt_path = tmp.path().join("worktree-1");
        // Use git CLI to create worktree (git2 worktree API is complex)
        let status = Command::new("git")
            .args([
                "-C",
                main_path.to_str().unwrap(),
                "worktree",
                "add",
                wt_path.to_str().unwrap(),
                "-b",
                "wt-branch",
            ])
            .status();

        if status.is_ok() && status.unwrap().success() {
            let root = main_repo_root(&wt_path).unwrap();
            assert_eq!(
                root.canonicalize().unwrap(),
                main_path.canonicalize().unwrap()
            );
        } else {
            // git worktree not available or failed — skip gracefully
            drop(repo);
        }
    }
}
