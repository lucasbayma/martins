---
phase: 5
slug: background-work-decoupling
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-04-24
---

# Phase 5 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Extracted verbatim from `05-RESEARCH.md §12 Validation Architecture`. Source remains the authoritative copy; this file exists to satisfy the Nyquist Dimension 8 gate and to serve as the single-page reference for executors.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` + `#[tokio::test]` (no external framework) |
| **Config file** | `Cargo.toml` `[dev-dependencies]` — `tempfile 3.10`, `assert_cmd 2.0`, `predicates 3`, `insta 1.40` |
| **Quick run command** | `cargo test --bin martins <specific_test_name> -- --nocapture` |
| **Full suite command** | `cargo test` (107 existing tests + new) |
| **Estimated runtime** | ~30s full suite |

---

## Sampling Rate

- **After every task commit:** Run targeted test for the task (e.g. `cargo test --bin martins save_state_spawn_is_nonblocking -- --nocapture`)
- **After every plan wave:** Run `cargo test && cargo clippy --all-targets -- -D warnings`
- **Before `/gsd-verify-work`:** Full suite green + manual UAT of all 5 ROADMAP success criteria
- **Max feedback latency:** ~30s (full suite); ~3s (single test)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 5-01-01 | 01 | 0 | BG-05 | T-5-01 | New `save_state_spawn_is_nonblocking` test fails-to-compile (helper not yet defined) | unit (red gate) | `cargo build --tests --bin martins` (expect compile error referencing `save_state_spawn`) | ❌ W0 | ⬜ pending |
| 5-01-02 | 01 | 0 | BG-04 | — | `debounce_rapid_burst_of_10` retunes existing test for 200ms window + 10-write burst | unit | `cargo test --bin martins watcher::tests::debounce_rapid_burst_of_10 -- --nocapture` | ✓ extend | ⬜ pending |
| 5-01-03 | 01 | 0 | BG-04, BG-05 | — | Wave 0 closure: tests defined, run-loop unchanged | grep | `rg 'fn save_state_spawn_is_nonblocking' src/app_tests.rs` → 1; `rg 'debounce_rapid_burst_of_10' src/watcher.rs` → 1 | new | ⬜ pending |
| 5-02-01 | 02 | 1 | BG-05 | T-5-02 | `pub(crate) fn save_state_spawn` exists, clones state + path, dispatches via `tokio::task::spawn_blocking` | unit (green) | `cargo test --bin martins save_state_spawn_is_nonblocking -- --nocapture` | new | ⬜ pending |
| 5-02-02 | 02 | 1 | BG-01, BG-02, BG-03 | T-5-03 | refresh_tick interval = 30s; watcher arm + refresh_tick arm both call `refresh_diff_spawn()` (no `.await`); graceful-exit `save_state()` at app.rs:262 stays sync | grep + unit | `rg 'interval\(Duration::from_secs\(30\)\)' src/app.rs` → 1; `rg 'self\.refresh_diff\(\)\.await' src/app.rs` → 1 (App::new only); full `cargo test --bin martins` | mixed | ⬜ pending |
| 5-02-03 | 02 | 1 | BG-04 | — | Watcher debounce window = 200ms | grep + unit | `rg 'Duration::from_millis\(200\)' src/watcher.rs` → 1; `cargo test --bin martins watcher` | mixed | ⬜ pending |
| 5-03-01 | 03 | 2 | BG-05 | T-5-04 | `events.rs` non-exit `save_state()` calls migrated to `save_state_spawn()` | grep + unit | `rg 'save_state_spawn\(\)' src/events.rs` → matches expected count from RESEARCH §5.5; `cargo test --bin martins events::tests` | exists | ⬜ pending |
| 5-03-02 | 03 | 2 | BG-05 | T-5-04 | `workspace.rs` save calls migrated; `archive_active_workspace`'s `std::fs::remove_dir_all` wrapped in `spawn_blocking` | grep + unit | `rg 'save_state_spawn\(\)' src/workspace.rs` → expected count; `rg 'spawn_blocking' src/workspace.rs` → ≥2 (existing tmux + new remove_dir_all); `cargo test --bin martins workspace::tests` | exists | ⬜ pending |
| 5-03-03 | 03 | 2 | BG-05 | T-5-04 | `modal_controller.rs` save calls migrated; full grep invariants pass | grep + full suite | `rg 'save_state_spawn\(\)' src/workspace.rs src/events.rs src/ui/modal_controller.rs` → ≥13; `cargo test && cargo clippy --all-targets -- -D warnings` | exists | ⬜ pending |
| 5-04-01 | 04 | 3 | BG-01..BG-05 | — | Manual UAT against the 5 ROADMAP success criteria | checkpoint:human-verify | (UAT script in 05-04-PLAN.md Task 1) | n/a | ⬜ pending |
| 5-04-02 | 04 | 3 | BG-01..BG-05 | — | PHASE-SUMMARY.md written with Deferred Items (Shape A queue, tmux ops, tracing spans, debouncer-mini upgrade) | docs | `test -f .planning/phases/05-background-work-decoupling/PHASE-SUMMARY.md` | new | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src/app_tests.rs` — add `save_state_spawn_is_nonblocking` test (50ms budget; mirrors Phase 4 `refresh_diff_spawn_is_nonblocking`)
- [ ] `src/watcher.rs` — retune existing `debounce_rapid` → `debounce_rapid_burst_of_10` for 200ms window + 10-write burst
- [ ] (Optional) `src/app_tests.rs` — `save_state_spawn_survives_burst` using `tokio::time::pause` if Shape A coalescing is later adopted (not required for Phase 5)
- [ ] No new framework install needed — `#[tokio::test]`, `tempfile`, `tokio::time::pause` all present

---

## Grep Invariants (Regression Guard for Phase 6+)

```text
# Positive invariants (must be TRUE after Phase 5)
rg 'interval\(Duration::from_secs\(30\)\)' src/app.rs                                  → 1
rg 'pub\(crate\) fn save_state_spawn' src/app.rs                                       → 1
rg -c 'refresh_diff_spawn\(\)' src/app.rs                                              → ≥3
rg 'Duration::from_millis\(200\)' src/watcher.rs                                       → 1
rg 'save_state_spawn\(\)' src/workspace.rs src/events.rs src/ui/modal_controller.rs    → ≥13

# Negative invariants (must be FALSE after Phase 5)
rg 'interval\(Duration::from_secs\(5\)\)' src/app.rs                                   → 1 (heartbeat only)
rg 'self\.refresh_diff\(\)\.await' src/app.rs                                          → 1 (App::new only)
rg 'Duration::from_millis\(750\)' src/watcher.rs                                       → 0

# Phase 2/3/4 invariants — must remain preserved
rg 'biased;' src/app.rs                                                                → 1
rg '// 1\. INPUT' src/app.rs                                                           → 1
rg 'if self\.dirty' src/app.rs                                                         → 1
rg 'status_tick' src/app.rs                                                            → 0
rg 'pub\(crate\) fn refresh_diff_spawn' src/app.rs                                     → 1
```

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| "No random lag spikes over several minutes of use" | BG-01..BG-05 / SC-5 | Subjective feel test, no metric gate per REQUIREMENTS Out of Scope | Run `cargo run --release`, sit in app for 5+ min, edit files in active workspace, switch workspaces, archive a workspace; report any visible stalls |
| "Right-sidebar diff updates within ~200ms after external editor save" | BG-04 / SC-2 | Requires external editor + visual timing | With workspace open, edit a tracked file in vscode, save; observe diff highlight appearance latency |
| "Burst of file changes (`cargo build`, `git checkout`) → ≤1 diff refresh" | BG-04 / SC-3 | Requires external workload trigger | Run `cargo build` from external terminal in workspace; observe diff-pane render activity (should refresh once after burst settles, not flurry) |
| "Workspace create / archive / delete feels instant" | BG-05 / SC-4 | Subjective UI responsiveness | Trigger create-workspace, archive-active-workspace, delete-workspace from modals; report any visible pause |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (5-04-01 is the documented manual checkpoint)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (`save_state_spawn_is_nonblocking`, `debounce_rapid_burst_of_10`)
- [x] No watch-mode flags
- [x] Feedback latency < 30s (full suite); < 3s (targeted)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
