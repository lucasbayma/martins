---
phase: 07-tmux-native-main-screen-selection
status: completed
completed: 2026-04-25
goal: "PTY-pane selection in non-mouse-app sessions feels indistinguishable from running tmux directly in Ghostty; mouse-app sessions retain Phase 6 REVERSED-XOR overlay"
requirements: [SEL-01, SEL-02, SEL-03, SEL-04]
plans_executed: 6
tests_before: 130
tests_after: 145
files_modified: 7
operator_signoff: "approved (2026-04-25)"
---

# Phase 7: tmux-native main-screen selection — Phase Summary

**One-liner:** Migrated PTY-pane drag-select from Martins' Phase 6 REVERSED-XOR overlay to the underlying tmux session's native copy-mode for non-mouse-app sessions, retained the overlay path end-to-end for mouse-app sessions (vim mouse=a, htop, btop) — yielding dual-path selection that feels indistinguishable from running tmux directly in Ghostty.

## Phase Goal Recap

> Migrate main-pane text selection from Martins' REVERSED-XOR overlay to the underlying tmux session's native copy-mode, so selection in the PTY pane feels indistinguishable from running tmux directly. Operator-flagged during Phase 6 UAT 2026-04-25 — current overlay works but feels non-native vs tmux's own selection.
>
> — `.planning/ROADMAP.md` §Phase 7

The Phase 6 overlay (built end-to-end across 6 plans) shipped SEL-01..04 with REVERSED-XOR rendering. Operator UAT 2026-04-25 reported the overlay was functionally correct but "felt non-native" vs running `tmux` directly in Ghostty. Phase 7 reroutes selection to tmux's own copy-mode in the dominant case (non-mouse-app sessions like bash/zsh) while preserving the overlay as fallback for sessions where the inner program has claimed mouse mode.

## Acceptance Criteria Status

SEL-01..SEL-04 are all carried forward from Phase 6 — Phase 7 adds **dual-path validation**: each criterion must hold in BOTH the new tmux-native delegate path AND the retained Phase 6 overlay path.

| Req | Criterion | Phase 6 Path (overlay) | Phase 7 Path (tmux native) | Validated By |
|-----|-----------|------------------------|----------------------------|--------------|
| SEL-01 | Drag-select with visible highlight that tracks the cursor with no lag | PASS — UAT-6 SEL-01 (Phase 6 regression sweep) + UAT-7-G/H (vim/htop) | PASS — UAT-7-A (bash drag-select; tmux's own reverse-video; feel matches Ghostty A) | 07-HUMAN-UAT.md (operator-signed 2026-04-25) |
| SEL-02 | `cmd+c` copies selection to macOS clipboard via pbcopy | PASS — UAT-6 SEL-02 + UAT-7-K(a) (overlay sel + cmd+c → overlay text) | PASS — UAT-7-A (auto-copy on mouse-up via `copy-pipe-and-cancel`) + UAT-7-F (cmd+c on tmux selection → Tier 2 `tmux save-buffer ┃ pbcopy`) + UAT-7-K(b) | 07-HUMAN-UAT.md |
| SEL-03 | Click-outside or Esc clears highlight in single frame / single press | PASS — UAT-6 SEL-03 + Phase 6 D-14 (Esc-precedence) holds | PASS — UAT-7-D (single-press Esc exits copy-mode via Plan 07-02 ensure_config override) + UAT-7-E (click-outside clears) | 07-HUMAN-UAT.md |
| SEL-04 | Highlight survives streaming PTY output (no flicker, jitter, disappear) | PASS — UAT-6 SEL-04 (Phase 6 scroll_generation anchoring) | PASS — PIT-7-6 (scroll-during-tmux-selection: coords stay anchored, no spurious scroll_generation bump) | 07-HUMAN-UAT.md |

**Subjective headline confirmation:** Operator confirmed YES on "feels indistinguishable from Ghostty+tmux direct" — the goal that drove the phase.

## Plans Executed

Phase 7 ran across 4 waves and 6 plans:

| Plan | Wave | One-line Summary | Commits |
|------|------|------------------|---------|
| 07-01 | 0 (encoder) | Pure `encode_sgr_mouse(MouseEventKind, KeyModifiers, u16, u16) -> Option<Vec<u8>>` free fn in `src/events.rs` + 8 TM-ENC-01..06 unit tests | `e707bfe`, `c6f13d7` |
| 07-02 | 1 (foundation) | tmux.conf 3-line override (`y`/`Enter` pbcopy + `Escape` cancel) + `save_buffer_to_pbcopy` (piped subprocess) + `cancel_copy_mode` (fire-and-forget) + `PtySession.tmux_in_copy_mode` and `tmux_drag_seen` `Arc<AtomicBool>` flags | `5666ad8`, `ffc7d14`, `ec2a651` |
| 07-03 | 1 (foundation) | 5 App helpers (`active_session_delegates_to_tmux`, `active_tmux_session_name`, `tmux_in_copy_mode`/`_set`, `tmux_drag_seen_set`/`_take`) + `set_active_tab` D-16 cancel-outgoing extension | `7a0d2ba` |
| 07-04 | 2 (dispatch) | `handle_mouse` conditional intercept gate (delegate→forward SGR; else→Phase 6 overlay) with Pitfall #1 modal/picker gate + Pitfall #2 stale-overlay clear; TM-DISPATCH-01..04 vt100 mode-flip integration tests | `4bfa55a`, `c254e59` |
| 07-05 | 3 (precedence) | `handle_key` cmd+c 3-tier precedence (Tier 1 overlay → Tier 2 tmux save-buffer → Tier 3 SIGINT) + Esc 3-tier precedence (Tier 1 overlay clear → Tier 2 forward 0x1b → Tier 3 fall-through); TM-CMDC-02 + TM-CANCEL-01 + TM-ESC-02 invariant tests | `397c0aa`, `f704035` |
| 07-06 | 4 (UAT) | Operator dual-path UAT (UAT-7-A..K + PIT-7-1/6 + Phase 6 regression sweep) — all PASS, subjective "feels indistinguishable" confirmation YES | `3398a86`, `1e3e585` |

## File Modification Surface Delta

`git diff --stat 4921dd4..HEAD -- src/`:

```
 src/app.rs                         |  88 +++++++++++
 src/events.rs                      | 151 ++++++++++++++++--
 src/main.rs                        |   3 +
 src/pty/session.rs                 |  20 +++
 src/selection_tests.rs             |  32 ++++
 src/tmux.rs                        |  84 +++++++++-
 src/tmux_native_selection_tests.rs | 309 +++++++++++++++++++++++++++++++++++++
 7 files changed, 676 insertions(+), 11 deletions(-)
```

| File | LOC delta | Role |
|------|-----------|------|
| `src/tmux_native_selection_tests.rs` | +309 (new module) | All Phase 7 unit tests: TM-ENC-01..06 (8), TM-CONF-01 (resides in tmux.rs inline, not here), TM-DISPATCH-01..04 (4), TM-CMDC-02 + TM-CANCEL-01 + TM-ESC-02 (3) — 15 tests total |
| `src/events.rs` | +151/-11 | `encode_sgr_mouse` free fn (Plan 07-01) + `handle_mouse` conditional intercept gate (Plan 07-04) + cmd+c & Esc 3-tier precedence chains (Plan 07-05) |
| `src/app.rs` | +88 | 5 helper fns (vt100-state delegate gate, session-name synthesis, atomic flag accessors) + `set_active_tab` D-16 extension (Plan 07-03) |
| `src/tmux.rs` | +84 | `ensure_config` extended with 3 Phase 7 override bindings + `save_buffer_to_pbcopy` (piped subprocess) + `cancel_copy_mode` (fire-and-forget) + TM-CONF-01 inline test (Plan 07-02) |
| `src/selection_tests.rs` | +32 | `flip_active_parser_to_mouse_mode` test helper + 4 call sites in Phase 6 fixtures so they continue exercising the overlay branch under Phase 7 dispatch (Plan 07-04 deviation, Rule 3) |
| `src/pty/session.rs` | +20 | 2 new `pub Arc<AtomicBool>` fields on `PtySession` (`tmux_in_copy_mode`, `tmux_drag_seen`) + Arc inits in `spawn_with_notify` + `Self` construction (Plan 07-02) |
| `src/main.rs` | +3 | `#[cfg(test)] mod tmux_native_selection_tests` registration (Plan 07-01) |

**Total: 676 insertions, 11 deletions across 7 files.**

The Phase 6 overlay primitives (`SelectionState`, `scroll_generation`, REVERSED-XOR render in `src/ui/terminal.rs:156-198`, double/triple-click counter, shift-click extend, `App::copy_selection_to_clipboard`) are **byte-for-byte preserved** and run end-to-end whenever `mouse_requested == true`. Zero deletion surface in `src/ui/terminal.rs`.

## Test Count Delta

| Stage | Test Count | Delta |
|-------|-----------:|------:|
| Pre-Phase 7 baseline (per STATE.md) | 129 | — |
| After Phase 6 review-fix tests landed (worktree base 4921dd4) | 130 | +1 |
| After Plan 07-01 (TM-ENC-01..06 + 2 negatives) | 138 | +8 |
| After Plan 07-02 (TM-CONF-01 in tmux.rs inline) | 138* | +0 (the +1 was already in baseline 130) |
| After Plan 07-04 (TM-DISPATCH-01..04) | 142 | +4 |
| After Plan 07-05 (TM-CMDC-02 + TM-CANCEL-01 + TM-ESC-02) | 145 | +3 |
| **Final post-Phase 7** | **145** | **+15 net** |

> \* TM-CONF-01 is the Plan 07-02 RED-gate test landed at commit `5666ad8` which was already inside the worktree base count of 130. Plan 07-02's GREEN commit makes it pass without adding a new test.

`cargo test --bin martins -- --test-threads=2` reports `145 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` (5.89s, captured at UAT start). Zero regressions.

## Decisions Adopted

Phase 7 implemented all 18 decisions from `07-CONTEXT.md` plus 1 RESEARCH-driven correction (D-09 narrowed):

| ID | Decision | Implementation |
|----|----------|----------------|
| D-01 | Tmux owns selection only when inner program has not claimed mouse mode | `App::active_session_delegates_to_tmux` reads vt100 `mouse_protocol_mode` + `alternate_screen` (Plan 07-03) |
| D-02 | Per-session mouse_requested flag fed by PTY drain mode-set/reset bytes | Substituted: vt100 parser tracks mouse_protocol_mode internally; App reads it via `try_read` (no separate flag needed — RESEARCH simplification) |
| D-03 | No subprocess query of tmux on demand for mouse_requested | Held — `active_session_delegates_to_tmux` is pure read against in-process vt100 state |
| D-04 | Forward raw SGR mouse bytes via `encode_sgr_mouse` | `pub(crate) fn encode_sgr_mouse` at `src/events.rs:32-67` (Plan 07-01) |
| D-05 | No `tmux send-keys -X` per mouse-move event | Held — only on cmd+c Tier 2 (off-thread `spawn_blocking`) and `set_active_tab` D-16 cancel |
| D-06 | Coords map 1:1 — Martins owns pane size | Held — `local_col`/`local_row` computed from `terminal_content_rect(app.last_panes...terminal)` minus pane origin |
| D-07/D-08 | Conditional intercept replaces Phase 6 D-10 always-intercept; core wiring change in `src/events.rs:46+` | `src/events.rs:73-141` — single-conditional dispatch gate inserted before existing Phase 6 match (Plan 07-04) |
| D-09 | tmux.conf bindings for `MouseDragEnd1Pane` + `y` + `Enter` | **Researcher narrowed to 3 explicit override lines** per RESEARCH §Tmux Defaults: only `y`/`Enter` pbcopy + `Escape` cancel are overrides; `MouseDragEnd1Pane` / `DoubleClick1Pane` / `TripleClick1Pane` are tmux 3.6a defaults that already do the right thing (Plan 07-02) |
| D-10 | cmd+c re-copies via tmux save-buffer\|pbcopy | `src/tmux.rs::save_buffer_to_pbcopy` (Plan 07-02) + `tokio::task::spawn_blocking` invocation in handle_key Tier 2 (Plan 07-05) |
| D-11 | cmd+c with no selection → Tier 3 SIGINT | Tier ordering preserved at `src/events.rs:481-518` — overlay sel → tmux delegate → SIGINT (Plan 07-05) |
| D-12/D-13 | Keep all Phase 6 overlay primitives; sleeps when delegating | Held — zero LOC deleted from `src/ui/terminal.rs`; Phase 6 path runs end-to-end whenever delegate gate returns false |
| D-14 | Esc-precedence: clear if selection active, else forward to tmux | `handle_key` Esc 3-tier at `src/events.rs:520-540` (Plan 07-05) |
| D-15 | Click-outside semantics same as D-14 | Pitfall #2 stale-overlay-clear at `src/events.rs:106-109` covers the cross-path transition (Plan 07-04) |
| D-16 | Tab/workspace switch cancels outgoing tmux copy-mode | `App::set_active_tab` extension at `src/app.rs:400-411` — `crate::tmux::cancel_copy_mode(outgoing_session_name)` BEFORE `self.active_tab = index` (Plan 07-03); UAT-7-J PASS |
| D-17 | tmux.conf bindings for `DoubleClick1Pane` + `TripleClick1Pane` | **Same narrowing as D-09** — tmux 3.6a defaults handle both natively; no override needed (Plan 07-02) |
| D-18 | Shift+click extend → SGR with shift modifier byte | Held — `encode_sgr_mouse` includes `KeyModifiers::SHIFT` → `+4` button-mask bit (Plan 07-01 TM-ENC-04 verifies) |

## Deviations from RESEARCH

Two deviations landed during execution, both auto-fixed (Rule 1) and documented in their plan SUMMARYs:

1. **DECSET 1006h vs 1000h is the actual mode-set** (Plan 07-04 SUMMARY)
   - **Issue:** Plan 07-04's PLAN.md and 07-RESEARCH.md `<delegate_flips_on_mouse_mode_set_reset>` test example claimed `parser.process(b"\x1b[?1006h")` would flip vt100's `mouse_protocol_mode` from `None` to non-None.
   - **Reality:** Verified empirically against vt100 0.16.2 — 1006h is purely the SGR encoding-format flag and does NOT toggle the tracking-mode enum. The actual mode-toggle sequences are 1000h (X10/PressRelease), 1002h (button-event), or 1003h (any-event).
   - **Fix:** Switched all 1006h DECSET feeds to 1000h in test fixtures (`src/selection_tests.rs::flip_active_parser_to_mouse_mode`, `tmux_native_selection_tests::drag_uses_overlay_when_inner_mouse_mode`, `delegate_flips_on_mouse_mode_set_reset`). Implementation behavior unchanged — production runtime correctness is unaffected because real terminal programs send 1000h to enter tracking + optionally 1006h for SGR encoding, and vt100 handles both correctly.

2. **TM-CONF-01 negative assertion vs plan-prescribed comment text** (Plan 07-02 SUMMARY)
   - **Issue:** The plan-prescribed comment `# Phase 7: pipe selection-via-keyboard to macOS pbcopy (defaults only pipe MouseDragEnd1Pane).` contained the literal substring `MouseDragEnd1Pane`. TM-CONF-01's negative assertion `!conf.contains("MouseDragEnd1Pane")` could not distinguish a comment mention from an actual rebinding.
   - **Fix:** Rephrased to `(mouse-drag-end already piped by tmux 3.6a default)` — same meaning, no literal token. Keeps the negative assertion maximally strict in plain `contains` form.

No architectural deviations (Rule 4) occurred — every deviation was inline-fixable, documented, and committed atomically with its task.

## Forward-Looking Notes

Copied verbatim from operator's UAT sign-off:

> None — Phase 7 closes the SEL-01..04 dual-path goal cleanly. Any future polish items (rectangle-select via Alt-drag, tmux 4.x compatibility audit) will be captured via /gsd-add-backlog.

## Operator UAT Timestamp + Sign-Off

- **UAT date:** 2026-04-25
- **Operator:** lucasobayma@gmail.com
- **Sign-off signal:** "approved" (resume signal in `/gsd-execute-phase 7` orchestration session)
- **All UAT-7-A..K:** PASS
- **PIT-7-1 + PIT-7-6:** PASS
- **Phase 6 regression sweep (UAT-6 SEL-01..04):** PASS
- **Subjective "feels indistinguishable from Ghostty+tmux direct":** YES

UAT log: `.planning/phases/07-tmux-native-main-screen-selection/07-HUMAN-UAT.md` (committed at `1e3e585`).

---

*Phase 7 closed: 2026-04-25.*
*Subsequent phase: TBD — Roadmap Phase 7 was the last open milestone in v1; Phase 5 plans (05-02..05-04) and Phase 4 (TBD plans) remain open from earlier waves.*
