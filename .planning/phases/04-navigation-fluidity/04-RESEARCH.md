# Phase 4: Navigation Fluidity — Research

**Researched:** 2026-04-24
**Domain:** sidebar keyboard/mouse navigation, workspace switching, tab switching, render-loop pacing under UI state mutation
**Confidence:** HIGH (Phase 2 + Phase 3 primitives already cover most of the path; the real work is finding the one or two remaining blocking-on-main-task operations and excising them)

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| NAV-01 | Keyboard navigation in the sidebar (up/down/select) responds within one frame with no visible stutter | §3 Sidebar Keyboard Path + §5 Pitfalls + §6 Validation |
| NAV-02 | Mouse click on a sidebar item activates it instantly with no visible pause | §4 Mouse Click Path + §5 Pitfalls + §6 Validation |
| NAV-03 | Switching between workspaces presents the target workspace's PTY view instantaneously (no re-render stutter or blank frame) | §4 Workspace-switch Blocking-on-refresh_diff (THE main finding) + §5 Pitfalls #1 |
| NAV-04 | Switching between tabs within a workspace is instantaneous | §3 Tab-Switch Path (trivial — already correct) + §5 Pitfalls |

## 1. Executive Summary

Phase 2 landed the dirty-flag gate and input-priority `tokio::select!`. Phase 3 validated the PTY-input path as synchronous and Ghostty-equivalent. **Phase 4 is what's left: make the input-arm *body* itself non-blocking under the nav-adjacent actions.**

After reading the full event-routing + workspace-lifecycle + draw path, there is **exactly one architecturally significant stutter source in the codebase, and three smaller ones:**

1. **BLOCKING (the real one) — `refresh_diff().await` inside the input arm.** When the user switches workspaces via keyboard (`activate_sidebar_item` → `refresh_diff().await`), clicks a workspace (`dispatch_action(Action::ClickWorkspace)` → `refresh_diff().await`), or clicks a file (same path), the event-arm body *awaits* a `tokio::task::spawn_blocking` that runs `git2::Repository::open` + `diff_tree_to_workdir_with_index` + `statuses()`. On a medium repo this is 10–100ms; on a large repo with thousands of untracked files, several hundred ms. **During that await, the `tokio::select!` loop is parked on the input arm's future.** The draw cannot fire, and the user sees a frozen frame between clicking/pressing and the target workspace appearing. [VERIFIED: `src/events.rs:509, 556`; `src/workspace.rs:143`; `src/git/diff.rs:33-112`]

2. **BLOCKING (smaller) — `pbcopy` spawn in `copy_selection_to_clipboard`.** Not a nav path; here for completeness. Phase 6 territory.

3. **SYNCHRONOUS tmux inside nav — `archive_active_workspace`.** Clicking the `✕` on an active workspace calls `crate::tmux::kill_session(...)` + `std::fs::remove_dir_all(&worktree_path)` inline on the input arm. Kill is fast (<10ms usually); `remove_dir_all` on a worktree with many files can block for hundreds of ms. Not in NAV success criteria (archive is a destructive op; user can accept a brief pause), but worth flagging. [VERIFIED: `src/workspace.rs:161-182`]

4. **RENDER-SIDE — workspace switch triggers one draw per state mutation step.** `switch_project` mutates 6+ fields (`active_project_idx`, `active_workspace_idx`, `active_tab`, `preview_lines`, `right_list`, plus the `watcher` swap) but the dirty-flag policy is coarse — the whole arm marks dirty once. This is actually *correct* and not a stutter source; flagging here only to confirm that Phase 4 does NOT need finer-grained marking. [VERIFIED: `src/workspace.rs:118-144`, `src/app.rs:211`]

**Primary recommendation:**

> **Make `refresh_diff` non-blocking with respect to the nav event-arm.** The call-sites that trigger it synchronously during navigation (`switch_project`, `ClickWorkspace`, `activate_sidebar_item`, `confirm_remove_project`) should fire-and-forget: spawn `refresh_diff` onto a tokio task, let it mutate `app.modified_files` + `mark_dirty` when it completes, and return immediately from the event arm. The user sees the workspace switch in the very next frame; the diff pane gets populated one tokio wakeup later. Visually indistinguishable from "instant" because the PTY pane is the load-bearing visual element; the right sidebar diff list trailing by 10–100ms is well under the perceptual threshold. [CITED: `src/events.rs:509, 556`; `src/workspace.rs:143`]

**Three concrete deliverables:**

- **Async-boundary `refresh_diff`.** Either (a) spawn it as a background task from the nav call-sites and drop the `.await`, or (b) introduce a `pending_refresh: bool` flag that the run-loop picks up on a new select branch. Option (a) is the minimum change and fits the existing `pending_workspace` fast-path pattern. Needs a shared-state channel (e.g., `tokio::sync::mpsc`) or an `Arc<Mutex<Vec<FileEntry>>>` to get the results back into `app.modified_files`.
- **Eager-paint on workspace switch.** After mutating `active_workspace_idx` / `active_tab` but before any `.await`, call `mark_dirty()` so that the next loop iteration draws the target workspace's PTY buffer *immediately*. The PTY buffer is already live (tmux sessions are persistent per `reattach_tmux_sessions`), so there's no "loading" step to hide. This is mostly already the case — the input-arm marks dirty at entry — but the `.await` on `refresh_diff` currently blocks that draw from happening.
- **Targeted automated tests for NAV-03.** Because we cannot automate "feels instant," we verify **absence of the await-block** structurally: a test that asserts `activate_sidebar_item` and the `ClickWorkspace` dispatch path return within N ms even when the git repo is large (use `tempfile::TempDir` + 10k-file fixture). The test is a behavior contract, not a latency metric.

**Idle CPU after Phase 4:** unchanged from Phase 3. No new timers, no new always-on tasks. Background `refresh_diff` tasks are self-terminating.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Keyboard nav keystroke reception | crossterm EventStream | tokio task (`App::run`) | Same entry path as PTY-input; crossterm thread + EventStream channel |
| Keymap resolution (key → Action) | `crate::keys::Keymap::resolve_normal` | pure fn in `src/keys.rs` | Pure HashMap lookup; sync |
| Sidebar list-selection update | `crate::events::move_sidebar_to_workspace` + `ListState::select` | ratatui `ListState` | Synchronous field write on `App.left_list`; constant-time |
| Active-workspace switch (state) | `crate::workspace::switch_project` + `App::select_active_workspace` | App struct field writes | Synchronous field writes EXCEPT `refresh_diff().await` at end |
| Active-tab switch (state) | `Action::ClickTab` / `SwitchTab` / `F<n>` handling | App field writes | Pure synchronous assignments on `app.active_tab` / `app.mode` |
| Diff refresh (git→ file list) | `crate::git::diff::modified_files` | tokio::spawn_blocking + git2 | **Currently awaited by nav arm — the stutter source** |
| Sidebar mouse hit-testing | `crate::events::handle_click` | `rect_contains` + `SidebarItem` table | Pure sync rect math |
| Draw decision (which workspace's PTY to show) | `crate::ui::draw::draw` → `crate::ui::terminal::render` | reads `app.active_workspace_idx`, `app.active_tab`, `app.active_sessions()` | Synchronous; already live (tmux persists sessions) |
| PTY view rendering | `tui_term::widget::PseudoTerminal` in draw arm | `vt100::Parser::try_read` | Non-blocking read; fall-through if locked |
| Working-dot indicator animation | 5s `heartbeat_tick` | `App::build_working_map` on draw | Already in place post-Phase-2 |

**Cross-tier correctness checks Phase 4 must preserve:**
- Phase 2 invariants: `biased;`, `// 1. INPUT`, `if self.dirty`, `mark_dirty()≥5`, `status_tick=0`, 8ms throttle, `write_input` sync doc-comment.
- PTY sessions stay live across workspace switches (tmux reattach pattern — already correct; do not regress).
- Working-dot animation still ticks.

## 2. Current State: Navigation Paths End-to-End

### Keyboard sidebar up/down (NAV-01)

Verified trace, `j`/`k` or Down/Up in Normal mode:

```
crossterm Event::Key(Down)
  └─> tokio::select! branch 1 (biased, events.next()) — priority proven PTY-02 test
      └─> self.mark_dirty()                    — Phase 2 policy (blanket)
      └─> crate::events::handle_event(self, event).await
          └─> handle_key (src/events.rs:257)
              └─> mode == Normal, not modal, not picker
              └─> keymap.resolve_normal → Action::NextItem
              └─> dispatch_action(app, Action::NextItem).await   // no await inside
                  └─> move_sidebar_to_workspace(&mut app.left_list, ...)   // O(items) constant, ≤ 50 iterations
                  └─> activate_sidebar_item(app, idx).await      // ← has await path
                      └─> match item {
                            Workspace(p_idx, w_idx) =>
                              app.select_active_workspace(w_idx);
                              app.refresh_diff().await;          // ← BLOCKING (10-100ms on medium repo)
                            _ => return
                          }
[loop iterates, draw fires]
```

**Stutter source:** `activate_sidebar_item` awaits `refresh_diff()` after EVERY up/down press that lands on a Workspace item (the common case when scrolling through workspaces under an expanded project). Holding Down repeats this at the OS key-repeat rate (~30 Hz on macOS default). At 50ms per refresh_diff, that's 50ms between each arrow-key redraw — **visible, per-press stutter.** [VERIFIED: `src/events.rs:367-376, 543-559`]

**Even worse for NAV-01 success criterion 1** ("even when holding the key to scroll a long list") — each refresh_diff awaits a `spawn_blocking` on a tokio worker, but the input arm's future is pending the entire time. No draws can happen between arrow keystrokes.

Items that do NOT trigger refresh_diff on activation:
- `SidebarItem::RemoveProject(idx)` — only switches project, no refresh (L546-550)
- `SidebarItem::ArchivedHeader`, `ArchivedWorkspace`, `AddProject`, `NewWorkspace` — fall through the `_ => {}` arm, no refresh (L558)

### Mouse sidebar click on a workspace (NAV-02)

```
crossterm Event::Mouse(Down Left)
  └─> select branch 1 → mark_dirty → handle_event → handle_mouse (events.rs:38)
      └─> handle_click (events.rs:125)
          └─> sidebar_items.get(local_row) → SidebarItem::Workspace(p, w)
          └─> dispatch_action(app, Action::ClickWorkspace(p, w)).await
              └─> if active_project_idx != Some(p):
                    crate::workspace::switch_project(app, p).await
                      └─> watcher.unwatch / watch            // sync fs
                      └─> field writes (6 assignments)
                      └─> self.refresh_diff().await           // ← BLOCKING
              └─> app.select_active_workspace(w)
              └─> app.refresh_diff().await                    // ← BLOCKING (again!)
              └─> if has_tabs: InputMode::Terminal; else: open_new_tab_picker()
```

**Stutter source (NAV-02 + NAV-03):** the click-workspace path can call `refresh_diff().await` **twice** when the click crosses a project boundary (once inside `switch_project`, once after `select_active_workspace`). [VERIFIED: `src/workspace.rs:143`, `src/events.rs:504-519`]

Items that do NOT await refresh_diff:
- `Action::ClickTab(idx)` → sync assignments, returns immediately. [VERIFIED: `src/events.rs:520-523`]
- `Action::ClickProject(idx)` when project is already active → toggles `project.expanded` + `save_state()`. `save_state()` is sync `fs::write` which can block briefly but is generally <5ms on state.json. [VERIFIED: `src/events.rs:490-503`, `src/state.rs`]
- `Action::ClickFile(idx)` → `create_tab(app, "diff ...").await` — spawns a tmux session (heavier but only on diff-file click, and user expects a new tab to appear).

### Workspace switching (NAV-03)

Two routes: keyboard (via `activate_sidebar_item` Enter) and mouse (via `ClickWorkspace`). Both converge on `select_active_workspace` + `refresh_diff().await`. If the project also changes, `switch_project` fires first — and itself awaits `refresh_diff` at line 143.

**The "blank frame" question:** is there ever a moment where the PTY view is missing during the switch?

- Tmux sessions are reattached at `App::new` via `reattach_tmux_sessions` and remain alive for the app's lifetime; each workspace's tabs have live `PtySession`s kept in `PtyManager.sessions`. [VERIFIED: `src/workspace.rs:24-116`, `src/pty/manager.rs:27`]
- `App::active_sessions()` looks up sessions by `(project_id, workspace_name, tab_id)`; the sessions are cheap HashMap hits. [VERIFIED: `src/app.rs:422-439`]
- `draw::draw` calls `app.active_sessions()` and `app.active_workspace()`; both are synchronous reads. [VERIFIED: `src/ui/draw.rs:60-84`]

**There is no loading state in the draw path.** The target workspace's PTY parser is already populated with its scrollback. A workspace switch is visually complete the instant `active_workspace_idx` is written AND the next `terminal.draw` fires.

So **the only reason the user perceives a "blank frame" or stutter during workspace switching is that `refresh_diff().await` blocks the input arm from returning, which blocks the loop iteration, which blocks the next `terminal.draw`.** Fix the await, fix the switch feel. [VERIFIED: full trace above]

### Tab switching within a workspace (NAV-04)

Keyboard (`SwitchTab(n)` via number keys 1-9, or `F(n)` direct in any mode):

```
Event::Key(Char('3')) in Normal mode
  └─> keymap.resolve → Action::SwitchTab(3)
  └─> dispatch_action (events.rs:434-444)
      app.active_tab = 2; app.mode = Terminal;
[return; loop draws]
```

Or `F3` in any mode (events.rs:258-269):
```
Event::Key(F(3))
  └─> handle_key sees F(3), sets app.active_tab = 2, app.mode = Terminal
      return (bypasses keymap entirely)
```

Mouse click on a tab strip:
```
handle_click → tab_at_column → TabClick::Select(idx) → Action::ClickTab(idx) (events.rs:520-523)
  app.active_tab = idx; app.mode = Terminal;
```

**All three tab-switch paths are pure synchronous field writes.** No await. No blocking. The current state already satisfies NAV-04 structurally. [VERIFIED: `src/events.rs:258-269, 434-444, 520-523`]

Verify this is not a regression risk: the draw path reads `active_tab` synchronously and renders the corresponding session via `tui_term::widget::PseudoTerminal`. Session parsers are live (not re-parsed on tab switch). Rendering cost is ~ratatui-draw cost, <10ms on typical terminals. **Tab switching is already instant — Phase 4 must not break this.** [VERIFIED: `src/ui/draw.rs:60-84`, `src/ui/terminal.rs:145-153`]

## 3. Why Is `refresh_diff` Awaited Here?

Reading the code, the author's intent on each call site:

| Call site | Why the await was added | Actually needed? |
|-----------|-------------------------|-------------------|
| `workspace::switch_project` L143 | Populate modified_files for the new project before the next draw | **No** — the draw can show `modified_files = []` for one frame without visible regression, because the right sidebar shows a file list, not load-bearing UI. One frame ≈ 16ms stale is imperceptible. |
| `events::ClickWorkspace` L509 | Populate modified_files for the new workspace inside the same project | **No** — same rationale. |
| `events::activate_sidebar_item` L556 | Populate modified_files when user keyboard-arrows onto a workspace | **No** — and this is the most user-visible stutter source (holding arrow key). |
| `events::ClickFile` L524-532 | N/A — doesn't await refresh; opens a diff tab | — |
| `App::new` L143 | Populate modified_files on startup | Yes — acceptable; pre-first-frame. |
| `refresh_tick` / watcher in `run` | Background refresh | Yes — already in the run-loop arm, not blocking input. |

**The pattern to introduce:** "fire-and-forget refresh" on workspace/project switch. The user sees the PTY pane update immediately; the right sidebar diff list updates one frame (or one git2 call duration) later. [VERIFIED by cross-referencing all call sites]

### Why not just keep the await and hope refresh_diff is fast?

1. **`git2::Repository::open` alone** takes 5–20ms on a fresh repo with a warm page cache. Cold (first switch after idle): 50–200ms.
2. **`diff_tree_to_workdir_with_index`** walks the index vs. tree and scans the worktree. Proportional to worktree size.
3. **`repo.statuses()` with `recurse_untracked_dirs(true)`** is the slowest — O(worktree file count). Per issue reports on the `git2-rs` tracker, a 10k-file repo takes 200–500ms. [CITED: libgit2 status perf PR discussions — https://github.com/libgit2/libgit2/issues/4230]
4. Cumulative: on medium Rust repos (like martins itself, ~30 files), refresh_diff is fast (~10ms). On the target usage — AI coding agents generating large worktrees — it can hit 100ms+. We have no knob and no cache. [VERIFIED: `src/git/diff.rs:33-112`]
5. `tokio::task::spawn_blocking` moves the work to a worker thread, but `.await`ing the JoinHandle parks the main task until it completes — no help for our event arm.

### The two shapes of the fix

**Option A — Fire-and-forget background task with shared channel (recommended).**

Introduce a `tokio::sync::mpsc::UnboundedSender<Vec<FileEntry>>` that the background task sends to. In the main loop, add a select branch that drains the receiver and updates `app.modified_files`. On workspace/project switch, call `refresh_diff_spawn(app)` which spawns the task and returns immediately.

```rust
// Shape sketch (not final):
pub(crate) fn refresh_diff_spawn(app: &mut App) {
    let Some((path, base_branch)) = app.active_refresh_args() else {
        app.modified_files.clear();
        app.right_list.select(None);
        app.mark_dirty();
        return;
    };
    let tx = app.diff_tx.clone();
    tokio::spawn(async move {
        if let Ok(files) = crate::git::diff::modified_files(path, base_branch).await {
            let _ = tx.send(files);
        }
    });
}

// New select branch in run loop:
Some(files) = app.diff_rx.recv() => {
    app.modified_files = files;
    // re-select logic moved here
    app.mark_dirty();
}
```

**Option B — pending_refresh flag + in-loop run fast-path.**

Mirror the existing `pending_workspace.take()` pattern. Set `app.needs_refresh = true` instead of `.await`ing; the top-of-loop inspection spawns a background task that `mark_dirty` on completion. Similar mechanics to Option A, but avoids adding an mpsc pair.

**Preferred:** Option A. The mpsc is a clean, standard tokio pattern (already used in the codebase's `notify-debouncer-mini` via `watcher.rs`). Option B requires ordering care — the "pending" flag coalesces multiple rapid refresh requests (a user scrolling the sidebar doesn't queue 10 refreshes; only the latest wins). Actually **this coalescing is a benefit for NAV-01** (holding arrow key fires one refresh per keystroke otherwise). Option A with unbounded channel + drain-all semantics also coalesces naturally (keep only the last send's result). A small Option B state flag + single background task with an `AtomicBool::compare_exchange` gate may be simpler still.

**Final recommendation for the planner:** prototype both in a spike; pick whichever reads cleanly. Both satisfy NAV-01/02/03.

## 4. Pitfalls / Don't Do These

### Pitfall 1: Keep the `.await` "just in case"

**What goes wrong:** Someone, reviewing the refactor, argues "but what if the user looks at the diff pane before the background refresh lands?" and keeps `.await` on one of the three call sites. That one site is now the common case and the stutter returns.
**Why it happens:** It feels correct to "wait for data before rendering."
**How to avoid:** The diff pane *always* shows stale-until-next-refresh data anyway (5s refresh_tick + watcher). Removing the await is a **zero-semantics-loss** change — everything that ever observes `modified_files` already tolerates staleness.
**Warning signs:** `rg 'refresh_diff\(\)\.await' src/events.rs src/workspace.rs` returns any hits after the refactor. Acceptance criterion: **0 hits** in those two files (only `src/app.rs::new` should retain it, pre-first-frame).

### Pitfall 2: Spawning refresh_diff on every arrow-keystroke

**What goes wrong:** Naive fire-and-forget spawns a new git2 task on every down-press. User holds the arrow key; 30 tasks are in flight. They all complete eventually, each one overwriting `modified_files` in an order determined by tokio's scheduler. The final state is correct but the intermediate flicker is visible.
**Why it happens:** Fire-and-forget without coalescing.
**How to avoid:** Use a **single in-flight token** — either an `AtomicBool` "refresh_in_flight" gate that skips spawning if one is already queued, OR use the mpsc `try_recv` drain-all pattern (multiple results land, only the last is applied).
**Warning signs:** Rapid workspace nav causes the right sidebar to flicker or show wrong counts.

### Pitfall 3: Breaking the Phase 2/3 invariants

**What goes wrong:** A refactor adds a background task and accidentally moves the `mark_dirty` call off the main task, or reorders select branches putting the new `diff_rx.recv()` before `events.next()`.
**Why it happens:** The "input priority" ordering is semantic, not syntactic.
**How to avoid:** Preserve grep invariants exactly: `biased;`=1, `// 1. INPUT`=1, `if self.dirty`=1, `mark_dirty()`≥5, `status_tick`=0. New select branch goes AFTER the existing branches (5 or 6 if 03-02 lands), NOT between INPUT and PTY output. Add a `// 7. Diff refresh results` annotation for navigability.
**Warning signs:** Any of the Phase 2 grep invariants drops. **Explicitly re-run the Phase 3 "grep invariant snapshot" in 03-01-SUMMARY before closing Phase 4.**

### Pitfall 4: Making tab switching async

**What goes wrong:** Someone "symmetrizes" the workspace-switch and tab-switch code paths by moving `active_tab = n` into a helper that awaits something (e.g., `ensure_tab_ready().await`). Tab switching, which is currently pure-sync and instant, gets a synthetic pause.
**Why it happens:** Refactoring toward "uniform" async shapes.
**How to avoid:** Tab switching is already `NAV-04`-correct — **do not touch it.** Keep `Action::ClickTab`, `Action::SwitchTab`, and the `F(n)` branch as pure field writes. Add a test that asserts tab switching mutates `active_tab` and returns without awaiting anything (compile-time or behavioral).
**Warning signs:** `rg '\.await' src/events.rs | rg -i 'tab'` finds any new hits.

### Pitfall 5: Draining the new mpsc receiver starves input

**What goes wrong:** A while-let `while let Ok(files) = app.diff_rx.try_recv()` drain inside the select arm could, under contrived conditions, loop and delay the next select iteration.
**Why it happens:** Over-eager draining.
**How to avoid:** Drain at most one result per select iteration (`Some(files) = app.diff_rx.recv()`), NOT a while loop. Tokio's biased select re-polls on the next iteration — further results will land naturally.
**Warning signs:** A `while let` or `loop { try_recv }` near the new branch body.

### Pitfall 6: Archive / delete nav paths going through refresh_diff

**What goes wrong:** Archive or delete triggers `refresh_active_workspace_after_change` which changes `active_workspace_idx`. If the refactor makes this path go through `refresh_diff` synchronously (it currently doesn't — both `archive_active_workspace` and `confirm_delete_workspace` call `save_state` but not `refresh_diff`), the archive button gets laggy.
**Why it happens:** Confusingly similar naming.
**How to avoid:** Audit all callers of the refactored refresh before landing. `save_state` is sync-fs and fine for these destructive ops (user expects a brief pause on archive).
**Warning signs:** `rg 'refresh_diff' src/workspace.rs src/events.rs` grows beyond the current 2 sites. [VERIFIED current count: workspace.rs:143, events.rs:509, events.rs:556 = 3; app.rs has the function def and a self-call from `refresh_tick` branch]

### Pitfall 7: Archive path `std::fs::remove_dir_all` inline

**What goes wrong:** `archive_active_workspace` at `src/workspace.rs:181` calls `std::fs::remove_dir_all(&worktree_path)` synchronously on the event arm. For a worktree with 10k+ files (npm node_modules territory), this can block for 1–5 seconds.
**Why it happens:** The worktree deletion is conceptually "finishing the archive" so it reads as part of the op.
**How to avoid:** Spawn the `remove_dir_all` onto `tokio::task::spawn_blocking` with a fire-and-forget pattern. It's destructive — no return value to handle on the main task.
**Warning signs:** User reports "archive feels slow when the workspace has a lot of files." Not in NAV-01/02/03/04 success criteria but adjacent enough to consider in-scope.
**Recommendation:** Optional for Phase 4; can defer if planner wants to keep scope tight.

## 5. Render-Loop Interactions — No Changes Needed

Phase 4 does NOT need to change:

- **Dirty-flag policy.** The coarse "mark dirty on every input arm entry" is correct. It already covers every nav path. [VERIFIED: `src/app.rs:211`]
- **`sync_pty_size`.** Runs outside the dirty gate; cheap no-op when size unchanged. Stays. [VERIFIED: `src/app.rs:181`]
- **`pending_workspace` fast-path.** Unaffected by nav work. [VERIFIED: `src/app.rs:183-195`]
- **`biased;` branch ordering.** Must be preserved. New branch (diff-result receiver) goes at the END of the select block, not in the middle.
- **Phase 3's synchronous `write_input` guarantee.** Nav paths don't touch PTY write; unaffected.

## 6. Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (built-in) + existing `insta`, `assert_cmd`, `predicates`, `tempfile`, `tokio-test` |
| Config file | None — standard cargo layout |
| Quick run command | `cargo test --lib navigation 2>&1 \| tail -40` (once tests exist) |
| Full suite command | `cargo test` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| NAV-01 | `move_sidebar_to_workspace` updates `ListState` in O(1) | unit | `cargo test --lib sidebar_up_down_is_sync` | ❌ Wave 0 |
| NAV-01 | `activate_sidebar_item` returns to caller **without awaiting git2** | behavioral | `cargo test --lib activate_sidebar_nonblocking` — wrap `activate_sidebar_item` with `tokio::time::timeout(50ms)` over a 10k-file fixture repo; must complete | ❌ Wave 0 |
| NAV-01 | Holding arrow key across N workspaces completes in <N×50ms | **manual feel test** | hold Down through 10 workspaces on a 10k-file repo, visually confirm no stutter | n/a — manual |
| NAV-02 | `Action::ClickWorkspace` dispatch returns inside 50ms on large repo | behavioral | `cargo test --lib click_workspace_nonblocking` | ❌ Wave 0 |
| NAV-02 | `Action::ClickTab` is pure sync | code-review + unit | `rg 'Action::ClickTab' src/events.rs` — body has no `.await`; `cargo test --lib click_tab_is_sync` | ❌ Wave 0 |
| NAV-03 | Workspace switch eagerly paints target PTY before diff completes | behavioral | `cargo test --lib workspace_switch_paints_pty_first` — asserts `active_workspace_idx` mutated and `dirty=true` before `modified_files` populated | ❌ Wave 0 |
| NAV-03 | No `refresh_diff().await` remains in nav paths | code-review | `rg 'refresh_diff\(\)\.await' src/events.rs src/workspace.rs` = 0 hits (gate) | n/a |
| NAV-04 | `SwitchTab`, `ClickTab`, `F(n)` branches contain no `.await` | code-review | `rg '\.await' src/events.rs` in SwitchTab/ClickTab arms = 0 | ❌ Wave 0 (structural unit test possible) |
| NAV-04 | Tab switch takes <10ms end-to-end | **manual feel test** | click tab strip, visually confirm instant swap | n/a — manual |

### Sampling Rate

- **Per task commit:** `cargo check && cargo clippy --all-targets -- -D warnings` (~5s)
- **Per wave merge:** `cargo test` (full suite, expected ~103 + new tests)
- **Phase gate:** full `cargo test` green + re-run Phase 2/3 grep invariants + manual UAT

### Wave 0 Gaps

- [ ] **New test file `src/navigation_tests.rs`** (mirroring `src/pty_input_tests.rs` pattern from Phase 3). Register via `#[cfg(test)] mod navigation_tests;` in `src/main.rs` (same deviation as Phase 3 — binary-only crate).
- [ ] **Large-repo fixture helper.** A `fn make_large_repo(file_count: usize) -> TempDir` helper that `git init`s a repo with N files, commits, then modifies some. Needed for the "50ms timeout" behavioral tests. Current `src/git/diff.rs` test `full_coverage` only uses 3 files — not large enough to demonstrate the stutter.
- [ ] **`App` harness for nav tests.** Tests need to call `activate_sidebar_item` / `dispatch_action(Action::ClickWorkspace(...))` on a live `App` instance. Current `src/app_tests.rs` has an `App::test_instance()`-style helper — reuse or extend. If not, the test can construct an `App` via `App::new` with a temp state file.
- [ ] **mpsc test helper.** If Option A mpsc pattern is chosen, tests need a way to drain the receiver to confirm background refresh results arrive. A 1s timeout per test.
- [ ] No framework install needed — `tokio::test`, `tempfile`, `git2`, `TestBackend` all present. [VERIFIED: `Cargo.toml`]

### Manual-Only Verifications (load-bearing)

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Up/down arrow feels instantaneous even holding the key through a long workspace list | NAV-01 | Feel test; no sub-frame automation | Create 10+ workspaces under a project; expand; hold Down for 3 seconds. Compare against Ghostty for reference feel. |
| Clicking a sidebar item activates pane without pause | NAV-02 | Subjective latency perception | Click various sidebar items (project, workspace, tab, file). Each click should trigger an immediate visual response. |
| Workspace switch shows target PTY immediately | NAV-03 | Visual inspection — no blank frame, no flash | Create two workspaces with distinct PTY content (e.g., one running `echo A`, one running `echo B`). Alt-click or keyboard-nav between them. The target content should be visible on the very next frame. |
| Tab switch is single-frame instantaneous | NAV-04 | Same | Create 3 tabs; F1/F2/F3 and click-tab. Each switch should be indistinguishable from instant. |

## 7. Implementation Approach (File-by-File)

### Estimated changes

- **`src/app.rs`** (+~30 lines): Add `pub diff_tx: mpsc::UnboundedSender<Vec<FileEntry>>` and `pub diff_rx: mpsc::UnboundedReceiver<Vec<FileEntry>>` fields (or use Option B pattern with an AtomicBool). Initialize in `App::new`. Add a new `refresh_diff_spawn(&mut self)` helper. Add a seventh select branch: `Some(files) = self.diff_rx.recv() => { apply diff results + mark_dirty }`. Leave the existing `refresh_diff().await` on `App::new` (pre-first-frame; acceptable).
- **`src/events.rs`** (net change ~0 lines, maybe +5 -5): Replace `.refresh_diff().await` at lines 509, 556 with `.refresh_diff_spawn()`. Optional: audit `ClickFile` and confirm the new diff-file tab path doesn't need refresh (it doesn't — creating a new tab is orthogonal to modified_files).
- **`src/workspace.rs`** (net change ~0): Replace `.refresh_diff().await` at line 143 inside `switch_project` with `.refresh_diff_spawn()`.
- **`src/git/diff.rs`**: No changes. The spawn_blocking remains; only the await site moves.
- **`src/main.rs`** (+1 line): `#[cfg(test)] mod navigation_tests;`
- **`src/navigation_tests.rs`** (NEW, ~150 lines): the ≥5 wave-0 tests + a `make_large_repo` helper.

### Recommended phase structure

- **Plan 04-01 (Wave 0 + spike):** Add `src/navigation_tests.rs` with failing tests that assert the non-blocking behavior. Run `cargo test` to confirm they fail (structural proof the problem exists). Also capture a baseline grep: `rg 'refresh_diff\(\)\.await' src/` should show 4 hits (app.rs, events.rs:509, events.rs:556, workspace.rs:143).
- **Plan 04-02 (Wave 1, implementation):** Introduce the mpsc channel + `refresh_diff_spawn` helper; replace the three `.await` sites; new select branch drains results. Tests turn green. Re-run Phase 2/3 grep invariants — all preserved.
- **Plan 04-03 (manual UAT, blocking):** User UAT per the four manual-test protocols in §6. If UAT passes, phase closes. If UAT flags a specific case failing (e.g., flicker under holding arrow), diagnose via pitfall #2 (is-in-flight coalescing).

Optional fourth plan if UAT surfaces archive-path slowness (pitfall #7): async-ify `remove_dir_all`. Defer unless user flags.

## Runtime State Inventory

Phase 4 is a code-only refinement phase. The following categories are verified:

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | None — no schema or persistent state added. `~/.martins/state.json` layout unchanged. | None |
| Live service config | tmux sessions stay 1:1 per tab and are never renamed/moved. | None |
| OS-registered state | None — no launchd, no Task Scheduler, no daemons. | None |
| Secrets / env vars | None changed | None |
| Build artifacts | `target/` — standard cargo rebuild | None |

**Canonical check:** After every nav path is made non-blocking, what runtime state still "knows" about the old shape? → Nothing. All state is in-process or in the in-process PtyManager. No OS-level registrations depend on the event-loop shape.

## Code Examples

### Verified pattern: input-priority select with biased (from Phase 2, preserve)

```rust
// Source: src/app.rs:206-239 (verified verbatim)
tokio::select! {
    biased;

    // 1. INPUT — highest priority.
    Some(Ok(event)) = events.next() => {
        self.mark_dirty();
        crate::events::handle_event(self, event).await;
    }
    // 2. PTY output.
    _ = self.pty_manager.output_notify.notified() => { self.mark_dirty(); }
    // ... watcher, heartbeat, refresh_tick ...
}
```

### Proposed pattern: background refresh_diff with mpsc (Phase 4 new branch)

```rust
// Proposed — not yet in codebase.
// Adds: diff_tx / diff_rx channel pair on App, a fn refresh_diff_spawn helper,
// and a new select branch that drains results.

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
        if let Ok(files) = crate::git::diff::modified_files(path, base_branch).await {
            let _ = tx.send(files);
        }
    });
}

// New select branch, AFTER refresh_tick branch:
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

### Verified pattern: nav field writes (already correct, preserve)

```rust
// Source: src/events.rs:520-523 — ClickTab (NAV-04; sync; perfect as-is)
Action::ClickTab(idx) => {
    app.active_tab = idx;
    app.mode = InputMode::Terminal;
}
```

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `refresh_diff` is the load-bearing stutter source for NAV-01/02/03 in practice | §1, §3 | LOW — verified via full code trace; no other awaits in nav arm bodies exist. If UAT flags a different cause, fallback is more invasive profile (tracing spans). |
| A2 | Right-sidebar diff-list staleness for 1 frame (~16ms) is not perceptible | §3 "The two shapes of the fix" | LOW — the diff list is already stale between 5s refresh_tick hits; adding 16ms of staleness is a rounding error. [ASSUMED for perceptual threshold; accepted by human-factors literature on UI responsiveness — "Nielsen's 100ms response limit"] |
| A3 | `git2::Repository::open` + `statuses()` on a 10k-file repo takes ≥100ms — worth removing from the input arm | §3 | MEDIUM — order-of-magnitude estimate based on libgit2 issue history; not measured on this machine. If it's actually fast (<16ms), Phase 4 may be redundant and UAT will show no improvement. Mitigation: the non-blocking refactor is still the correct shape even if the current numbers are fine — future repos will be larger. |
| A4 | Tmux sessions stay live across workspace switches (no re-spawn needed) | §2 "The blank frame question" | LOW — verified in `reattach_tmux_sessions` + PtyManager; sessions are cached by composite key. |
| A5 | Option A (mpsc) vs Option B (AtomicBool flag) — either works; preferred is mpsc | §3 "The two shapes of the fix" | LOW — both satisfy NAV requirements; preference is stylistic. Planner may choose. |
| A6 | `archive_active_workspace`'s `remove_dir_all` is out of scope unless UAT flags it | §4 Pitfall #7 | LOW — archive is a destructive op where a brief pause is acceptable; not in NAV-01-04. |
| A7 | 50ms is a generous budget for the non-blocking nav path — any git2 work landing inside this window is fine | §6 | MEDIUM — if anybody has a repo where even the spawned task coordination takes >50ms synchronously (before the spawn returns control), the test fails. Mitigation: raise timeout to 100ms or measure empirically on the user's machine. |

**Verify with user during discuss-phase or planner review** whether A3 (refresh_diff is perceptibly slow today) is observable on the user's current workloads, or whether Phase 4 is preventative. If the user hasn't felt workspace-switch stutter in daily use, the phase can still ship — the code shape is correct regardless. If UAT shows no improvement over Phase 3 baseline, that's evidence the user's repos are small enough to mask the bug; the fix still prevents future regression.

## Open Questions

1. **Is `archive_active_workspace`'s `remove_dir_all` in scope for Phase 4?**
   - What we know: currently synchronous. Not in NAV-01-04 literal requirements.
   - What's unclear: whether the user feels it as nav-adjacent lag.
   - **Recommendation:** out of scope by default; include only if UAT surfaces it. Pitfall #7 documents the fix pattern.

2. **mpsc vs AtomicBool flag — which shape does the planner prefer?**
   - What we know: both work; mpsc is more idiomatic tokio, flag is smaller footprint.
   - **Recommendation:** planner's call. Both preserve Phase 2/3 invariants.

3. **Is there a case where the user wants `modified_files` to be fresh *before* the draw fires?**
   - What we know: No — the right sidebar is informational; nothing routes from it into control flow for the first frame post-switch.
   - **Recommendation:** confirm with user during discuss-phase if uncertain. If the user says "the diff list appearing slightly late is fine," lock that as a decision.

4. **Should Phase 4 add a tracing span for nav paths to diagnose future regressions?**
   - What we know: OBS-01 is v2-deferred.
   - **Recommendation:** out of scope.

5. **Is the Phase 3 deferred `03-02-PLAN.md` (frame-budget gate) relevant to Phase 4?**
   - What we know: 03-02 was conditional on UAT; skipped because Phase 3 UAT passed. It addresses draw-cost coalescing under PTY burst, NOT nav-arm blocking. Different problem.
   - **Recommendation:** orthogonal; do not combine. If Phase 4 UAT flags a draw-cost stutter separate from the refresh_diff fix, revisit 03-02 as a Phase 4 sub-plan.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| tokio (full features, mpsc) | New async channel pattern | ✓ | 1.36 | — |
| ratatui | TUI | ✓ | 0.30 | — |
| crossterm (event-stream) | Input | ✓ | 0.29 | — |
| git2 | diff refresh | ✓ | 0.17 | — |
| tempfile | nav-tests fixture | ✓ | 3.10 (dev) | — |
| cargo / rustc | Build | ✓ | edition 2024, MSRV 1.85 | — |
| `/bin/sh`, `git` CLI | fixture repo init | ✓ | macOS system | — |

**No new dependencies required.** `tokio::sync::mpsc` is in `tokio = { features = ["full"] }` already.

## Project Constraints (from CLAUDE.md)

- **Rust edition 2024, MSRV 1.85.** Let-else, let-chains, `if let` in match arms — already used in codebase. New code in Phase 4 follows the same style.
- **macOS-only runtime.** Nav behavior is platform-independent at the tokio/ratatui/git2 level; no platform branching.
- **Single-language codebase, 100% Rust.** No cross-language contracts.
- **tokio full features.** mpsc, spawn, spawn_blocking all available.

## Sources

### Primary (HIGH confidence)

- Codebase direct inspection (verbatim): `src/app.rs` (run loop), `src/events.rs` (handle_event / handle_key / handle_mouse / handle_click / dispatch_action / activate_sidebar_item), `src/workspace.rs` (switch_project, archive_active_workspace, reattach_tmux_sessions, create_workspace, create_tab), `src/ui/draw.rs` (draw pipeline), `src/ui/sidebar_left.rs` (sidebar render), `src/ui/terminal.rs` (PseudoTerminal), `src/git/diff.rs` (modified_files), `src/pty/manager.rs` (session storage), `src/pty/session.rs` (PTY lifecycle), `src/keys.rs` (keymap + actions). All line numbers cited above are verified.
- `.planning/phases/02-event-loop-rewire/02-RESEARCH.md` — Phase 2 primitives, biased-select semantics (VERIFIED)
- `.planning/phases/02-event-loop-rewire/PHASE-SUMMARY.md` — Phase 2 completion checkpoints (VERIFIED)
- `.planning/phases/03-pty-input-fluidity/03-RESEARCH.md` — Phase 3 keystroke path, deferred frame-budget gate (VERIFIED)
- `.planning/phases/03-pty-input-fluidity/03-01-SUMMARY.md` — Phase 3 close + grep-invariant snapshot used as regression anchor (VERIFIED)
- [docs.rs/tokio/macro.select.html](https://docs.rs/tokio/latest/tokio/macro.select.html) — `biased;` branch-ordering semantics (already verified in Phase 2 research)
- [docs.rs/tokio/latest/tokio/sync/mpsc/](https://docs.rs/tokio/latest/tokio/sync/mpsc/) — unbounded channel semantics, drain pattern (CITED via training; stable API)

### Secondary (MEDIUM confidence)

- [github.com/libgit2/libgit2/issues/4230](https://github.com/libgit2/libgit2/issues/4230) — libgit2 status perf on large worktrees (CITED; general awareness of the scale of refresh_diff cost)
- [ratatui.rs/concepts/rendering/under-the-hood/](https://ratatui.rs/concepts/rendering/under-the-hood/) — buffer diff (same as Phase 2/3 usage)

### Tertiary (LOW confidence)

- Nielsen's 100ms response limit for perceived instant feedback — human-factors rule-of-thumb informing Assumption A2 [ASSUMED — long-standing UI principle; not re-verified in this session]

## Metadata

**Confidence breakdown:**
- Sidebar keyboard path trace & stutter source: HIGH — line-by-line verified in codebase
- Mouse click path: HIGH — verified verbatim
- Workspace-switch is NOT a "blank frame" problem (tmux sessions live): HIGH — verified via PtyManager + reattach_tmux_sessions
- Tab switch is already correct: HIGH — trivial code paths, all sync
- `refresh_diff()` is the specific stutter source: HIGH — only 3 awaits in nav-adjacent code; all lead here
- Mpsc vs flag tradeoff: HIGH (functional equivalence) / MEDIUM (stylistic)
- Current repo-scale numbers (how slow is refresh_diff today?): MEDIUM — qualitative estimate; not measured on user's machine

**Research date:** 2026-04-24
**Valid until:** 2026-05-24 (30 days — ratatui 0.30, tokio 1.36, crossterm 0.29, git2 0.17 all stable; Phase 2/3 baseline locked)
