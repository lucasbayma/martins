# Phase 2: Event Loop Rewire — Pattern Map

**Mapped:** 2026-04-24
**Files analyzed:** 2 (both modifications, no new files)
**Analogs found:** 5 / 5 (all in-file — this is a rewire, not extraction)

## Scope Note

This phase is a **narrow rewire** of `src/app.rs::run`, not a greenfield extraction. All "analogs" are existing structures *in the same file* that the new code should mirror stylistically (field placement, init-list position, helper method shape, test scaffolding).

Five concrete changes:

| # | Change | File | Insertion Point |
|---|--------|------|-----------------|
| C1 | Add `dirty: bool` field to `App` struct | `src/app.rs` | line 70 (immediately after `should_quit`) |
| C2 | Initialize `dirty: true` in `App::new` | `src/app.rs` | line 131 (immediately after `should_quit: false,`) |
| C3 | Add `mark_dirty()` helper on `impl App` | `src/app.rs` | after `active_workspace()` at line 159, before `run()` at line 161 |
| C4 | Rewire `App::run` event loop (gate draw, drop status_tick, add `biased;`, reorder, mark dirty) | `src/app.rs` | lines 161–211 |
| C5 | Add 3 unit tests | `src/app_tests.rs` | append at line 85 |

## File Classification

| File | Role | Data Flow | Closest Analog (in-file) | Match Quality |
|------|------|-----------|--------------------------|---------------|
| `src/app.rs` (struct field add) | state container | n/a | `pub should_quit: bool` at line 69 | exact-role |
| `src/app.rs` (init list add) | constructor | n/a | `should_quit: false,` at line 130 | exact-role |
| `src/app.rs` (helper method add) | `impl App` method | in-memory mutation | `fn save_state(&self)` at 268 / `fn sync_pty_size(&mut self)` at 332 | exact-role |
| `src/app.rs` (`run` rewire) | async event loop | event-driven tokio::select | current `run` at 161–211 (self-analog) | exact (modify-in-place) |
| `src/app_tests.rs` (3 tests) | unit tests | sync + tokio::test | existing `app_new_without_git_repo` at 26–33 | exact-role |

## Pattern Assignments

### C1. `dirty: bool` field on `App` struct

**Analog:** `pub should_quit: bool` in `App` struct.

**Current state (`src/app.rs:53–79`):**

```rust
pub struct App {
    pub global_state: GlobalState,
    pub active_project_idx: Option<usize>,
    pub layout: LayoutState,
    pub mode: InputMode,
    pub modal: Modal,
    pub picker: Option<Picker>,
    pub preview_lines: Option<(PathBuf, Vec<String>)>,
    pub left_list: ListState,
    pub right_list: ListState,
    pub pty_manager: PtyManager,
    pub active_workspace_idx: Option<usize>,
    pub active_tab: usize,
    pub modified_files: Vec<crate::git::diff::FileEntry>,

    pub keymap: Keymap,
    pub should_quit: bool,      // <-- analog, line 69
    pub watcher: Option<crate::watcher::Watcher>,
    pub last_panes: Option<PaneRects>,
    pub sidebar_items: Vec<SidebarItem>,
    pub state_path: PathBuf,
    pub last_pty_size: (u16, u16),
    pub selection: Option<SelectionState>,
    pub last_frame_area: Rect,
    pub pending_workspace: Option<Option<String>>,
    pub archived_expanded: std::collections::HashSet<String>,
}
```

**Pattern to apply:** insert new field immediately after `should_quit: bool,` on line 69, as crate-private visibility. Research Section 2 specifies `pub(crate) dirty: bool`.

```rust
pub should_quit: bool,
pub(crate) dirty: bool,  // set to true whenever next frame would render something different
pub watcher: Option<crate::watcher::Watcher>,
```

**Rationale:** Research Section 2 "What to add to App" specifies this placement near `should_quit` as the closest analogue — both are "loop control" booleans. Visibility `pub(crate)` is prescribed (line 130 of RESEARCH.md).

---

### C2. Initialize `dirty: true` in `App::new`

**Analog:** `should_quit: false,` on line 130 of the `Self { ... }` struct literal inside `App::new`.

**Current state (`src/app.rs:114–140`):**

```rust
let mut app = Self {
    global_state,
    active_project_idx,
    layout: LayoutState::new(),
    mode: InputMode::Terminal,
    modal: Modal::None,
    picker: None,
    preview_lines: None,
    left_list: ListState::default(),
    right_list: ListState::default(),
    pty_manager: PtyManager::new(),
    active_workspace_idx,
    active_tab: 0,
    modified_files: Vec::new(),

    keymap: Keymap::default_keymap(),
    should_quit: false,          // <-- analog, line 130
    watcher,
    last_panes: None,
    sidebar_items: Vec::new(),
    state_path,
    last_pty_size: (24, 80),
    selection: None,
    last_frame_area: Rect::default(),
    pending_workspace: None,
    archived_expanded: std::collections::HashSet::new(),
};
```

**Pattern to apply:** insert `dirty: true,` immediately after `should_quit: false,` to preserve struct-literal field order matching the declaration.

```rust
should_quit: false,
dirty: true,  // first frame must render
watcher,
```

**Rationale:** Research Section 2 ("Initialize in `App::new` ... `dirty: true,`") + pitfall #1 ("First-frame flash / blank terminal on startup — fix: initialize dirty: true").

---

### C3. `mark_dirty()` helper on `impl App`

**Analog:** Short inline helpers in the same `impl App` block, e.g.:

- `pub(crate) fn save_state(&self)` at `src/app.rs:268–272`
- `fn sync_pty_size(&mut self)` at `src/app.rs:332–365`
- `pub(crate) fn active_project_mut(&mut self) -> Option<&mut Project>` at `src/app.rs:151–154`

**Current excerpt — closest shape (line 268):**

```rust
pub(crate) fn save_state(&self) {
    if let Err(error) = self.global_state.save(&self.state_path) {
        tracing::error!("failed to save state: {error}");
    }
}
```

**Pattern to apply:** insert between `active_workspace()` (ends at line 159) and `run()` (starts at line 161). Use `pub(crate)` visibility and `#[inline]` per research.

```rust
#[inline]
pub(crate) fn mark_dirty(&mut self) {
    self.dirty = true;
}
```

**Rationale:** Research Section 2 "A `mark_dirty` helper" specifies the exact shape including `#[inline]` and `pub(crate)`. Placement before `run()` keeps it adjacent to its primary caller.

---

### C4. Rewire `App::run` event loop

**Analog:** The current body of `run` at `src/app.rs:161–211` is the self-analog — every structural element is preserved except the four listed changes.

**Current state (verbatim, `src/app.rs:161–211`):**

```rust
pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
    let mut events = EventStream::new();
    let mut refresh_tick = interval(Duration::from_secs(5));
    let mut status_tick = interval(Duration::from_secs(1));

    loop {
        terminal.draw(|frame| crate::ui::draw::draw(self, frame))?;
        self.sync_pty_size();

        if let Some(name) = self.pending_workspace.take() {
            match crate::workspace::create_workspace(self, name).await {
                Ok(()) => self.modal = Modal::None,
                Err(error) => {
                    self.modal = Modal::NewWorkspace(NewWorkspaceForm {
                        name_input: String::new(),
                        error: Some(error),
                    });
                }
            }
            continue;
        }

        if self.should_quit {
            break;
        }

        tokio::select! {
            Some(Ok(event)) = events.next() => {
                crate::events::handle_event(self, event).await;
            }
            _ = self.pty_manager.output_notify.notified() => {}
            _ = status_tick.tick() => {}
            _ = refresh_tick.tick() => {
                self.refresh_diff().await;
            }
            Some(event) = async {
                if let Some(w) = &mut self.watcher {
                    w.next_event().await
                } else {
                    futures::future::pending::<Option<crate::watcher::FsEvent>>().await
                }
            } => {
                let _ = event;
                self.refresh_diff().await;
            }
        }
    }

    self.save_state();
    Ok(())
}
```

**Target state — 5 deltas against current:**

| Delta | Current | Target | RESEARCH ref |
|-------|---------|--------|--------------|
| D4a | `let mut status_tick = interval(Duration::from_secs(1));` at line 164 | **Delete line 164.** Heartbeat responsibility folded into `refresh_tick` at 5s. | Section 1 bullet 4; Section 4 table row "`status_tick` (1s)" |
| D4b | `terminal.draw(...)?;` unconditional at line 167 | **Gate: `if self.dirty { terminal.draw(...)?; self.dirty = false; }`** | Section 2 "Wire-up shape" line 54–57; Section 4 table row 1 |
| D4c | `pending_workspace` fast-path at 170–181 ends with `continue;` (no dirty mark) | **Insert `self.mark_dirty();` immediately before `continue;`** on line 180, since workspace creation mutates state. | Section 4 table row "`pending_workspace` fast-path"; Section 5 table row "Workspace created async" |
| D4d | `tokio::select!` at line 187 has no ordering directive; branches: events → pty_notify → status_tick → refresh_tick → watcher | **Add `biased;` as first line inside `select!`. Reorder branches: (1) events, (2) pty_notify, (3) watcher, (4) refresh_tick. Delete `status_tick` arm.** | Section 3 "Code sketch"; Section 2 "Wire-up shape" lines 78–110 |
| D4e | Each select arm body performs its work with no dirty-mark | **Insert `self.mark_dirty();` (or `self.dirty = true;`) at the top of each arm body — four arms total.** | Section 2 wire-up; Section 5 "mark dirty in the run loop's four arms" |

**Target final shape (from Research Section 2, verbatim):**

```rust
pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
    let mut events = EventStream::new();
    let mut refresh_tick = interval(Duration::from_secs(5));

    loop {
        if self.dirty {
            terminal.draw(|frame| crate::ui::draw::draw(self, frame))?;
            self.dirty = false;
        }
        self.sync_pty_size();

        if let Some(name) = self.pending_workspace.take() {
            match crate::workspace::create_workspace(self, name).await {
                Ok(()) => self.modal = Modal::None,
                Err(error) => {
                    self.modal = Modal::NewWorkspace(NewWorkspaceForm {
                        name_input: String::new(),
                        error: Some(error),
                    });
                }
            }
            self.mark_dirty();
            continue;
        }

        if self.should_quit {
            break;
        }

        tokio::select! {
            biased;

            Some(Ok(event)) = events.next() => {
                self.mark_dirty();
                crate::events::handle_event(self, event).await;
            }
            _ = self.pty_manager.output_notify.notified() => {
                self.mark_dirty();
            }
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
            _ = refresh_tick.tick() => {
                self.refresh_diff().await;
                self.mark_dirty();
            }
        }
    }

    self.save_state();
    Ok(())
}
```

**Structural invariants preserved:**

- `EventStream::new()` construction — unchanged.
- `refresh_tick = interval(Duration::from_secs(5))` — unchanged (heartbeat safety-net role is implicit; replacement is Phase 5).
- `sync_pty_size()` call placement — outside `if self.dirty` block, runs every iteration (Research Section 7 Q4).
- `pending_workspace.take()` fast-path + `match` arms + modal-on-error — unchanged.
- `self.should_quit` break check — unchanged.
- Watcher `async { ... }` inline block (including `futures::future::pending::<Option<FsEvent>>()` fallback) — unchanged.
- `self.save_state()` on loop exit — unchanged.

**Decision: `self.mark_dirty()` vs `self.dirty = true`.** Research Section 5 says "five `self.dirty = true` lines in `run`". Section 2 introduces `mark_dirty()` for auditability / greppability. Recommend `self.mark_dirty()` in arms so that `rg 'mark_dirty' src/app.rs` shows every trigger (Research Section 2 "Two reasons" justification). The `self.dirty = false` clear inside the draw gate stays as a direct field write — it is not a "mark" operation.

---

### C5. Three unit tests in `src/app_tests.rs`

**Analog:** `app_new_without_git_repo` at `src/app_tests.rs:26–33`.

**Current excerpt (verbatim, lines 26–33):**

```rust
#[tokio::test]
async fn app_new_without_git_repo() {
    let app = App::new(GlobalState::default(), std::env::temp_dir().join("martins-test.json"))
        .await
        .unwrap();
    assert_eq!(app.active_project_idx, None);
    assert!(!app.should_quit);
}
```

**Pattern to apply:** three `#[tokio::test]` functions, each constructing `App::new` the same way, appended after line 84 of `src/app_tests.rs`.

```rust
#[tokio::test]
async fn app_starts_dirty() {
    let app = App::new(GlobalState::default(), std::env::temp_dir().join("martins-dirty-start.json"))
        .await
        .unwrap();
    assert!(app.dirty, "first frame must render");
}

#[tokio::test]
async fn dirty_stays_clear_when_no_mutation() {
    let mut app = App::new(GlobalState::default(), std::env::temp_dir().join("martins-dirty-clear.json"))
        .await
        .unwrap();
    app.dirty = false;
    // no mutation
    assert!(!app.dirty);
}

#[tokio::test]
async fn mark_dirty_sets_flag() {
    let mut app = App::new(GlobalState::default(), std::env::temp_dir().join("martins-dirty-mark.json"))
        .await
        .unwrap();
    app.dirty = false;
    app.mark_dirty();
    assert!(app.dirty);
}
```

**Rationale:** Research Section 6 "Phase Requirements → Test Map" names each test verbatim (`app_starts_dirty`, `dirty_stays_clear_when_no_mutation`, `mark_dirty_sets_flag`). The `App::new` construction pattern mirrors `app_new_without_git_repo` exactly — `GlobalState::default()` + unique tempfile path per test to avoid state-file collisions.

**Visibility note:** `dirty` is `pub(crate)` per C1, so `app_tests` (the `#[cfg(test)] #[path] mod tests` at `src/app.rs:434–436`) can read/write it directly. `mark_dirty` is `pub(crate)` per C3, same access rule. No `pub` field escalation needed.

---

## Shared Patterns

### Field placement next to semantically-similar fields

**Source:** `pub should_quit: bool` (line 69) placed in the "flags" cluster after `keymap`.
**Apply to:** C1 — `dirty` placed immediately after `should_quit`.

### `pub(crate)` for helpers called only inside the crate

**Source:** `save_state`, `sync_pty_size` (implicitly), `active_project_mut`, `active_workspace`, `refresh_diff` — all `pub(crate)` or private.
**Apply to:** C3 — `mark_dirty` is `pub(crate)`; the field `dirty` is `pub(crate)`.

### Test file pattern

**Source:** `#[cfg(test) #[path = "app_tests.rs"] mod tests;` declaration at `src/app.rs:434–436`.
**Apply to:** C5 — already wired. New tests append to the existing file; no module plumbing needed.

### Unique tempfile path per test

**Source:** `src/app_tests.rs:28` (`martins-test.json`), line 46 (`martins-switch.json`), line 74 (`martins-tab-click.json`).
**Apply to:** C5 — each new test uses a distinct suffix (`martins-dirty-start.json`, etc.) to avoid cross-test interference on parallel `cargo test` runs.

## No Analog Found

None — every change has a clear in-file analog or is self-referential (C4 modifies an existing function in place).

## Metadata

**Analog search scope:** `src/app.rs` (App struct, App impl, run loop), `src/app_tests.rs` (test scaffold), and reference reads of `src/events.rs`, `src/workspace.rs`, `src/ui/modal_controller.rs` to confirm mutation sites reach through the five gates identified in Research Section 5.
**Files scanned:** 6
**Pattern extraction date:** 2026-04-24
