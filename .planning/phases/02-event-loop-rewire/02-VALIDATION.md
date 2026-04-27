---
phase: 2
slug: event-loop-rewire
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-24
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (built-in) + existing `insta`, `assert_cmd`, `predicates`, `tempfile` |
| **Config file** | None — standard cargo layout |
| **Quick run command** | `cargo test --lib app::tests 2>&1 \| tail -40` |
| **Full suite command** | `cargo test` |
| **Estimated runtime** | ~15–30 seconds (current baseline: 97 tests) |

---

## Sampling Rate

- **After every task commit:** `cargo check && cargo clippy --all-targets -- -D warnings`
- **After every plan wave:** `cargo test` (full suite)
- **Before `/gsd-verify-work`:** Full suite green + manual idle-CPU + heavy-PTY feel tests
- **Max feedback latency:** ~30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 02-01-* | 01 | 1 | ARCH-02 | — | `App::new` sets dirty=true; first frame renders | unit | `cargo test app::tests::app_starts_dirty` | ❌ Wave 0 | ⬜ pending |
| 02-01-* | 01 | 1 | ARCH-02 | — | `mark_dirty()` flips dirty to true | unit | `cargo test app::tests::mark_dirty_sets_flag` | ❌ Wave 0 | ⬜ pending |
| 02-01-* | 01 | 1 | ARCH-02 | — | Dirty stays false when no mutation occurs | unit | `cargo test app::tests::dirty_stays_clear_when_no_mutation` | ❌ Wave 0 | ⬜ pending |
| 02-01-* | 01 | 1 | ARCH-02 | — | `terminal.draw()` gated behind `if self.dirty` in `App::run` | grep-review | `rg 'if .*dirty' src/app.rs` shows gate at draw call site | ✅ src/app.rs | ⬜ pending |
| 02-02-* | 02 | 2 | ARCH-03 | — | `tokio::select!` opens with `biased;` | grep-review | `rg 'biased' src/app.rs` returns a hit inside `run` | ✅ src/app.rs | ⬜ pending |
| 02-02-* | 02 | 2 | ARCH-03 | — | Input (crossterm `events.next()`) is the first `select!` branch | grep-review | Manual inspection of `src/app.rs::run` confirms first branch post-`biased;` | ✅ src/app.rs | ⬜ pending |
| 02-02-* | 02 | 2 | ARCH-02 | — | Every `select!` arm marks dirty on a mutation | grep-review | `rg 'self.dirty = true\|mark_dirty' src/app.rs` ≥ 5 hits | ✅ src/app.rs | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*
*Specific task IDs will be populated once `/gsd-plan-phase` writes `02-01-PLAN.md` and `02-02-PLAN.md`.*

---

## Wave 0 Requirements

- [ ] `src/app_tests.rs` — extend existing file (85 lines from Phase 1) with three new tests:
  - `app_starts_dirty` — assert `App::new(...)` produces `dirty == true`
  - `mark_dirty_sets_flag` — construct App, set `dirty = false`, call `mark_dirty()`, assert `dirty == true`
  - `dirty_stays_clear_when_no_mutation` — construct App, set `dirty = false`, do nothing, assert `dirty == false`
- [ ] No new shared fixtures — `App::new` test constructibility already proven by Phase 1 tests
- [ ] No framework install — `cargo test` + dev-deps already configured

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Idle CPU drops to near-zero | ARCH-02 (success criterion 1) | ROADMAP decision: subjective feel test, no ms metric. No pty-harness exists to automate CPU sampling. | Build release binary: `cargo build --release`. Run `./target/release/martins` in a clean repo. Leave idle for 30s. Run `top -pid <pid>` (macOS) — CPU% should be < 1%. Fans should audibly quiet on a laptop. |
| Keyboard input remains responsive under heavy PTY output | ARCH-03 (success criterion 3) | No pty-harness for automated latency measurement; success is subjective feel against Ghostty baseline. | In one workspace tab, run `cat /usr/share/dict/words` or `yes`. While output streams, try: (a) `Ctrl-b` to exit terminal mode, (b) arrow keys in normal mode, (c) clicking the sidebar. Each keystroke/click must register within one frame — compare subjectively against Ghostty on the same machine. |
| Working-dot animation still advances | ARCH-02 (trade-off from dropping 1s tick to 5s heartbeat) | UX check — dot transition from "working"→"idle" now has up to 5s lag after `status_tick` raised. | Start a long-running command (e.g., `sleep 10`) in a tab. Observe sidebar working indicator appears within ~5s. After the command exits, observe dot clears within ~5s. |
| Cursor behavior during idle | ARCH-02 (Assumption A1 from RESEARCH) | `tui-term` draws cursor as part of frame — dirty-gated draw means cursor stops blinking during idle. User acceptance required. | Leave app idle in terminal mode for 30s. Confirm cursor is visibly "on" (solid, not blinking). If unacceptable, flag as follow-up — mitigation is a 500ms blink tick. |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies resolved
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references (3 new unit tests in `src/app_tests.rs`)
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter after planner populates task IDs
- [ ] Manual-only verifications documented with concrete steps (idle CPU + heavy-PTY feel test + working-dot + cursor)

**Approval:** pending
