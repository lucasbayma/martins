//! Git worktree CRUD operations via git2.

#![allow(dead_code)]

use anyhow::{Context, Result};
use git2::{Repository, WorktreeAddOptions};
use std::path::PathBuf;

#[derive(Debug, Clone, thiserror::Error)]
pub enum WorktreeError {
    #[error("worktree name already exists: {0}")]
    NameExists(String),
    #[error("invalid worktree name: {0}")]
    InvalidName(String),
    #[error("git2 error: {0}")]
    Git2(String),
    #[error("I/O error: {0}")]
    Io(String),
}

impl From<git2::Error> for WorktreeError {
    fn from(e: git2::Error) -> Self {
        WorktreeError::Git2(e.message().to_string())
    }
}

#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub name: String,
    pub path: PathBuf,
    pub branch: String,
}

/// List all worktrees for the repository at `repo_path`.
pub async fn list(repo_path: PathBuf) -> Result<Vec<WorktreeInfo>> {
    tokio::task::spawn_blocking(move || {
        let repo = Repository::open(&repo_path)
            .with_context(|| format!("failed to open repo at {}", repo_path.display()))?;
        let names = repo.worktrees()?;
        let mut result = Vec::new();
        for name in names.iter().flatten() {
            if let Ok(wt) = repo.find_worktree(name) {
                let path = wt.path().to_path_buf();
                let branch = wt.name().unwrap_or(name).to_string();
                result.push(WorktreeInfo {
                    name: name.to_string(),
                    path,
                    branch,
                });
            }
        }
        Ok(result)
    })
    .await?
}

/// Create a new worktree at `{repo_parent}/{repo_name}-{name}` on a new branch.
fn default_worktree_base() -> PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join(".martins")
}

pub async fn create(
    repo_path: PathBuf,
    name: String,
    base_branch: String,
) -> Result<PathBuf, WorktreeError> {
    create_in(repo_path, name, base_branch, None).await
}

pub async fn create_in(
    repo_path: PathBuf,
    name: String,
    base_branch: String,
    base_dir: Option<PathBuf>,
) -> Result<PathBuf, WorktreeError> {
    crate::mpb::validate(&name).map_err(|e| WorktreeError::InvalidName(e.to_string()))?;

    tokio::task::spawn_blocking(move || {
        let repo = Repository::open(&repo_path).map_err(WorktreeError::from)?;

        if let Ok(names) = repo.worktrees() {
            for n in names.iter().flatten() {
                if n == name.as_str() {
                    return Err(WorktreeError::NameExists(name.clone()));
                }
            }
        }

        let repo_name = repo_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repo");
        let wt_base = base_dir.unwrap_or_else(default_worktree_base);
        std::fs::create_dir_all(&wt_base)
            .map_err(|e| WorktreeError::Io(e.to_string()))?;
        let wt_path = wt_base.join(format!("{}-{}", repo_name, name));

        let base_ref = repo
            .find_branch(&base_branch, git2::BranchType::Local)
            .map_err(WorktreeError::from)?;
        let base_commit = base_ref
            .get()
            .peel_to_commit()
            .map_err(WorktreeError::from)?;

        let branch = repo
            .branch(&name, &base_commit, false)
            .map_err(WorktreeError::from)?;

        let mut opts = WorktreeAddOptions::new();
        opts.reference(Some(branch.get()));
        repo.worktree(&name, &wt_path, Some(&opts))
            .map_err(WorktreeError::from)?;

        Ok(wt_path)
    })
    .await
    .map_err(|e| WorktreeError::Git2(e.to_string()))?
}

/// Remove a worktree (and optionally its branch).
pub async fn prune(
    repo_path: PathBuf,
    name: String,
    delete_branch: bool,
) -> Result<(), WorktreeError> {
    tokio::task::spawn_blocking(move || {
        let repo = Repository::open(&repo_path).map_err(WorktreeError::from)?;

        let wt = repo.find_worktree(&name).map_err(WorktreeError::from)?;

        let wt_path = wt.path().to_path_buf();

        let mut prune_opts = git2::WorktreePruneOptions::new();
        prune_opts.working_tree(true);
        wt.prune(Some(&mut prune_opts))
            .map_err(WorktreeError::from)?;

        if wt_path.exists() {
            std::fs::remove_dir_all(&wt_path).map_err(|e| WorktreeError::Io(e.to_string()))?;
        }

        if delete_branch {
            if let Ok(mut branch) = repo.find_branch(&name, git2::BranchType::Local) {
                let _ = branch.delete();
            }
        }

        Ok(())
    })
    .await
    .map_err(|e| WorktreeError::Git2(e.to_string()))?
}

/// Count commits in worktree branch that are ahead of base_branch.
pub async fn count_unpushed_commits(
    repo_path: PathBuf,
    worktree_name: String,
    base_branch: String,
) -> Result<usize> {
    tokio::task::spawn_blocking(move || {
        let repo = Repository::open(&repo_path)
            .with_context(|| format!("failed to open repo at {}", repo_path.display()))?;

        let base = repo
            .revparse_single(&base_branch)
            .with_context(|| format!("base branch '{}' not found", base_branch))?
            .id();
        let head = repo
            .revparse_single(&worktree_name)
            .with_context(|| format!("worktree branch '{}' not found", worktree_name))?
            .id();

        let mut revwalk = repo.revwalk()?;
        revwalk.push(head)?;
        revwalk.hide(base)?;
        let count = revwalk.count();
        Ok(count)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;
    use std::path::Path;
    use tempfile::TempDir;

    fn init_repo_with_commit(dir: &Path) -> Repository {
        let repo = Repository::init(dir).unwrap();
        let sig = git2::Signature::now("test", "test@example.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        drop(tree);
        repo
    }

    #[tokio::test]
    async fn create_worktree() {
        let tmp = TempDir::new().unwrap();
        let main_path = tmp.path().join("myrepo");
        std::fs::create_dir_all(&main_path).unwrap();
        init_repo_with_commit(&main_path);

        let repo = Repository::open(&main_path).unwrap();
        let base_branch = repo
            .head()
            .unwrap()
            .shorthand()
            .unwrap_or("master")
            .to_string();
        drop(repo);

        let wt_base = tmp.path().join("worktrees");
        let wt_path = create_in(
            main_path.clone(),
            "caetano".to_string(),
            base_branch.clone(),
            Some(wt_base),
        )
        .await;
        assert!(wt_path.is_ok(), "create failed: {:?}", wt_path);
        let wt_path = wt_path.unwrap();
        assert!(wt_path.exists(), "worktree dir should exist");
        assert!(wt_path.ends_with("myrepo-caetano"));
    }

    #[tokio::test]
    async fn duplicate_name_error() {
        let tmp = TempDir::new().unwrap();
        let main_path = tmp.path().join("myrepo2");
        std::fs::create_dir_all(&main_path).unwrap();
        init_repo_with_commit(&main_path);

        let repo = Repository::open(&main_path).unwrap();
        let base_branch = repo
            .head()
            .unwrap()
            .shorthand()
            .unwrap_or("master")
            .to_string();
        drop(repo);

        let wt_base = tmp.path().join("worktrees");
        create_in(main_path.clone(), "gil".to_string(), base_branch.clone(), Some(wt_base.clone()))
            .await
            .unwrap();
        let result = create_in(main_path.clone(), "gil".to_string(), base_branch, Some(wt_base)).await;
        assert!(matches!(result, Err(WorktreeError::NameExists(_))));
    }

    #[tokio::test]
    async fn count_ahead() {
        let tmp = TempDir::new().unwrap();
        let main_path = tmp.path().join("myrepo3");
        std::fs::create_dir_all(&main_path).unwrap();
        let repo = init_repo_with_commit(&main_path);
        let base_branch = repo
            .head()
            .unwrap()
            .shorthand()
            .unwrap_or("master")
            .to_string();
        drop(repo);

        let wt_base = tmp.path().join("worktrees");
        let wt_path = create_in(main_path.clone(), "elis".to_string(), base_branch.clone(), Some(wt_base))
            .await
            .unwrap();

        let wt_repo = Repository::open(&wt_path).unwrap();
        let sig = git2::Signature::now("test", "test@example.com").unwrap();
        for i in 0..2 {
            let tree_id = wt_repo.index().unwrap().write_tree().unwrap();
            let tree = wt_repo.find_tree(tree_id).unwrap();
            let parent = wt_repo.head().unwrap().peel_to_commit().unwrap();
            wt_repo
                .commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &format!("commit {}", i),
                    &tree,
                    &[&parent],
                )
                .unwrap();
            drop(tree);
        }
        drop(wt_repo);

        let count = count_unpushed_commits(main_path, "elis".to_string(), base_branch)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }
}
