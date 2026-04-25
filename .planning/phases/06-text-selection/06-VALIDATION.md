---
phase: 6
slug: text-selection
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-24
---

# Phase 6 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (Rust edition 2024) |
| **Config file** | `Cargo.toml` (test harness built-in) |
| **Quick run command** | `cargo test --lib selection -- --nocapture` |
| **Full suite command** | `cargo test --all -- --nocapture` |
| **Estimated runtime** | ~30 seconds (quick) / ~90 seconds (full) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib selection -- --nocapture`
- **After every plan wave:** Run `cargo test --all -- --nocapture`
- **Before `/gsd-verify-work`:** Full suite green + manual macOS UAT (see Manual-Only Verifications)
- **Max feedback latency:** 30 seconds for quick runs; 90 seconds for full

---

## Per-Task Verification Map

*Populated by planner — each task must map to an automated test or be listed under Manual-Only.*

---

## Wave 0 Requirements

- [ ] `src/selection_tests.rs` — unit tests for `SelectionState` anchored-coords translation, cmd+c dispatch, Esc precedence, word/line boundary, shift-click extension
- [ ] `src/pty_input_tests.rs` extensions — integration test: streaming PTY output while selection is active should not jitter/flicker (assert selection.start/end screen rows translate correctly across scroll_generation bumps)
- [ ] No new framework install required — existing `#[cfg(test)]` + `pty_input_tests.rs` pattern covers this phase

*Baseline: `src/pty_input_tests.rs` (Phase 3) + `src/navigation_tests.rs` (Phase 4) already provide harness pattern for driving `handle_mouse` / `handle_key`.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Drag highlight tracks cursor with no visible lag/tearing | SEL-01 | 60fps render feel cannot be asserted from unit tests | Run `cargo run --release`, drag across a multi-line agent transcript, visually confirm no tearing |
| cmd+c on macOS places selection on clipboard | SEL-02 | Clipboard integration is a system call, and kitty-keyboard-protocol delivery of `SUPER+c` must be confirmed against live Terminal.app / iTerm / Ghostty | In martins, select text, press cmd+c, open another terminal, run `pbpaste` — output must equal selection |
| Selection survives streaming PTY output | SEL-04 | Visual stability under 60fps render is perceptual | Start an agent stream, drag-select before stream finishes, confirm highlight stays put and text in highlight remains correct as rows scroll |
| Inverted-cell highlight matches Ghostty/iTerm feel | D-20, D-21 | Visual parity | Compare side-by-side against Ghostty at same prompt |

---

## Manual-Only UAT (06-04 deferred from automation)

| UAT-ID | Behavior | Procedure | Pass criteria |
|--------|----------|-----------|---------------|
| UAT-06-04-A | cmd+c with no selection in Terminal mode forwards SIGINT to active PTY | 1. Launch Martins; in active tab run `sleep 30`. 2. Confirm no selection is active (drag-select, then Esc to clear). 3. Press cmd+c. | `sleep` exits within 1s with the shell prompt re-displayed; no clipboard write occurred (verify by pasting into a notes app — clipboard contains whatever was there before). |
| UAT-06-04-B | Esc with no selection in Terminal mode forwards 0x1b to active PTY (preserves Phase 5 behavior) | 1. Launch Martins; in active tab run `vim` (or `nvim`). 2. Press `i` to enter insert mode. 3. Press Esc. | Vim returns to Normal mode (status bar at bottom changes from `-- INSERT --` to empty/`-- NORMAL --`). |

Rationale: automating these would require either a test-mode branch in `App::write_active_tab_input` (rejected per CLAUDE.md minimal-surface) or fragile subprocess echo semantics. Manual UAT covers them deterministically on the operator's macOS shell.

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
