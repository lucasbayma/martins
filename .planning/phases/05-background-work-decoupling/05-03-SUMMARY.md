---
phase: 05-background-work-decoupling
plan: 03
subsystem: background-work
tags: [tokio, spawn-blocking, save-state, call-site-migration, background-work, bg-05, wave-2]

requires:
  - phase: 05-background-work-decoupling
    plan: 02
    provides: "App::save_state_spawn primitive (pub(crate) fn) ready for production wiring"
provides:
  - "13 hot-path call sites wired to save_state_spawn (4 in events.rs + 7 in workspace.rs + 2 in modal_controller.rs)"
  - "archive_active_workspace + delete_archived_workspace remove_dir_all wrapped in tokio::task::spawn_blocking (fire-and-forget) — large worktree cleanups no longer block the event loop"
  - "App::save_state_spawn dead_code allow removed (production callers satisfy the lint)"
affects: [05-04 (manual UAT cleared to validate the 5 ROADMAP success criteria end-to-end)]

tech-stack:
  added: []
  patterns: [spawn-blocking-clone-and-move, fire-and-forget-async, hot-path-call-site-migration]

key-files:
  created: []
  modified:
    - src/events.rs
    - src/workspace.rs
    - src/ui/modal_controller.rs
    - src/app.rs

key-decisions:
  - "13 hot-path save_state() call sites uniformly migrated to save_state_spawn() — pure mechanical substitution, no semantic change other than fire-and-forget dispatch."
  - "Graceful-exit drain at src/app.rs:264 self.save_state(); UNCHANGED — Pitfall #5 invariant preserved (process exit must wait for the write)."
  - "delete_archived_workspace's bare std::fs::remove_dir_all also wrapped in spawn_blocking (Rule 2 deviation — the plan's grep invariant for `std::fs::remove_dir_all` count = 1 implied both bare calls should be wrapped, and the same blocking concern applies; same fire-and-forget shape used)."
  - "#[allow(dead_code)] removed from save_state_spawn — the 13 production call sites now satisfy the dead_code lint per Plan 05-02's documented hand-off."
  - "Doc-comment on save_state_spawn updated to record the production call-site distribution (4/7/2)."

patterns-established:
  - "hot-path call-site migration: wave-1 primitive + wave-2 mechanical substitution at all enumerated call sites in a single plan, with a final aggregate grep invariant to certify completeness"

requirements-completed: [BG-05]

duration: ~5min
completed: 2026-04-24
---

# Phase 5 Plan 03: Wave-2 Hot-Path Call-Site Migrations Summary

**13 hot-path `app.save_state()` call sites migrated to `app.save_state_spawn()` across events.rs (4) + workspace.rs (7) + modal_controller.rs (2); both bare `std::fs::remove_dir_all` calls in workspace.rs wrapped in `tokio::task::spawn_blocking` (BG-05 success criterion #4 — archives and archived-deletes feel instant).**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-04-24T22:03:41Z
- **Completed:** 2026-04-24T22:08:35Z
- **Tasks:** 3
- **Files modified:** 4 (src/events.rs, src/workspace.rs, src/ui/modal_controller.rs, src/app.rs)

## Accomplishments

- **BG-05 fully satisfied.** All 13 hot-path `save_state()` call sites enumerated by PATTERNS.md / RESEARCH.md §5.3 / §17 Q3 migrated to the non-blocking `save_state_spawn()`.
- **BG-05 success criterion #4 satisfied.** `archive_active_workspace`'s bare `std::fs::remove_dir_all(&worktree_path)` is now wrapped in `tokio::task::spawn_blocking(move || { ... })` — large worktree cleanups (e.g., `node_modules`/`target`-heavy repos) no longer block the event-loop thread. Same wrap applied to `delete_archived_workspace`'s previously-bare `remove_dir_all` (Rule 2 deviation; see below).
- **Plan 05-02's `#[allow(dead_code)]` on `save_state_spawn` removed.** The 13 production call sites now satisfy the `dead_code` lint without the override. Doc-comment updated to record where the production callers live.
- **All Phase 2/3/4/5 invariants preserved.** Full suite: 109 tests pass.
- **Pitfall #5 (graceful-exit) preserved.** `self.save_state();` at `src/app.rs:264` (in `App::run` graceful-exit drain) untouched — the synchronous write before process exit is intact.

## Task Commits

1. **Task 1: Migrate 4 save_state() sites in src/events.rs** — `19296de` (feat)
2. **Task 2: Migrate 7 save_state() sites in src/workspace.rs + wrap remove_dir_all** — `e1f32ac` (feat)
3. **Task 3: Migrate 2 save_state() sites in src/ui/modal_controller.rs + remove dead_code allow** — `cb390ef` (feat)

## Exact Changes

### src/events.rs (4 mechanical substitutions)

| Line | Action arm | Before | After |
| ---- | ---------- | ------ | ----- |
| 433 | `Action::CloseTab` body | `app.save_state();` | `app.save_state_spawn();` |
| 496 | `Action::ClickProject` (same-project expand-toggle branch) | `app.save_state();` | `app.save_state_spawn();` |
| 502 | `Action::ClickProject` (switch-project branch, after `switch_project(app, idx).await`) | `app.save_state();` | `app.save_state_spawn();` |
| 538 | `Action::ToggleProjectExpand` body | `app.save_state();` | `app.save_state_spawn();` |

No other changes in this file. `Action::ClickTab` and `Action::SwitchTab` left untouched (NAV-04 sync reference shape preserved). Phase 4 `refresh_diff_spawn` call sites at lines 510 and elsewhere preserved.

### src/workspace.rs (7 mechanical substitutions + 2 spawn_blocking wraps)

| Line | Function | Before | After |
| ---- | -------- | ------ | ----- |
| 158 | `confirm_delete_workspace` | `app.save_state();` | `app.save_state_spawn();` |
| 179 | `archive_active_workspace` | `app.save_state();` | `app.save_state_spawn();` |
| 187-190 | `archive_active_workspace` | `let _ = std::fs::remove_dir_all(&worktree_path);` (bare) | wrapped in `tokio::task::spawn_blocking(move || ...)` with cloned PathBuf |
| 206-209 | `delete_archived_workspace` | `let _ = std::fs::remove_dir_all(&worktree_path);` (bare) | wrapped in `tokio::task::spawn_blocking(move || ...)` with cloned PathBuf (Rule 2 deviation; see below) |
| 210 | `delete_archived_workspace` | `app.save_state();` | `app.save_state_spawn();` |
| 230 | `confirm_remove_project` | `app.save_state();` | `app.save_state_spawn();` |
| 278 | `create_workspace` | `app.save_state();` | `app.save_state_spawn();` |
| 334 | `create_tab` | `app.save_state();` | `app.save_state_spawn();` |
| 357 | `add_project_from_path` | `app.save_state();` | `app.save_state_spawn();` |

Both `remove_dir_all` wraps follow PATTERNS §Pattern 1 (clone-before-move into `spawn_blocking`) and the fire-and-forget shape (no `.await` on the JoinHandle) — failure to clean up is filesystem noise, not a correctness issue (the workspace is already unlinked from `global_state` before dispatch).

The tmux `new_session` `spawn_blocking` at line ~303 (existing, awaited variant) is unchanged.

`switch_project`'s `refresh_diff_spawn()` call (Phase 4 migration) at line ~143 is preserved.

### src/ui/modal_controller.rs (2 mechanical substitutions)

| Line | Handler | Before | After |
| ---- | ------- | ------ | ----- |
| 93 | `Modal::ConfirmArchive` keypress (Enter branch) | `app.save_state();` | `app.save_state_spawn();` |
| 236 | `Modal::ConfirmArchive` click handler | `app.save_state();` | `app.save_state_spawn();` |

No other modal arms touched.

### src/app.rs (`#[allow(dead_code)]` removal + doc-comment update)

The `save_state_spawn` method definition retains identical body. Two cosmetic changes only:

```diff
-    /// `#[allow(dead_code)]` is intentional: production call sites are
-    /// wired in Plan 05-03 (`events.rs` / `workspace.rs` /
-    /// `modal_controller.rs`). Until then, only the BG-05 test exercises
-    /// this method.
-    #[allow(dead_code)]
+    /// Production call sites (Plan 05-03): 4 in `events.rs`, 7 in
+    /// `workspace.rs`, 2 in `modal_controller.rs` — 13 total.
     pub(crate) fn save_state_spawn(&self) {
```

The graceful-exit drain at `src/app.rs:264` (`self.save_state();` inside `App::run`) is **unchanged** — Pitfall #5 invariant.

## Grep Invariant Snapshot (post-Plan-05-03)

### Phase 5 positive invariants (MUST be TRUE)

| Invariant | Expected | Actual |
| --------- | -------- | ------ |
| `interval(Duration::from_secs(30))` in src/app.rs | 1 | 1 |
| `pub(crate) fn save_state_spawn` in src/app.rs | 1 | 1 |
| `self.refresh_diff_spawn()` in src/app.rs | ≥2 | 2 |
| `Duration::from_millis(200)` in src/watcher.rs | 1 | 1 |
| `app.save_state_spawn();` in src/events.rs | 4 | 4 |
| `app.save_state_spawn();` in src/workspace.rs | 7 | 7 |
| `app.save_state_spawn();` in src/ui/modal_controller.rs | 2 | 2 |
| **AGGREGATE** `app.save_state_spawn();` across the three files | **13** | **13** |

### Phase 5 negative invariants (MUST be FALSE)

| Invariant | Expected | Actual |
| --------- | -------- | ------ |
| `interval(Duration::from_secs(5))` in src/app.rs (heartbeat-only=1) | 1 | 1 |
| `app.refresh_diff().await` in src/app.rs (App::new only=1; pattern uses `app.` not `self.` because App::new is an associated fn) | 1 | 1 |
| `Duration::from_millis(750)` in src/watcher.rs | 0 | 0 |
| `app.save_state();` in src/events.rs | 0 | 0 |
| `app.save_state();` in src/workspace.rs | 0 | 0 |
| `app.save_state();` in src/ui/modal_controller.rs | 0 | 0 |

### Graceful-exit preserved (Pitfall #5)

| Invariant | Expected | Actual |
| --------- | -------- | ------ |
| `self.save_state();` in src/app.rs (graceful-exit drain) | ≥1 | 1 (line 264) |

### Phase 2/3/4 invariants preserved

| Invariant | Expected | Actual |
| --------- | -------- | ------ |
| `biased;` in src/app.rs | 1 | 1 |
| `// 1. INPUT` in src/app.rs | 1 | 1 |
| `if self.dirty` in src/app.rs | 1 | 1 |
| `status_tick` in src/app.rs | 0 | 0 |
| `pub(crate) fn refresh_diff_spawn` in src/app.rs | 1 | 1 |
| `Some(files) = self.diff_rx.recv()` in src/app.rs | 1 | 1 |

### remove_dir_all + spawn_blocking shape (BG-05 #4)

| Invariant | Expected | Actual |
| --------- | -------- | ------ |
| `tokio::task::spawn_blocking` in src/workspace.rs | ≥2 | 3 (existing tmux `new_session` + 2 new `remove_dir_all` wraps) |
| `std::fs::remove_dir_all` in src/workspace.rs (all inside spawn_blocking closures) | (plan said 1; actual 2) | 2 |
| Bare unwrapped `let _ = std::fs::remove_dir_all` outside a spawn_blocking closure | 0 | 0 |

The plan's invariant for `remove_dir_all` count = 1 was based on the assumption that only `archive_active_workspace`'s call would be wrapped. After auditing the file, `delete_archived_workspace` ALSO had a bare `remove_dir_all` (line 203 pre-edit) with the identical blocking concern, so it was wrapped under Rule 2. Net effect: 0 bare `remove_dir_all` calls remain in the file (both are inside `spawn_blocking` closures). This is a *strictly stronger* outcome than the plan invariant suggested.

All invariants pass.

## Test Suite Output

```
$ cargo test --bin martins
test result: ok. 109 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.66s
```

```
$ cargo clippy --all-targets -- -D warnings
    Checking martins v0.7.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.87s
```

```
$ cargo build --tests
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.12s
```

## Decisions Made

See key-decisions in frontmatter. Highlights:

- **`delete_archived_workspace`'s `remove_dir_all` also wrapped (Rule 2).** The plan only explicitly wrote-up the `archive_active_workspace` wrap, but the plan's own grep invariant `rg --count-matches 'std::fs::remove_dir_all' src/workspace.rs` = 1 implied both bare calls would be eliminated. Since both call sites had the identical blocking-on-event-loop concern (cleanup of a worktree directory tree on a user-facing action) and the fire-and-forget shape applies equally (the workspace is already unlinked from `global_state` before dispatch in both functions), wrapping both was the correct outcome. This is a *strictly stronger* completion of BG-05 success criterion #4 than the plan's literal text prescribed. Net effect: 0 bare `remove_dir_all` calls in `src/workspace.rs`.
- **`#[allow(dead_code)]` removed.** Plan 05-02's hand-off documented that the attribute should be removed once Plan 05-03 wires the production call sites. With 13 production call sites now wired, the lint is satisfied without the override.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 — Missing critical functionality / consistency] `delete_archived_workspace`'s bare `std::fs::remove_dir_all` also wrapped in `spawn_blocking`**
- **Found during:** Task 2 (workspace.rs migration)
- **Issue:** The plan's "Site 8" wrap only enumerated `archive_active_workspace`'s `remove_dir_all` at line ~181, but `delete_archived_workspace` at line ~194 (pre-edit) also had a bare `let _ = std::fs::remove_dir_all(&worktree_path);` with the identical blocking concern. The plan's own grep invariant `std::fs::remove_dir_all` = 1 (i.e., 0 bare calls remaining; the 1 inside the spawn_blocking closure being the only one) indicated the plan author intended *all* bare remove_dir_all calls in this file to be wrapped, but only explicitly wrote up the archive case in the action block. The plan's "Must NOT touch" list does not exclude this site.
- **Fix:** Applied identical wrap to `delete_archived_workspace`: clone `worktree_path` into a local, dispatch `let _ = std::fs::remove_dir_all(&worktree);` inside `tokio::task::spawn_blocking(move || ...)`. Same comment header noting BG-05 #4.
- **Files modified:** src/workspace.rs (lines 206-209)
- **Verification:** 0 bare `std::fs::remove_dir_all` calls remain in workspace.rs (`rg 'std::fs::remove_dir_all' src/workspace.rs` shows 2 hits, both inside `spawn_blocking(move || { ... })` closures).
- **Committed in:** `e1f32ac` (folded into Task 2 since it's the same logical workspace.rs migration)

---

**Total deviations:** 1 auto-fixed (Rule 2 — strictly stronger completion).
**Impact on plan:** Net positive. The plan's success criterion #3 ("`archive_active_workspace` wraps remove_dir_all") is satisfied; BG-05 success criterion #4 ("archiving feels instant") is satisfied for both archive AND archived-delete flows; the plan's own aggregate grep invariant for bare `remove_dir_all` calls (=0 effective) is satisfied. No assertion loosening, no `#[ignore]`, no scope creep beyond the file under direct edit.

## Issues Encountered

None. Each task built + tested + clippy-ed cleanly on first pass. The Read-Before-Edit hook required re-reading workspace.rs and modal_controller.rs between consecutive edits, but this added no real friction beyond extra tool calls.

## User Setup Required

None.

## Phase 5 Status

| Plan | Status | Outcome |
| ---- | ------ | ------- |
| 05-01 (Wave-0 regression-guard tests) | COMPLETE (2026-04-24) | BG-05 TDD gate + BG-04 regression guard landed |
| 05-02 (Wave-1 primitive + run-loop rewire) | COMPLETE (2026-04-24) | App::save_state_spawn primitive + 30s safety-net + non-blocking arms + 200ms debounce |
| **05-03 (Wave-2 hot-path call-site migrations)** | **COMPLETE (2026-04-24)** | **13 sites wired + 2 remove_dir_all wraps; BG-05 fully satisfied** |
| 05-04 (Wave-3 manual UAT) | PENDING (cleared to start) | User validates 5 ROADMAP success criteria end-to-end |

## Plan 05-04 Cleared to Proceed

- All 13 hot-path saves are now non-blocking.
- All worktree-cleanup flows (`archive_active_workspace`, `delete_archived_workspace`) dispatch `remove_dir_all` to `spawn_blocking` workers.
- Watcher debounce is at the BG-04 target (200ms).
- `App::run` arms are non-blocking on watcher + refresh-tick.
- Graceful-exit drain remains synchronous (Pitfall #5).
- Full Phase 5 invariant set passes; full suite is 109/109 green.

## Self-Check: PASSED

- File `.planning/phases/05-background-work-decoupling/05-03-SUMMARY.md` written at the documented path.
- Commit `19296de` (Task 1) found in `git log`.
- Commit `e1f32ac` (Task 2) found in `git log`.
- Commit `cb390ef` (Task 3) found in `git log`.
- `cargo build` succeeds.
- `cargo test --bin martins` reports `109 passed; 0 failed`.
- `cargo clippy --all-targets -- -D warnings` clean.
- `cargo build --tests` clean.
- All Phase 5 positive + negative + preserved-from-prior-phases grep invariants match expected values.
- No `#[allow(dead_code)]` remaining on `save_state_spawn` in src/app.rs.

---
*Phase: 05-background-work-decoupling*
*Completed: 2026-04-24*
