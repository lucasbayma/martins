# Martins

## What This Is

Martins is a Rust TUI workspace orchestrator for macOS that manages git worktrees, tmux-backed PTY sessions, and per-workspace AI agent tabs (Claude, Opencode, Codex). It's a single-user dev tool that runs locally, targeting developers who want a keyboard-driven home base for parallel AI-assisted work across multiple repos and branches.

## Core Value

**Input-to-pixel responsiveness must feel like a native GPU-accelerated terminal.** If typing, clicking, switching workspaces, or selecting text in the PTY pane feels laggy, the app fails — users will reach for Ghostty/Alacritty + tmux directly.

## Current State

**v1.0 Fluidity — SHIPPED 2026-04-27.** All 7 phases / 22 plans / 145 tests delivered. Architectural split, dirty-flag rendering, input-priority select, PTY input fluidity, navigation fluidity, background-work decoupling, and Ghostty-style PTY-pane text selection (overlay + tmux-native dual-path) all validated by operator UAT.

Full archive: [`.planning/milestones/v1.0-ROADMAP.md`](milestones/v1.0-ROADMAP.md).

## Next Milestone Goals

(Awaiting planning. Run `/gsd-new-milestone` to start.)

Backlog candidates from v1.0 (parked, not yet sequenced):

- **Block/rectangle selection mode toggle** — alternative to default stream selection in PTY-pane (operator post-GAP-7-01)
- **5 Info findings from Phase 7 code review** — IN-01..IN-05 polish-time consolidation
- **Code review back-fill for Phases 1–6** — only Phase 7 had `/gsd-code-review` run during v1.0
- **Security gate for Phase 7** — `/gsd-secure-phase 7` not run; accept-or-revisit
- **v2 candidates** — observability (OBS-01/02 tracing spans, optional FPS overlay), scrollback (SCR-01/02 search + full copy), robustness (ROB-01/02 `.unwrap()` audit, workspace transactional rollback)

## Validated Capabilities

<!-- Inferred from existing code at `.planning/codebase/` + v1.0 milestone deliverables. -->

### Pre-existing (validated at milestone start)

- ✓ Multi-project workspace model with git worktree per workspace
- ✓ Per-workspace tmux session persistence (survives app restart)
- ✓ Per-workspace configurable AI agent (Claude/Opencode/Codex) + multiple tabs
- ✓ 3-pane responsive TUI layout (sidebar / PTY main / diff sidebar) via ratatui
- ✓ Modal dialog system (new workspace, new project, confirm, file picker)
- ✓ Live git diff tracking (modified files) via file-watcher + periodic refresh
- ✓ Workspace lifecycle (create, archive, delete, prune) via CLI + TUI
- ✓ Atomic state persistence to `~/.martins/state.json` + backup
- ✓ Homebrew distribution + universal macOS binary

### Validated in v1.0 Fluidity

- ✓ **PTY input fluidity** (PTY-01..03) — Phase 3: each keystroke renders within one frame; agent log streaming doesn't delay input; idle CPU drops to near-zero
- ✓ **Navigation fluidity** (NAV-01..04) — Phase 4: sidebar/workspace/tab switching all respond instantly via `refresh_diff_spawn` fire-and-forget + mpsc drain branch
- ✓ **Background-work decoupling** (BG-01..05) — Phase 5: event-driven diff (debounced `notify` + 30s safety net), async state save, no lag spikes
- ✓ **Architectural split** (ARCH-01..03) — Phases 1+2: `src/app.rs` 2000+ → 436 LOC; events/workspace/modal_controller/draw modules; dirty-flag rendering; input-priority `tokio::select!` biased branch
- ✓ **PTY-pane text selection** (SEL-01..04, dual-path) — Phases 6+7: drag-select with REVERSED-XOR overlay (mouse-app sessions) + tmux-native copy-mode delegate (non-mouse-app sessions); cmd+c 3-tier (overlay → tmux paste-buffer → SIGINT); Esc 3-tier; tab-switch cancel; selection survives streaming PTY output via `Arc<AtomicU64> scroll_generation` anchoring

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
| Success criterion = subjective feel test (not ms metric) | User wants "feels like Ghostty," not a number to chase; metrics risk goodharting | ✓ Validated across v1.0 — every phase used qualitative UAT; no numeric SLA goodharting |
| Text selection scope = drag-select + `cmd+c` copy (Ghostty-like) | Matches native terminal baseline; scrollback search deferred | ✓ Validated in Phase 6 (2026-04-25), extended to dual-path (overlay + tmux-native) in Phase 7 (2026-04-25). GAP-7-01 visual style mismatch resolved via tmux mode-style render (2026-04-27) |
| Diff refresh → event-driven + 30s safety net (drop 5s timer) | `notify` already watches; 5s timer is redundant and causes periodic lag spikes | ✓ Validated in Phase 5 — BG-01..05 all met; no random lag spikes reported |
| `src/app.rs` refactor is in-scope for this milestone | User approved riding momentum while touching event loop; avoids re-touching same code later | ✓ Validated in Phase 1 — `src/app.rs` 2000+ → 436 LOC; every subsequent phase built on the split without regressions |
| No platform expansion (macOS-only stays) | Keeps surface area small; cross-platform conflicts with responsiveness goal | ✓ Good (held throughout v1.0) |
| Phase 7 = dual-path (tmux delegate for non-mouse-app, Phase 6 overlay for mouse-app) | Operator wanted "feels like native tmux" but inner mouse-app TUIs (vim mouse=a, htop, opencode) need their own mouse handling | ✓ Validated 2026-04-25 — operator UAT signed off on dual-path; D-01 boundary held |

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
*Last updated: 2026-04-27 at v1.0 Fluidity milestone close. Next: `/gsd-new-milestone` to define v1.1.*
