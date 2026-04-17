//! Git diff operations: modified file list vs base branch.

#![allow(dead_code)]

use anyhow::{Context, Result};
use git2::{Delta, Repository, StatusOptions};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub status: FileStatus,
}

#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    #[error("base branch not found: {0}")]
    BaseBranchMissing(String),
    #[error("git2 error: {0}")]
    Git2(#[from] git2::Error),
}

/// Get list of files changed vs base_branch in the worktree at `worktree_path`.
pub async fn modified_files(
    worktree_path: PathBuf,
    base_branch: String,
) -> Result<Vec<FileEntry>, DiffError> {
    tokio::task::spawn_blocking(move || {
        let repo = Repository::open(&worktree_path)?;

        let base_obj = repo
            .revparse_single(&base_branch)
            .map_err(|_| DiffError::BaseBranchMissing(base_branch.clone()))?;
        let base_commit = base_obj.peel_to_commit()?;
        let base_tree = base_commit.tree()?;

        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.include_untracked(false);

        let diff = repo.diff_tree_to_workdir_with_index(Some(&base_tree), Some(&mut diff_opts))?;

        let mut entries: Vec<FileEntry> = Vec::new();

        diff.foreach(
            &mut |delta, _| {
                let status = match delta.status() {
                    Delta::Modified => FileStatus::Modified,
                    Delta::Added => FileStatus::Added,
                    Delta::Deleted => FileStatus::Deleted,
                    Delta::Renamed => FileStatus::Renamed,
                    _ => return true,
                };
                let path = delta
                    .new_file()
                    .path()
                    .or_else(|| delta.old_file().path())
                    .map(PathBuf::from)
                    .unwrap_or_default();
                entries.push(FileEntry { path, status });
                true
            },
            None,
            None,
            None,
        )?;

        // Get untracked files via status API
        let mut status_opts = StatusOptions::new();
        status_opts.include_untracked(true);
        status_opts.exclude_submodules(true);
        status_opts.recurse_untracked_dirs(true);

        let statuses = repo.statuses(Some(&mut status_opts))?;
        for entry in statuses.iter() {
            if entry.status().contains(git2::Status::WT_NEW) {
                if let Some(path) = entry.path() {
                    entries.push(FileEntry {
                        path: PathBuf::from(path),
                        status: FileStatus::Untracked,
                    });
                }
            }
        }

        // Sort: untracked first, then alphabetical
        entries.sort_by(|a, b| {
            let a_u = matches!(a.status, FileStatus::Untracked);
            let b_u = matches!(b.status, FileStatus::Untracked);
            match (a_u, b_u) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.path.cmp(&b.path),
            }
        });

        Ok(entries)
    })
    .await
    .map_err(|e| DiffError::Git2(git2::Error::from_str(&e.to_string())))?
}

/// Check if a file is binary.
pub async fn is_binary(worktree_path: PathBuf, relative_path: PathBuf) -> Result<bool> {
    tokio::task::spawn_blocking(move || {
        let repo = Repository::open(&worktree_path)
            .with_context(|| format!("failed to open repo at {}", worktree_path.display()))?;
        let full_path = worktree_path.join(&relative_path);
        match repo.blob_path(&full_path) {
            Ok(oid) => Ok(repo.find_blob(oid)?.is_binary()),
            Err(_) => Ok(false),
        }
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn init_repo_with_commit(dir: &Path) -> (Repository, String) {
        let repo = Repository::init(dir).unwrap();
        let sig = git2::Signature::now("test", "test@example.com").unwrap();
        fs::write(dir.join("initial.txt"), b"initial").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("initial.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        {
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
                .unwrap();
        }
        let branch = repo
            .head()
            .unwrap()
            .shorthand()
            .unwrap_or("master")
            .to_string();
        (repo, branch)
    }

    #[tokio::test]
    async fn empty_diff() {
        let tmp = TempDir::new().unwrap();
        let (_, branch) = init_repo_with_commit(tmp.path());
        let files = modified_files(tmp.path().to_path_buf(), branch)
            .await
            .unwrap();
        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn full_coverage() {
        let tmp = TempDir::new().unwrap();
        let (repo, branch) = init_repo_with_commit(tmp.path());

        fs::write(tmp.path().join("initial.txt"), b"modified").unwrap();

        let mut index = repo.index().unwrap();
        fs::write(tmp.path().join("added.txt"), b"new").unwrap();
        index.add_path(Path::new("added.txt")).unwrap();
        index.write().unwrap();

        fs::write(tmp.path().join("untracked.txt"), b"untracked").unwrap();

        let files = modified_files(tmp.path().to_path_buf(), branch)
            .await
            .unwrap();

        assert!(
            files
                .iter()
                .any(|f| f.path == Path::new("initial.txt") && f.status == FileStatus::Modified)
        );
        assert!(
            files
                .iter()
                .any(|f| f.path == Path::new("added.txt") && f.status == FileStatus::Added)
        );
        assert!(
            files
                .iter()
                .any(|f| f.path == Path::new("untracked.txt") && f.status == FileStatus::Untracked)
        );
        assert!(matches!(files[0].status, FileStatus::Untracked));
    }

    #[tokio::test]
    async fn missing_base_branch() {
        let tmp = TempDir::new().unwrap();
        init_repo_with_commit(tmp.path());
        let result = modified_files(tmp.path().to_path_buf(), "nonexistent".to_string()).await;
        assert!(matches!(result, Err(DiffError::BaseBranchMissing(_))));
    }
}
