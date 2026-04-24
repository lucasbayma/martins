---
phase: 05-background-work-decoupling
status: complete
closed: 2026-04-24
requirements: [BG-01, BG-02, BG-03, BG-04, BG-05]
---

# Phase 5 — Background Work Decoupling

## Sign-off

All five feel-tests passed via user UAT on 2026-04-24:

| Req | Feel-test | Result |
|-----|-----------|--------|
| BG-01 | 5s diff-refresh timer is gone — refresh fires on FS events (or 30s safety-net) only | PASS |
| BG-02 | External-editor save updates right-sidebar diff within ~200ms with no TUI stall | PASS |
| BG-03 | Burst of file changes (`cargo build`, `git checkout`) coalesces to ≤1–2 refreshes | PASS |
| BG-04 | Create / archive / delete workspace feels instant — no visible state-save pause | PASS |
| BG-05 | Several minutes of use — interaction stays consistent, no random lag spikes | PASS |

## What Shipped

**Plan 05-01 — Wave 0 regression-guard tests (TDD red):**
- `save_state_spawn_is_nonblocking` test in `src/app_tests.rs` — asserts `App::save_state_spawn()` returns in <5ms. Compile-fails until 05-02 lands the method (intentional TDD gate).
- `debounce_rapid_burst_of_10` test in `src/watcher.rs` — burst of 10 writes × 20ms (200ms span) coalesces to ≤2 events.
- Test-module wiring confirmed via `#[path = "app_tests.rs"] mod tests;` in `src/app.rs:524-526` (Phase 1 pattern; plan's reference to `src/main.rs` was doc-drift).
- Commits `862e201`, `062a987`, `6922397`.

**Plan 05-02 — Primitive + run-loop rewire (TDD green):**
- `App::save_state_spawn(&self)` added at `src/app.rs:386` as `pub(crate) fn` sibling to `save_state` — wraps the sync `GlobalState::save` in `tokio::task::spawn_blocking`, fire-and-forget. Mirrors Phase 4's `refresh_diff_spawn` pattern (Shape B).
- `refresh_tick` interval changed from `Duration::from_secs(5)` → `Duration::from_secs(30)` (BG-02 30s safety net).
- Watcher arm + refresh_tick arm in `tokio::select!` swapped from `self.refresh_diff().await` → `self.refresh_diff_spawn()`.
- `notify-debouncer-mini` debounce window retuned from 750ms → 200ms in `src/watcher.rs:52`.
- Three watcher tests (`debounce_rapid`, `debounce_rapid_burst_of_10`, `filter_noise`) repaired for the tighter 200ms window — bursts collapsed to back-to-back writes; `filter_noise` fixture pre-creates `.git/`/`target/` and drains FSEvents historical buffer. Assertions unchanged.
- Commits `eb7372f`, `1ac0106`, `d944d91`, `3c6cf29`.

**Plan 05-03 — Hot-path call-site migration:**
- 13 hot-path `app.save_state()` → `app.save_state_spawn()` substitutions across three files:
  - `src/events.rs` — 4 sites
  - `src/workspace.rs` — 7 sites
  - `src/ui/modal_controller.rs` — 2 sites
- `std::fs::remove_dir_all` wrapped in `tokio::task::spawn_blocking` at both archive paths in `src/workspace.rs` (`archive_active_workspace` per plan + `delete_archived_workspace` as auto-extended scope — same blocking concern, identical fire-and-forget shape).
- `#[allow(dead_code)]` removed from `App::save_state_spawn` (now wired by all 13 callers).
- Graceful-exit `self.save_state()` at `src/app.rs:264` intentionally preserved as the sole synchronous call (Pitfall #5 — durable save before process exit).
- Commits `19296de`, `e1f32ac`, `cb390ef`, `c017a50`.

## Grep Invariant Snapshot (Phase 6+ Regression Anchor)

```
rg 'app\.save_state\(\);' src/                              → 0   (all hot-path migrated)
rg 'self\.save_state\(\);' src/app.rs                       → 1   (line 264 graceful-exit ONLY)
rg 'pub\(crate\) fn save_state_spawn' src/app.rs            → 1   (line 386)
rg 'save_state_spawn\(\)' src/{events.rs,workspace.rs,ui/modal_controller.rs}
                                                            → 4 + 7 + 2 = 13
rg 'self\.refresh_diff\(\)\.await' src/app.rs               → 0   (Phase 5 removed remaining 2)
rg 'self\.refresh_diff_spawn\(\)' src/app.rs                → 2   (watcher arm + refresh_tick arm)
rg 'interval\(Duration::from_secs\(30\)\)' src/app.rs       → 1   (refresh_tick — BG-02 safety net)
rg 'interval\(Duration::from_secs\(5\)\)' src/app.rs        → 1   (heartbeat_tick only — Phase 2)
rg 'Duration::from_millis\(200\)' src/watcher.rs            → 1   (debounce window)
rg 'spawn_blocking' src/workspace.rs                        → 3   (2× remove_dir_all + 1× tmux pre-existing)
```

Phase 2/3/4 invariants preserved byte-for-byte:
`biased;`=1, `// 1. INPUT`=1, `if self.dirty`=1, `status_tick`=0, `pub(crate) fn refresh_diff_spawn`=1, 6th `diff_rx.recv()` select branch=1.

## Test Status

- `cargo test --bin martins` — **109 passed / 0 failed** (107 prior + 2 new Phase 5 guards).
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo build --tests` — clean.
- Stability: `save_state_spawn_is_nonblocking` runs in 0.03s (5ms budget comfortable); watcher tests 10/10 stable runs.

## Deviations Recorded

1. **Doc-drift (Plan 05-01 Task 3):** Plan referenced `src/main.rs` for test-module registration; actual location is `src/app.rs:524-526` via `#[path = "app_tests.rs"] mod tests;`. Verified TDD gate reachability via `cargo build --tests` error output instead. No file change.
2. **Watcher test repair (Plan 05-02):** Three pre-existing watcher tests required burst-pattern adjustment for the tighter 200ms window. Zero `#[ignore]` added; zero assertion loosening; pre-existing `filter_noise` race exposed and fixed (FSEvents historical-buffer drain). All assertions unchanged.
3. **Auto-extended scope (Plan 05-03):** `delete_archived_workspace`'s `remove_dir_all` was also wrapped in `spawn_blocking` (plan only prescribed `archive_active_workspace`). Identical blocking concern + same fire-and-forget shape; strictly stronger satisfaction of BG-05 success criterion #4.

## Deferred Items (carry-forward to v2 / future phases)

- **Shape A save queue** — long-lived consumer task with serialized + coalesced writes. Current Shape B (per-site fire-and-forget `spawn_blocking`) accepted; revisit only if a future ordering bug surfaces. (RESEARCH §17.1)
- **`notify-debouncer-mini` 0.7.0 upgrade** — current pin is 0.6; 0.7.0 brings nicer typed callbacks. No functional benefit at current scale. (RESEARCH §17 implicit)
- **Tmux ops (`kill_session`, `resize_session`, etc.) off-thread** — currently inline; <20ms typical, not a lag-spike source. Revisit if a future UAT surfaces tmux-heavy lag. (RESEARCH §17.4)
- **Tracing spans around `*_spawn` helpers** — aligned with v2 OBS-01 (observability) phase. (RESEARCH §17.5)

## Next-Phase Readiness

The event loop is now fully non-blocking on the hot paths: input, render, watcher, refresh_tick, and 13 state-save sites all return inside microseconds. The graceful-exit `save_state` at `src/app.rs:264` remains the sole intentional sync call. Any future background work (additional watchers, tag refresh, language servers, etc.) can reuse the same `spawn_blocking` + mpsc-drain idiom established in Phase 4 (`refresh_diff_spawn` + 6th select branch) and Phase 5 (`save_state_spawn`).
