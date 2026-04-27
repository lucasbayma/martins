---
phase: 3
slug: pty-input-fluidity
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-24
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (built-in) + existing `insta`, `assert_cmd`, `predicates`, `tempfile`, `tokio-test` |
| **Config file** | None — standard cargo layout |
| **Quick run command** | `cargo test --lib pty_input 2>&1 \| tail -40` |
| **Full suite command** | `cargo test` |
| **Estimated runtime** | ~30 seconds (full suite after new tests land) |

---

## Sampling Rate

- **After every task commit:** Run `cargo check && cargo clippy --all-targets -- -D warnings` (~5s)
- **After every plan wave:** Run `cargo test` (full suite, ~30s)
- **Before `/gsd-verify-work`:** Full `cargo test` green + manual smoke tests
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 03-01-01 | 01 | 0 | PTY-01 | — | N/A | unit | `cargo test --lib keystroke_writes_to_pty` | ❌ W0 | ⬜ pending |
| 03-01-02 | 01 | 0 | PTY-01 | — | N/A | integration | `cargo test --lib typing_appears_in_buffer` | ❌ W0 | ⬜ pending |
| 03-01-03 | 01 | 0 | PTY-02 | — | N/A | unit | `cargo test --lib biased_select_input_wins_over_notify` | ❌ W0 | ⬜ pending |
| 03-01-04 | 01 | 1 | PTY-01 | — | Synchronous keystroke write preserved | code-review | `rg 'spawn' src/app.rs` shows 0 hits inside `write_active_tab_input` | ✅ src/app.rs | ⬜ pending |
| 03-01-05 | 01 | 1 | PTY-03 | — | Select loop remains `biased` with 5 branches | code-review | manual: inspect `tokio::select!` in `src/app.rs::run` | ✅ src/app.rs | ⬜ pending |
| 03-02-01 | 02 | 0 | PTY-02 | — | N/A (conditional) | unit | `cargo test --lib dirty_defers_draw_until_frame_budget` | ❌ W0 | ⬜ conditional |
| 03-02-02 | 02 | 1 | PTY-02 | — | Frame-budget gate active under heavy output | integration | manual UAT feel-test | n/a | ⬜ conditional |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

*Plan 03-02 tasks are conditional — executed only if Plan 03-01's manual UAT flags PTY-01 or PTY-02 as failing.*

---

## Wave 0 Requirements

- [ ] `src/pty_input_tests.rs` (new) — stubs for PTY-01/02/03 automated tests
- [ ] `keystroke_writes_to_pty` — construct App with PTY session backed by `/bin/cat`, simulate `KeyEvent` through `handle_event`, assert PTY writer received input
- [ ] `typing_appears_in_buffer` — same setup; after keystroke, wait for `output_notify` (bounded 200ms timeout), drive one `terminal.draw` with `TestBackend`, assert rendered buffer contains typed char
- [ ] `biased_select_input_wins_over_notify` — pure tokio test; pre-seeded `mpsc` + pre-signaled `Notify`; assert `biased;` select picks the event branch
- [ ] No framework install needed — `tokio::test`, `tempfile`, `TestBackend` all in `Cargo.toml`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Keystroke feels indistinguishable from Ghostty | PTY-01 | Sub-frame latency can't be measured without a camera rig; STATE.md decision: "subjective feel test, not a ms metric" | Open a PTY tab (`shell`). Type alphabet rapidly. Compare subjective feel against Ghostty on same machine. |
| Typing under heavy PTY output still feels immediate | PTY-02 | Subjective comparison | In a tab, run `yes \| head -n 1000000`. While output streams, try typing `Ctrl-B`, arrows, `?`. Each should register in one frame. |
| Idle CPU drops to near-zero | PTY-03 | CPU sampling is timing-dependent and noisy | `./target/release/martins` in clean repo. Idle 30s. `top -pid <pid>` on macOS. CPU% < 1%. |
| 30s-idle-then-keystroke has no warmup lag | PTY-03 | Measuring "warmup lag" requires sub-ms timing | Idle 30s, press any key in a terminal tab. Keystroke should render with no perceptible delay. |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
