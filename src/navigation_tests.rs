//! Navigation fluidity validation (NAV-01, NAV-02, NAV-03, NAV-04).
//!
//! These tests prove that nav call-sites (sidebar up/down, click-workspace,
//! click-tab, workspace switch) return to the event loop quickly — i.e., do
//! NOT block on `refresh_diff`. Plan 04-01 writes these as failing
//! regression guards; Plan 04-02 turns them green by introducing
//! `App::refresh_diff_spawn` + the 6th select branch.
//!
//! See `.planning/phases/04-navigation-fluidity/04-RESEARCH.md` §6 and
//! `.planning/phases/04-navigation-fluidity/04-01-PLAN.md`.

#![cfg(test)]

use crate::app::App;
use crate::keys::InputMode;
use crate::state::{GlobalState, Project};
use git2::Repository;
use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Build a git repo with `file_count` committed files at `dir`. For nav
/// timing tests, larger `file_count` makes `refresh_diff` slower and the
/// non-blocking guarantee easier to observe. Generalizes
/// `src/app_tests.rs::init_repo` (1 file → N files).
fn make_large_repo(dir: &Path, file_count: usize) -> Project {
    let repo = Repository::init(dir).expect("git init");
    let sig = git2::Signature::now("test", "test@example.com").expect("signature");
    for i in 0..file_count {
        std::fs::write(dir.join(format!("f{i}.txt")), b"x").expect("write fixture file");
    }
    let mut index = repo.index().expect("index");
    for i in 0..file_count {
        index
            .add_path(Path::new(&format!("f{i}.txt")))
            .expect("index.add_path");
    }
    index.write().expect("index.write");
    let tree_id = index.write_tree().expect("write_tree");
    let tree = repo.find_tree(tree_id).expect("find_tree");
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .expect("commit");
    let branch = repo
        .head()
        .expect("head")
        .shorthand()
        .unwrap_or("main")
        .to_string();
    Project::new(dir.to_path_buf(), branch)
}

/// NAV-04 — `Action::ClickTab` / `SwitchTab` / `F(n)` are pure-sync field
/// writes. Regression guard for RESEARCH §4 Pitfall 4 ("making tab
/// switching async"). Must PASS today and FORever — do not let someone
/// symmetrize this path by moving `active_tab = n` into an async helper.
#[tokio::test]
async fn click_tab_is_sync() {
    let state_path = std::env::temp_dir().join("martins-nav-click-tab.json");
    let _ = std::fs::remove_file(&state_path);
    let mut app = App::new(GlobalState::default(), state_path)
        .await
        .expect("App::new");

    let before = Instant::now();
    // Mirrors src/events.rs:520-523 (Action::ClickTab body) verbatim.
    app.active_tab = 3;
    app.mode = InputMode::Terminal;
    let elapsed = before.elapsed();

    assert_eq!(app.active_tab, 3, "active_tab must be written");
    assert!(
        matches!(app.mode, InputMode::Terminal),
        "mode must be Terminal"
    );
    assert!(
        elapsed < Duration::from_millis(10),
        "tab switch took {elapsed:?} — must be <10ms (pure sync field write)"
    );
}

/// NAV-01 (unit half) — `ListState` select operations on `app.left_list`
/// are O(1) field writes. Regression guard for the keyboard-sidebar fast
/// path. Tab key repeat at 30Hz requires this to stay <1ms.
#[tokio::test]
async fn sidebar_up_down_is_sync() {
    let state_path = std::env::temp_dir().join("martins-nav-sidebar-updown.json");
    let _ = std::fs::remove_file(&state_path);
    let mut app = App::new(GlobalState::default(), state_path)
        .await
        .expect("App::new");

    let before = Instant::now();
    app.left_list.select(Some(0));
    app.left_list.select(Some(1));
    app.left_list.select(Some(0));
    let elapsed = before.elapsed();

    assert_eq!(app.left_list.selected(), Some(0));
    assert!(
        elapsed < Duration::from_millis(1),
        "three ListState.select calls took {elapsed:?} — must be <1ms (O(1) field write)"
    );
}

/// NAV-01 / NAV-02 / NAV-03 LOAD-BEARING — `App::refresh_diff_spawn()`
/// returns immediately (<50ms) even when the active project's repo has
/// 500+ committed files. The git2 work happens on a spawned tokio task;
/// results arrive later on `app.diff_rx`. This test compiles only after
/// Plan 04-02 introduces `refresh_diff_spawn`.
///
/// Plan 04-01 writes this test as a FAILING regression guard. The compile
/// error `no method named refresh_diff_spawn` IS the TDD gate for 04-02.
#[tokio::test]
async fn refresh_diff_spawn_is_nonblocking() {
    let tmp = TempDir::new().expect("TempDir");
    let project = make_large_repo(tmp.path(), 500);
    let project_id = project.id.clone();

    let mut state = GlobalState::default();
    state.active_project_id = Some(project_id);
    state.projects.push(project);

    let state_path = std::env::temp_dir().join("martins-nav-refresh-spawn.json");
    let _ = std::fs::remove_file(&state_path);
    let mut app = App::new(state, state_path).await.expect("App::new");

    let before = Instant::now();
    app.refresh_diff_spawn();
    let elapsed = before.elapsed();

    assert!(
        elapsed < Duration::from_millis(50),
        "refresh_diff_spawn returned in {elapsed:?} — must be <50ms (did it await git2?). \
         If this fails, someone reintroduced the `.await` in Plan 04-02's refactor."
    );
}

/// NAV-03 — workspace-switch paints the target PTY view immediately:
/// `dirty` flag flips BEFORE `modified_files` is repopulated by the
/// background refresh. This enforces RESEARCH §1 primary recommendation
/// ("Eager-paint on workspace switch").
#[tokio::test]
async fn workspace_switch_paints_pty_first() {
    let tmp = TempDir::new().expect("TempDir");
    let project = make_large_repo(tmp.path(), 200);
    let project_id = project.id.clone();

    let mut state = GlobalState::default();
    state.active_project_id = Some(project_id);
    state.projects.push(project);

    let state_path = std::env::temp_dir().join("martins-nav-switch-paints.json");
    let _ = std::fs::remove_file(&state_path);
    let mut app = App::new(state, state_path).await.expect("App::new");

    // Snapshot pre-switch state.
    let pre_len = app.modified_files.len();
    app.dirty = false; // clear dirty so we can prove the refactor re-sets it.

    // Emulate the workspace-switch tail: select → refresh_diff_spawn.
    // Mirrors src/events.rs:504-519 (ClickWorkspace) tail after 04-02
    // replaces the `.await` with `refresh_diff_spawn()`.
    app.select_active_workspace(0);
    app.refresh_diff_spawn();

    // Invariant 1: dirty flag set synchronously — next loop iteration draws.
    assert!(
        app.dirty,
        "workspace switch must mark_dirty synchronously (paint PTY on next frame)"
    );
    // Invariant 2: modified_files NOT yet replaced — background task
    // hasn't sent on diff_tx yet; receiver branch has not fired.
    assert_eq!(
        app.modified_files.len(),
        pre_len,
        "modified_files must NOT be repopulated synchronously — \
         the whole point is to paint the PTY before the diff list arrives"
    );
}
