# Milestone v1.0 — Project Summary

**Generated:** 2026-04-27
**Purpose:** Team onboarding and project review
**Status:** SHIPPED 2026-04-27 (tag `v1.0`)
**Source archives:** [`milestones/v1.0-ROADMAP.md`](../milestones/v1.0-ROADMAP.md) · [`milestones/v1.0-REQUIREMENTS.md`](../milestones/v1.0-REQUIREMENTS.md)

---

## 1. Project Overview

**What Martins is:** A Rust TUI workspace orchestrator for macOS that manages git worktrees, tmux-backed PTY sessions, and per-workspace AI agent tabs (Claude, Opencode, Codex). Single-user dev tool, runs locally, targets developers who want a keyboard-driven home base for parallel AI-assisted work across multiple repos and branches.

**Core value:** Input-to-pixel responsiveness must feel like a native GPU-accelerated terminal (Ghostty/Alacritty baseline). If typing, clicking, switching workspaces, or selecting text in the PTY pane feels laggy, the app fails — users will reach for Ghostty/Alacritty + tmux directly.

**v1.0 mission:** Make Martins feel native-terminal-fluid. Eliminate input lag, render-loop CPU burn, and background-work spikes; ship Ghostty-style text selection on the PTY main pane.

**Mission status:** Achieved. All 7 phases passed operator UAT against the qualitative "feels like Ghostty" gate. No numeric SLA was set (deliberate — see Decision D-1.0-1) and none was needed.

---

## 2. Architecture & Technical Decisions

### Pre-existing structural primitives (validated, kept as-is)

- Multi-project workspace model with **git worktree per workspace**
- **Per-workspace tmux session persistence** (survives app restart) via `src/tmux.rs` subprocess helpers
- Per-workspace configurable AI agent (Claude/Opencode/Codex) + multiple tabs
- 3-pane responsive TUI layout (sidebar / PTY main / diff sidebar) via **ratatui**
- Modal dialog system, file picker, command-args form
- Live git diff tracking via **`notify` file-watcher** + (formerly) periodic refresh
- Atomic state persistence to `~/.martins/state.json` + backup
- **portable-pty** for PTY spawning, **vt100** crate for terminal emulation, **tokio** runtime
- Universal macOS binary via `lipo` + Homebrew tap distribution

### v1.0 architectural changes

| Decision | Why | Where it lives |
|----------|-----|----------------|
| **Split `src/app.rs` (2000+ LOC) into single-responsibility modules** | The monolith tangled event routing, modal dispatch, workspace lifecycle, and draw orchestration; impossible to reason about in isolation. Split before perf work so subsequent phases had a clean surface. | `src/app.rs` (436 LOC core) + `src/events.rs` + `src/ui/modal_controller.rs` + `src/workspace.rs` + `src/ui/draw.rs` |
| **Dirty-flag rendering** (ARCH-02) | `terminal.draw()` ran every select-loop iteration regardless of whether anything changed → idle CPU burn + risk of starving input. | `App.dirty: bool` + `mark_dirty()` set at every state mutation; render gates on it; idle CPU drops to near-zero |
| **Biased input-priority `tokio::select!`** (ARCH-03) | PTY output bursts and 5s diff timer could starve keyboard/mouse events. | `tokio::select! { biased; ... }` with input branch first |
| **Event-driven diff refresh** (BG-01..02) | 5s polling timer was redundant with `notify` watcher and was the visible source of "random lag spikes." | Drop 5s timer; debounce `notify` events ~200ms; 30s safety-net fallback |
| **Fire-and-forget `refresh_diff_spawn`** (NAV-03..04) | Tab/workspace switching `await`-ed diff refresh on the hot path → visible blank-frame stutter. | New `mpsc` drain branch in `tokio::select!`; nav handlers spawn diff and return; result delivered when ready |
| **`save_state_spawn` primitive** (BG-05) | 13 hot-path call sites synchronously wrote `state.json` → blocked input during workspace mutations. | Migrated all 13 to fire-and-forget `spawn`; `archive` flow's `remove_dir_all` wrapped in `spawn_blocking` |
| **`Arc<AtomicU64> scroll_generation` on `PtySession`** (SEL-04) | Selection coords needed to survive PTY scroll without flicker. | Drain loop bumps gen on detected scroll (SCROLLBACK-LEN heuristic: `cursor was at last row AND top row hash changed`); selection stores anchored gen+row, render translates `current_row = anchored_row - (current_gen - anchored_gen)` |
| **REVERSED-XOR overlay highlight** (Phase 6, SEL-01) | Phase 6 ship: drag-select with cell-level highlight that survives streaming output. | `cell.modifier.toggle(Modifier::REVERSED)` in `terminal.rs` render pass |
| **Replaced overlay highlight with tmux mode-style** (GAP-7-01 fix) | Operator dual-pane comparison vs native tmux exposed XOR-toggling reads as visually divergent. | `cell.modifier.remove(REVERSED); cell.fg = Black; cell.bg = Yellow;` — uniform yellow block matching tmux 3.6a default `mode-style` |
| **Conditional `handle_mouse` intercept** (Phase 7, D-01) | Operator wanted "feels like native tmux" for plain shells but mouse-app TUIs (vim mouse=a, htop, opencode) need their own mouse handling. | When `active_session_delegates_to_tmux()` (vt100 reports `mouse_protocol_mode == None && !alternate_screen`), forward SGR mouse bytes via `encode_sgr_mouse(...)` to wrapped tmux PTY; otherwise run Phase 6 overlay path |
| **Per-gesture delegation latch** (Phase 7, WR-02 fix) | Without latch, mid-gesture flips of inner `mouse_protocol_mode` would orphan tmux's button state. | New `Arc<AtomicBool> tmux_gesture_delegating` on `PtySession`; latch set on forwarded `Down(Left)`, cleared on `Up(Left)`; Drag/Up always honor latch even if live `delegates_to_tmux()` flipped |
| **3-tier keyboard precedence** (cmd+c, Esc) | Multiple legitimate consumers per key needed deterministic ordering. | `handle_key`: cmd+c → overlay sel? → tmux sel? → SIGINT. Esc → overlay sel? → tmux copy-mode? → fall-through to PTY. |
| **`set_active_tab` D-16 cancel-outgoing** | Tab switch left tmux's outgoing-tab selection in copy-mode permanently. | `crate::tmux::cancel_copy_mode(&name)` + flag clears for outgoing session BEFORE `self.active_tab = index` |
| **Override-only tmux.conf philosophy** | Re-binding tmux defaults is drift-prone across versions. | `ensure_config()` writes only the 3 lines diverging from tmux 3.6a defaults (`y`/`Enter` `pbcopy`, `Escape` cancel) |
| **Env-var-gated diagnostic instrumentation** (`MARTINS_MOUSE_DEBUG`) | Empirical mouse-event tracing is invaluable for selection bugs but costs zero when off. | `if std::env::var_os("MARTINS_MOUSE_DEBUG").is_some() { eprintln!(...) }` at `handle_mouse` top + selection render. Permanent, opt-in. |

### Tech stack

- **Rust** edition 2024, MSRV 1.85, single binary
- **ratatui** + **crossterm** + **tokio** + **portable-pty** + **vt100** + tmux subprocess
- **Anti-decisions:** No framework swap (D-1.0-2); no platform expansion beyond macOS (D-1.0-3); no GPU renderer; no quantitative latency SLA.

---

## 3. Phases Delivered

| # | Phase | Plans | Completed | One-liner |
|---|-------|:----:|-----------|-----------|
| 1 | Architectural Split | 5 | 2026-04-24 | Carved `src/app.rs` into focused modules — single-responsibility surface for every subsequent phase |
| 2 | Event Loop Rewire | 2 | 2026-04-24 | Dirty-flag rendering (ARCH-02) + biased input-priority `tokio::select!` (ARCH-03) — the structural primitives every interaction-latency goal depended on |
| 3 | PTY Input Fluidity | 2 | 2026-04-24 | TDD-driven sync-write doc-contract for `write_input` + manual UAT (PTY-01/02/03); frame-budget gate plan landed as considered-alternative (skipped after UAT pass) |
| 4 | Navigation Fluidity | 3 | 2026-04-24 | Fire-and-forget diff refresh (`refresh_diff_spawn` + `mpsc` drain branch) on the nav hot path; sidebar/workspace/tab switching now instant (NAV-01..04) |
| 5 | Background Work Decoupling | 4 | 2026-04-24 | Event-driven diff (debounced `notify` + 30s safety net), 13 hot-path `save_state` call sites migrated, archive `remove_dir_all` async-wrapped |
| 6 | Text Selection | 6 | 2026-04-25 | Drag-select with REVERSED-XOR overlay + `Arc<AtomicU64> scroll_generation` for anchored coordinate translation; cmd+c→pbcopy, Esc/click clears, double/triple-click word/line, shift+click extend, clear-on-tab-switch |
| 7 | tmux-native main-screen selection | 6 | 2026-04-25 | Conditional `handle_mouse` intercept: non-mouse-app sessions delegate SGR bytes to wrapped tmux; mouse-app sessions retain Phase 6 overlay end-to-end. cmd+c 3-tier (overlay → tmux paste-buffer → SIGINT). Esc 3-tier. Tab-switch cancels outgoing tmux selection. **GAP-7-01 fix:** overlay highlight switched from XOR-REVERSED to tmux mode-style for visual parity |

**Total:** 7 phases / 22 plans / all complete

**Phase dependencies:** 1 → 2 → {3, 4, 5} (parallel-ish) → 6 → 7. Each phase has its own SUMMARY.md / VERIFICATION.md / HUMAN-UAT.md under `.planning/phases/{N}-{slug}/`.

---

## 4. Requirements Coverage

**19/19 v1 requirements validated.**

| ID | Requirement | Phase | Validated |
|----|-------------|-------|-----------|
| ARCH-01 | `src/app.rs` split into focused modules ≤ ~500 lines | Phase 1 | 2026-04-24 (final 436 LOC) |
| ARCH-02 | Event loop dirty-flag decouples mutation from draw | Phase 2 | 2026-04-24 |
| ARCH-03 | Input events have a higher-priority branch in `tokio::select!` | Phase 2 | 2026-04-24 |
| PTY-01 | Typing renders each keystroke within one frame | Phase 3 | 2026-04-24 |
| PTY-02 | Keystrokes during heavy PTY output are not delayed | Phase 3 | 2026-04-24 |
| PTY-03 | Render only redraws on state change (dirty-flag) | Phase 3 | 2026-04-24 |
| NAV-01 | Keyboard sidebar nav responds within one frame | Phase 4 | 2026-04-24 |
| NAV-02 | Mouse click on sidebar item activates instantly | Phase 4 | 2026-04-24 |
| NAV-03 | Workspace switching is instant (no blank frame) | Phase 4 | 2026-04-24 |
| NAV-04 | Tab switching is instant | Phase 4 | 2026-04-24 |
| BG-01 | Diff refresh is event-driven (no 5s polling timer) | Phase 5 | 2026-04-24 |
| BG-02 | 30s safety-net timer fallback | Phase 5 | 2026-04-24 |
| BG-03 | Diff refresh runs as background tokio task, never blocks | Phase 5 | 2026-04-24 |
| BG-04 | File-watcher events debounced ~200ms | Phase 5 | 2026-04-24 |
| BG-05 | State save is async, never blocks | Phase 5 | 2026-04-24 |
| SEL-01 | Drag-select with visible highlight tracking cursor with no lag | Phase 6 + 7 (dual-path) | 2026-04-25 |
| SEL-02 | `cmd+c` copies selection to macOS clipboard via `pbcopy` | Phase 6 + 7 (dual-path) | 2026-04-25 |
| SEL-03 | Click outside or Escape clears highlight in single frame | Phase 6 + 7 (dual-path) | 2026-04-25 |
| SEL-04 | Highlight survives streaming PTY output without flicker | Phase 6 + 7 (dual-path) | 2026-04-25 |

**Validation method:** Subjective feel test against Ghostty+tmux baseline (operator UAT per phase). No numeric SLA was set or pursued — see decision D-1.0-1.

**Deferred to v2 (parked):** OBS-01/02 (tracing spans, FPS overlay), SCR-01/02 (scrollback search, scrollback copy), ROB-01/02 (`.unwrap()` audit, workspace transactional rollback).

---

## 5. Key Decisions Log

### Milestone-level decisions (`PROJECT.md`)

| ID | Decision | Why | Outcome |
|----|----------|-----|---------|
| D-1.0-1 | Success = subjective feel test, not ms metric | Metrics risk goodharting; user wanted "feels like Ghostty," not a number | ✓ Held — every phase used qualitative UAT |
| D-1.0-2 | No framework swap (stay on ratatui+crossterm+tokio+portable-pty+tmux) | Perf gains from wiring, not replacement | ✓ Held |
| D-1.0-3 | macOS-only (no platform expansion) | Cross-platform conflicts with responsiveness goal | ✓ Held |
| D-1.0-4 | `src/app.rs` refactor in-scope this milestone | Avoid re-touching same code later | ✓ Validated in Phase 1 |
| D-1.0-5 | Diff refresh → event-driven + 30s safety net (drop 5s timer) | `notify` already watches; 5s timer redundant | ✓ Validated in Phase 5 |
| D-1.0-6 | Phase 7 = dual-path (tmux delegate non-mouse-app, overlay mouse-app) | Operator wanted native-tmux feel but inner TUIs need own mouse handling | ✓ Validated 2026-04-25 |

### Phase 6 selection-stability decisions (`06-CONTEXT.md`)

- **D-01:** Auto-copy on Left-mouse-up (releasing drag → `pbcopy`)
- **D-02:** `cmd+c` while selection active also re-copies (both paths converge on `App::copy_selection_to_clipboard`)
- **D-03:** `cmd+c` with NO selection → forward `0x03` (SIGINT) to active PTY
- **D-04:** Successful copy does NOT clear highlight (lets user copy/paste/re-copy)
- **D-05:** Add `scroll_generation: u64` (sourced from PTY drain loop, not render)
- **D-06:** `SelectionState` stores anchored coords `(gen, screen_row, col)`; render computes `current_row = anchored_row - (current_gen - anchored_gen)`

### Phase 7 dual-path decisions (`07-CONTEXT.md`)

- **D-01:** Tmux owns selection only when inner program has not requested mouse mode
- **D-02:** vt100 already tracks `mouse_protocol_mode` — reuse, don't add a second scanner
- **D-03:** Do NOT query tmux on demand (`display-message -p '#{mouse_any_flag}'`) — subprocess per drag would tank latency
- **D-04:** When delegating, forward raw SGR mouse events (`\x1b[<0;col;rowM`) to wrapped tmux PTY — same code path as running tmux directly
- **D-05:** Do NOT drive tmux via `send-keys -X` for drag tracking (one subprocess per move = lag)
- **D-06:** Coords map 1:1 — Martins owns pane size, tmux runs at that exact size
- **D-14:** Single Esc exits tmux copy-mode (override default vi-mode behavior via `ensure_config`)
- **D-16:** Tab switch cancels outgoing tmux selection
- **D-20/21 → GAP-7-01 fix:** Originally XOR-REVERSED overlay; flipped to tmux mode-style (fg=Black, bg=Yellow) after operator dual-pane comparison surfaced visual mismatch

---

## 6. Tech Debt & Deferred Items

### Acknowledged at milestone close (logged in `STATE.md`)

| Item | Status | Note |
|------|--------|------|
| `01-HUMAN-UAT.md` flagged by audit | passed, 0 pending scenarios | Audit residual — lists any UAT.md regardless of pass status |
| `06-HUMAN-UAT.md` flagged by audit | passed, 0 pending scenarios | Same |
| `07-HUMAN-UAT.md` flagged by audit | passed, 0 pending scenarios | Same |
| Phase 1 verification originally `human_needed` | Resolved → `passed` via implicit milestone validation | 5 PTY/visual checks couldn't run in cargo test; transitively validated by Phases 02-07 building atop split without regressions |

### Deferred to v1.1 (parked, not blocking)

- **5 Info findings from Phase 7 code review** (`07-REVIEW.md`):
  - **IN-01:** Legacy inline encoder at `events.rs:287-292` diverges from the consolidated `encode_sgr_mouse` on modifier handling and coord saturation
  - **IN-02:** `tmux_in_copy_mode` flag set on `Down(Left)` before tmux actually enters copy-mode (single click doesn't enter it); should move to `Drag` arm
  - **IN-03:** `Arc<AtomicBool>` for `tmux_in_copy_mode` / `tmux_drag_seen` with no cross-thread sharing — plain `AtomicBool` would suffice
  - **IN-04:** `save_buffer_to_pbcopy` returns `true` even when `pbcopy.wait()` errors — doc/behavior mismatch
  - **IN-05:** DECSET-mode caveat duplicated between `selection_tests.rs` and `tmux_native_selection_tests.rs`
- **Block/rectangle selection mode toggle** — alternative to default stream selection (operator post-GAP-7-01 captured this; not v1.0 blocker)
- **`/gsd-secure-phase 7`** — security gate not run for Phase 7
- **Code review back-fill for Phases 1–6** — only Phase 7 had `/gsd-code-review` invoked
- **Pre-existing PTY scroll-generation test flakiness** — `selection_tests::scroll_generation_increments_on_vertical_scroll` is timing-based; manifests at default `--test-threads`; mitigation: deadline extended 500ms→2000ms in REVIEW-MINOR-04. Test reliably passes with `--test-threads=2`.

### v2 candidates (parked from v1.0 requirements)

- **OBS-01/02:** Tracing spans around render/input/PTY paths; optional FPS / frame-time overlay (toggleable via keybind)
- **SCR-01/02:** Search scrollback buffer in PTY pane; copy entire scrollback to clipboard
- **ROB-01/02:** Full `.unwrap()` audit; workspace-creation transactional rollback on partial failure

---

## 7. Getting Started

### Run the project

```bash
# Built and tested binary at:
target/release/martins

# Or build fresh:
cargo build --release

# Or via Homebrew (after `git push origin v1.0` triggers release pipeline):
brew install lucasbayma/martins/martins
brew upgrade martins
```

### Run the test suite

```bash
# Recommended (matches CI; reduces PTY race flakiness):
cargo test --bin martins -- --test-threads=2

# Expected: test result: ok. 145 passed; 0 failed; 0 ignored
```

> **Note:** Martins is a binary-only crate (no `[lib]` target). `cargo test --lib` errors with "no library targets found" — use `--bin martins`. This convention is documented in every Phase 07 plan SUMMARY.

### Diagnostic instrumentation

```bash
# Mouse-event + selection-render tracing (zero cost when env var unset):
MARTINS_MOUSE_DEBUG=1 ./target/release/martins 2>/tmp/martins-debug.log

# Captures every mouse event with kind, raw coords, inner rect, in_terminal,
# delegate decision, plus selection bounds and gen/delta math each render.
# Built during GAP-7-01 investigation; permanent, opt-in.
```

### Key directories

```
src/
├── main.rs                       Entry point + module registration
├── app.rs                        App state + run loop (436 LOC core)
├── events.rs                     handle_event / handle_key / handle_mouse + encode_sgr_mouse (Phase 1 + 7)
├── workspace.rs                  Workspace lifecycle (Phase 1)
├── pty/session.rs                PtySession + scroll_generation + tmux flags (Phase 6 + 7)
├── tmux.rs                       tmux subprocess helpers, ensure_config, save_buffer_to_pbcopy, cancel_copy_mode (Phase 7)
├── selection_tests.rs            Phase 6 selection regression tests
├── tmux_native_selection_tests.rs Phase 7 TM-ENC / TM-DISPATCH / TM-CMDC / TM-ESC / TM-CANCEL tests
└── ui/
    ├── draw.rs                   Top-level draw orchestration (Phase 1)
    ├── modal_controller.rs       Modal dispatch (Phase 1)
    ├── terminal.rs               PTY pane render + selection highlight pass (Phase 6 + GAP-7-01 fix)
    ├── modal.rs                  Modal types
    └── picker.rs                 Picker (file/command/agent)

.planning/                        GSD project planning surface
├── PROJECT.md                    Living project doc (evolved 2026-04-27)
├── ROADMAP.md                    Active roadmap (collapsed at v1.0 close)
├── STATE.md                      Current state (between_milestones after v1.0)
├── milestones/
│   ├── v1.0-ROADMAP.md           Full v1.0 archive
│   └── v1.0-REQUIREMENTS.md      All 19 v1 reqs marked Complete
├── phases/{N}-{slug}/            Per-phase: PLAN.md, SUMMARY.md, VERIFICATION.md, HUMAN-UAT.md, REVIEW.md, etc.
├── debug/                        Active + resolved debug sessions
├── seeds/                        Forward-looking ideas (SEED-001 realized in Phase 7)
└── reports/                      This file lives here
```

### Where to look first by task

| Task | Start at |
|------|----------|
| Understand a feature shipped in v1.0 | `.planning/phases/{N}-{slug}/PHASE-SUMMARY.md` (Phase 7) or `{N}-SUMMARY.md` |
| Understand a v1.0 design decision | `.planning/phases/{N}-{slug}/{N}-CONTEXT.md` `<decisions>` section |
| Understand the dual-path mouse handling | `src/events.rs:73-200` `handle_mouse` + `src/app.rs:554-625` (delegate helpers) |
| Understand selection rendering | `src/ui/terminal.rs:155-225` (overlay highlight pass) |
| Understand PTY-input fluidity | `src/app.rs` (dirty-flag + biased select) + `03-CONTEXT.md` |
| Understand background-work decoupling | `src/app.rs` (`refresh_diff_spawn` + mpsc drain branch) + `05-CONTEXT.md` |
| Reproduce a selection bug | `MARTINS_MOUSE_DEBUG=1 ./target/release/martins 2>/tmp/log` |
| Run release pipeline | `release-martins` skill (Cargo bump → Formula → CHANGELOG → GitHub release → Homebrew tap → brew upgrade test) |

### Constraints to honor going forward

- **Tech stack frozen:** ratatui + crossterm + tokio + portable-pty + tmux. Perf comes from how things are wired, not from replacing dependencies.
- **macOS-only.** tmux + pbcopy dependencies are macOS-specific by design. Do NOT add cross-platform conditionals.
- **`~/.martins/state.json` v2 schema must continue to load.** No silent migrations that break current users.
- **Subjective feel parity, not numeric SLA.** New phases should target qualitative "feels like Ghostty" UAT, not ms thresholds.
- **Refactor discipline:** structural changes to event loop / render / state are welcome **only** in service of an active perf or feature goal — no gratuitous restructuring.

---

## Stats

- **Timeline:** 2026-04-23 → 2026-04-27 (4 days, single contributor)
- **Phases:** 7/7 complete
- **Plans:** 22/22 complete
- **Git commits:** 150 (range: `258343d` → `046bf9c`, tag `v1.0`)
- **Source code:** 60 `.rs` files, 10,912 LOC total (+5,044 / −1,700 vs. milestone start)
- **Tests:** 0 → 145 (greenfield perf milestone — no existing test surface for fluidity work)
- **Contributor:** Lucas Bayma

---

*Generated 2026-04-27 by `/gsd-milestone-summary 1.0` from the v1.0 archive at `.planning/milestones/`.*
