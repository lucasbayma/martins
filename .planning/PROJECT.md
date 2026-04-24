# Martins

## What This Is

Martins is a Rust TUI workspace orchestrator for macOS that manages git worktrees, tmux-backed PTY sessions, and per-workspace AI agent tabs (Claude, Opencode, Codex). It's a single-user dev tool that runs locally, targeting developers who want a keyboard-driven home base for parallel AI-assisted work across multiple repos and branches.

## Core Value

**Input-to-pixel responsiveness must feel like a native GPU-accelerated terminal.** If typing, clicking, switching workspaces, or selecting text in the PTY pane feels laggy, the app fails — users will reach for Ghostty/Alacritty + tmux directly.

## Requirements

### Validated

<!-- Inferred from existing code at `.planning/codebase/`. -->

- ✓ Multi-project workspace model with git worktree per workspace — existing
- ✓ Per-workspace tmux session persistence (survives app restart) — existing
- ✓ Per-workspace configurable AI agent (Claude/Opencode/Codex) + multiple tabs — existing
- ✓ 3-pane responsive TUI layout (sidebar / PTY main / diff sidebar) via ratatui — existing
- ✓ Modal dialog system (new workspace, new project, confirm, file picker) — existing
- ✓ Live git diff tracking (modified files) via file-watcher + periodic refresh — existing
- ✓ Workspace lifecycle (create, archive, delete, prune) via CLI + TUI — existing
- ✓ Atomic state persistence to `~/.martins/state.json` + backup — existing
- ✓ Homebrew distribution + universal macOS binary — existing
- ✓ **REQ-PERF-02**: Sidebar navigation responds instantly — validated in Phase 4 (Navigation Fluidity): NAV-01..04 user UAT sign-off 2026-04-24, `refresh_diff` made fire-and-forget on nav hot path via `refresh_diff_spawn` + mpsc drain branch

### Active

<!-- Current milestone: make Martins feel native-terminal-fluid. -->

- [ ] **REQ-PERF-01**: Typing in the PTY pane feels immediate — no perceptible lag between keystroke and on-screen character (baseline: Ghostty/Alacritty feel)
- [ ] **REQ-PERF-03**: Workspace/tab switching is instantaneous — no visible pause or re-render stutter
- [ ] **REQ-PERF-04**: Text selection in the PTY main pane works via mouse drag, with `cmd+c` copy (Ghostty-style), with no lag
- [ ] **REQ-PERF-05**: Periodic lag spikes caused by background work (git diff refresh, file watcher, state saves) are eliminated
- [ ] **REQ-ARCH-01**: Refactor `src/app.rs` (2000+ line monolith) into focused modules during perf work — event routing, modal controller, workspace lifecycle, draw orchestration

### Out of Scope

- Linux / Windows support — macOS-only by design; cross-platform adds surface area that conflicts with responsiveness goal
- Quantitative latency targets (sub-16ms, sub-8ms) — success is subjective feel test against Ghostty, not a metric gate
- Scrollback search / buffer querying — nice-to-have, not required for this milestone
- New features unrelated to responsiveness (new modals, new agents, etc.) — deferred until fluidity lands
- Full `.unwrap()` audit across crate — touched opportunistically where it intersects perf work, not a dedicated pass

## Context

**Codebase state (from `.planning/codebase/`):**

- ~12k LOC Rust, single binary, edition 2024, MSRV 1.85
- Event loop in `src/app.rs` uses `tokio::select!` over crossterm events, PTY output, status/refresh ticks, file watcher
- **Known perf concerns already flagged in `CONCERNS.md`:**
  - `terminal.draw()` runs on **every** select-loop iteration — no dirty-flag; idle redraws burn CPU and can starve input
  - `refresh_diff()` runs on a 5s timer even when nothing changed — overlaps with `notify` file watcher
  - PTY read buffers (16KB) under heavy output may contend with the input-handling path
- `src/app.rs` is monolithic (event loop + state + input + modal + PTY orchestration + draw in one file) — refactor is welcome as part of perf work

**User-reported symptoms:**
- Lag is constant (background) plus random spikes (likely the 5s diff refresh)
- All interaction surfaces feel slow: typing in PTY, sidebar nav, mouse click, text select
- Reference baseline is Ghostty / Alacritty — user expects native GPU-terminal feel

**Prior exploration:** None. No spikes, sketches, or prior milestones toward fluidity.

## Constraints

- **Tech stack**: Must stay on `ratatui` + `crossterm` + `tokio` + `portable-pty` + `tmux` — no framework swap. Perf gains come from how the event loop, render pipeline, and PTY integration are wired, not from replacing dependencies.
- **Platform**: macOS only (tmux, pbcopy dependencies). Universal binary via `lipo`.
- **Distribution**: Homebrew tap + GitHub release pipeline stays intact. Release process (`release-martins` skill) must continue to work.
- **Compatibility**: Existing `~/.martins/state.json` (v2 schema) must continue to load. No silent data migrations that break current users.
- **Feel parity**: Responsiveness is judged subjectively against Ghostty/Alacritty, by the single user (project owner). No numeric SLA.
- **Refactor discipline**: `src/app.rs` split is allowed and encouraged, but only in service of perf work — no gratuitous restructuring.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Success criterion = subjective feel test (not ms metric) | User wants "feels like Ghostty," not a number to chase; metrics risk goodharting | — Pending |
| Text selection scope = drag-select + `cmd+c` copy (Ghostty-like) | Matches native terminal baseline; scrollback search deferred | — Pending |
| Diff refresh → event-driven + 30s safety net (drop 5s timer) | `notify` already watches; 5s timer is redundant and causes periodic lag spikes | — Pending |
| `src/app.rs` refactor is in-scope for this milestone | User approved riding momentum while touching event loop; avoids re-touching same code later | — Pending |
| No platform expansion (macOS-only stays) | Keeps surface area small; cross-platform conflicts with responsiveness goal | ✓ Good |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-04-24 after Phase 4 (Navigation Fluidity) completion*
