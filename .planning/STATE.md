---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: executing
stopped_at: Completed 05-01 Wave-0 regression-guard tests — BG-05 TDD gate armed (save_state_spawn missing-method compile error), BG-04 burst-of-10 test passing on 750ms window
last_updated: "2026-04-24T21:38:37Z"
last_activity: 2026-04-24 -- Phase 05 Plan 01 complete (Wave-0 regression-guard tests)
progress:
  total_phases: 6
  completed_phases: 4
  total_plans: 16
  completed_plans: 16
  percent: 100
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-04-23)

**Core value:** Input-to-pixel responsiveness must feel like a native GPU-accelerated terminal (Ghostty/Alacritty baseline).
**Current focus:** Phase 05 — background-work-decoupling

## Current Position

Phase: 05 (background-work-decoupling) — EXECUTING
Plan: 2 of 4 (next)
Status: Executing Phase 05 — Wave 0 complete
Last activity: 2026-04-24 -- Phase 05 Plan 01 complete (Wave-0 regression-guard tests)

Progress: [██████████] 100%

## Performance Metrics

**Velocity:**

- Total plans completed: 16
- Average duration: —
- Total execution time: —

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01 | 6 | - | - |
| 02 | 3 | - | - |
| 03 | 2 | - | - |
| 04 | 4 | - | - |

**Recent Trend:**

- Last 5 plans: —
- Trend: —

*Updated after each plan completion*
| Phase 01 P01-01 | 15m | 2 tasks | 3 files |
| Phase 01 P01-02 | ~15m | 2 tasks | 3 files |
| Phase 01 P01-03 | ~20m | 3 tasks | 3 files |
| Phase 01 P01-04 | ~15m | 3 tasks | 3 files |
| Phase 01 P01-05 | ~20m | 4 tasks | 5 files |
| Phase 02 P01 | 2m | 2 tasks | 2 files |
| Phase 03 P01 | ~15m | 3 tasks | 3 files |
| Phase 05 P01 | ~10m | 3 tasks | 2 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- Success criterion = subjective feel test against Ghostty/Alacritty (not a ms metric)
- Text selection scope = drag-select + `cmd+c` copy only (scrollback search deferred to v2)
- Diff refresh → event-driven + 30s safety net (drop 5s timer)
- `src/app.rs` refactor is in-scope — rides the momentum of touching the event loop
- No platform expansion — macOS-only stays
- Draw orchestration free-function pattern (ui::draw) established for phase-1 extractions
- App::active_workspace, build_working_map, active_sessions elevated to pub(crate) for module extraction
- Modal controller extracted (01-02): free-function + std::mem::take pattern preserved
- Event routing extracted (01-03): src/events.rs owns handle_event/key/mouse/click/scroll + dispatch_action; TabClick + 10 App methods promoted to pub(crate); crate::events::key_to_bytes pub(crate) for App::forward_key_to_pty
- App delegator dead-code pattern: #[allow(dead_code)] keeps plan-prescribed delegators when intra-module call paths route through crate::events::* directly
- Workspace lifecycle extracted (01-04): src/workspace.rs owns 9 lifecycle free functions (switch_project/create_workspace/create_tab/add_project_from_path/archive_active_workspace/delete_archived_workspace/confirm_delete_workspace/confirm_remove_project/queue_workspace_creation) + tab_program_for_new/resume helpers; App methods become one-line delegators; subprocess call ordering preserved verbatim
- save_state / refresh_active_workspace_after_change / select_active_workspace stay in app.rs as App-field-only helpers — workspace.rs calls them via app.*
- Final slim-down (01-05): all 17 App delegators removed (12 planned + 8 event dead-code + fn draw inlined); call sites in events.rs/modal_controller.rs/App::run rewritten to crate::X::fn(app, ...) directly; reattach_tmux_sessions extracted to workspace.rs; tests relocated to src/app_tests.rs via #[path=...] mod tests; src/app.rs at 436 lines (under 500 ROADMAP target)
- Phase 1 COMPLETE: every ROADMAP Phase 1 success criterion PASS; PHASE-SUMMARY.md written
- Phase 02-01: dirty-flag rendering installed on App — pub(crate) dirty bool + mark_dirty() helper + dirty-gated terminal.draw in run(); tokio::select! gains biased; with events-first ordering; status_tick(1s) replaced by heartbeat_tick(5s) per RESEARCH pitfall #5
- Phase 03-01: PTY-input validation — three regression-guard tests (src/pty_input_tests.rs) + write_input doc-comment affirming synchronous-by-design; test module registered in src/main.rs (binary-only crate deviation from plan which said src/lib.rs)
- Phase 03 closes at Plan 03-01: user UAT approved all four feel-tests → PTY-01/02/03 satisfied by Phase 2 primitives; Plan 03-02 (frame-budget gate) skipped and retained on disk as considered-alternative
- Phase 05-01: Wave-0 regression-guard tests landed — `save_state_spawn_is_nonblocking` (BG-05 TDD gate, fails to compile until Plan 05-02) + `debounce_rapid_burst_of_10` (BG-04 200ms-window guard, passes today on 750ms); Task 3 verification adapted to read app_tests registration from src/app.rs (Phase 1 layout) instead of src/main.rs as plan claimed

### Pending Todos

None yet.

### Blockers/Concerns

- Phase 1 touches ~2000 LOC of `src/app.rs`; borrow-checker friction across extracted state is the main risk — flagged in CONCERNS.md

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| *(none)* | | | |

## Session Continuity

Last session: 2026-04-24T21:38:37Z
Stopped at: Completed 05-01 Wave-0 regression-guard tests — BG-05 TDD gate armed (cargo build --tests fails on save_state_spawn missing-method), BG-04 burst-of-10 test passing on 750ms window; production cargo build green
Resume file: .planning/phases/05-background-work-decoupling/05-02-PLAN.md
Next: Phase 5 Plan 02 (Wave 1) — implement App::save_state_spawn (makes BG-05 gate compile + pass) and tighten watcher debounce 750ms → 200ms

**Completed Phase:** 3 (PTY Input Fluidity) — 1 of 1 plan executed (03-02 skipped per UAT) — 2026-04-24

**Planned Phase:** 5 (Background Work Decoupling) — 4 plans — 2026-04-24T21:30:42.888Z
