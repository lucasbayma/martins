//! Unit tests for `crate::app`. Declared as `#[path]` module from src/app.rs
//! so the tests live in a sibling file and keep src/app.rs focused on the
//! App struct and run loop.

use super::*;
use crate::state::{Agent, Project, TabSpec};
use git2::Repository;
use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn init_repo(dir: &Path) -> Project {
    let repo = Repository::init(dir).unwrap();
    let sig = git2::Signature::now("test", "test@example.com").unwrap();
    std::fs::write(dir.join("initial.txt"), b"initial").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("initial.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .unwrap();
    let branch = repo.head().unwrap().shorthand().unwrap_or("main").to_string();
    Project::new(dir.to_path_buf(), branch)
}

#[tokio::test]
async fn app_new_without_git_repo() {
    let app = App::new(GlobalState::default(), std::env::temp_dir().join("martins-test.json"))
        .await
        .unwrap();
    assert_eq!(app.active_project_idx, None);
    assert!(!app.should_quit);
}

#[tokio::test]
async fn switch_project_updates_context() {
    let tmp1 = TempDir::new().unwrap();
    let tmp2 = TempDir::new().unwrap();
    let mut state = GlobalState::default();
    let project1 = init_repo(tmp1.path());
    let project2 = init_repo(tmp2.path());
    state.active_project_id = Some(project1.id.clone());
    state.projects.push(project1);
    state.projects.push(project2.clone());

    let mut app = App::new(state, std::env::temp_dir().join("martins-switch.json"))
        .await
        .unwrap();
    crate::workspace::switch_project(&mut app, 1).await;
    assert_eq!(app.active_project_idx, Some(1));
    assert_eq!(app.active_project().map(|p| p.id.as_str()), Some(project2.id.as_str()));
}

#[tokio::test]
async fn tab_click_detects_select_close_and_add() {
    let tmp = TempDir::new().unwrap();
    let mut state = GlobalState::default();
    let mut project = init_repo(tmp.path());
    project.add_workspace(Workspace {
        name: "caetano".to_string(),
        worktree_path: tmp.path().join("caetano"),
        base_branch: "main".to_string(),
        agent: Agent::Opencode,
        status: crate::state::WorkspaceStatus::Active,
        created_at: "2024-01-01T00:00:00Z".to_string(),
        tabs: vec![
            TabSpec { id: 0, command: "opencode".to_string() },
            TabSpec { id: 1, command: "shell".to_string() },
        ],
    });
    state.active_project_id = Some(project.id.clone());
    state.projects.push(project);

    let app = App::new(state, std::env::temp_dir().join("martins-tab-click.json"))
        .await
        .unwrap();
    let terminal = Rect { x: 0, y: 0, width: 80, height: 20 };

    assert_eq!(app.tab_at_column(terminal, 1), Some(TabClick::Select(0)));
    assert_eq!(app.tab_at_column(terminal, 10), Some(TabClick::Close(0)));
    assert_eq!(app.tab_at_column(terminal, 13), Some(TabClick::Select(1)));
    assert_eq!(app.tab_at_column(terminal, 19), Some(TabClick::Close(1)));
    assert_eq!(app.tab_at_column(terminal, 21), Some(TabClick::Add));
}

#[tokio::test]
async fn app_starts_dirty() {
    let app = App::new(
        GlobalState::default(),
        std::env::temp_dir().join("martins-dirty-start.json"),
    )
    .await
    .unwrap();
    assert!(app.dirty, "first frame must render");
}

#[tokio::test]
async fn dirty_stays_clear_when_no_mutation() {
    let mut app = App::new(
        GlobalState::default(),
        std::env::temp_dir().join("martins-dirty-clear.json"),
    )
    .await
    .unwrap();
    app.dirty = false;
    // no mutation
    assert!(!app.dirty);
}

#[tokio::test]
async fn mark_dirty_sets_flag() {
    let mut app = App::new(
        GlobalState::default(),
        std::env::temp_dir().join("martins-dirty-mark.json"),
    )
    .await
    .unwrap();
    app.dirty = false;
    app.mark_dirty();
    assert!(app.dirty);
}

/// BG-05 LOAD-BEARING — `App::save_state_spawn()` returns in <5ms even on a
/// pathological-size `GlobalState` (100 projects). The serialize + fs::write +
/// atomic rename runs on `tokio::task::spawn_blocking`; results do not feed
/// back into App state.
///
/// Plan 05-01 writes this test as a FAILING regression guard — `cargo build
/// --tests` FAILS today with `no method named save_state_spawn found for
/// struct App`. That compile error IS the TDD gate for Plan 05-02. Do NOT
/// stub `save_state_spawn` to silence the error.
///
/// After Plan 05-02 lands, this test must pass. Budget: 5ms (tighter than
/// Phase 4's 50ms because spawn_blocking dispatch is pure channel-send +
/// clone, with no git2 floor).
///
/// See: .planning/phases/05-background-work-decoupling/05-PATTERNS.md
/// §"src/app_tests.rs" + 05-RESEARCH.md §12 line 440.
#[tokio::test]
async fn save_state_spawn_is_nonblocking() {
    let tmp = TempDir::new().expect("TempDir");
    let mut state = GlobalState::default();
    for i in 0..100 {
        state.add_project(&tmp.path().join(format!("repo-{i}")), "main".to_string());
    }

    let state_path = std::env::temp_dir().join("martins-bg-save-spawn.json");
    let _ = std::fs::remove_file(&state_path);
    let app = App::new(state, state_path).await.expect("App::new");

    let before = Instant::now();
    app.save_state_spawn();
    let elapsed = before.elapsed();

    assert!(
        elapsed < Duration::from_millis(5),
        "save_state_spawn returned in {elapsed:?} — must be <5ms (did it block on fs::write?). \
         If this fails, someone reintroduced the sync save call path in Plan 05-02's refactor."
    );
}
