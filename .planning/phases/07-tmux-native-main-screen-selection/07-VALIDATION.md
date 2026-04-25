---
phase: 7
slug: tmux-native-main-screen-selection
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-25
---

# Phase 7 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution. Concrete tasks/IDs filled in by the planner.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in (`cargo test`, inline `#[cfg(test)]` modules) |
| **Config file** | `Cargo.toml` (no extra harness — precedent: `src/pty_input_tests.rs`, Phase 6 selection tests in `src/events.rs` / `src/app.rs`) |
| **Quick run command** | `cargo test --lib -- --nocapture` |
| **Full suite command** | `cargo test --all-targets` |
| **Estimated runtime** | ~30s clean, ~5s incremental |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib <module>::tests` for the touched module (incremental, < 5s).
- **After every plan wave:** Run `cargo test --all-targets` (full).
- **Before `/gsd-verify-work`:** Full suite must be green AND manual UAT executed (dual-path).
- **Max feedback latency:** 30 seconds.

---

## Per-Task Verification Map

> Filled in by planner. Minimum gates the planner MUST emit:

| Gate | Where | Test Type | Coverage |
|------|-------|-----------|----------|
| SGR encoder helper | `src/events.rs` (or `src/sgr.rs` if extracted) | unit | Down(Left)/Drag(Left)/Up(Left) → exact `\x1b[<0;c;rM` / `\x1b[<32;c;rM` / `\x1b[<0;c;rm` bytes; shift adds +4; alt adds +8 |
| Mouse-mode read via vt100 | `src/pty/session.rs` test | unit | Calling `screen.mouse_protocol_mode()` after parser is fed `\x1b[?1006h` returns the expected variant; reset flips it back |
| Conditional intercept dispatch | `src/events.rs` | unit | Given `mouse_requested=false`, Drag(Left) returns "forward bytes" path; given `true`, returns "overlay" path. No tmux subprocess invoked. |
| cmd+c precedence (Tier 1→2→3) | `src/app.rs` / `src/events.rs` | unit + integration | Overlay-selection-present → copies overlay; tmux-buffer-present → copies via `tmux save-buffer -`; neither → SIGINT (`0x03`) forwarded to PTY |
| Esc / click-outside clearing | `src/events.rs` | unit | Esc with overlay active → clears `App::selection`; Esc with tmux-path active → forwards `\x1b` byte (or invokes cancel per researcher recommendation) |
| Tab/workspace switch cancel | switch handler test | unit | Outgoing-session cancel call is fire-and-forget (exit code ignored); no panic when session not in copy-mode |
| tmux.conf generation | `src/tmux.rs::ensure_config` test | unit | Generated config contains the 3 override lines (Esc cancel, y copy-pipe, Enter copy-pipe) and is idempotent on regeneration |

*Status legend: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

No new framework needed (Rust built-in). Wave 0 is empty.

If planner extracts an `sgr` module, Wave 0 may add `src/sgr.rs` skeleton with public encode helper + test stub. Otherwise: **"Existing infrastructure covers all phase requirements."**

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Native-feel parity | Phase 7 goal | Qualitative — operator-flagged "feels non-native" in Phase 6 UAT, no automated metric | Side-by-side compare Martins PTY-pane drag-select vs Ghostty-direct-tmux drag-select. Selection visual feedback, drag latency, auto-copy-on-release, double/triple-click word/line snapping must feel identical. |
| Phase 6 SEL-01..SEL-04 still hold (overlay path) | Carried forward | Mouse-app sessions (vim `mouse=a`, htop, btop) still need the Phase 6 overlay; verify it didn't regress | Run `vim` or `htop` in pane → drag-select → verify XOR overlay renders, cmd+c copies (Phase 6 baseline) |
| Phase 6 SEL-01..SEL-04 still hold (tmux path) | Carried forward | Same SEL-01..SEL-04 acceptance criteria but now via tmux-native | In bash (no mouse-app) → drag-select → tmux highlight renders → release → clipboard contains selected text (no cmd+c needed); cmd+c re-copy works |
| Tab/workspace switch cancels selection | D-16 | Multi-tab interaction state | Start tmux selection in tab A → switch to tab B → switch back to tab A → tmux is no longer in copy-mode (highlight gone) |
| Esc precedence (D-14) | Carried | Both paths | Tmux-path active selection: Esc clears it without forwarding to inner shell. No selection: Esc forwards to inner shell normally. |
| Click-outside (D-15) | Carried | Both paths | Tmux selection active → click elsewhere → tmux exits copy-mode |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references (currently: none required)
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
