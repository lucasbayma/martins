# Phase 5: Background Work Decoupling — Pattern Map

**Mapped:** 2026-04-24
**Files analyzed:** 6 source files + 1 test file + Cargo.toml
**Analogs found:** 6 / 6 (every Phase-5 edit has a proven precedent already in the codebase)

## File Classification

| Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---------------|------|-----------|----------------|---------------|
| `src/app.rs` (run loop arms 3 + 5) | event loop scheduler | event-driven (tokio::select! arm) | arm 6 drain pattern at `src/app.rs:247-258` (Phase 4) | exact — same file, same loop, same macro |
| `src/app.rs` (new `save_state_spawn`) | state-persistence dispatcher | fire-and-forget (spawn_blocking) | `refresh_diff_spawn` at `src/app.rs:306-325` (Phase 4) | exact — same role (non-blocking variant of sync sibling), same file |
| `src/app.rs` (interval 5s → 30s) | safety-net timer | timer tick | `heartbeat_tick` at `src/app.rs:181` | exact — identical `interval(Duration::from_secs(N))` shape |
| `src/watcher.rs` (750ms → 200ms) | watcher debounce tuning | config constant | `src/watcher.rs:47-69` itself (in-place retune) | trivial — single-literal change |
| `src/workspace.rs` (7 `save_state()` call sites) | workspace lifecycle | state persistence trigger | `src/workspace.rs:143` `app.refresh_diff_spawn()` (Phase 4 migration precedent) | exact — same substitution shape (`.save_state()` → `.save_state_spawn()`) |
| `src/events.rs` (4 `save_state()` call sites) | input-arm action handler | state persistence trigger | `src/events.rs:510, 557` `app.refresh_diff_spawn()` (Phase 4 migration precedent) | exact — same substitution shape, co-located in same match arms |
| `src/ui/modal_controller.rs` (2 `save_state()` call sites) | modal confirm handler | state persistence trigger | `src/workspace.rs:143` `app.refresh_diff_spawn()` | role-match — same substitution shape, different file |
| `src/app_tests.rs` (new test `save_state_spawn_is_nonblocking`) | test | timing assertion | `src/navigation_tests.rs:113-138` `refresh_diff_spawn_is_nonblocking` | exact — this is the explicitly referenced template (RESEARCH §12 line 451) |
| `src/watcher.rs` tests (retune `debounce_rapid`) | test | timing assertion | `src/watcher.rs:139-170` itself | trivial — existing test retuned to 200ms window |
| `Cargo.toml` (optional `notify-debouncer-mini = "0.7"`) | config | dependency version | existing direct dep | trivial — single version bump |

## Pattern Assignments

### `src/app.rs` — run-loop arms 3 + 5 (event loop, event-driven)

**Analog:** `src/app.rs:213-259` itself (the Phase 4 loop; the migration is in-place).

**Existing arm-body pattern — the shape to copy** (lines 213-259, from the just-read Phase 4 output):
```rust
// Source: src/app.rs:213-259 (verbatim — current state)
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
        self.refresh_diff().await;    // ← Phase 5 replaces with refresh_diff_spawn()
        self.mark_dirty();
    }
    // 4. Heartbeat — 5s tick to advance sidebar working-dot.
    _ = heartbeat_tick.tick() => {
        self.mark_dirty();
    }
    // 5. Safety-net diff refresh — Phase 5 replaces with event-driven.
    _ = refresh_tick.tick() => {
        self.refresh_diff().await;    // ← Phase 5 replaces with refresh_diff_spawn()
        self.mark_dirty();
    }
    // 6. Diff-refresh results — drain background refresh_diff_spawn outputs.
    Some(files) = self.diff_rx.recv() => {
        self.modified_files = files;
        if self.modified_files.is_empty() {
            self.right_list.select(None);
        } else if self.right_list.selected().is_none() {
            self.right_list.select(Some(0));
        } else if let Some(selected) = self.right_list.selected() {
            self.right_list
                .select(Some(selected.min(self.modified_files.len() - 1)));
        }
        self.mark_dirty();
    }
}
```

**Interval-construction pattern** (lines 176-181, to copy/retune):
```rust
// Source: src/app.rs:176-181 (verbatim)
let mut events = EventStream::new();
let mut refresh_tick = interval(Duration::from_secs(5));     // Phase 5: change 5 → 30
// Heartbeat: keeps the sidebar "working dot" animation advancing
// without a high-frequency wakeup. Dropped from 1s to 5s now that
// draw is dirty-gated. (See 02-RESEARCH §2 pitfall #5.)
let mut heartbeat_tick = interval(Duration::from_secs(5));   // Phase 5: unchanged (working-dot floor)
```

**Graceful-exit sync-save pattern** (line 262, KEEP AS-IS per Pitfall #5):
```rust
// Source: src/app.rs:262 — intentional sync save for durability on process exit
self.save_state();
Ok(())
```

**Phase 5 delta (arm 3 + arm 5 become non-blocking):**
```rust
// Arm 3 after Phase 5:
} => {
    let _ = event;
    self.refresh_diff_spawn();   // was: self.refresh_diff().await;
    // mark_dirty is called inside refresh_diff_spawn — no extra call needed
}

// Arm 5 after Phase 5:
_ = refresh_tick.tick() => {
    self.refresh_diff_spawn();   // was: self.refresh_diff().await;
    // mark_dirty is called inside refresh_diff_spawn
}

// And the interval:
let mut refresh_tick = interval(Duration::from_secs(30));   // was: 5
```

---

### `src/app.rs` — new `save_state_spawn` primitive (state persistence, fire-and-forget)

**Analog:** `src/app.rs:306-325` `refresh_diff_spawn`. This is the **explicit template** — RESEARCH §7 Pattern 1 + §14.2 both reference it by line number.

**Existing sibling — `refresh_diff_spawn` (lines 306-325)** — the shape to mirror exactly:
```rust
// Source: src/app.rs:306-325 (verbatim — Phase 4 primitive)
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
    self.mark_dirty();
}
```

**Existing sync `save_state` — the error-handling contract to preserve** (lines 358-362):
```rust
// Source: src/app.rs:358-362 (verbatim — the log-and-continue contract)
pub(crate) fn save_state(&self) {
    if let Err(error) = self.global_state.save(&self.state_path) {
        tracing::error!("failed to save state: {error}");
    }
}
```

**Doc-comment pattern to copy from Phase 4 primitive** (lines 290-305, the cross-reference shape):
```rust
// Source: src/app.rs:290-305 (verbatim — doc style Phase 5 should mirror)
/// Non-blocking variant of [`refresh_diff`] for the navigation hot path.
///
/// Spawns the git2 work onto a tokio task and returns immediately. Results
/// arrive on `diff_rx` and are applied by the 6th select branch in
/// `App::run`. Use from nav call-sites (workspace switch, sidebar
/// activate) where blocking the input arm on git2 causes perceptible
/// stutter. See
/// `.planning/phases/04-navigation-fluidity/04-RESEARCH.md` §3 + §7.
///
/// The existing async [`refresh_diff`] stays in use for the `App::new`
/// pre-first-frame call and the watcher / refresh_tick branches — those
/// are not on the user-facing input hot path.
///
/// Do NOT collapse with [`refresh_diff`] — the sync-by-design return
/// shape is load-bearing for the nav hot path. Plan 04-01's test
/// `refresh_diff_spawn_is_nonblocking` locks in a <50ms return budget.
```

**Phase 5 target shape (RESEARCH §14.2):**
```rust
// Source: target shape — sibling to refresh_diff_spawn at src/app.rs:306
/// Non-blocking variant of [`save_state`].
///
/// Clones `global_state` + `state_path` and dispatches the fs::write + atomic
/// rename to a tokio blocking worker. Errors are logged via tracing::error!
/// (same contract as the synchronous [`save_state`]).
///
/// Use from every call site EXCEPT the graceful-exit drain in [`App::run`],
/// where we need the write to complete before process exit.
///
/// See `.planning/phases/05-background-work-decoupling/05-RESEARCH.md`
/// §9 Pattern 2 + §8 Pitfall #5.
pub(crate) fn save_state_spawn(&self) {
    let state = self.global_state.clone();
    let path = self.state_path.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(error) = state.save(&path) {
            tracing::error!("failed to save state: {error}");
        }
    });
}
```

**Key contract carried from analog:**
- `tokio::spawn(async move { ... })` vs `tokio::task::spawn_blocking(move || { ... })`: the **only** shape difference. `refresh_diff_spawn` uses `tokio::spawn` because `modified_files` is already async (internally spawn_blocking'd); `save_state_spawn` uses `spawn_blocking` because `GlobalState::save` is a blocking sync fn. Do not collapse.
- Clone-before-move: same discipline — `self.global_state.clone()` + `self.state_path.clone()` before the task body, never borrow `&self` across the `tokio::task::spawn_blocking` boundary.
- `&self` receiver (not `&mut self`) — `save_state_spawn` has no reason to mutate app state (no `mark_dirty`; state save does not affect render).
- `tracing::error!` on failure — preserves the existing `save_state` contract (line 360).

---

### `src/watcher.rs` — debounce window retune (watcher, config constant)

**Analog:** `src/watcher.rs:47-69` itself (in-place single-literal change).

**Existing debouncer construction** (lines 43-75, full context):
```rust
// Source: src/watcher.rs:43-75 (verbatim)
impl Watcher {
    pub fn new() -> Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel::<FsEvent>();
        let tx = Arc::new(tx);

        let debouncer = new_debouncer(
            Duration::from_millis(750),                    // ← Phase 5: 750 → 200
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
```

**Phase 5 delta:** `Duration::from_millis(750)` → `Duration::from_millis(200)`. Closure body, noise filter, existence-check mapping, mpsc plumbing — **all unchanged**.

**Existing test — `debounce_rapid` (lines 139-170)** — the shape to retune:
```rust
// Source: src/watcher.rs:139-170 (verbatim — retune deadlines for 200ms window)
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
```

**Phase 5 retune notes:**
- Inter-write `sleep` is 50ms (matches — 5×50ms = 250ms burst); with a 200ms debounce window, the whole burst still lands inside one debounce cycle. Keep `count <= 2`.
- Existing 2000ms global deadline: still correct, no change needed.
- Per BG-04 success criterion: "burst of 10 rapid writes ≤ 2 events" — consider adding a 10-write variant (RESEARCH §12 Wave 0 Gaps).

---

### `src/workspace.rs` — 7 `save_state()` → `save_state_spawn()` call sites (workspace lifecycle, state persistence trigger)

**Analog:** `src/workspace.rs:143` — the Phase 4 precedent for in-place call-site migration.

**Existing Phase-4 migration shape** (line 143, in `switch_project`):
```rust
// Source: src/workspace.rs:137-144 (verbatim — Phase 4 migrated refresh_diff().await → refresh_diff_spawn())
    app.active_project_idx = Some(idx);
    app.global_state.active_project_id = Some(new_project_id);
    app.active_workspace_idx = app.global_state.projects[idx].active().next().map(|_| 0);
    app.active_tab = 0;
    app.preview_lines = None;
    app.right_list.select(None);
    app.refresh_diff_spawn();    // ← Phase 4 migration pattern. Phase 5: `app.save_state()` sites copy this shape.
}
```

**Existing call sites to migrate** (all verified from grep):

```rust
// Source: src/workspace.rs:152-159 — confirm_delete_workspace
pub fn confirm_delete_workspace(app: &mut App, form: &DeleteForm) {
    let name = form.workspace_name.clone();
    if let Some(project) = app.active_project_mut() {
        project.remove(&name);
    }
    app.refresh_active_workspace_after_change();
    app.save_state();                                // line 158 → save_state_spawn()
}
```

```rust
// Source: src/workspace.rs:161-182 — archive_active_workspace
// NOTE: RESEARCH §17 Q3 recommends also wrapping remove_dir_all(worktree_path) at line 181
// in spawn_blocking for BG-05's "archive feels instant" success criterion.
pub fn archive_active_workspace(app: &mut App) {
    // ... kill sessions, archive, refresh_active_workspace_after_change ...
    app.save_state();                                // line 179 → save_state_spawn()

    let _ = std::fs::remove_dir_all(&worktree_path); // line 181 — Q3 option: wrap in spawn_blocking
}
```

```rust
// Source: src/workspace.rs:184-196 — delete_archived_workspace
// line 195: app.save_state()                       → save_state_spawn()

// Source: src/workspace.rs:198-216 — confirm_remove_project
// line 215: app.save_state()                       → save_state_spawn()

// Source: src/workspace.rs:218-267 — create_workspace
// line 263: app.save_state()                       → save_state_spawn()
// (burst site: workspace-create → create_tab → save_state = 2 back-to-back saves;
//  Shape B fire-and-forget accepts theoretical ordering risk per RESEARCH §8 Pitfall #4)

// Source: src/workspace.rs:269-321 — create_tab
// line 319: app.save_state()                       → save_state_spawn()

// Source: src/workspace.rs:323-350 — add_project_from_path
// line 342: app.save_state()                       → save_state_spawn()
```

**Uniform substitution pattern** — every site is a trailing statement with no return-value consumer:
```rust
// Before:
app.save_state();

// After:
app.save_state_spawn();
```

---

### `src/events.rs` — 4 `save_state()` → `save_state_spawn()` call sites (input-arm action handler, state persistence trigger)

**Analog:** `src/events.rs:510, 557` — Phase 4 co-located precedent in the same `match action { ... }` dispatch.

**Existing Phase-4 migrations in the same file** (lines 510 and 557):
```rust
// Source: src/events.rs:505-520 — ClickWorkspace (Phase 4 migrated: shape to copy)
Action::ClickWorkspace(project_idx, workspace_idx) => {
    if app.active_project_idx != Some(project_idx) {
        crate::workspace::switch_project(app, project_idx).await;
    }
    app.select_active_workspace(workspace_idx);
    app.refresh_diff_spawn();       // ← Phase 4 migration — NOT `.await`
    // ...
}

// Source: src/events.rs:552-558 — SidebarItem::Workspace (Phase 4 migrated)
SidebarItem::Workspace(project_idx, workspace_idx) => {
    if app.active_project_idx != Some(project_idx) {
        crate::workspace::switch_project(app, project_idx).await;
    }
    app.select_active_workspace(workspace_idx);
    app.refresh_diff_spawn();       // ← Phase 4 migration
}
```

**Existing call sites to migrate** (all verified from grep):
```rust
// Source: src/events.rs:416-434 — close-tab (Action::CloseTab)
// line 433: app.save_state();                      → save_state_spawn()

// Source: src/events.rs:491-504 — Action::ClickProject (TWO save_state calls)
// line 496: app.save_state();                      → save_state_spawn()
// line 502: app.save_state();                      → save_state_spawn()
Action::ClickProject(idx) => {
    if app.active_project_idx == Some(idx) {
        if let Some(project) = app.global_state.projects.get_mut(idx) {
            project.expanded = !project.expanded;
        }
        app.save_state();           // line 496 → save_state_spawn()
    } else {
        crate::workspace::switch_project(app, idx).await;
        if let Some(project) = app.global_state.projects.get_mut(idx) {
            project.expanded = true;
        }
        app.save_state();           // line 502 → save_state_spawn()
    }
}

// Source: src/events.rs:534-539 — Action::ToggleProjectExpand
// line 538: app.save_state();                      → save_state_spawn()
```

**Uniform substitution pattern** — identical to `workspace.rs`: trailing statement, no return consumer, straight `save_state()` → `save_state_spawn()`.

---

### `src/ui/modal_controller.rs` — 2 `save_state()` → `save_state_spawn()` call sites (modal confirm handler, state persistence trigger)

**Analog:** `src/workspace.rs:143` (same substitution pattern across files).

**Existing call sites** (verified from grep):
```rust
// Source: src/ui/modal_controller.rs:85-95 — Modal::ConfirmArchive keypress handler
Modal::ConfirmArchive(form) => match key.code {
    KeyCode::Esc => app.modal = Modal::None,
    KeyCode::Enter => {
        if let Some(project) = app.active_project_mut() {
            project.archive(&form.workspace_name);
        }
        app.modal = Modal::None;
        app.refresh_active_workspace_after_change();
        app.save_state();           // line 93 → save_state_spawn()
    }
    _ => app.modal = Modal::ConfirmArchive(form),
},

// Source: src/ui/modal_controller.rs:230-239 — Modal::ConfirmArchive click handler
if row == modal_button_row_y(modal_area) {
    if is_modal_first_button(modal_area, col, 17) {
        if let Some(project) = app.active_project_mut() {
            project.archive(&form.workspace_name);
        }
        app.refresh_active_workspace_after_change();
        app.save_state();           // line 236 → save_state_spawn()
    }
    app.modal = Modal::None;
}
```

**Uniform substitution pattern** — identical.

---

### `src/app_tests.rs` — new test `save_state_spawn_is_nonblocking` (test, timing assertion)

**Analog:** `src/navigation_tests.rs:113-138` `refresh_diff_spawn_is_nonblocking`. This is **the explicitly-named template** in RESEARCH §12 Wave 0 Gaps: "pattern mirrors Phase 4 `refresh_diff_spawn_is_nonblocking`."

**Existing template to copy** (lines 105-138):
```rust
// Source: src/navigation_tests.rs:105-138 (verbatim — THE template for Phase 5's new test)
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

    let state = GlobalState {
        active_project_id: Some(project_id),
        projects: vec![project],
        ..Default::default()
    };

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
```

**Phase 5 target shape (mirror exactly, swap subject):**
```rust
// Source: target shape for Phase 5 Wave 0 test (BG-05 regression guard)
#[tokio::test]
async fn save_state_spawn_is_nonblocking() {
    let tmp = TempDir::new().expect("TempDir");
    // Pathological GlobalState — 100 projects per RESEARCH §12 criterion.
    // If no make_large_state helper exists, inline build via add_project loop.
    let mut state = GlobalState::default();
    for i in 0..100 {
        state.add_project(
            &tmp.path().join(format!("repo-{i}")),
            "main".to_string(),
        );
    }

    let state_path = std::env::temp_dir().join("martins-bg-save-spawn.json");
    let _ = std::fs::remove_file(&state_path);
    let mut app = App::new(state, state_path).await.expect("App::new");

    let before = Instant::now();
    app.save_state_spawn();
    let elapsed = before.elapsed();

    assert!(
        elapsed < Duration::from_millis(5),
        "save_state_spawn returned in {elapsed:?} — must be <5ms (did it block on fs::write?). \
         If this fails, someone reintroduced the sync save call path."
    );
}
```

**Contract carried from analog:**
- `#[tokio::test]` attribute (sets up a tokio runtime for `spawn_blocking` to have a thread pool).
- `TempDir` fixture to avoid polluting real `~/.martins`.
- `std::env::temp_dir().join(...)` for `state_path` (separate from the `TempDir` — matches Phase 4 convention at line 125).
- `let before = Instant::now(); <spawn_call>; let elapsed = before.elapsed();` — the timing harness.
- `assert!(elapsed < Duration::from_millis(N), "...")` — the budget assertion with a diagnostic explaining the regression mode.
- Budget: Phase 4 uses 50ms for git2-offloading; Phase 5 uses **5ms** per RESEARCH §12 ("assert elapsed <5ms"). This tighter bound is justified because `spawn_blocking` dispatch is pure channel-send + clone — no git2 latency floor.

---

### `Cargo.toml` — optional `notify-debouncer-mini` version bump (config, dependency)

**Analog:** none required; single version-bump.

**Current state** (verified via cargo tree in RESEARCH §6 A6): direct dep is `notify = "8.2"`, transitive via `notify-debouncer-mini = "0.4.1"` pulls `notify 6.1.1` — two copies of `notify` in the tree.

**Phase 5 delta (optional hygiene):**
```toml
# Cargo.toml — change:
notify-debouncer-mini = "0.7"   # was "0.4"
```

**Assumption A6:** API surface `new_debouncer(Duration, callback)` + `Watcher::watch/unwatch/next_event` is stable across 0.4 → 0.7. If upgrade breaks, revert. This bump is **not load-bearing for Phase 5 goals** — planner may defer to a v2 hygiene ticket.

---

## Shared Patterns

### Pattern 1 — Clone-and-move into `tokio::task::spawn_blocking`

**Source:** `src/workspace.rs:285-293` (existing precedent for `spawn_blocking` in this codebase).

```rust
// Source: src/workspace.rs:285-293 (verbatim — existing spawn_blocking shape for tmux::new_session)
let tmux_name_c = tmux_name.clone();
let worktree_c = worktree_path.clone();
let program_c = program.clone();
tokio::task::spawn_blocking(move || {
    crate::tmux::new_session(&tmux_name_c, &worktree_c, &program_c, cols, rows)
})
.await
.map_err(|e| e.to_string())?
.map_err(|e| e.to_string())?;
```

**Apply to:** `save_state_spawn` body.

**Difference to respect:** the existing `spawn_blocking` at `src/workspace.rs:288` is **awaited** because tmux creation must complete before the next steps. `save_state_spawn` must be **fire-and-forget** (no `.await`) because that is its whole point. Therefore: copy the `move || { ... }` body shape, but do NOT append `.await` — per the `refresh_diff_spawn` fire-and-forget precedent at `src/app.rs:319-323`.

### Pattern 2 — `tracing::error!` for non-UI-surfaced failure

**Source:** `src/app.rs:359-360` (existing `save_state` contract).

```rust
// Source: src/app.rs:358-361
pub(crate) fn save_state(&self) {
    if let Err(error) = self.global_state.save(&self.state_path) {
        tracing::error!("failed to save state: {error}");
    }
}
```

**Apply to:** the `move || { ... }` body of `save_state_spawn`. Preserves the user-visible error-handling contract exactly — same message, same level, same swallow-and-continue semantics.

### Pattern 3 — `pub(crate) fn` + sibling naming convention

**Source:** `src/app.rs:266` (`pub(crate) async fn refresh_diff`) + `src/app.rs:306` (`pub(crate) fn refresh_diff_spawn`) — the **sync variant is named `<sibling>_spawn`**.

**Apply to:** `save_state` (line 358) → new `save_state_spawn`. Same visibility (`pub(crate)`), same `_spawn` suffix convention. Same module location (`impl App` block in `src/app.rs`).

### Pattern 4 — Phase 2/3/4 invariants to preserve

Carried forward from RESEARCH §4 "Cross-tier correctness checks":
- `biased;` first in `tokio::select!` (line 214) — DO NOT remove.
- `// 1. INPUT` comment on first arm (line 216) — DO NOT remove.
- `if self.dirty` dirty-gate on draw (line 184) — DO NOT remove.
- `mark_dirty()` called in every arm body — Phase 5 preserves via inclusion inside `refresh_diff_spawn`.
- `heartbeat_tick = interval(Duration::from_secs(5))` (line 181) — DO NOT change (working-dot floor).
- `status_tick` must remain absent — DO NOT reintroduce.
- `refresh_diff_spawn` defined exactly once (line 306) — DO NOT collapse with `refresh_diff`.
- 6th select branch drains `diff_rx` (lines 246-258) — DO NOT remove.
- Graceful-exit `self.save_state();` at line 262 — DO NOT change to `save_state_spawn` (Pitfall #5).

---

## No Analog Found

**None.** Every Phase 5 edit has a direct precedent in the codebase — this phase is pure composition of Phase 2/3/4 primitives plus single-literal retunes. Specifically:

- `save_state_spawn` ↔ `refresh_diff_spawn` (line-by-line mirror with `spawn` vs `spawn_blocking` adaptation).
- Arm 3 / arm 5 `.await` removal ↔ `src/events.rs:510, 557` and `src/workspace.rs:143` (Phase 4 call-site migrations).
- 30s interval ↔ existing 5s `heartbeat_tick`/`refresh_tick` literal.
- 200ms debounce ↔ existing 750ms literal in same file.
- `save_state_spawn_is_nonblocking` test ↔ `refresh_diff_spawn_is_nonblocking` (RESEARCH §12 names this explicitly).
- Workspace/events/modal call-site substitutions ↔ Phase 4 `refresh_diff().await` → `refresh_diff_spawn()` migrations in the same files.

The **only** optional new-territory item is **Pitfall #4 Shape A (serialized save queue via `mpsc::Sender<GlobalState>` + consumer task)** — RESEARCH §14.4 documents a target shape but classifies as "optional, Shape B is the MVP." If the planner adopts Shape A, the closest analog is the `diff_tx`/`diff_rx` + 6th select branch plumbing from Phase 4 (`src/app.rs:117-118, 246-258`) — an mpsc-consumer loop with `try_recv` coalescing is a small extension of that shape.

---

## Metadata

**Analog search scope:**
- `src/app.rs` (run loop, refresh_diff, refresh_diff_spawn, save_state, graceful-exit drain — lines 1-325, 358-362)
- `src/watcher.rs` (full file — 172 LOC)
- `src/state.rs` (GlobalState::save + StateError + Clone derive — lines 130-230)
- `src/workspace.rs` (all save_state + refresh_diff_spawn sites — lines 115-350)
- `src/events.rs` (action dispatch + save_state + refresh_diff_spawn sites — lines 420-560)
- `src/ui/modal_controller.rs` (modal confirm save sites — lines 80-240)
- `src/navigation_tests.rs` (Phase 4 test template — lines 95-183)

**Files scanned:** 7 source + 1 Cargo.toml.

**Grep sweeps executed:**
- `save_state\(\)` → 14 call sites across 4 files (matches RESEARCH §5.3 enumeration exactly)
- `refresh_diff_spawn\(\)` → 7 call sites (3 in navigation_tests, 1 workspace, 2 events, plus 1 definition in app.rs) — matches RESEARCH §4 Cross-tier check ("3 call sites outside `src/app.rs`")

**Pattern extraction date:** 2026-04-24

**Confidence:** HIGH — every analog is a direct precedent in the same repo, same idiom, same module. Phase 5 is pure pattern-replication; the planner does not need to invent any new shapes.
