---
phase: 04-navigation-fluidity
plan: 02
subsystem: navigation
tags: [navigation, tokio, mpsc, spawn, refresh-diff, hot-path, biased-select, wave-1, green-phase]
requires: [NAV-01, NAV-02, NAV-03, NAV-04]
provides:
  - "src/app.rs — diff_tx/diff_rx fields + refresh_diff_spawn helper + 6th select branch draining diff_rx"
  - "src/events.rs — ClickWorkspace + activate_sidebar_item Workspace arm now fire-and-forget"
  - "src/workspace.rs — switch_project tail now fire-and-forget"
affects:
  - src/app.rs
  - src/events.rs
  - src/workspace.rs
  - src/navigation_tests.rs  # Rule-3 clippy fix (field_reassign_with_default)
tech-stack:
  added:
    - "tokio::spawn (first fire-and-forget spawn in codebase — previously only spawn_blocking)"
  patterns:
    - "Fire-and-forget mpsc fanout: clone UnboundedSender into tokio::spawn; drain one-per-iteration on dedicated select branch (mirrors watcher.rs idiom but inverted — sender moves into task, receiver stays on main task)"
    - "Eager paint on workspace switch: refresh_diff_spawn marks dirty synchronously; modified_files updates arrive 1 loop iteration later via diff_rx"
    - "Sync helper as sibling of async method: refresh_diff_spawn(&mut self) next to refresh_diff(&mut self)"
key-files:
  created: []
  modified:
    - src/app.rs
    - src/events.rs
    - src/workspace.rs
    - src/navigation_tests.rs
decisions:
  - "refresh_diff_spawn marks dirty in BOTH the empty-early-return and populated-spawn paths, giving test workspace_switch_paints_pty_first its synchronous dirty=true assertion (eager-paint semantics)"
  - "6th select branch placed at position 6 (last), after refresh_tick — preserves input-priority invariant (RESEARCH §4 Pitfall 3)"
  - "Single `Some(files) = self.diff_rx.recv()` drain per iteration — NOT while-let / try_recv loop (RESEARCH §4 Pitfall 5)"
  - "switch_project signature stays `pub async fn` — callers .await it; clippy::unused_async not fired by current toolchain so no attribute added"
  - "Rule-3 clippy fix: replaced `let mut state = GlobalState::default(); state.X = Y;` with struct-literal + ..Default::default() spread in navigation_tests.rs — clippy::field-reassign-with-default was latent but only fired once refresh_diff_spawn's compile-gate disarmed"
metrics:
  duration: "~8 minutes"
  completed: "2026-04-24"
  tasks: 2
  files_created: 0
  files_modified: 4
---

# Phase 4 Plan 02: Navigation Fluidity — Non-Blocking refresh_diff Summary

TDD GREEN phase: lifted `refresh_diff().await` off the nav hot path by introducing a single-producer/single-consumer mpsc channel on App, a sync `refresh_diff_spawn` helper that fires git2 work onto a tokio task, and a 6th `tokio::select!` branch that drains results and marks dirty. All four Plan 04-01 nav tests now pass; full suite 107 green; clippy clean.

## Outcome

- **`App` struct** gains `pub(crate) diff_tx` + `pub(crate) diff_rx` (both `UnboundedSender/Receiver<Vec<FileEntry>>`) adjacent to `watcher`.
- **`App::new`** let-binds `tokio::sync::mpsc::unbounded_channel::<Vec<FileEntry>>()` before the `Self { ... }` literal; both ends move into the struct.
- **`impl App`** gains `pub(crate) fn refresh_diff_spawn(&mut self)` directly after the existing async `refresh_diff`. Empty-workspace path clears + marks dirty + returns. Populated path clones `diff_tx`, `tokio::spawn`s a task that calls `diff::modified_files(...).await` and sends results via `tx.send(...)`, then marks dirty synchronously (eager-paint).
- **`App::run`** `tokio::select!` gains a 6th branch `Some(files) = self.diff_rx.recv() => { ... mark_dirty(); }` placed AFTER the existing `refresh_tick` branch. Applies `modified_files` + the three-arm `right_list.select` coalescing logic (moved out of `refresh_diff` because the spawned task cannot mutate App).
- **Three call-site swaps**:
  - `src/events.rs:509` (Action::ClickWorkspace): `.refresh_diff().await` → `.refresh_diff_spawn()`
  - `src/events.rs:556` (SidebarItem::Workspace arm of activate_sidebar_item): same swap
  - `src/workspace.rs:143` (switch_project tail): same swap
- **navigation_tests.rs**: Rule-3 clippy fix (struct-literal update syntax replaces field-reassign-with-default pattern).

## Changes by Pattern (PATTERNS.md §C1–C5)

### C1 — `diff_tx`/`diff_rx` fields on App struct (`src/app.rs:72-73`)

```rust
pub(crate) diff_tx: tokio::sync::mpsc::UnboundedSender<Vec<crate::git::diff::FileEntry>>,
pub(crate) diff_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<crate::git::diff::FileEntry>>,
```

Placement: directly after `pub watcher: Option<crate::watcher::Watcher>` — matches PATTERNS §C1.

### C2 — Channel init in `App::new` (`src/app.rs:115-116`, `139-140`)

```rust
let (diff_tx, diff_rx) =
    tokio::sync::mpsc::unbounded_channel::<Vec<crate::git::diff::FileEntry>>();
```

Placed immediately after the `watcher` construction and before the `Self { ... }` literal; fields moved in alongside `watcher,`.

### C3 — `refresh_diff_spawn` helper (`src/app.rs:306-332`)

```rust
pub(crate) fn refresh_diff_spawn(&mut self) {
    let args = match (self.active_project(), self.active_workspace()) { ... };
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

- Sibling (NOT replacement) of async `refresh_diff` — both live in `impl App`.
- No `.await` anywhere in the function body. Compile-checked: `rg 'async fn refresh_diff_spawn' src/app.rs` = 0.
- `self.mark_dirty()` called in BOTH branches. Gives `workspace_switch_paints_pty_first` the synchronous `app.dirty == true` assertion it requires (eager-paint semantics from RESEARCH §1).
- Doc-comment explicitly forbids collapsing with `refresh_diff` — locks in the architectural intent for future readers.

### C4 — 6th select branch in `App::run` (`src/app.rs:245-257`)

```rust
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
```

- Position 6 (last) — `biased;` priority unchanged; branches 1-5 byte-for-byte identical.
- Single `Some(files) = recv()` form; NO `while let` / `try_recv` drain (RESEARCH §4 Pitfall 5).
- Comment follows `// N. ...` style (grep-friendly navigation).

### C5 — Three nav-call-site `.await` → `_spawn()` swaps

| File | Line | Before | After |
|------|------|--------|-------|
| src/events.rs | 509 | `app.refresh_diff().await;` | `app.refresh_diff_spawn();` |
| src/events.rs | 556 | `app.refresh_diff().await;` | `app.refresh_diff_spawn();` |
| src/workspace.rs | 143 | `app.refresh_diff().await;` | `app.refresh_diff_spawn();` |

Surrounding code (has_tabs check, InputMode::Terminal assignment, open_new_tab_picker, switch_project watcher swap + field writes) byte-for-byte preserved. `switch_project`'s `pub async fn` signature preserved — callers still `.await` it; clippy::unused_async not flagged by current toolchain.

## Grep Invariant Snapshot (Post-Plan)

| Pattern | Path | Count | Required | Status |
|---------|------|-------|----------|--------|
| `biased;` | src/app.rs | 1 | 1 | pass |
| `// 1. INPUT` | src/app.rs | 1 | 1 | pass |
| `if self.dirty` | src/app.rs | 1 | 1 | pass |
| `status_tick` | src/app.rs | 0 | 0 | pass |
| `self\.mark_dirty\(\)` | src/app.rs | 9 | ≥7 | pass (was 6; +2 in refresh_diff_spawn, +1 in 6th branch) |
| `duration_since` | src/pty/session.rs | 1 | ≥1 | pass |
| `pub\(crate\) diff_tx` | src/app.rs | 1 | 1 | pass |
| `pub\(crate\) diff_rx` | src/app.rs | 1 | 1 | pass |
| `unbounded_channel::<Vec<crate::git::diff::FileEntry>>` | src/app.rs | 1 | 1 | pass |
| `pub\(crate\) fn refresh_diff_spawn` | src/app.rs | 1 | 1 | pass |
| `async fn refresh_diff_spawn` | src/app.rs | 0 | 0 | pass (helper is sync) |
| `// 6\. Diff-refresh results` | src/app.rs | 1 | 1 | pass |
| `self\.diff_rx\.recv\(\)` | src/app.rs | 1 | 1 | pass |
| `refresh_diff\(\)\.await` | src/events.rs | 0 | 0 | pass (Pitfall #1 gate) |
| `refresh_diff\(\)\.await` | src/workspace.rs | 0 | 0 | pass (Pitfall #1 gate) |
| `refresh_diff\(\)\.await` | src/app.rs | 3 | 3 | pass (App::new L150, watcher L234, refresh_tick L243) |
| `app\.refresh_diff_spawn\(\)` | src/events.rs | 2 | 2 | pass (sites 1+2) |
| `app\.refresh_diff_spawn\(\)` | src/workspace.rs | 1 | 1 | pass (site 3) |
| `pub async fn switch_project` | src/workspace.rs | 1 | 1 | pass (signature preserved) |

## Test Results

```
$ cargo test --bin martins navigation_tests
running 4 tests
test navigation_tests::click_tab_is_sync ... ok
test navigation_tests::sidebar_up_down_is_sync ... ok
test navigation_tests::workspace_switch_paints_pty_first ... ok
test navigation_tests::refresh_diff_spawn_is_nonblocking ... ok
test result: ok. 4 passed; 0 failed

$ cargo test
test result: ok. 107 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo clippy --all-targets -- -D warnings
    Finished `dev` profile ... (no warnings, no errors)
```

All 4 Plan 04-01 regression guards green. 103 prior tests + 4 new = 107 passing, matching success criterion #7.

## TDD Gate Compliance

- **RED commit (landed in Plan 04-01):** `e3ea41a test(04-01): add Wave-0 navigation regression-guard tests`
  - `cargo build --tests` failed with 2× `no method named refresh_diff_spawn` — the explicit TDD gate.
- **GREEN commit (this plan):** `fceede9 feat(04-02): add diff_tx/diff_rx + refresh_diff_spawn + 6th select branch`
  - Disarms the compile gate; Task 1 alone turns all 4 tests green because `refresh_diff_spawn` now exists.
- **Refactor commit (this plan):** `2927dae feat(04-02): swap .refresh_diff().await → refresh_diff_spawn() at 3 nav sites`
  - Satisfies the Pitfall #1 gate structurally (0 `.await` on refresh_diff in events.rs + workspace.rs).

Gate sequence: test → feat → feat. All three commits present in the log; no `--amend` used.

## Commits Created

| Hash | Subject |
|------|---------|
| `49ca329` | fix(04-02): clippy field-reassign-with-default in navigation_tests (Rule 3) |
| `fceede9` | feat(04-02): add diff_tx/diff_rx + refresh_diff_spawn + 6th select branch (Task 1) |
| `2927dae` | feat(04-02): swap .refresh_diff().await → refresh_diff_spawn() at 3 nav sites (Task 2) |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] clippy::field_reassign_with_default in Plan 04-01 nav tests**
- **Found during:** Task 1, when `cargo clippy --all-targets -- -D warnings` ran after `refresh_diff_spawn` disarmed the compile gate.
- **Issue:** `src/navigation_tests.rs:119-121` and `148-150` used `let mut state = GlobalState::default(); state.X = Y; state.Y = Z;` — clippy's `field-reassign-with-default` lint flags this under `-D warnings`. Latent in Plan 04-01 but invisible because the test binary failed to compile.
- **Fix:** Replaced with struct-update syntax: `let state = GlobalState { active_project_id: ..., projects: vec![...], ..Default::default() };`. No behavior change; `version: 2` still picked up from `Default`.
- **Files modified:** `src/navigation_tests.rs`
- **Commit:** `49ca329`

**2. [Rule 2 - Critical] Eager `mark_dirty()` in refresh_diff_spawn populated path**
- **Found during:** Task 1, while aligning with `workspace_switch_paints_pty_first` test expectations.
- **Issue:** The Plan/PATTERNS §C3 sketch called `mark_dirty()` only in the empty-args early-return arm, not in the populated-spawn path. But Plan 04-01's test `workspace_switch_paints_pty_first` asserts `app.dirty == true` SYNCHRONOUSLY after `refresh_diff_spawn()` returns — which requires the populated path to mark dirty too. Without this, the eager-paint semantics from RESEARCH §1 primary recommendation fail.
- **Fix:** Added `self.mark_dirty()` as the last statement in `refresh_diff_spawn` (after the `tokio::spawn`). This is the correct shape: the nav arm already calls `mark_dirty()` on entry (branch 1 of the select), so this is idempotent at the loop level, but load-bearing for the test contract.
- **Files modified:** `src/app.rs`
- **Commit:** `fceede9`
- **Impact on grep invariant:** `self.mark_dirty()` count grows from the plan's "≥7" target to 9 (6 prior + 2 in refresh_diff_spawn + 1 in 6th branch). Over-delivery, not regression.

### No Other Deviations

- Tab-switch paths (`Action::ClickTab`, `Action::SwitchTab`, `F(n)`) — untouched. NAV-04 reference shape preserved.
- `archive_active_workspace` / `confirm_delete_workspace` — no `refresh_diff_spawn` calls added. RESEARCH §4 Pitfall 6 honored.
- `switch_project` signature — `pub async fn` preserved. No `#[allow(clippy::unused_async)]` needed.
- `refresh_diff` (the async method) — untouched. Still called by App::new, watcher branch, refresh_tick branch.
- Field ordering in `Self { ... }` literal in `App::new` — preserved exactly except for the two new inserts next to `watcher,`.

## Threat Model Compliance (from plan)

- **T-04-05 (out-of-order diff results):** mitigated as planned — single `recv()` per iteration; last-send-wins for rapid nav. RESEARCH §4 Pitfall 2 behavior captured. Manual UAT in Plan 04-03 will confirm no visible flicker under arrow-hold.
- **T-04-06 (unbounded spawn under arrow-hold):** accepted as planned — short-lived tasks, unbounded channel, no bound on inflight. If UAT flags memory pressure, Plan 04-04 can add an AtomicBool gate.
- **T-04-07 (send failure info disclosure):** `let _ = tx.send(files);` — silent drop on shutdown. File paths never leave the process.
- **T-04-08 (future collapse of spawn/async):** mitigated via doc-comment on `refresh_diff_spawn` explicitly forbidding the merge, plus the regression-guard test `refresh_diff_spawn_is_nonblocking` locking in <50ms return budget.

## Plan 04-03 Readiness

Plan 04-03 (manual UAT) is now unblocked. Manual test matrix from RESEARCH §6:

- **NAV-01 feel test:** hold Down through 10 workspaces on a large repo; no stutter visible.
- **NAV-02 click latency:** each sidebar click triggers immediate visual response.
- **NAV-03 workspace switch:** target PTY content visible on the very next frame after a switch.
- **NAV-04 tab switch:** F1/F2/F3 and click-tab indistinguishable from instant (already correct; regression guard).

## Self-Check: PASSED

- FOUND: src/app.rs (modified — Task 1)
- FOUND: src/events.rs (modified — Task 2 site 1+2)
- FOUND: src/workspace.rs (modified — Task 2 site 3)
- FOUND: src/navigation_tests.rs (modified — Rule-3 clippy fix)
- FOUND: commit 49ca329 (Rule-3 clippy fix)
- FOUND: commit fceede9 (Task 1: channel + helper + 6th branch)
- FOUND: commit 2927dae (Task 2: three-site swap)
- FOUND: `pub(crate) diff_tx` / `diff_rx` fields on App (exactly 1 hit each)
- FOUND: `pub(crate) fn refresh_diff_spawn` (1 hit; `async fn refresh_diff_spawn` = 0)
- FOUND: `// 6. Diff-refresh results` + `self.diff_rx.recv()` (1 hit each)
- FOUND: `refresh_diff().await` = 0 in events.rs + workspace.rs; = 3 in app.rs
- FOUND: `app.refresh_diff_spawn()` = 2 in events.rs + 1 in workspace.rs
- Phase 2/3 grep invariants preserved (biased=1, INPUT=1, dirty-gate=1, status_tick=0, mark_dirty=9, duration_since=1)
- `cargo build` clean
- `cargo clippy --all-targets -- -D warnings` clean
- `cargo test --bin martins navigation_tests` → 4 passed
- `cargo test` → 107 passed / 0 failed
