---
phase: 04-navigation-fluidity
status: complete
closed: 2026-04-24
requirements: [NAV-01, NAV-02, NAV-03, NAV-04]
---

# Phase 4 — Navigation Fluidity

## Sign-off

All four feel-tests passed via user UAT on 2026-04-24:

| Req | Feel-test | Result |
|-----|-----------|--------|
| NAV-01 | Hold Up/Down to scroll long workspace list under expanded project on large repo | approved |
| NAV-02 | Click any sidebar item activates it + highlight follows | approved (after 04-03 highlight fix) |
| NAV-03 | Workspace switch paints PTY on next frame, no blank/loading flash | approved |
| NAV-04 | Tab switch via F1-F3, number keys, click-on-strip — indistinguishable from instant | approved |

## What Shipped

**Plan 04-01 — Wave 0 regression-guard tests (TDD red):**
- New `src/navigation_tests.rs` with 4 `#[tokio::test]` behavioural guards: `click_tab_is_sync`, `sidebar_up_down_is_sync`, `refresh_diff_spawn_is_nonblocking`, `workspace_switch_paints_pty_first`.
- `make_large_repo(dir, N)` fixture helper (generalizes `app_tests::init_repo` from 1 file to N).
- `#[cfg(test)] mod navigation_tests;` registered in `src/main.rs` adjacent to the Phase-3 `mod pty_input_tests;`.
- Commits `e3ea41a`, `7cee21d`.

**Plan 04-02 — Non-blocking `refresh_diff` (TDD green):**
- `App` gained `diff_tx: UnboundedSender<Vec<FileEntry>>` + `diff_rx: UnboundedReceiver<Vec<FileEntry>>` initialized in `App::new`.
- New `pub(crate) fn refresh_diff_spawn(&mut self)` sibling to the existing async `refresh_diff` — spawns the expensive git-diff work on the tokio runtime, returns immediately, eagerly marks dirty.
- Three nav hot-path call-sites swapped `refresh_diff().await` → `refresh_diff_spawn()` in `src/events.rs` (2) and `src/workspace.rs` (1).
- 6th `tokio::select!` branch added to `App::run` drains `diff_rx.recv()`, repopulates `modified_files`, marks dirty for the next paint.
- Three legit `refresh_diff().await` sites preserved in `src/app.rs` (`App::new` pre-first-frame, watcher branch, refresh_tick branch) — these are outside the nav hot path.
- Commits `49ca329`, `fceede9`, `2927dae`, `2892f09`.

**Plan 04-03 — UAT + NAV-02 highlight gap closure:**
- User UAT uncovered a pre-existing bug the faster nav path exposed: clicking a sidebar row never updated `app.left_list` (ratatui `ListState`), so the activation worked but the highlight stayed on the keyboard cursor's old position.
- One-line fix in the mouse click handler at `src/events.rs:176` — `app.left_list.select(Some(local_row));` before dispatching the sidebar item action.
- Commit `f89277a`.

## Grep Invariant Snapshot (Phase 5+ Regression Anchor)

```
rg 'refresh_diff\(\)\.await' src/events.rs src/workspace.rs  → 0
rg 'refresh_diff\(\)\.await' src/app.rs                       → 3
rg 'pub\(crate\) fn refresh_diff_spawn' src/app.rs            → 1
rg 'refresh_diff_spawn\(\)' src/events.rs src/workspace.rs    → events.rs:2 + workspace.rs:1 = 3
rg '^\s*biased;' src/app.rs                                   → 1  (Phase 2 invariant preserved)
rg '// 6\.' src/app.rs                                        → 1  (the new drain branch)
rg '(diff_tx|diff_rx):' src/app.rs                            → 2  (field declarations)
```

Phase 2/3 invariants: `// 1. INPUT`=1, `if self.dirty`=1, `status_tick`=0, `mark_dirty()`≥7, `duration_since`=1 — all byte-for-byte preserved.

## Test Status

`cargo test` — 107 passed / 0 failed (103 prior + 4 new nav tests).
`cargo clippy --all-targets -- -D warnings` — clean.

## Next-Phase Readiness (Phase 5 — Background Work Decoupling)

`refresh_diff` is now fire-and-forget on all three nav call-sites, so Phase 5's debounced-notify refactor no longer has to preserve the `.await`-semantics of nav clicks. The mpsc channel pair on `App` is the primitive Phase 5 extends — additional background tasks (fs-watcher, ctag refresh, etc.) can reuse the same `diff_tx`/`diff_rx` pattern or stand up sibling channels against the same 6th-select-branch idiom.
