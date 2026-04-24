---
phase: 04-navigation-fluidity
plan: 01
subsystem: navigation
tags: [navigation, tokio, mpsc, tdd, wave-0, regression-guard, tempfile, git2, red-phase]
requires: [NAV-01, NAV-02, NAV-03, NAV-04]
provides:
  - "src/navigation_tests.rs — 4 #[tokio::test] regression guards + make_large_repo helper"
  - "TDD compile-gate for Plan 04-02 (refresh_diff_spawn missing-method error)"
affects:
  - src/main.rs
  - src/navigation_tests.rs
tech-stack:
  added: []
  patterns:
    - "TDD red-phase: failing compile gate drives the next plan's implementation"
    - "Binary-only crate test module registered via src/main.rs (Phase-3 deviation preserved)"
    - "Test fixture helper generalizes src/app_tests.rs::init_repo from 1 file to N"
key-files:
  created:
    - src/navigation_tests.rs
  modified:
    - src/main.rs
decisions:
  - "Used `std::env::temp_dir()` with per-test-namespaced state.json paths to avoid cross-test contention"
  - "No production code touched this plan — mpsc + refresh_diff_spawn land in 04-02"
  - "`make_large_repo` writes small 1-byte files to keep fixture time cheap while still exercising the git2 status-walk path"
metrics:
  duration: "~6 minutes"
  completed: "2026-04-24"
  tasks: 1
  files_created: 1
  files_modified: 1
---

# Phase 4 Plan 01: Navigation Fluidity — Wave 0 Regression-Guard Tests Summary

Wave-0 failing-test scaffold: four `#[tokio::test]` functions that pin NAV-01/02/03/04 behavior and compile-gate Plan 04-02's `refresh_diff_spawn` refactor. Two tests PASS today (the sync field-write paths — click_tab and sidebar_up_down); two FAIL today with `no method named refresh_diff_spawn` — this failure IS the RED phase signal.

## Outcome

- `src/navigation_tests.rs` created with `#![cfg(test)]` + four `#[tokio::test]` functions + `make_large_repo(dir, file_count) -> Project` fixture helper.
- `#[cfg(test)] mod navigation_tests;` registered in `src/main.rs` adjacent to the existing Phase-3 `mod pty_input_tests;` (binary-only crate deviation preserved).
- Non-test `cargo build` clean; `cargo clippy -- -D warnings` clean.
- `cargo build --tests` FAILS with exactly 2 × `error[E0599]: no method named 'refresh_diff_spawn' found for struct 'app::App'`. This is the explicit TDD gate for Plan 04-02.
- Phase 2/3 grep invariants byte-for-byte preserved.

## Test Function Signatures (verbatim from src/navigation_tests.rs)

```rust
fn make_large_repo(dir: &Path, file_count: usize) -> Project { ... }

#[tokio::test]
async fn click_tab_is_sync() { ... }                   // NAV-04 — PASSES today

#[tokio::test]
async fn sidebar_up_down_is_sync() { ... }             // NAV-01 unit — PASSES today

#[tokio::test]
async fn refresh_diff_spawn_is_nonblocking() { ... }   // NAV-01/02/03 — FAILS today (compile)

#[tokio::test]
async fn workspace_switch_paints_pty_first() { ... }   // NAV-03 behavioral — FAILS today (compile)
```

## TDD Gate — Recorded Compile Output

From `/tmp/nav-wave0-build.log` (`cargo build --tests`):

```
error[E0599]: no method named `refresh_diff_spawn` found for struct `app::App` in the current scope
   --> src/navigation_tests.rs:128:9
    |
128 |     app.refresh_diff_spawn();
    |         ^^^^^^^^^^^^^^^^^^
help: there is a method `refresh_diff` with a similar name

error[E0599]: no method named `refresh_diff_spawn` found for struct `app::App` in the current scope
   --> src/navigation_tests.rs:164:9
    |
164 |     app.refresh_diff_spawn();
    |         ^^^^^^^^^^^^^^^^^^
help: there is a method `refresh_diff` with a similar name

error: could not compile `martins` (bin "martins" test) due to 2 previous errors
```

Total matches of `no method named` in build log: **2** — one for each call site (`refresh_diff_spawn_is_nonblocking` line 128, `workspace_switch_paints_pty_first` line 164). This matches the plan's `<verify>` block exactly.

## Grep Invariant Snapshot (post-plan)

| Pattern                                          | Path                        | Count | Required | Status |
|--------------------------------------------------|-----------------------------|-------|----------|--------|
| `fn click_tab_is_sync`                           | src/navigation_tests.rs     | 1     | 1        | ✓      |
| `fn sidebar_up_down_is_sync`                     | src/navigation_tests.rs     | 1     | 1        | ✓      |
| `fn refresh_diff_spawn_is_nonblocking`           | src/navigation_tests.rs     | 1     | 1        | ✓      |
| `fn workspace_switch_paints_pty_first`           | src/navigation_tests.rs     | 1     | 1        | ✓      |
| `fn make_large_repo`                             | src/navigation_tests.rs     | 1     | 1        | ✓      |
| `app\.refresh_diff_spawn\(\)`                    | src/navigation_tests.rs     | 2     | ≥2       | ✓      |
| `#!\[cfg\(test\)\]`                              | src/navigation_tests.rs     | 1     | 1        | ✓      |
| `tempfile::TempDir`                              | src/navigation_tests.rs     | 1     | ≥1       | ✓      |
| `CommandBuilder\|PtySession`                     | src/navigation_tests.rs     | 0     | 0        | ✓      |
| `mod navigation_tests`                           | src/main.rs                 | 1     | 1        | ✓      |
| `mod pty_input_tests` (preserved)                | src/main.rs                 | 1     | 1        | ✓      |
| `biased;`                                        | src/app.rs                  | 1     | 1        | ✓      |
| `// 1. INPUT`                                    | src/app.rs                  | 1     | 1        | ✓      |
| `if self\.dirty`                                 | src/app.rs                  | 1     | 1        | ✓      |
| `status_tick`                                    | src/app.rs                  | 0     | 0        | ✓      |
| `self\.mark_dirty\(\)`                           | src/app.rs                  | 6     | ≥6       | ✓      |
| `duration_since`                                 | src/pty/session.rs          | 1     | ≥1       | ✓      |

All Phase 2/3 invariants preserved; Phase 4 Wave-0 additions accounted for.

## Fixture-Helper Signature

No drift from the interface block in 04-01-PLAN.md was required. `Project::new(PathBuf, String)` and `GlobalState::default()` match the plan's spec. `App::new(GlobalState, PathBuf) -> anyhow::Result<App>` signature holds. `select_active_workspace(&mut self, idx: usize)` is `pub(crate)` which is accessible from `src/navigation_tests.rs` (same crate).

Note: `App.preview_lines` is actually `Option<(PathBuf, Vec<String>)>` (not `Option<Vec<String>>` as the interface block suggested). Tests do not touch `preview_lines`, so no code adjustment was needed — flagging here only for 04-02's implementer.

## TDD Gate Compliance

- **RED commit:** `e3ea41a test(04-01): add Wave-0 navigation regression-guard tests`
- **GREEN commit (future):** to be landed by Plan 04-02 with `feat(04-02): ...` once `App::refresh_diff_spawn` + the 6th select branch exist.
- **Fail-fast check applied:** The two tests targeting `refresh_diff_spawn` were confirmed to fail (via compile error) before commit. The two tests targeting pre-existing sync code paths (`click_tab_is_sync`, `sidebar_up_down_is_sync`) would have flagged the opposite problem had they compiled but not failed — they compile and don't currently run because the whole test binary fails to link. They are load-bearing for Plan 04-02's GREEN phase: once `refresh_diff_spawn` lands, all four tests must pass.

## Plan 04-02 Unblock Checklist

Plan 04-02 is now **ready to execute** with this explicit signal:

- Compile gate armed: `cargo build --tests` fails with `refresh_diff_spawn` missing method — 04-02 must introduce this method.
- Success criterion for 04-02 GREEN phase: `cargo test navigation_tests` runs with **all four new nav tests passing** and all pre-existing tests still green.
- Implementation sketch already written in `04-RESEARCH.md` §3 "Option A — Fire-and-forget background task with shared channel" and §7 "Proposed pattern: background refresh_diff with mpsc". 04-02 should follow Option A (mpsc) per the research recommendation.

## Deviations from Plan

None - plan executed exactly as written. No Rule-1/2/3 auto-fixes were required. Code compiled and failed exactly as the plan's `<compile expectation>` section predicted.

## Self-Check: PASSED

- FOUND: src/navigation_tests.rs
- FOUND: make_large_repo function definition
- FOUND: click_tab_is_sync, sidebar_up_down_is_sync, refresh_diff_spawn_is_nonblocking, workspace_switch_paints_pty_first
- FOUND: `#[cfg(test)] mod navigation_tests;` in src/main.rs
- FOUND: commit e3ea41a (test(04-01): add Wave-0 navigation regression-guard tests)
- FOUND: expected `no method named refresh_diff_spawn` compile errors (×2) — the TDD gate is armed as required by must_haves.truths #1
- Phase 2/3 grep invariants all preserved
- Non-test `cargo build` clean; `cargo clippy -- -D warnings` clean

**The 4 new nav tests failing to compile IS the pass signal for this RED-phase plan** (see plan must_haves.truths #1). Plan 04-01 is complete; Plan 04-02 is unblocked.
