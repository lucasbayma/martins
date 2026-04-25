# Roadmap: Martins — Fluidity Milestone

## Overview

Martins currently feels laggy under every interaction surface — typing, clicking, workspace switching, text selection — because three things in the event loop are wrong: `terminal.draw()` runs every select-loop iteration (no dirty-flag), a 5s periodic `refresh_diff()` timer overlaps with the `notify` file-watcher to produce random lag spikes, and `src/app.rs` is a 2000+ line monolith that tangles event routing, modal dispatch, workspace lifecycle, and draw orchestration.

This milestone untangles the event loop in dependency order. First we split `src/app.rs` into focused modules so subsequent perf work has a clean surface to sit on. Then we land the two structural primitives — dirty-flag rendering and input-priority select — that unblock every interaction-latency requirement. From there we chase the constant-lag targets (PTY typing, navigation) and the spike-lag targets (diff refresh, file watcher, state save) in parallel-ish phases. Text selection ships last, once the render path is stable enough to overlay a highlight that survives PTY output.

Success is judged subjectively against Ghostty/Alacritty by the single user — no numeric SLA. Every phase's success criteria is a behavior the user can feel.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Architectural Split** - Carve `src/app.rs` into focused modules (event routing, modal controller, workspace lifecycle, draw) so downstream perf work has a clean surface
- [x] **Phase 2: Event Loop Rewire** - Introduce dirty-flag rendering and input-priority select branches — the two primitives every interaction-latency goal depends on
- [x] **Phase 3: PTY Input Fluidity** - Typing in the agent pane renders each keystroke immediately, even under heavy PTY output
- [x] **Phase 4: Navigation Fluidity** - Sidebar, workspace, and tab switching all respond instantly with no stutter on keyboard or mouse
- [x] **Phase 5: Background Work Decoupling** - Diff refresh, file watcher, and state save never block the event loop or cause random lag spikes
- [x] **Phase 6: Text Selection** - Drag-select + `cmd+c` copy in the PTY pane, Ghostty-style, with a highlight that survives streaming output (2026-04-25)
- [ ] **Phase 7: tmux-native main-screen selection** - Migrate PTY-pane selection from Martins' REVERSED-XOR overlay to the underlying tmux session's native copy-mode, so selection feels indistinguishable from running tmux directly

## Phase Details

### Phase 1: Architectural Split
**Goal**: Decompose `src/app.rs` into single-responsibility modules so the event loop, modal state, workspace lifecycle, and draw orchestration can each be reasoned about and modified independently. This is the surface every later phase builds on.
**Depends on**: Nothing (first phase)
**Requirements**: ARCH-01
**Success Criteria** (what must be TRUE):
  1. `src/app.rs` is no larger than ~500 lines and contains only the top-level `App` struct and the main `run()` loop
  2. Event routing lives in its own module (`src/events.rs` or equivalent) and is independently navigable
  3. Modal dispatch lives in its own module (`src/ui/modal_controller.rs` or equivalent)
  4. Workspace lifecycle (create/archive/delete) lives in its own module (`src/workspace.rs` or equivalent)
  5. The app compiles, runs, and behaves identically to pre-refactor from the user's perspective — no regressions in existing flows
**Plans:** 5 plans
Plans:
- [x] 01-01-PLAN.md — Extract draw orchestration into src/ui/draw.rs
- [x] 01-02-PLAN.md — Extract modal dispatch into src/ui/modal_controller.rs
- [x] 01-03-PLAN.md — Extract event routing into src/events.rs
- [x] 01-04-PLAN.md — Extract workspace lifecycle into src/workspace.rs
- [x] 01-05-PLAN.md — Final slim-down of src/app.rs to ≤500 lines

### Phase 2: Event Loop Rewire
**Goal**: Install the two structural perf primitives every interaction-latency requirement depends on — a dirty-flag that gates `terminal.draw()`, and a dedicated higher-priority input branch in the `tokio::select!` loop so PTY output and timers can't starve keyboard/mouse events.
**Depends on**: Phase 1
**Requirements**: ARCH-02, ARCH-03
**Success Criteria** (what must be TRUE):
  1. When nothing has changed, the app does not call `terminal.draw()` — idle CPU visibly drops (fans quiet down, `top` shows near-zero CPU on an idle session)
  2. The event loop exposes an explicit "dirty" signal that state mutations set and render consumes — the coupling between state change and redraw is obvious to a reader
  3. Under heavy PTY output (e.g., `cat` of a large file, `claude --verbose`) the app still accepts keyboard input without visible delay
  4. A reader can point to the single place in the event loop where input takes priority over PTY output and timers
**Plans:** 2 plans
Plans:
- [x] 02-01-PLAN.md — Dirty-flag rendering (ARCH-02): add `App.dirty` + `mark_dirty()`, gate `terminal.draw()`, rewire run loop
- [x] 02-02-PLAN.md — Input-priority tokio::select! (ARCH-03): annotate and verify `biased;` + input-first branch ordering

### Phase 3: PTY Input Fluidity
**Goal**: Typing in the PTY main pane feels like typing into Ghostty — each keystroke renders immediately, and heavy background output (streaming agent logs) does not delay input.
**Depends on**: Phase 2
**Requirements**: PTY-01, PTY-02, PTY-03
**Success Criteria** (what must be TRUE):
  1. Typing a burst of characters into the PTY pane renders each one with no perceptible delay — feels indistinguishable from Ghostty to the user
  2. While an agent is streaming verbose output, the user can still type into the input line and see characters appear immediately
  3. Idle the app for 30 seconds, then press a key — the first keystroke renders with no warmup lag (no starvation from idle redraws)
  4. `top` / Activity Monitor shows CPU at near-zero when the app is idle with no PTY output
**Plans:** 2 plans
Plans:
- [x] 03-01-PLAN.md — TDD: three failing PTY-input tests + write_input sync-guarantee doc-comment + manual UAT (PTY-01/02/03)
- [x] 03-02-PLAN.md — CONDITIONAL (triggered by 03-01 UAT fail): frame-budget gate in App::run + should_draw helper + sleep_until branch (PTY-01/02) — SKIPPED, remains on disk as considered-alternative

### Phase 4: Navigation Fluidity
**Goal**: Every way of moving around the app — keyboard sidebar nav, mouse clicks on sidebar items, workspace switching, tab switching — feels instant with no stutter or blank frames.
**Depends on**: Phase 2
**Requirements**: NAV-01, NAV-02, NAV-03, NAV-04
**Success Criteria** (what must be TRUE):
  1. Pressing up/down in the sidebar feels instantaneous — no visible stutter even when holding the key to scroll a long list
  2. Clicking any sidebar item (project, workspace, tab) activates it with no perceptible pause before the pane updates
  3. Switching workspaces shows the target PTY view immediately — no blank frame, no "loading" flash, no re-render stutter
  4. Switching tabs within a workspace is instantaneous — the previous tab's view is replaced in a single frame
**Plans:** 3 plans
Plans:
- [x] 04-01-PLAN.md — Wave 0: four failing nav regression-guard tests + make_large_repo fixture (TDD gate for 04-02)
- [x] 04-02-PLAN.md — Wave 1: add diff_tx/diff_rx + refresh_diff_spawn + 6th select branch; replace 3 .await call-sites
- [x] 04-03-PLAN.md — Wave 2: manual UAT (four feel-tests) + PHASE-SUMMARY.md on approved

### Phase 5: Background Work Decoupling
**Goal**: Eliminate the random lag spikes caused by background work. Git diff refresh becomes event-driven (debounced `notify` events, 30s safety net), file watcher bursts coalesce, and state saves never block input or render.
**Depends on**: Phase 2
**Requirements**: BG-01, BG-02, BG-03, BG-04, BG-05
**Success Criteria** (what must be TRUE):
  1. The 5s diff-refresh timer is gone — diff refresh only fires on actual file-system events (or the 30s safety-net fallback)
  2. Editing files in a workspace with an external editor updates the right-sidebar diff view within ~200ms without any visible stall in the TUI
  3. A burst of file changes (e.g., `cargo build`, `git checkout`) produces at most one diff refresh, not a flurry
  4. Creating, archiving, or deleting a workspace feels instant — the state-save to `~/.martins/state.json` never produces a visible pause
  5. Sitting in the app for several minutes, the user experiences no random lag spikes — interaction feels consistent, not "sometimes fine, sometimes stuck"
**Plans:** 4 plans
Plans:
- [x] 05-01-PLAN.md — Wave 0: TDD tests (save_state_spawn_is_nonblocking fails-to-compile + debounce_rapid_burst_of_10)
- [ ] 05-02-PLAN.md — Wave 1: add save_state_spawn primitive + 30s refresh_tick + non-blocking watcher/refresh arms + 200ms debounce
- [ ] 05-03-PLAN.md — Wave 2: migrate 13 hot-path save_state() call sites + wrap archive remove_dir_all in spawn_blocking
- [ ] 05-04-PLAN.md — Wave 3: manual UAT of 5 ROADMAP criteria + PHASE-SUMMARY.md

### Phase 6: Text Selection
**Goal**: Drag-select text in the PTY main pane with a visible highlight, copy with `cmd+c`, clear with click/Escape — matching Ghostty's feel. The highlight survives streaming PTY output until the user explicitly clears it.
**Depends on**: Phase 3
**Requirements**: SEL-01, SEL-02, SEL-03, SEL-04
**Success Criteria** (what must be TRUE):
  1. Click-and-drag on the PTY main pane shows a highlight that tracks the cursor with no lag or tearing
  2. Pressing `cmd+c` while a selection is active puts the selected text on the macOS clipboard (verifiable via `pbpaste` in another terminal)
  3. Clicking outside the selection, or pressing Escape, clears the highlight immediately in a single frame
  4. While text is selected, new PTY output (e.g., agent streaming a reply) does not cause the highlight to flicker, jitter, or disappear — it stays put until the user clears it
**Plans**: TBD

### Phase 7: tmux-native main-screen selection
**Goal**: Migrate main-pane text selection from Martins' REVERSED-XOR overlay to the underlying tmux session's native copy-mode, so selection in the PTY pane feels indistinguishable from running tmux directly. Operator-flagged during Phase 6 UAT 2026-04-25 — current overlay works but feels non-native vs tmux's own selection.
**Depends on**: Phase 6
**Requirements**: TBD

**Investigation surface:**
- Does Martins' main pane render the tmux session directly enough to enable tmux's copy-mode bindings (mouse + cmd+c forwarded into tmux)?
- How does tmux copy-mode interact with Martins' own `SelectionState` and clear-on-tab/workspace-switch wiring?
- Alt-screen apps (vim, less) should keep the overlay since they don't have tmux-native selection to delegate to.

**Reference:** `.planning/phases/06-text-selection/06-HUMAN-UAT.md` "Forward-Looking Notes" section.

**Plans**: TBD (run `/gsd-plan-phase 7` to break down)

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5 → 6 → 7

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Architectural Split | 5/5 | Complete | 2026-04-24 |
| 2. Event Loop Rewire | 1/2 | In progress | - |
| 3. PTY Input Fluidity | 1/1 | Complete | 2026-04-24 |
| 4. Navigation Fluidity | 0/TBD | Not started | - |
| 5. Background Work Decoupling | 0/4 | Not started | - |
| 6. Text Selection | 6/6 | Complete | 2026-04-25 |
| 7. tmux-native main-screen selection | 0/TBD | Not started | - |

## Backlog

(Empty — promoted items move into the active phase list above.)
