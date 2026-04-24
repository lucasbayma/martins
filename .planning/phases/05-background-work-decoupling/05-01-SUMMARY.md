---
phase: 05-background-work-decoupling
plan: 01
subsystem: background-work
tags: [tdd, regression-guard, wave-0, debounce, spawn-blocking, bg-04, bg-05]
status: complete
completed_at: "2026-04-24T21:38:37Z"
duration: ~10m
tasks_completed: 3
files_changed: 2
commits:
  - hash: 862e201
    type: test
    message: "test(05-01): add save_state_spawn_is_nonblocking BG-05 TDD gate"
  - hash: 062a987
    type: test
    message: "test(05-01): add debounce_rapid_burst_of_10 BG-04 regression guard"
requires:
  - Phase 1 Plan 01-05 (app_tests.rs registration via #[path] in src/app.rs)
  - Phase 4 Plan 04-01 (template: refresh_diff_spawn_is_nonblocking)
  - notify_debouncer_mini (existing dep)
  - tempfile (existing dev-dep)
provides:
  - BG-05 TDD gate: save_state_spawn_is_nonblocking — fails to compile until Plan 05-02 introduces App::save_state_spawn
  - BG-04 regression guard: debounce_rapid_burst_of_10 — passes today on 750ms window, will still pass on 200ms post-05-02
affects:
  - Plan 05-02 (must add App::save_state_spawn to make BG-05 test compile + pass; may retune debounce window 750ms → 200ms)
tech-stack:
  added: []
  patterns: [tdd-failing-test-first, regression-guard]
key-files:
  created: []
  modified:
    - src/app_tests.rs
    - src/watcher.rs
decisions:
  - "Plan Task 3 verification adapted: app_tests is registered in src/app.rs (Phase 1 Plan 01-05 layout), not src/main.rs as plan claimed — Rule 3 deviation, plan doc-drift not a regression"
  - "Used GlobalState::add_project(&Path, String) for the 100-project loop — exact match of plan's preferred signature, no struct construction needed"
  - "Task 1 committed before Task 2; verified Task 2 in isolation via temporary git-restore of app_tests.rs to pre-Task-1 state before running cargo test, then restored Task 1 changes (working-tree-only manipulation, no commit churn)"
metrics:
  duration_minutes: 10
  files_modified: 2
  lines_added: 90
  tasks_completed: 3
  commits: 2
---

# Phase 05 Plan 01: Wave-0 Regression-Guard Tests Summary

**One-liner:** Added two Wave-0 regression-guard tests — `save_state_spawn_is_nonblocking` (BG-05 TDD gate, fails to compile today) and `debounce_rapid_burst_of_10` (BG-04 200ms-window guard, passes today) — locking in Phase 5 success criteria before any production code lands.

## What Changed

### `src/app_tests.rs` (+40 lines)

Added imports `Duration, Instant` to the existing `use std::time::{...};` line (was `std::path::Path`; added new line `use std::time::{Duration, Instant};`).

Appended one new `#[tokio::test]` after the existing `mark_dirty_sets_flag` test:

```rust
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
```

`GlobalState::add_project(&Path, String)` matched the plan's preferred signature exactly — no fallback to direct struct construction needed.

### `src/watcher.rs` (+50 lines)

Appended one new `#[tokio::test]` inside the existing `#[cfg(test)] mod tests` block, after `debounce_rapid`:

```rust
/// BG-04 LOAD-BEARING — a burst of 10 rapid file writes (at 20ms
/// spacing) produces at most 2 debounced events. On the current
/// 750ms window, the 200ms burst trivially lands in one debounce
/// cycle. On the post-05-02 200ms window, the burst boundary matches
/// the debouncer window but still coalesces to ≤2 events.
///
/// This test must PASS both pre-05-02 (window=750ms) and post-05-02
/// (window=200ms). If a future debounce retune causes it to fail,
/// that is the signal.
///
/// See: .planning/phases/05-background-work-decoupling/05-RESEARCH.md
/// §12 line 439 + §8 Pitfall #1.
#[tokio::test]
async fn debounce_rapid_burst_of_10() {
    let tmp = TempDir::new().unwrap();
    let mut watcher = Watcher::new().unwrap();
    watcher.watch(tmp.path()).unwrap();

    std::thread::sleep(Duration::from_millis(100));

    // Write 10 times rapidly at 20ms spacing → 200ms total burst
    for i in 0..10 {
        std::fs::write(
            tmp.path().join("burst10.txt"),
            format!("write {}", i),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }

    // Drain events until a 2000ms deadline
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
        "expected at most 2 debounced events from 10-write burst, got {}",
        count
    );
    assert!(count >= 1, "expected at least 1 event from 10-write burst");
}
```

`debounce_rapid` was NOT modified. Debounce window remains `Duration::from_millis(750)` at line 48 — Plan 05-02 retunes.

## TDD Gate Confirmation

`cargo build --tests` produces the EXPECTED compile error (proof the gate is armed):

```
error[E0599]: no method named `save_state_spawn` found for struct `app::App` in the current scope
   --> src/app_tests.rs:153:9
    |
153 |     app.save_state_spawn();
    |         ^^^^^^^^^^^^^^^^
    |
   ::: src/app.rs:53:1
    |
 53 | pub struct App {
    | -------------- method `save_state_spawn` not found for this struct
    |
help: there is a method `save_state` with a similar name
    |
153 -     app.save_state_spawn();
153 +     app.save_state();
    |

For more information about this error, try `rustc --explain E0599`.
error: could not compile `martins` (bin "martins" test) due to 1 previous error
```

`cargo build` (production, without `--tests`) **succeeds clean** — production source untouched.

## BG-04 Test Verification (Task 2)

Verified in isolation by temporarily restoring `src/app_tests.rs` to its pre-Task-1 state (so the test binary could compile), running the watcher tests, then re-applying Task 1's changes. Working-tree-only manipulation; no commit churn.

```
running 4 tests
test watcher::tests::detect_change ... ok
test watcher::tests::filter_noise ... ok
test watcher::tests::debounce_rapid_burst_of_10 ... ok
test watcher::tests::debounce_rapid ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 104 filtered out; finished in 2.42s
```

Both `debounce_rapid` (existing 5-write × 50ms = 250ms burst) AND `debounce_rapid_burst_of_10` (new 10-write × 20ms = 200ms burst) pass on the current 750ms window, and remain tight enough to assert meaningfully when Plan 05-02 tightens the window to 200ms.

## Task 3: Mod Registration Verification

`src/main.rs` mod registrations (no edit, assertion-only):

```
21:mod pty_input_tests;
24:mod navigation_tests;
```

`mod app_tests` is NOT in `src/main.rs` — it is registered in `src/app.rs:524-526` via the `#[path]` attribute (Phase 1 Plan 01-05 layout, see doc-comment in `src/app_tests.rs:1-3`):

```
524:#[cfg(test)]
525:#[path = "app_tests.rs"]
526:mod tests;
```

The TDD gate is reachable — `cargo build --tests` surfaces the `save_state_spawn` missing-method error 4 times in the output, proving the test module is being compiled (not silently skipped). No edits to `src/main.rs`.

## Phase 2/3/4 Invariants Preserved (src/app.rs)

| Invariant | Expected | Actual |
|---|---|---|
| `biased;` | 1 | 1 |
| `// 1. INPUT` | 1 | 1 |
| `if self.dirty` | 1 | 1 |
| `status_tick` | 0 | 0 |
| `pub(crate) fn refresh_diff_spawn` | 1 | 1 |
| `save_state_spawn` (production not yet added) | 0 | 0 |

All preserved.

## Watcher Debounce Window (Plan 05-02 retune site)

`src/watcher.rs:48` — `Duration::from_millis(750)` (unchanged this plan; Plan 05-02 changes to 200ms).

## Deviations from Plan

### [Rule 3 - Blocking issue] Task 3 verification target wrong in plan

- **Found during:** Task 3
- **Issue:** Plan asserted `rg 'mod app_tests' src/main.rs` should return ≥1 hit. Actual layout (Phase 1 Plan 01-05) registers `app_tests` in `src/app.rs:524-526` via `#[path = "app_tests.rs"] mod tests;`, not in `src/main.rs`. The doc-comment at `src/app_tests.rs:1-3` even calls this out: "Declared as `#[path]` module from src/app.rs."
- **Fix:** Adapted the verification — checked `src/app.rs` for the registration AND confirmed the TDD gate is reachable via `cargo build --tests` surfacing `save_state_spawn` in the error output. Both pass. Plan's intent (test module registered + reachable) is satisfied.
- **Files modified:** None (verification-only task)
- **Commit:** None (Task 3 has `<files>(none — verification only)</files>`)

### [Workflow] Task 2 verification required temporary working-tree manipulation

- **Found during:** Task 2
- **Issue:** Task 1's intentional compile error in `src/app_tests.rs` blocks the entire test binary from compiling, so `cargo test --bin martins debounce_rapid_burst_of_10` cannot be run end-to-end while the gate is armed.
- **Fix:** Temporarily restored `src/app_tests.rs` to its pre-Task-1 state (`git show HEAD~1:src/app_tests.rs > src/app_tests.rs`), ran the watcher test (passed), then restored Task 1's changes from a saved copy. No commit churn; working-tree-only manipulation. The plan author seems to have anticipated this constraint (Task 2 acceptance criterion notes "this acceptance criterion is satisfied if the only clippy/build failures are traceable to Task 1's missing-method error") but the verify command itself assumed runnable tests.
- **Files modified:** None (post-restore, working tree matches HEAD for app_tests.rs)
- **Commit:** None

## Auth Gates

None encountered.

## Known Stubs

None. Both tests are full implementations; the only "stub" is the absent `App::save_state_spawn` method, which is the intentional TDD gate for Plan 05-02.

## Threat Flags

None. The new tests use existing trust boundaries (TempDir for filesystem, std::env::temp_dir for the state.json path with explicit cleanup). No new network endpoints, no new auth paths, no new schema changes.

## Plan 05-02 Cleared

- BG-05 TDD gate armed: `App::save_state_spawn` is the missing-method compile error.
- BG-04 regression guard in place: 10-write burst test passes on current 750ms window, will pass on 200ms post-retune.
- Production build green; clippy on production binary clean.

## Self-Check: PASSED

- File `src/app_tests.rs` exists and contains `save_state_spawn_is_nonblocking` (`grep -c` returns 1).
- File `src/watcher.rs` exists and contains `debounce_rapid_burst_of_10` (`grep -c` returns 1).
- Commit `862e201` exists in `git log` (Task 1).
- Commit `062a987` exists in `git log` (Task 2).
- Production `cargo build` succeeds.
- `cargo build --tests` fails with exactly the expected `no method named save_state_spawn` error (TDD gate armed).
- All Phase 2/3/4 grep invariants preserved.
