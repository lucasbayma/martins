# Requirements: Martins — Fluidity Milestone

**Defined:** 2026-04-23
**Core Value:** Input-to-pixel responsiveness must feel like a native GPU-accelerated terminal (Ghostty/Alacritty baseline).

## v1 Requirements

Requirements for the fluidity milestone. Each maps to exactly one roadmap phase.

### PTY Input

<!-- Typing in the main agent pane must feel native-terminal immediate. -->

- [ ] **PTY-01**: Typing in the PTY pane renders each keystroke within one frame — no perceptible lag between keypress and on-screen character
- [ ] **PTY-02**: Keystrokes during heavy PTY output (agent streaming logs) are not delayed by the output render path — input takes priority over background work
- [ ] **PTY-03**: The render loop only redraws when state changed (dirty-flag), so idle CPU drops and input events are not starved by continuous redraws

### Navigation

<!-- Sidebar, workspace switching, tab switching — keyboard and mouse. -->

- [ ] **NAV-01**: Keyboard navigation in the sidebar (up/down/select) responds within one frame with no visible stutter
- [ ] **NAV-02**: Mouse click on a sidebar item (project, workspace, tab) activates it instantly with no visible pause
- [ ] **NAV-03**: Switching between workspaces presents the target workspace's PTY view instantaneously (no re-render stutter or blank frame)
- [ ] **NAV-04**: Switching between tabs within a workspace is instantaneous (no re-render stutter)

### Text Selection

<!-- Ghostty-style drag-select + cmd+c copy in the main PTY pane. -->

- [x] **SEL-01
**: Mouse drag on the PTY main pane starts a text selection with a visible highlight that tracks the cursor with no lag
- [x] **SEL-02
**: `cmd+c` while a selection is active copies the selected text to the macOS clipboard via `pbcopy`
- [x] **SEL-03
**: Click (or Escape) outside the selection clears the highlight immediately
- [x] **SEL-04
**: Selection highlight does not flicker or disappear when the underlying PTY buffer receives new output — the selection stays stable until the user clears it

### Background Work

<!-- Diff refresh, file watcher, state persistence must not cause lag spikes. -->

- [x] **BG-01
**: Git diff refresh is event-driven — triggered only by debounced `notify` file-system events, not a 5s periodic timer
- [x] **BG-02
**: A safety-net timer at 30s (not 5s) re-runs diff as a fallback if no file events fire
- [x] **BG-03
**: Diff refresh runs as a background tokio task and never blocks the event loop or input path
- [x] **BG-04
**: File watcher events are debounced (target ~200ms) so bursts of file changes produce at most one diff refresh
- [x] **BG-05
**: State save (`~/.martins/state.json`) runs asynchronously — it never blocks input or render, even during workspace mutations

### Architecture

<!-- Structural work needed to enable the perf goals above. -->

- [x] **ARCH-01
**: `src/app.rs` is split into focused modules: event routing, modal controller, workspace lifecycle, draw orchestration — each file ≤ ~500 lines and single-responsibility
- [x] **ARCH-02
**: The event loop exposes a clear "dirty" signal that render reads, decoupling state mutation from draw
- [ ] **ARCH-03**: Input events (keyboard/mouse) have a dedicated, higher-priority branch in the `tokio::select!` loop so PTY output and timers can't starve them

## v2 Requirements

Deferred. Tracked but not in this milestone's roadmap.

### Observability

- **OBS-01**: Tracing spans around render, input, and PTY paths so future perf regressions are diagnosable
- **OBS-02**: Optional on-screen FPS / frame-time overlay (toggleable via keybind) for diagnosis

### Scrollback

- **SCR-01**: Search scrollback buffer in the PTY pane
- **SCR-02**: Copy entire scrollback to clipboard

### Robustness

- **ROB-01**: Full `.unwrap()` audit — convert fallible paths to `?` propagation (opportunistic in v1 where it intersects perf work)
- **ROB-02**: Workspace-creation transactional rollback on partial failure

## Out of Scope

| Feature | Reason |
|---------|--------|
| Quantitative latency SLA (sub-16ms / sub-8ms) | Success is subjective feel test against Ghostty, not a metric gate |
| Linux / Windows support | macOS-only by design; cross-platform conflicts with responsiveness goal |
| Framework swap (ratatui → other) | Constraint: keep current stack; gains come from wiring, not replacement |
| New modals, agents, or features unrelated to fluidity | Deferred until responsiveness lands |
| Scrollback search / buffer query | Nice-to-have, not required for fluidity baseline |
| GPU-accelerated renderer | Out of ratatui's model; not pursued in this milestone |

## Traceability

Which phases cover which requirements. Populated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| ARCH-01 | Phase 1 (Architectural Split) | Pending |
| ARCH-02 | Phase 2 (Event Loop Rewire) | Pending |
| ARCH-03 | Phase 2 (Event Loop Rewire) | Pending |
| PTY-01 | Phase 3 (PTY Input Fluidity) | Pending |
| PTY-02 | Phase 3 (PTY Input Fluidity) | Pending |
| PTY-03 | Phase 3 (PTY Input Fluidity) | Pending |
| NAV-01 | Phase 4 (Navigation Fluidity) | Pending |
| NAV-02 | Phase 4 (Navigation Fluidity) | Pending |
| NAV-03 | Phase 4 (Navigation Fluidity) | Pending |
| NAV-04 | Phase 4 (Navigation Fluidity) | Pending |
| BG-01 | Phase 5 (Background Work Decoupling) | Pending |
| BG-02 | Phase 5 (Background Work Decoupling) | Pending |
| BG-03 | Phase 5 (Background Work Decoupling) | Pending |
| BG-04 | Phase 5 (Background Work Decoupling) | Pending |
| BG-05 | Phase 5 (Background Work Decoupling) | Pending |
| SEL-01 | Phase 6 (Text Selection) | Pending |
| SEL-02 | Phase 6 (Text Selection) | Pending |
| SEL-03 | Phase 6 (Text Selection) | Pending |
| SEL-04 | Phase 6 (Text Selection) | Pending |

**Coverage:**
- v1 requirements: 19 total
- Mapped to phases: 19 ✓
- Unmapped: 0

---
*Requirements defined: 2026-04-23*
*Last updated: 2026-04-23 after roadmap creation — traceability populated*
