---
phase: 02
plan: 02
subsystem: event-loop
tags: [event-loop, tokio-select, input-priority, arch-03, annotation]
dependency-graph:
  requires:
    - src/app.rs::App::run (post-02-01 shape with biased; + input-first)
    - 02-01-SUMMARY.md (prior wave outcome)
  provides:
    - Priority-ordinal comments on every select branch (1–5)
    - Grep-locatable `// 1. INPUT — highest priority` marker
    - ARCH-03 block-header comment pointing at the canonical location
  affects:
    - Reader-discoverability of input priority (ROADMAP success #4)
status: complete
commits:
  - ee50e0e docs(02-02) annotate input-priority tokio::select! in App::run
verification: human-verify approved
---

# Plan 02-02 — Input-Priority `tokio::select!` Annotation

## What was built

Annotated the `tokio::select!` block in `src/app.rs::App::run` (lines 201–238)
with an ARCH-03 block-header comment and five priority ordinals on the arms.
No structural change — every executable line is byte-identical to 02-01's
output. Annotation-only.

```rust
// Input-priority event loop (ARCH-03): the `biased` directive forces
// select! to poll branches top-to-bottom. events.next() sits first so
// keyboard/mouse input is processed on the very next iteration — PTY
// output and timers cannot starve it.
tokio::select! {
    biased;

    // 1. INPUT — highest priority. Keyboard, mouse, paste, resize.
    Some(Ok(event)) = events.next() => { ... }
    // 2. PTY output — high-volume under streaming agents.
    _ = self.pty_manager.output_notify.notified() => { ... }
    // 3. File watcher — debounced filesystem events.
    Some(event) = async { ... } => { ... }
    // 4. Heartbeat — 5s tick to advance sidebar working-dot.
    _ = heartbeat_tick.tick() => { ... }
    // 5. Safety-net diff refresh — Phase 5 replaces with event-driven.
    _ = refresh_tick.tick() => { ... }
}
```

## Grep-verifiable acceptance (all pass)

| Query | Expected | Actual |
|---|---|---|
| `rg -c 'biased;' src/app.rs` | 1 | 1 |
| `rg -c '// 1\. INPUT' src/app.rs` | 1 | 1 |
| `rg -c '// 2\. PTY output' src/app.rs` | 1 | 1 |
| `rg -c '// 3\. File watcher' src/app.rs` | 1 | 1 |
| `rg -c '// 4\. Heartbeat' src/app.rs` | 1 | 1 |
| `rg -c '// 5\. Safety-net' src/app.rs` | 1 | 1 |
| `rg -c 'ARCH-03' src/app.rs` | ≥1 | 1 |
| Branch order — `events.next()` first after `biased;` | yes | yes (line 210) |
| `cargo build` | 0 | 0 |
| `cargo test` | 100 passed | 100/100 passed |
| `cargo clippy --all-targets -- -D warnings` | 0 | 0 |

## Manual feel test (Task 2, human-verify)

User executed all 5 ROADMAP manual checks against `./target/release/martins`:

| # | Check | Result | Notes |
|---|---|---|---|
| 1 | Idle CPU < 1% | Partial | Baseline near-zero, but `refresh_tick` (5s) firing `refresh_diff()` spikes to 9%. Phase 5 scope — ROADMAP line 87 explicitly plans to drop this timer and make diff refresh event-driven. Not a Phase 2 regression. |
| 2 | Input under PTY load | ✓ Pass | No perceptible delay |
| 3 | Working indicator | N/A for `sleep` | `sleep 10` produces no PTY output so the `⚡` indicator stays dormant (`is_working` threshold = 2s of last_output). Behavior correct; ROADMAP test wording could be tightened in Phase 5. |
| 4 | Solid cursor in terminal mode | ✓ Pass | Expected trade-off from dirty-gated draws |
| 5 | Regression checks (create/switch workspace, type in PTY, drag-select) | ✓ Pass | No stutter, all Phase 1 behavior preserved |

User typed **"approved"** after review.

## Deviations from plan

None. Annotation block matches plan verbatim; no behavior change;
pre-check for `biased;` + `events.next()`-first passed on entry (02-01 landed
them correctly).

## Known follow-ups (scope-documented, not regressions)

- **5s `refresh_tick` CPU spike (~9%)** — owned by Phase 5 (Background Work Decoupling). No action here.
- **`⚡` working indicator not lit for commands with no output** — behavior is correct (threshold = 2s of last PTY output). Update ROADMAP manual-test wording to suggest `yes | head` or similar when Phase 5 revisits.
- **Cursor blink during idle** — accepted per plan Q2 default. 500ms blink tick is a future mitigation if UAT flips.

## Files modified

- `src/app.rs` — +8 annotation lines in `App::run` (no executable delta)

## Commits

- `ee50e0e` — `docs(02-02): annotate input-priority tokio::select! in App::run`

## Self-Check: PASSED

All acceptance_criteria verified; human-verify gate approved; no structural change.
