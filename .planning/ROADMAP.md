# Roadmap: Martins

## Shipped Milestones

- ✅ **v1.0 — Fluidity** (shipped 2026-04-27) — Architectural split, dirty-flag rendering, input-priority select, PTY input fluidity, navigation fluidity, background-work decoupling, Ghostty-style PTY-pane text selection (overlay + tmux-native dual-path). 7 phases / 22 plans / 150 commits / 145 tests / 5044 LOC added. Full archive: [`.planning/milestones/v1.0-ROADMAP.md`](milestones/v1.0-ROADMAP.md). Requirements archive: [`.planning/milestones/v1.0-REQUIREMENTS.md`](milestones/v1.0-REQUIREMENTS.md).

## Active Milestone

(No active milestone. Run `/gsd-new-milestone` to start the next one.)

## Backlog

Items captured during v1.0 that may inform the next milestone:

- **Block/rectangle selection mode toggle** — alternative to default stream selection in PTY-pane (operator post-GAP-7-01 captured this — not a v1.0 blocker, captured as enhancement)
- **5 Info findings from Phase 7 code review** (IN-01..IN-05 in `.planning/phases/07-tmux-native-main-screen-selection/07-REVIEW.md`) — polish-time consolidation work, no functional impact
- **`/gsd-secure-phase 7`** — security gate not run for Phase 7; accept-or-revisit
- **Code review for Phases 1–6** — only Phase 7 had code-review run during v1.0; back-fill if useful
- **v2 candidates** (parked from v1.0 requirements):
  - OBS-01/02 (tracing spans, optional FPS overlay)
  - SCR-01/02 (scrollback search, full scrollback copy)
  - ROB-01/02 (`.unwrap()` audit, workspace transactional rollback)

Promote to active via `/gsd-review-backlog` when starting the next milestone.
