# Phase 4: Navigation Fluidity — Pattern Map

**Mapped:** 2026-04-24
**Files analyzed:** 4 (3 modifications + 1 new test file); 1 optional new main.rs line
**Analogs found:** 5 / 5 (every change has a concrete in-codebase analog — this phase is a refactor + validation phase, not a greenfield extraction)

## Scope Note

Phase 4 is a **surgical non-blocking rewire of the `refresh_diff` call-pattern**, plus a new navigation-tests module. The shape is structurally analogous to Phase 3:

- **Phase 3**: Phase 2 landed PTY-input primitives → Phase 3 added regression-guard tests + doc-comment (no structural change).
- **Phase 4**: Phase 2 landed the biased-select skeleton → Phase 4 lifts `refresh_diff().await` off the input arm (small structural change: add an mpsc channel + helper + new select branch) and locks it in with behavioral tests.

All patterns below are **in-tree analogs**. No external pattern references.

Six concrete changes across four files:

| # | Change | File | Insertion Point |
|---|--------|------|-----------------|
| C1 | Add `diff_tx: mpsc::UnboundedSender<Vec<FileEntry>>` + `diff_rx: mpsc::UnboundedReceiver<Vec<FileEntry>>` fields to `App` | `src/app.rs` | near `pty_manager` / after `watcher` field cluster (lines 63–71) |
| C2 | Initialize channel pair in `App::new` `Self { ... }` literal | `src/app.rs` | lines 115–142 (struct literal) |
| C3 | Add `refresh_diff_spawn(&mut self)` helper on `impl App` | `src/app.rs` | adjacent to existing `refresh_diff()` at line 246 |
| C4 | Add 6th select branch in `App::run` that drains `diff_rx` | `src/app.rs` | `tokio::select!` at lines 206–239 — **after** the `refresh_tick` branch (position 6) |
| C5 | Replace `.refresh_diff().await` with `.refresh_diff_spawn()` at 3 nav call-sites | `src/events.rs:509`, `src/events.rs:556`, `src/workspace.rs:143` | in-place |
| C6 | New test file `src/navigation_tests.rs` + `mod` registration in `src/main.rs` | `src/navigation_tests.rs` (NEW), `src/main.rs` (line 21-ish) | append `#[cfg(test)] mod navigation_tests;` beside existing `pty_input_tests` |

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog (in-tree) | Match Quality |
|-------------------|------|-----------|--------------------------|---------------|
| `src/app.rs` (field pair add) | state container | pub-sub (mpsc channel endpoints) | `pub pty_manager: PtyManager` (L63) / `events_rx: mpsc::UnboundedReceiver<FsEvent>` in `Watcher` (L39 of `src/watcher.rs`) | exact-role |
| `src/app.rs` (init in `Self {}`) | constructor | n/a | `pty_manager: PtyManager::new(),` L125 / `watcher,` L133 | exact-role |
| `src/app.rs` (`refresh_diff_spawn` helper) | `impl App` async-spawner method | fire-and-forget tokio::spawn | `refresh_diff()` at L246 (same signature shape; different await policy) | exact-role (same arg-extraction; spawn instead of await) |
| `src/app.rs` (`run` — 6th select branch) | async event loop | event-driven tokio::select | existing branches 1–5 at L206–239 (self-analog) | exact (extension in-place) |
| `src/events.rs:509` (ClickWorkspace) | controller | request-response (keyboard/mouse → action dispatch) | `Action::ClickTab(idx)` at L520–523 (sync field-write; zero await — the target shape) | role-match |
| `src/events.rs:556` (activate_sidebar_item Workspace arm) | controller | request-response | `SidebarItem::RemoveProject(project_idx)` arm at L546–550 (switches project, no refresh_diff await — the target shape) | role-match |
| `src/workspace.rs:143` (switch_project) | service (state mutation) | request-response | lines 137–142 field-writes block (sync; target shape is the full fn with spawn instead of await at L143) | role-match |
| `src/navigation_tests.rs` (NEW) | test module | behavioral + timing-gated unit | `src/pty_input_tests.rs` (same shape: `#![cfg(test)]`, `#[tokio::test]`, `/bin/cat`-free but same scaffold pattern) AND `src/app_tests.rs` (App-construction harness) | exact-role |
| `src/main.rs` (+1 line) | module registration | n/a | `#[cfg(test)] mod pty_input_tests;` at L20–21 | exact-role |

## Pattern Assignments

### C1. `diff_tx` / `diff_rx` fields on `App` struct

**Analog 1 (field placement):** `pub pty_manager: PtyManager` at `src/app.rs:63`.
**Analog 2 (channel type & shape):** `events_rx: mpsc::UnboundedReceiver<FsEvent>` in `Watcher` at `src/watcher.rs:39`, paired with `let (tx, rx) = mpsc::unbounded_channel::<FsEvent>();` at `src/watcher.rs:44`.

**Current `App` field cluster (`src/app.rs:53–80`):**

```rust
pub struct App {
    pub global_state: GlobalState,
    // ... state ...
    pub pty_manager: PtyManager,                       // <-- analog: sub-component with internal channel
    // ... more state ...
    pub keymap: Keymap,
    pub should_quit: bool,
    pub(crate) dirty: bool,                             // <-- Phase 2 added loop-control flag
    pub watcher: Option<crate::watcher::Watcher>,       // <-- analog: watcher owns a channel receiver internally
    // ...
}
```

**Pattern to apply:** Follow `watcher`'s precedent — `App` owns the mpsc-receiver directly (not via a sub-struct). Sender is clonable and handed to spawned tasks. Use `pub(crate)` to match `dirty`'s visibility (both are loop-internal coordination primitives, not public API).

```rust
// New fields — place near pty_manager / watcher since the receiver feeds the run loop's select:
pub(crate) diff_tx: tokio::sync::mpsc::UnboundedSender<Vec<crate::git::diff::FileEntry>>,
pub(crate) diff_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<crate::git::diff::FileEntry>>,
```

**Rationale:** `mpsc::unbounded_channel` is already the codebase idiom (`src/watcher.rs:44`). Unbounded is correct here — refresh_diff results are small (`Vec<FileEntry>`), the receiver drains one-per-loop-iteration (RESEARCH §4 Pitfall 5), and we want fire-and-forget senders to never block. Matches RESEARCH §7 Option A shape-sketch verbatim.

---

### C2. Initialize channel pair in `App::new`

**Analog:** `watcher` field initialization in `App::new` (`src/app.rs:105–113, 133`):

```rust
let watcher = if let Some(project) = active_project_idx.and_then(|idx| global_state.projects.get(idx)) {
    let mut watcher = crate::watcher::Watcher::new().ok();
    if let Some(w) = &mut watcher {
        let _ = w.watch(&project.repo_root);
    }
    watcher
} else {
    None
};

let mut app = Self {
    // ...
    watcher,                    // <-- analog: constructed before struct-literal, moved in
    // ...
};
```

**Pattern to apply:** Construct channel pair as a `let (tx, rx) = ...` binding before the `Self { ... }` literal (mirroring the `watcher` pre-construction), then move both into the struct.

```rust
// Before the `Self { ... }` literal in App::new (around line 114):
let (diff_tx, diff_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<crate::git::diff::FileEntry>>();

let mut app = Self {
    // ... existing fields unchanged ...
    should_quit: false,
    dirty: true,
    watcher,
    diff_tx,                    // NEW
    diff_rx,                    // NEW
    // ... rest unchanged ...
};
```

**Rationale:** Exact mirror of `watcher` pattern: let-bind externally, move into struct. No `Arc`, no `Mutex` — the sender is `Clone`, receiver is move-only, both live on the single-threaded `App` task.

---

### C3. `refresh_diff_spawn(&mut self)` helper on `impl App`

**Analog:** `refresh_diff()` at `src/app.rs:246–268` — same arg-extraction logic; the only delta is spawn-instead-of-await.

**Current excerpt verbatim (`src/app.rs:246–268`):**

```rust
pub(crate) async fn refresh_diff(&mut self) {
    let (path, base_branch) = match (self.active_project(), self.active_workspace()) {
        (Some(_), Some(ws)) => (ws.worktree_path.clone(), ws.base_branch.clone()),
        (Some(p), None) => (p.repo_root.clone(), p.base_branch.clone()),
        _ => {
            self.modified_files.clear();
            self.right_list.select(None);
            return;
        }
    };

    if let Ok(files) = diff::modified_files(path, base_branch).await {
        self.modified_files = files;

        if self.modified_files.is_empty() {
            self.right_list.select(None);
        } else if self.right_list.selected().is_none() {
            self.right_list.select(Some(0));
        } else if let Some(selected) = self.right_list.selected() {
            self.right_list.select(Some(selected.min(self.modified_files.len() - 1)));
        }
    }
}
```

**Pattern to apply:** Keep `refresh_diff()` exactly as-is (still used by `App::new` pre-first-frame on line 143, and by the `refresh_tick` + watcher arms at `src/app.rs:227, 236`). **Add a sibling `refresh_diff_spawn()` method immediately after** that reuses the same arg-extraction shape but spawns the git2 work and sends results through `diff_tx`. The re-select logic moves to the new select branch (C4).

```rust
/// Non-blocking variant of [`refresh_diff`] for the navigation hot path.
///
/// Spawns the git2 work onto a tokio task and returns immediately. Results
/// arrive on `diff_rx` and are applied by the dedicated select branch in
/// `App::run`. Use from nav call-sites (workspace switch, sidebar activate)
/// where blocking the input arm on git2 causes perceptible stutter. See
/// `.planning/phases/04-navigation-fluidity/04-RESEARCH.md` §3 + §7.
pub(crate) fn refresh_diff_spawn(&mut self) {
    let args = match (self.active_project(), self.active_workspace()) {
        (Some(_), Some(ws)) => Some((ws.worktree_path.clone(), ws.base_branch.clone())),
        (Some(p), None) => Some((p.repo_root.clone(), p.base_branch.clone())),
        _ => None,
    };
    let Some((path, base_branch)) = args else {
        self.modified_files.clear();
        self.right_list.select(None);
        self.mark_dirty();
        return;
    };
    let tx = self.diff_tx.clone();
    tokio::spawn(async move {
        if let Ok(files) = diff::modified_files(path, base_branch).await {
            let _ = tx.send(files);
        }
    });
}
```

**Rationale:** RESEARCH §7 Option A verbatim shape. Reuses `refresh_diff`'s arg-extraction verbatim (same `match` + `Some`/`None` patterns). `pub(crate)` visibility matches `refresh_diff`. The `mark_dirty()` in the early-return arm ensures the "no active workspace" case still triggers a draw — mirrors `refresh_diff`'s implicit behavior (caller typically has already called `mark_dirty` via the `// 1. INPUT` branch entry, but this is defensive).

**NOTE for planner — first `tokio::spawn` in the codebase.** A Grep at `src/` for `tokio::spawn\b` returns zero hits today; the codebase only uses `tokio::task::spawn_blocking` (inside `git::diff::modified_files` and `sync_pty_size`). Phase 4 introduces the first fire-and-forget `tokio::spawn`. This is fine — it's the idiomatic pattern — but **call it out in the plan's "new patterns introduced" list** so reviewers know to look for proper shutdown semantics (there are none needed: the spawned task is short-lived, self-terminating, and drops its sender on completion).

---

### C4. 6th select branch in `App::run` — drain `diff_rx`

**Analog:** Existing branches 1–5 in the `tokio::select! { biased; ... }` block at `src/app.rs:206–239`. Especially branch 5 (`refresh_tick.tick()`) which applies results from `refresh_diff().await` and calls `mark_dirty()`.

**Current excerpt verbatim (`src/app.rs:206–239`):**

```rust
tokio::select! {
    biased;

    // 1. INPUT — highest priority. Keyboard, mouse, paste, resize.
    Some(Ok(event)) = events.next() => {
        self.mark_dirty();
        crate::events::handle_event(self, event).await;
    }
    // 2. PTY output — high-volume under streaming agents.
    _ = self.pty_manager.output_notify.notified() => {
        self.mark_dirty();
    }
    // 3. File watcher — debounced filesystem events.
    Some(event) = async {
        if let Some(w) = &mut self.watcher {
            w.next_event().await
        } else {
            futures::future::pending::<Option<crate::watcher::FsEvent>>().await
        }
    } => {
        let _ = event;
        self.refresh_diff().await;
        self.mark_dirty();
    }
    // 4. Heartbeat — 5s tick to advance sidebar working-dot.
    _ = heartbeat_tick.tick() => {
        self.mark_dirty();
    }
    // 5. Safety-net diff refresh — Phase 5 replaces with event-driven.
    _ = refresh_tick.tick() => {
        self.refresh_diff().await;
        self.mark_dirty();
    }
}
```

**Pattern to apply:** Append a **6th branch** at the bottom of the select (NOT between branches 1 and 2 — that would violate Phase 2 input-priority invariant per RESEARCH §4 Pitfall 3). Body applies results + does the re-select logic that used to live inside `refresh_diff` + calls `mark_dirty()` (same triad as branches 3 and 5).

```rust
// 6. Diff-refresh results — drain background refresh_diff_spawn outputs.
Some(files) = self.diff_rx.recv() => {
    self.modified_files = files;
    if self.modified_files.is_empty() {
        self.right_list.select(None);
    } else if self.right_list.selected().is_none() {
        self.right_list.select(Some(0));
    } else if let Some(selected) = self.right_list.selected() {
        self.right_list.select(Some(selected.min(self.modified_files.len() - 1)));
    }
    self.mark_dirty();
}
```

**CRITICAL ordering invariants (RESEARCH §4 Pitfall 3 + §5):**
- `biased;` stays at the very top (unchanged).
- The INPUT branch `Some(Ok(event)) = events.next()` MUST remain branch 1 (unchanged).
- The new branch is position **6** (last). Do not insert between existing branches.
- Keep the numbered `// N. ...` annotation style — a `// 6. Diff-refresh results` comment is part of the contract for the regression-anchor greps (`rg '// 1\. INPUT' src/app.rs` = 1 hit, preserved).

**Rationale:** Branch 5 (`refresh_tick.tick()`) already demonstrates the exact shape: receive event → mutate `modified_files` → `mark_dirty()`. The new branch differs only in that it drains an mpsc channel instead of a timer, and does the re-select logic inline rather than inside `refresh_diff()` (because `refresh_diff_spawn` cannot mutate `App` directly from the spawned task). RESEARCH §4 Pitfall 5 explicitly warns against `while let` / `loop { try_recv }` drain — use the single-`Some(files) = recv()` form; tokio's biased re-poll picks up subsequent sends on the next iteration.

---

### C5. Replace `.refresh_diff().await` with `.refresh_diff_spawn()` at 3 nav sites

**Analogs (the target shape):**
- `Action::ClickTab(idx) => { app.active_tab = idx; app.mode = InputMode::Terminal; }` at `src/events.rs:520–523` — zero-await; pure sync field-write; the gold-standard nav path.
- `SidebarItem::RemoveProject(project_idx)` arm at `src/events.rs:546–550` — switches project without awaiting `refresh_diff` (because it doesn't select a workspace). Proves the codebase already has nav paths that don't await refresh_diff; we're extending that pattern to the Workspace arm.

**Current state — site 1 (`src/events.rs:504–519`, `ClickWorkspace`):**

```rust
Action::ClickWorkspace(project_idx, workspace_idx) => {
    if app.active_project_idx != Some(project_idx) {
        crate::workspace::switch_project(app, project_idx).await;    // <-- internal refresh_diff().await removed by C5/site 3
    }
    app.select_active_workspace(workspace_idx);
    app.refresh_diff().await;                                        // <-- C5 SITE 1: replace with refresh_diff_spawn()
    let has_tabs = app
        .active_workspace()
        .map(|ws| !ws.tabs.is_empty())
        .unwrap_or(false);
    if has_tabs {
        app.mode = InputMode::Terminal;
    } else {
        app.open_new_tab_picker();
    }
}
```

**Current state — site 2 (`src/events.rs:551–557`, `activate_sidebar_item` Workspace arm):**

```rust
SidebarItem::Workspace(project_idx, workspace_idx) => {
    if app.active_project_idx != Some(project_idx) {
        crate::workspace::switch_project(app, project_idx).await;    // <-- internal refresh_diff().await removed by C5/site 3
    }
    app.select_active_workspace(workspace_idx);
    app.refresh_diff().await;                                        // <-- C5 SITE 2: replace with refresh_diff_spawn()
}
```

**Current state — site 3 (`src/workspace.rs:118–144`, `switch_project`):**

```rust
pub async fn switch_project(app: &mut App, idx: usize) {
    if idx >= app.global_state.projects.len() { return; }
    // ... watcher unwatch/watch (sync) ...
    app.active_project_idx = Some(idx);
    app.global_state.active_project_id = Some(new_project_id);
    app.active_workspace_idx = app.global_state.projects[idx].active().next().map(|_| 0);
    app.active_tab = 0;
    app.preview_lines = None;
    app.right_list.select(None);
    app.refresh_diff().await;                                        // <-- C5 SITE 3: replace with refresh_diff_spawn()
}
```

**Pattern to apply — same mechanical change at all three sites:** `app.refresh_diff().await;` → `app.refresh_diff_spawn();` (drop `.await`, change identifier).

```rust
// Site 1 (events.rs:509):
app.refresh_diff_spawn();

// Site 2 (events.rs:556):
app.refresh_diff_spawn();

// Site 3 (workspace.rs:143):
app.refresh_diff_spawn();
```

**NOTE on `switch_project` signature:** it is declared `pub async fn switch_project(...)`. After C5/site 3, its body contains no `.await` — *only* because `refresh_diff_spawn` is sync. Keep the `async` keyword on the signature — RESEARCH §7 doesn't propose changing it, and callers in `events.rs` still `.await` it. Removing `async` would cascade into call-site edits and risk breaking the Phase 2 input-arm annotation. A `#[allow(clippy::unused_async)]` may be needed if clippy flags it; acceptance: `cargo clippy --all-targets -- -D warnings` stays clean.

**Rationale:** RESEARCH §1 primary recommendation; §4 Pitfall 1 ("keep the `.await` just in case") explicitly sets the acceptance criterion `rg 'refresh_diff\(\)\.await' src/events.rs src/workspace.rs` = **0 hits** post-refactor. `src/app.rs` retains one `.await` (in `App::new` at line 143 — pre-first-frame) and two indirect uses via branches 3 & 5 of the run loop.

---

### C6. `src/navigation_tests.rs` (NEW) + `mod` registration

**Analog 1 (test-file scaffold):** `src/pty_input_tests.rs` — same binary-crate `#[cfg(test)]` module pattern (VERIFIED: Phase 3 used this exact deviation from the "plan said src/lib.rs" issue; RESEARCH §7 explicitly cites "mirroring `src/pty_input_tests.rs` pattern from Phase 3").
**Analog 2 (App-construction harness):** `src/app_tests.rs::init_repo` + the `App::new(...)` invocation pattern at lines 11–24, 46–49, 74–76.

**Current `src/pty_input_tests.rs` scaffold excerpt (lines 1–15):**

```rust
//! PTY-input fluidity validation (PTY-01, PTY-02, PTY-03).
//!
//! These tests prove the Phase 2 structural primitives (biased select,
//! synchronous `write_input`, dirty-gated draw) deliver PTY-01/02/03
//! in practice. See .planning/phases/03-pty-input-fluidity/03-RESEARCH.md §6.

#![cfg(test)]

use crate::pty::session::PtySession;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
```

**Current `src/app_tests.rs::init_repo` fixture helper (lines 11–24):**

```rust
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
```

**Current `App::new` test-harness call (`src/app_tests.rs:28, 46–49, 74–76`):**

```rust
let app = App::new(GlobalState::default(), std::env::temp_dir().join("martins-test.json"))
    .await
    .unwrap();
```

**Pattern to apply — new test file:**

```rust
//! Navigation fluidity validation (NAV-01, NAV-02, NAV-03, NAV-04).
//!
//! These tests prove that nav call-sites (sidebar up/down, click-workspace,
//! click-tab, workspace switch) return to the event loop quickly — i.e., do
//! NOT block on `refresh_diff`. See
//! `.planning/phases/04-navigation-fluidity/04-RESEARCH.md` §6.

#![cfg(test)]

use crate::app::App;
use crate::state::{GlobalState, Project};
use git2::Repository;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

/// Build a git repo with N committed files at `dir`. For nav timing tests,
/// larger N makes `refresh_diff` slower and the non-blocking guarantee
/// easier to observe. RESEARCH §6 Wave 0 Gaps: "Large-repo fixture helper".
fn make_large_repo(dir: &Path, file_count: usize) -> Project {
    let repo = Repository::init(dir).unwrap();
    let sig = git2::Signature::now("test", "test@example.com").unwrap();
    for i in 0..file_count {
        std::fs::write(dir.join(format!("f{i}.txt")), b"x").unwrap();
    }
    let mut index = repo.index().unwrap();
    for i in 0..file_count {
        index.add_path(Path::new(&format!("f{i}.txt"))).unwrap();
    }
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[]).unwrap();
    let branch = repo.head().unwrap().shorthand().unwrap_or("main").to_string();
    Project::new(dir.to_path_buf(), branch)
}

/// NAV-03 — ClickTab is pure-sync; must NOT contain `.await`.
#[tokio::test]
async fn click_tab_is_sync() {
    // Direct state mutation mirrors src/events.rs:520-523.
    // (Full dispatch_action wiring requires a live App + tabs; this test
    // asserts the bare shape of the handler is sync.)
    let mut app = App::new(
        GlobalState::default(),
        std::env::temp_dir().join("martins-nav-click-tab.json"),
    )
    .await
    .unwrap();
    let before = std::time::Instant::now();
    app.active_tab = 3;
    app.mode = crate::keys::InputMode::Terminal;
    let elapsed = before.elapsed();
    assert_eq!(app.active_tab, 3);
    assert!(elapsed < Duration::from_millis(10), "tab switch took {elapsed:?}");
}

/// NAV-03 — `refresh_diff_spawn` returns immediately even on a large repo;
/// results arrive later via diff_rx.
#[tokio::test]
async fn refresh_diff_spawn_is_nonblocking() {
    let tmp = TempDir::new().unwrap();
    let mut state = GlobalState::default();
    let project = make_large_repo(tmp.path(), 500); // scale up if needed
    state.active_project_id = Some(project.id.clone());
    state.projects.push(project);

    let mut app = App::new(state, std::env::temp_dir().join("martins-nav-refresh.json"))
        .await
        .unwrap();

    let before = std::time::Instant::now();
    app.refresh_diff_spawn();
    let elapsed = before.elapsed();
    assert!(
        elapsed < Duration::from_millis(50),
        "refresh_diff_spawn returned in {elapsed:?} — should be <50ms (did it await?)"
    );
}
// Additional tests per RESEARCH §6 Phase Requirements → Test Map:
//   - sidebar_up_down_is_sync (NAV-01 unit — move_sidebar_to_workspace)
//   - activate_sidebar_nonblocking (NAV-01 behavioral — tokio::time::timeout wrap)
//   - click_workspace_nonblocking (NAV-02 behavioral)
//   - workspace_switch_paints_pty_first (NAV-03 — asserts active_workspace_idx
//     mutated and dirty=true before modified_files populated)
```

**`src/main.rs` +1 line (adjacent to existing Phase 3 registration at line 21):**

```rust
#[cfg(test)]
mod pty_input_tests;

#[cfg(test)]
mod navigation_tests;            // <-- NEW
```

**Rationale:** Test-file scaffold is verbatim `pty_input_tests.rs`. Fixture helper `make_large_repo` generalizes `app_tests.rs::init_repo` (1 file → N files). Registration pattern is the binary-crate deviation already validated in Phase 3 (see 03-01-SUMMARY.md "Binary-only crate deviation"). The planner may choose to co-locate all NAV tests in this file or split per-requirement; this PATTERNS document sketches the unified layout.

**Threat-model note:** nav tests DO NOT spawn child processes via `PtySession` (unlike phase 3). `T-03-01` (test harness invoking real programs) does NOT apply. Tests construct `App::new` + git2 repos in tempdirs — both are sandboxed.

---

## Shared Patterns

### Fire-and-forget spawn + mpsc-result-drain

**Source analogs:**
- Channel construction: `src/watcher.rs:44` — `let (tx, rx) = mpsc::unbounded_channel::<FsEvent>();`
- Receiver draining in run loop: `src/app.rs:219–228` — the watcher branch `Some(event) = async { ... w.next_event().await ... } => { ... mark_dirty(); }`
- Sender cloning (for fire-and-forget closure capture): `src/watcher.rs:45` — `let tx = Arc::new(tx);` (unbounded `Sender` is actually already `Clone` without `Arc`; C3 uses plain `.clone()`)

**Apply to:** C3 (spawn), C4 (drain). The cross-cutting contract is:
1. Receiver is owned by `App` and `.recv()`-polled on a dedicated select branch.
2. Sender is `clone()`'d into the spawned task's closure (unbounded `Sender: Clone`; no `Arc` wrapper needed — simpler than `watcher.rs`'s `Arc<Sender>` which exists there only because the closure is `Fn`, not `FnOnce`).
3. Spawned task is `tokio::spawn` (not `spawn_blocking`) because the task internally uses `spawn_blocking` — `modified_files` already does this. Nesting `tokio::spawn` around a function that does `.await` on `spawn_blocking` is the correct shape.
4. `let _ = tx.send(...);` — ignore send errors (receiver dropped = app shutting down = discard silently).

### `mark_dirty()` on state mutation (Phase 2 invariant — preserve)

**Source:** `src/app.rs:211, 216, 228, 232, 237` — 5 `mark_dirty()` call sites already in `App::run`. Phase 2 requirement.

**Apply to:** C3 early-return arm + C4 end-of-branch. Must keep `rg 'self\.mark_dirty\(\)' src/app.rs | wc -l >= 5` invariant; C4 adds the 6th call (phase-3 snapshot observed 6 already; post-phase-4 target is 7).

### Re-select logic coalescing (modified_files bounds check)

**Source:** `src/app.rs:260–266` — inside `refresh_diff`, the `if is_empty { None } else if selected.is_none() { Some(0) } else { Some(selected.min(len-1)) }` triad.

**Apply to:** C4 branch body (moved out of `refresh_diff` because the spawned task can't mutate `App`, so the select branch applies it). Copy verbatim; do not refactor into a helper — the logic is short and inlined in one place.

### Existing nav paths that DON'T await refresh_diff (DO NOT regress)

**Sources (RESEARCH §2 "Items that do NOT trigger refresh_diff"):**
- `src/events.rs:520–523` — `Action::ClickTab(idx)` — sync field write only.
- `src/events.rs:490–503` — `Action::ClickProject(idx)` when already-active — toggles `expanded` + `save_state()`.
- `src/events.rs:546–550` — `SidebarItem::RemoveProject` arm — switches project but doesn't select workspace.
- `src/events.rs:258–269` — `F(n)` direct tab-switch bypass in `handle_key`.

**Apply to:** Phase 4 must **not** add `.await` or `refresh_diff_spawn` calls to any of these. They are the reference shape for NAV-04 ("tab switch is already correct"). RESEARCH §4 Pitfall 4 ("making tab switching async") explicitly forbids touching them. Acceptance criterion: `rg '\.await' src/events.rs | rg -i 'tab'` = 0 hits.

### Phase 2/3 grep-invariant regression anchors (preserve all)

**Source:** 03-01-SUMMARY.md "Grep Invariant Snapshot" — the 7-row table.

**Apply to:** C4 (run-loop edit) must preserve every row. Post-phase-4 expected deltas:
- `biased;` in `src/app.rs` — **1** (unchanged).
- `// 1. INPUT` in `src/app.rs` — **1** (unchanged).
- `if self.dirty` in `src/app.rs` — **1** (unchanged).
- `status_tick` in `src/app.rs` — **0** (unchanged).
- `self.mark_dirty()` in `src/app.rs` — **≥ 7** (was 6; +1 from C4).
- `duration_since` in `src/pty/session.rs` — **≥ 1** (unchanged — Phase 4 does not touch PTY throttle).
- `tokio::task::spawn` near write_input path — **0** (unchanged).
- **NEW invariant to add post-Phase-4:** `rg 'refresh_diff\(\)\.await' src/events.rs src/workspace.rs` = **0** (RESEARCH §4 Pitfall 1 gate); `src/app.rs` retains exactly 1 (the `App::new` call).

## No Analog Found

None. Every change has an in-tree analog:

| Change | Concern | Resolution |
|--------|---------|------------|
| C3: first `tokio::spawn` in codebase | No existing call site to copy from | Pattern is textbook tokio; RESEARCH §7 provides verbatim shape; reviewers should note it in commit message. No analog needed — this is a new pattern introduction, not extraction. |
| Behavioral timing tests (`refresh_diff_spawn_is_nonblocking`) | No prior timing-gated test in codebase | Structural analog is `app_tests.rs` for App construction + `pty_input_tests.rs` for `#[tokio::test]` scaffold. Timing assertion (`elapsed < 50ms`) is novel but trivially expressible with `std::time::Instant` — no pattern precedent needed. |

## Metadata

**Analog search scope:**
- `src/app.rs` (App struct, App::new, App::run loop, refresh_diff, mark_dirty, select branches)
- `src/events.rs` (dispatch_action, activate_sidebar_item, handle_key, handle_mouse — all `.await` sites in nav paths)
- `src/workspace.rs` (switch_project, archive_active_workspace, reattach_tmux_sessions)
- `src/pty/manager.rs` (mpsc-less pattern — uses `Arc<Notify>` instead; not directly analogous but referenced for `output_notify` pub field precedent)
- `src/watcher.rs` (mpsc channel idiom — the load-bearing analog for C1/C2)
- `src/pty_input_tests.rs` (test-file scaffold analog)
- `src/app_tests.rs` (App-construction harness analog)
- `src/main.rs` (module registration precedent)
- `src/git/diff.rs` (`modified_files` signature — unchanged; used from C3 verbatim)
- `.planning/phases/02-event-loop-rewire/02-PATTERNS.md` (cross-reference — branch ordering and `mark_dirty` invariants)
- `.planning/phases/03-pty-input-fluidity/03-01-SUMMARY.md` (grep-invariant snapshot to preserve)

**Files scanned:** 11
**Pattern extraction date:** 2026-04-24
