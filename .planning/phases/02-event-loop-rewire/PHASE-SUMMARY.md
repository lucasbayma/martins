---
phase: 02
name: event-loop-rewire
status: complete
requirements_closed: [ARCH-02, ARCH-03]
plans: [02-01, 02-02]
started: 2026-04-24
completed: 2026-04-24
---

# Phase 2 — Event Loop Rewire

## Goal

Install the two structural perf primitives every interaction-latency
requirement depends on: a dirty-flag that gates `terminal.draw()`, and a
dedicated higher-priority input branch in the `tokio::select!` loop so PTY
output and timers cannot starve keyboard/mouse events.

## Requirements Closed

- **ARCH-02** — Dirty-flag rendering. `App.dirty: bool` + `mark_dirty()`
  helper; `terminal.draw()` gated behind `if self.dirty`; every select arm
  + `pending_workspace` branch marks dirty. Initial frame renders because
  `App::new` initializes `dirty: true`.
- **ARCH-03** — Input-priority `tokio::select!`. `biased;` as first
  statement, `events.next()` as first branch, annotated with explicit
  priority ordinals (`// 1. INPUT` through `// 5. Safety-net`) and an
  ARCH-03 block-header comment pointing a reader at the canonical
  location.

## ROADMAP Success Criteria

| # | Criterion | Status |
|---|-----------|--------|
| 1 | `terminal.draw()` not called when nothing changed — idle CPU visibly drops | ✓ Pass (baseline near-zero; 5s `refresh_tick` spikes are Phase 5 scope) |
| 2 | Explicit "dirty" signal that state mutations set and render consumes | ✓ Pass (`grep mark_dirty src/app.rs` → 6 hits) |
| 3 | Keyboard input accepted without delay under heavy PTY output | ✓ Pass (manual UAT confirmed) |
| 4 | Reader can point to the single place where input takes priority | ✓ Pass (`grep 'biased;' src/app.rs` + `// 1. INPUT` marker) |

## Plans

- **[02-01](02-01-SUMMARY.md)** — Dirty-flag rendering (ARCH-02).
  3 commits (RED / GREEN / docs). Added 3 unit tests; full suite 100/100.
- **[02-02](02-02-SUMMARY.md)** — Input-priority annotation (ARCH-03).
  1 commit. Pure annotation — no structural change.

## Combined Code Delta

- `src/app.rs`: +~60 lines (new field, helper, rewired `run`, annotations)
- `src/app_tests.rs`: +3 `#[tokio::test]` unit tests (dirty semantics)

## Test Gate

- `cargo build` — clean
- `cargo test` — 100/100 passed (97 prior + 3 new)
- `cargo clippy --all-targets -- -D warnings` — clean

## Decisions Confirmed

- **Q1: `status_tick` fate** — renamed `heartbeat_tick`, raised to 5s, kept as dirty-marker to advance sidebar working indicator.
- **Q2: Cursor blink during idle** — accepted. Solid cursor in terminal mode is the documented trade-off. 500ms blink tick is a future mitigation if UAT flips.
- **Q3: Per-handler return values** — skipped. Unconditional `mark_dirty()` per arm is simpler and sufficient.
- **Q4: `sync_pty_size` placement** — kept outside the `if self.dirty` block.

## Known Follow-ups (handed to later phases)

- **5s `refresh_tick` CPU spike (~9% every 5s)** — owned by **Phase 5**
  (Background Work Decoupling). ROADMAP already scopes the event-driven
  replacement (`notify`-debounced + 30s safety net).
- **`⚡` working indicator** — lights only when PTY has output within 2s.
  Behavior correct; ROADMAP manual-test wording could be refined (use
  `yes | head` or similar instead of `sleep`).

## Handoff to Phase 3 (PTY Input Fluidity)

The two structural primitives Phase 3 depends on are now in place:

- Dirty-flag rendering — Phase 3 can rely on `mark_dirty()` as the single
  signal that drives a redraw. No more blind per-iteration draws.
- Input-priority select — Phase 3's PTY-typing optimizations can assume
  keyboard events are always polled first on every loop iteration.

Phase 3's PTY-01/02/03 work can layer on top without touching the event
loop shape.
