---
phase: 03-pty-input-fluidity
plan: 01
subsystem: pty
tags: [pty, tokio, biased-select, ratatui, tui-term, vt100, validation-tests]

# Dependency graph
requires:
  - phase: 02-event-loop-rewire
    provides: dirty-flag gate on terminal.draw, biased; + input-first tokio::select!, synchronous PtySession::write_input, 8ms throttled output_notify
provides:
  - Three regression-guard tests locking in Phase 2 PTY-input primitives (keystroke→writer, round-trip through parser+TestBackend, biased-select priority proof)
  - Doc-comment on PtySession::write_input affirming the synchronous-write guarantee and forbidding the tokio::task::spawn refactor
  - User UAT sign-off closing PTY-01/PTY-02/PTY-03 at Plan 03-01
affects: [04-navigation-fluidity, 05-background-work-decoupling, 06-text-selection]

# Tech tracking
tech-stack:
  added: []  # All deps (ratatui TestBackend, tui_term, tokio::sync::Notify, vt100 parser) already present; no new crates.
  patterns:
    - "Validation-only plan pattern: Phase N lands primitives; Phase N+1 proves them with regression tests + UAT (no structural changes)"
    - "PTY validation tests use ONLY /bin/cat + /bin/echo string literals via PtySession::spawn* — no raw portable-pty CommandBuilder access, no user-controlled program/args (closes T-03-01)"
    - "biased-select priority proof pattern: pre-signaled Notify + pre-seeded mpsc, one select! iteration, assert chosen branch (reusable for future priority-inversion regression tests)"

key-files:
  created:
    - "src/pty_input_tests.rs — three validation tests: keystroke_writes_to_pty (PTY-01), typing_appears_in_buffer (PTY-01 round-trip), biased_select_input_wins_over_notify (PTY-02 priority proof)"
  modified:
    - "src/main.rs — `#[cfg(test)] mod pty_input_tests;` registration (deviation: plan said src/lib.rs, but Martins is a binary-only crate)"
    - "src/pty/session.rs — doc-comment on PtySession::write_input asserting synchronous-by-design guarantee, citing PTY-01/02, forbidding tokio::task::spawn refactor (Pitfall #2 defense)"

key-decisions:
  - "Test module registered in src/main.rs (binary crate) rather than src/lib.rs as plan directed — there is no lib target"
  - "PTY-01, PTY-02, PTY-03 all close at Plan 03-01 via user UAT sign-off; Plan 03-02 (frame-budget gate) is NOT executed and remains on disk as the considered-alternative"
  - "Phase 2 structural primitives (biased select, input-first branch, dirty-gate, 8ms output throttle, synchronous write_input) are sufficient — no frame-budget gate needed"

patterns-established:
  - "Phase 2→3 validation pattern: primitives phase (2) + validation phase (3) with conditional fallback plan on UAT fail — proven viable; reusable for Phase 4/5 if structural change carries UAT risk"
  - "Grep invariant snapshot as regression anchor: capture `biased;`, `// 1. INPUT`, `if self.dirty`, `status_tick=0`, `mark_dirty()>=5`, `duration_since>=1`, `write_input tokio::task::spawn=0` at plan close so future phases can re-run the same greps to detect regression"

requirements-completed: [PTY-01, PTY-02, PTY-03]

# Metrics
duration: ~15min (execution) + manual UAT
completed: 2026-04-24
---

# Phase 3 Plan 01: PTY Input Fluidity Validation Summary

**Three regression-guard tests + synchronous-write doc-comment close PTY-01/02/03 via user UAT — Phase 2 primitives proven sufficient, frame-budget gate (03-02) not needed.**

## Performance

- **Duration:** ~15 min automated execution + manual UAT
- **Started:** 2026-04-24 (Tasks 1 & 2 by prior agent, commits abe2140 and 14068fb)
- **Completed:** 2026-04-24 (Task 3 UAT approved)
- **Tasks:** 3 (2 automated + 1 manual UAT checkpoint)
- **Files modified:** 3 (1 created, 2 modified)

## Accomplishments

- Three PTY-input validation tests locked in under `cargo test pty_input` (3/3 green, full suite 103/103 green)
- `PtySession::write_input` bears the synchronous-by-design doc-comment — future refactors cannot silently move it onto `tokio::task::spawn` without tripping the acceptance criteria or code-review
- User UAT approved all four feel-tests (Ghostty-equivalent keystroke feel, input-under-heavy-output, idle CPU < 1%, no warmup stall after idle) → PTY-01, PTY-02, PTY-03 all close at Plan 03-01
- Plan 03-02 (frame-budget gate) remains on disk as the considered-alternative — documented evidence of the fallback path we did NOT need to take
- Phase 2 structural primitives (biased select, `// 1. INPUT` branch, dirty-gate, heartbeat_tick replacing status_tick, 8ms output_notify throttle, synchronous write_input) remain intact per grep invariant snapshot below

## Task Commits

1. **Task 1: Write three PTY-input validation tests** — `abe2140` (test)
2. **Task 2: Document synchronous-write guarantee + verify Phase 2 invariants preserved** — `14068fb` (docs)
3. **Task 3: Manual UAT — subjective feel test vs Ghostty** — user replied `approved` (no commit; manual checkpoint)

**Plan metadata:** pending (docs: complete plan — added alongside this SUMMARY)

## Files Created/Modified

- `src/pty_input_tests.rs` — **CREATED** — three tests (PTY-01, PTY-01 round-trip, PTY-02 priority proof); `#![cfg(test)]` module; uses only `/bin/cat` via `PtySession::spawn*` (T-03-01 mitigation)
- `src/main.rs` — **MODIFIED** — added `#[cfg(test)] mod pty_input_tests;` registration
- `src/pty/session.rs` — **MODIFIED** — doc-comment on `PtySession::write_input` (lines 134–143 region) citing "synchronous by design", "PTY-01, PTY-02", and "Do NOT move this onto a `tokio::task::spawn`"

## Decisions Made

- **Binary-only crate deviation:** Test module registered in `src/main.rs` rather than the plan-directed `src/lib.rs` — Martins has no library target. See Deviations section.
- **Phase-close at 03-01:** UAT approved → PTY-01/02/03 all closed here. Plan 03-02 (frame-budget gate + `should_draw` helper + `sleep_until` branch) is skipped and remains on disk as evidence of the considered fallback. If a future phase (e.g., Phase 4 navigation) re-surfaces a frame-pacing need, 03-02 is the reusable starting point.
- **Grep invariant snapshot captured** as a regression anchor for Phase 4+ — see section below.

## Grep Invariant Snapshot (regression anchor for Phase 4+)

Captured at plan close; any future phase should re-run and confirm these still hold:

| Invariant | Path | Expected | Observed |
|-----------|------|----------|----------|
| `biased;` (input-priority select) | `src/app.rs` | 1 | 1 |
| `// 1. INPUT` (branch-order annotation) | `src/app.rs` | 1 | 1 |
| `if self.dirty` (dirty-flag gate) | `src/app.rs` | 1 | 1 |
| `status_tick` (removed in Phase 2) | `src/app.rs` | 0 | 0 |
| `self.mark_dirty()` (state mutation coupling) | `src/app.rs` | ≥ 5 | 6 |
| `duration_since` (8ms output_notify throttle) | `src/pty/session.rs` | ≥ 1 | 1 |
| `tokio::task::spawn` near `write_active_tab_input` / `forward_key_to_pty` | `src/app.rs` + `src/events.rs` | 0 | 0 (only `spawn_blocking` calls exist, unrelated to keystroke write path) |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Test module registered in `src/main.rs` instead of `src/lib.rs`**
- **Found during:** Task 1 (writing `src/pty_input_tests.rs`)
- **Issue:** Plan directed registration in `src/lib.rs`, but Martins is a binary-only crate — `Cargo.toml` declares only `[package]` + `[[bin]]`-equivalent defaults with `src/main.rs` as the entrypoint. There is no `src/lib.rs` to register the module in.
- **Fix:** Added `#[cfg(test)] mod pty_input_tests;` to `src/main.rs` (line 21) — `cargo test` picks up integration tests via the bin crate's own `cfg(test)` sub-tree.
- **Files modified:** `src/main.rs`
- **Verification:** `cargo test pty_input` runs all three tests and passes (3/3 green).
- **Committed in:** `abe2140` (Task 1 commit — prior agent)

---

**Total deviations:** 1 auto-fixed (Rule 3 — blocking dep between plan directive and actual crate shape)
**Impact on plan:** No scope creep. All three tests register and run as intended; the only change is the file the `mod` line lives in. All other plan acceptance criteria met verbatim.

## Issues Encountered

None during execution. UAT returned `approved` on first run.

## Known Stubs

None — all primitives being validated already exist from Phase 2; no placeholder data or mock components introduced.

## Build/Test State at Close

- `cargo build` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test pty_input` — 3/3 green in ~0.01s
- `cargo test` — full suite 103/103 green (prior 100 + 3 new)

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- **Phase 3 closes at Plan 03-01.** PTY-01, PTY-02, PTY-03 all satisfied by Phase 2 primitives + Phase 3 validation + user UAT.
- **Plan 03-02 NOT executed.** Remains on disk as the conditional fallback (frame-budget gate + should_draw helper + sleep_until branch) if a future phase uncovers frame-pacing need.
- **Ready for Phase 4 (Navigation Fluidity).** Phase 2's biased-select + dirty-gate + synchronous keystroke path are the foundation Phase 4 will build navigation UX atop.
- **No blockers, no deferred items carried forward.**

## Self-Check: PASSED

Verified:
- [x] `src/pty_input_tests.rs` exists with three `fn` definitions (grep confirmed)
- [x] `src/main.rs` registers `mod pty_input_tests` (grep confirmed line 21)
- [x] `src/pty/session.rs` carries "synchronous by design" doc-comment (grep confirmed line 136)
- [x] Commits `abe2140` and `14068fb` exist in git log
- [x] `cargo test pty_input` returns 3/3 green
- [x] All six grep invariants match expected values

---
*Phase: 03-pty-input-fluidity*
*Completed: 2026-04-24*
