---
phase: 03-pty-input-fluidity
verified: 2026-04-24T00:00:00Z
status: passed
score: 10/10 must-haves verified
overrides_applied: 2
overrides:
  - must_have: "Test module registration lives in src/lib.rs"
    reason: "Martins is a binary-only crate — no src/lib.rs exists. Plan 03-01 SUMMARY documents this deviation; module is registered at src/main.rs:21. Functionally equivalent: cargo test picks up the module via the bin crate's cfg(test) sub-tree."
    accepted_by: "plan-03-01 SUMMARY (auto-fix, Rule 3)"
    accepted_at: "2026-04-24T00:00:00Z"
  - must_have: "cargo test --lib pty_input passes"
    reason: "Binary-only crate has no --lib target. Equivalent command `cargo test pty_input` runs all three tests and returns 3/3 green."
    accepted_by: "plan-03-01 SUMMARY (auto-fix, Rule 3)"
    accepted_at: "2026-04-24T00:00:00Z"
---

# Phase 3: PTY Input Fluidity Verification Report

**Phase Goal:** Deliver the subjective feel-test success criterion for PTY input fluidity — keystrokes under heavy output feel Ghostty-equivalent, idle CPU is minimal, no warmup lag after idle. Phase 2 landed the structural primitives; Phase 3 validates them with regression tests + manual UAT.
**Verified:** 2026-04-24
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### ROADMAP Success Criteria (Phase 3)

| # | Truth (from ROADMAP) | Status | Evidence |
|---|----------------------|--------|----------|
| 1 | Typing a burst of characters into the PTY pane renders each one with no perceptible delay — feels indistinguishable from Ghostty | ✓ VERIFIED | Manual UAT `approved` reply (PTY-01 feel-test) + `keystroke_writes_to_pty` and `typing_appears_in_buffer` regression tests GREEN |
| 2 | While an agent is streaming verbose output, the user can still type into the input line and see characters appear immediately | ✓ VERIFIED | Manual UAT `approved` reply (PTY-02 heavy-output feel-test) + `biased_select_input_wins_over_notify` unit test GREEN + grep `biased;=1`, `// 1. INPUT=1` in src/app.rs |
| 3 | Idle the app for 30 seconds, then press a key — the first keystroke renders with no warmup lag | ✓ VERIFIED | Manual UAT `approved` reply (PTY-03 30s-idle-then-keystroke) + dirty-gate preserved (`if self.dirty=1`, `status_tick=0`, no always-on ticks) |
| 4 | `top` / Activity Monitor shows CPU at near-zero when the app is idle with no PTY output | ✓ VERIFIED | Manual UAT `approved` reply (PTY-03 idle CPU < 1%) + dirty-flag gate intact + heartbeat_tick only 5s (not 1s) |

### Observable Truths (from PLAN 03-01 frontmatter)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `cargo test --lib keystroke_writes_to_pty` passes — keystroke bytes reach PTY writer synchronously | ✓ VERIFIED | `cargo test pty_input` shows `keystroke_writes_to_pty ... ok` (override: --lib N/A on binary crate) |
| 2 | `cargo test --lib typing_appears_in_buffer` passes — typed char round-trips through /bin/cat and into ratatui TestBackend buffer | ✓ VERIFIED | `cargo test pty_input` shows `typing_appears_in_buffer ... ok` |
| 3 | `cargo test --lib biased_select_input_wins_over_notify` passes — proves tokio `biased;` + event-branch-first picks the event branch | ✓ VERIFIED | `cargo test pty_input` shows `biased_select_input_wins_over_notify ... ok` |
| 4 | `PtySession::write_input` bears a doc-comment affirming the synchronous-write guarantee | ✓ VERIFIED | src/pty/session.rs:134-150 contains "synchronous by design", "PTY-01, PTY-02", "Do NOT move this onto a `tokio::task::spawn`" |
| 5 | User types in a PTY tab during heavy output and keystrokes feel indistinguishable from Ghostty (manual UAT) | ✓ VERIFIED | User replied `approved` in Plan 03-01 Task 3 UAT |
| 6 | Idle for 30s then a keystroke renders with no warmup lag (manual UAT) | ✓ VERIFIED | User replied `approved` in Plan 03-01 Task 3 UAT |

**Score:** 10/10 truths verified (4 ROADMAP SC + 6 PLAN truths, no overlap)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/pty_input_tests.rs` | Three automated PTY-input fluidity tests | ✓ VERIFIED | File exists; contains `keystroke_writes_to_pty`, `typing_appears_in_buffer`, `biased_select_input_wins_over_notify`; uses only `/bin/cat` via `PtySession::spawn*` (T-03-01 mitigation satisfied); no `CommandBuilder` or `spawn_command` references |
| `src/main.rs` (override for `src/lib.rs`) | Test module registration (`#[cfg(test)]`) | ✓ VERIFIED | src/main.rs:20-21 contains `#[cfg(test)] mod pty_input_tests;` — deviation documented in SUMMARY (binary-only crate) |
| `src/pty/session.rs` | Synchronous write_input with doc-comment guarantee | ✓ VERIFIED | Lines 134-150 carry full doc-comment with "synchronous by design (PTY-01, PTY-02)", "Do NOT move this onto a `tokio::task::spawn`", reference to RESEARCH.md §Common Pitfalls #2 |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| `src/pty_input_tests.rs::keystroke_writes_to_pty` | `src/pty/session.rs::write_input` | direct `session.write_input` call on `/bin/cat`-backed PTY + parser readback | ✓ WIRED | Confirmed in file: `PtySession::spawn(..., "/bin/cat", ...)` → `session.write_input(b"a\n")` → polls parser for `'a'` |
| `src/pty_input_tests.rs::typing_appears_in_buffer` | `ratatui::backend::TestBackend + tui_term::widget::PseudoTerminal` | terminal.draw after parser poll | ✓ WIRED | Test constructs `TestBackend::new(80, 24)`, renders `PseudoTerminal::new(screen_guard.screen())`, asserts `cell.symbol() == "x"` |
| `src/pty_input_tests.rs::biased_select_input_wins_over_notify` | `tokio::select! { biased; ... }` | pre-signaled Notify + pre-seeded mpsc receiver | ✓ WIRED | Test creates `notify.notify_one()` + `tx.send("event")`, runs `select! { biased; Some(e)=rx.recv()=>e, _=notify.notified()=>"notify" }`, asserts chosen == `"event"` |

### Data-Flow Trace (Level 4)

N/A — Phase 3 delivers regression-guard tests and doc-comments. No dynamic-data-rendering artifacts (no dashboards, pages, or components fetching from APIs). The tests themselves exercise real PTY data-flow through `/bin/cat` → vt100 parser → ratatui TestBackend buffer, which IS the data-flow verification.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Three PTY-input tests pass | `cargo test pty_input` | `3 passed; 0 failed` in 0.01s | ✓ PASS |
| Full test suite green | `cargo test` | `103 passed; 0 failed; 0 ignored` in 2.52s | ✓ PASS |
| Phase 2 biased-select invariant preserved | `rg 'biased;' src/app.rs` | 1 match | ✓ PASS |
| Phase 2 input-first branch annotation preserved | `rg '// 1\. INPUT' src/app.rs` | 1 match | ✓ PASS |
| Phase 2 dirty-gate invariant preserved | `rg 'if self\.dirty' src/app.rs` | 1 match | ✓ PASS |
| Phase 2 status_tick removal preserved | `rg 'status_tick' src/app.rs` | 0 matches | ✓ PASS |
| mark_dirty() coupling preserved (≥5 required) | `rg 'self\.mark_dirty\(\)' src/app.rs` | 6 matches | ✓ PASS |
| 8ms output_notify throttle preserved | `rg 'duration_since' src/pty/session.rs` | 1 match (line 98, `>= 8 ms` throttle) | ✓ PASS |
| No tokio::task::spawn in keystroke path | `rg 'tokio::task::spawn' src/app.rs src/events.rs` | Only `spawn_blocking` at src/app.rs:393 (unrelated — git/state work, not `write_active_tab_input` / `forward_key_to_pty`) | ✓ PASS |
| write_input doc-comment landed | `rg 'synchronous by design' src/pty/session.rs` | 1 match at line 136 | ✓ PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| PTY-01 | 03-01-PLAN.md (skipped 03-02) | Typing renders each keystroke within one frame — no perceptible lag | ✓ SATISFIED | Manual UAT approved + `keystroke_writes_to_pty` + `typing_appears_in_buffer` tests GREEN; synchronous `write_input` doc-comment landed |
| PTY-02 | 03-01-PLAN.md (skipped 03-02) | Keystrokes during heavy PTY output are not delayed — input priority over background work | ✓ SATISFIED | Manual UAT approved (heavy `yes | head` streaming test passed) + `biased_select_input_wins_over_notify` test GREEN + Phase 2 `biased;` + `// 1. INPUT` invariants preserved |
| PTY-03 | 03-01-PLAN.md | Render loop only redraws when state changed (dirty-flag), idle CPU drops | ✓ SATISFIED | Manual UAT approved (idle CPU < 1% + 30s-idle-then-keystroke no warmup lag) + dirty-gate (`if self.dirty=1`) and status_tick removal (`=0`) preserved |

All three phase requirements (PTY-01, PTY-02, PTY-03) satisfied. No orphaned requirements — REQUIREMENTS.md maps PTY-01/02/03 to Phase 3 exclusively, all declared in Plan 03-01 frontmatter.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | No blockers, warnings, or info-level anti-patterns detected | ✓ | Code review (03-REVIEW.md) reported 0 Critical, 0 Warning, 3 Info findings — all informational only |

No TODO/FIXME/XXX/HACK, no `return null` stubs, no empty handler patterns. The doc-comment reference to `tokio::task::spawn` is intentional (Pitfall #2 defense, forbidding the refactor).

### Human Verification Required

None outstanding. Plan 03-01 Task 3 included two manual UAT items (keystroke-feel vs Ghostty under heavy output, 30s-idle-then-keystroke warmup test). The user replied `approved`, confirming PTY-01, PTY-02, and PTY-03 all pass against the Ghostty baseline.

#### Already-Verified Manual Items (for audit trail)

1. **User types in a PTY tab during heavy output (`yes | head -n 1000000`) and keystrokes feel indistinguishable from Ghostty** — Confirmed by `approved` reply, 2026-04-24.
2. **Idle for 30s then a keystroke renders with no warmup lag** — Confirmed by `approved` reply, 2026-04-24.
3. **Idle CPU < 1% over 30s window (`top -pid $(pgrep martins)`)** — Confirmed by `approved` reply, 2026-04-24.
4. **PTY-01 Ghostty-equivalent keystroke feel (rapid `abcdefghijklmnop` typing)** — Confirmed by `approved` reply, 2026-04-24.

### Gaps Summary

No gaps. Phase 3 delivers its stated goal:

- All three PTY-input regression-guard tests exist, are registered, and pass (`cargo test pty_input` → 3/3 green, full suite 103/103 green).
- `PtySession::write_input` carries the synchronous-write doc-comment explicitly forbidding a `tokio::task::spawn` refactor.
- All five Phase 2 structural invariants (biased select, input-first annotation, dirty-gate, status_tick removal, 8ms throttle) are preserved.
- Manual UAT signed off by user → PTY-01/02/03 all satisfied via Phase 2 primitives + Phase 3 validation.
- Plan 03-02 (frame-budget gate) correctly skipped per its conditional gate; 03-02-SUMMARY.md records `status: skipped` with rationale.

Code review (03-REVIEW.md) passed clean (0 Critical, 0 Warning, 3 Info). No deferred items to propagate forward.

---

*Verified: 2026-04-24*
*Verifier: Claude (gsd-verifier)*
