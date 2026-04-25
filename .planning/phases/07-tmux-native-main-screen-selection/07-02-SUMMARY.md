---
phase: 07-tmux-native-main-screen-selection
plan: 02
subsystem: tmux-integration
tags: [phase-7, tmux, conf, subprocess, pty-session, atomic-flags, copy-mode-vi]

# Dependency graph
requires:
  - phase: 06-text-selection
    provides: PtySession Arc<AtomicU64> field pattern (scroll_generation), App::copy_selection_to_clipboard pbcopy spawn pattern, src/tmux.rs subprocess helpers (send_key/pane_command)
provides:
  - "ensure_config emits 3 Phase 7 override bindings (y/Enter pbcopy + Escape cancel) on top of existing 5-line config"
  - "tmux::save_buffer_to_pbcopy(session) — piped subprocess: tmux save-buffer -> pbcopy stdin (cmd+c Tier 2)"
  - "tmux::cancel_copy_mode(session) — fire-and-forget tmux send-keys -X cancel for Esc/tab-switch fallback"
  - "PtySession.tmux_in_copy_mode: Arc<AtomicBool> — Martins-side copy-mode state flag"
  - "PtySession.tmux_drag_seen: Arc<AtomicBool> — transient drag-vs-click discriminator on Up(Left)"
affects: [07-03, 07-04, 07-05]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "tmux.conf override-only philosophy: bind only the 3 lines that diverge from tmux 3.6a defaults; never re-bind defaults (drift-prone)"
    - "Subprocess split: piped-stdout pattern (save_buffer_to_pbcopy mirrors pane_command + pbcopy spawn from app.rs:492-501) vs fire-and-forget (cancel_copy_mode mirrors send_key)"
    - "Arc<AtomicBool> on PtySession for cross-thread state flags; Arc-clone-init pattern matches scroll_generation precedent (Phase 6)"

key-files:
  created: []
  modified:
    - "src/tmux.rs (lines 32-44 ensure_config body extension; lines 159-211 new pub fns save_buffer_to_pbcopy + cancel_copy_mode; lines 250-280 inline TM-CONF-01 test)"
    - "src/pty/session.rs (lines 32-46 two new pub Arc<AtomicBool> fields; lines 93-94 spawn_with_notify Arc inits; lines 162-163 Self construction)"

key-decisions:
  - "Used the literal token-free comment '(mouse-drag-end already piped by tmux 3.6a default)' instead of the original 'MouseDragEnd1Pane' wording so the negative assertion in TM-CONF-01 stays maximally strict (cannot match anywhere in the config string, comments included)"
  - "Adopted the plan-prescribed override-only set (3 lines: y/Enter/Escape) — no MouseDragEnd1Pane / DoubleClick1Pane / TripleClick1Pane re-bindings, per RESEARCH §Tmux Defaults"
  - "Fields kept inert this plan; #![allow(dead_code)] on src/pty/session.rs:3 covers them until Plan 07-03 wires App-side accessors"
  - "Drain thread NOT modified — flags are set/cleared exclusively by handle_mouse (Plan 07-04) and handle_key Esc (Plan 07-05); per RESEARCH §State Source Option (a) Martins-side state machine"

patterns-established:
  - "tmux.conf override-only: comment + binding pair for each Phase-7 deviation from tmux 3.6a defaults; never add bindings that match upstream"
  - "Negative assertion safety: when a TDD assertion forbids a token, ensure the implementation does not contain that token anywhere — including comments"

requirements-completed: [SEL-01, SEL-02, SEL-03, SEL-04]

# Metrics
duration: 2min
completed: 2026-04-25
---

# Phase 7 Plan 02: tmux conf overrides + cmd+c/cancel helpers + PtySession atomic flags

**Three-line tmux.conf override (y/Enter pbcopy + Escape cancel), two new fire-and-forget/piped subprocess helpers (save_buffer_to_pbcopy, cancel_copy_mode), and two Arc<AtomicBool> flags on PtySession (tmux_in_copy_mode, tmux_drag_seen) following the scroll_generation precedent.**

## Performance

- **Duration:** 2 min
- **Started:** 2026-04-25T14:19:24Z
- **Completed:** 2026-04-25T14:22:00Z
- **Tasks:** 2
- **Files modified:** 2 (src/tmux.rs, src/pty/session.rs)

## Accomplishments

- `ensure_config()` now emits 8 lines (existing 5 + 3 Phase-7 overrides). Override comment-pair pattern gives future readers the "why" inline.
- `tmux::save_buffer_to_pbcopy(session: &str) -> bool` — piped subprocess pattern, returns false on (a) tmux save-buffer non-zero exit, (b) empty stdout, or (c) pbcopy spawn failure. Caller falls through to next cmd+c tier on false.
- `tmux::cancel_copy_mode(session: &str)` — fire-and-forget; idempotent against "not in a mode" exit-1 case (RESEARCH §Subprocess Behavior verified empirically).
- `PtySession.tmux_in_copy_mode: Arc<AtomicBool>` + `PtySession.tmux_drag_seen: Arc<AtomicBool>` — both initialized to `false` in `spawn_with_notify`; PTY drain thread does NOT touch them.
- TM-CONF-01 inline test green; full suite green at 138 tests (Phase 6 baseline 129 + 8 from Plan 07-01 + 1 new from this plan = 138, exactly matching plan's projection).

## Task Commits

Each task was committed atomically; Task 1 followed full TDD RED→GREEN cycle:

1. **Task 1 RED — TM-CONF-01 failing test** — `5666ad8` (test)
2. **Task 1 GREEN — ensure_config + save_buffer_to_pbcopy + cancel_copy_mode** — `ffc7d14` (feat)
3. **Task 2 — PtySession.tmux_in_copy_mode + tmux_drag_seen fields** — `ec2a651` (feat)

## Files Created/Modified

- `src/tmux.rs` — ensure_config body extended in place (lines 32-44); two new pub fns (`save_buffer_to_pbcopy` lines 159-194, `cancel_copy_mode` lines 200-211) inserted between `send_key` and `kill_session`; inline `#[cfg(test)] mod tests` extended with `ensure_config_writes_phase7_bindings` (TM-CONF-01).
- `src/pty/session.rs` — Two new `pub Arc<std::sync::atomic::AtomicBool>` fields appended to `PtySession` struct (lines 32-46); two `Arc::new(... AtomicBool::new(false))` inits added in `spawn_with_notify` after `scroll_gen_clone` (lines 93-94); `Ok(Self { ... })` construction extended with the two new fields (lines 162-163). No drain-thread changes.

## Decisions Made

- **Comment wording for the binding-block header:** The plan dictated the exact comment text, but the literal token `MouseDragEnd1Pane` in that comment caused TM-CONF-01's negative assertion to trip (the assertion checks `!conf.contains("MouseDragEnd1Pane")`). I rephrased the comment to avoid the literal token while preserving its intent. Treating this as a Rule 1 self-introduced bug per the deviation rules — fix applied inline, no functional impact, negative assertion now maximally strict.
- All other choices followed plan-prescribed exact bodies (RESEARCH-cited patterns, no improvisation).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] TM-CONF-01 negative assertion tripped by comment text**
- **Found during:** Task 1 GREEN (first cargo test run after implementing ensure_config extension)
- **Issue:** The plan-prescribed comment line `# Phase 7: pipe selection-via-keyboard to macOS pbcopy (defaults only pipe MouseDragEnd1Pane).` contained the literal substring `MouseDragEnd1Pane`. TM-CONF-01's negative assertion `!conf.contains("MouseDragEnd1Pane")` could not distinguish a comment mention from an actual `bind-key ... MouseDragEnd1Pane` re-binding. The intent of the negative assertion is "no rebinding of tmux 3.6a defaults"; treating it as also "the literal name should not appear at all" keeps the assertion maximally strict and self-documenting.
- **Fix:** Rephrased the comment to `(mouse-drag-end already piped by tmux 3.6a default)` — same meaning, no literal token. Preserves the comment's documentary purpose while keeping the negative assertion in its plain `contains` form.
- **Files modified:** src/tmux.rs (one comment line)
- **Verification:** `cargo test --bin martins tmux::tests` reports all 5 tests passing including TM-CONF-01.
- **Committed in:** `ffc7d14` (Task 1 GREEN commit — fix landed alongside the implementation)

---

**Total deviations:** 1 auto-fixed (1 Rule 1 bug)
**Impact on plan:** Single-line comment rewording, no behavioral or interface change. No scope creep.

## Issues Encountered

- Project is a binary-only crate (no `[lib]` target). Plan's verification command `cargo test --lib tmux::tests::ensure_config_writes_phase7_bindings` errored with `no library targets found in package 'martins'`. Used `cargo test --bin martins tmux::tests` instead — same test set, just routed through the binary's test profile per the existing convention noted in STATE.md (Phase 03-01 documented this same deviation: "binary-only crate deviation from plan which said src/lib.rs"). No code change; verification command adjusted in this report.
- Pre-existing `dead_code` warning on `pub(crate) fn encode_sgr_mouse` in `src/events.rs:32` — not in scope of this plan (introduced by sibling parallel agent). Out-of-scope per executor SCOPE BOUNDARY; not modified.

## User Setup Required

None - this plan only changes generated `~/.martins/tmux.conf` content; existing sessions will pick up the new bindings on next session creation. No env vars, no dashboard config.

## Next Phase Readiness

- Plan 07-03 (App helpers) can now reference: `crate::tmux::save_buffer_to_pbcopy`, `crate::tmux::cancel_copy_mode`, `PtySession.tmux_in_copy_mode`, `PtySession.tmux_drag_seen`. All four are `pub` and accessible via the active session lookup pattern (`active_sessions().get(active_tab)`).
- Plan 07-04 (handle_mouse delegation) can now set `tmux_in_copy_mode` on forwarded `Down(Left)` and toggle `tmux_drag_seen` on `Drag(Left)` / read+clear on `Up(Left)`.
- Plan 07-05 (handle_key Esc) can now read `tmux_in_copy_mode` to decide whether to forward `\x1b` byte (when true) vs let the existing Phase 6 selection-clear path run (when false).
- No blockers; the plan landed exactly the foundation Plan 07-03 needs.

## Self-Check: PASSED

- src/tmux.rs save_buffer_to_pbcopy at line 169: FOUND
- src/tmux.rs cancel_copy_mode at line 202: FOUND
- src/tmux.rs `bind-key -T copy-mode-vi Escape send-keys -X cancel` at line 43: FOUND
- src/pty/session.rs `pub tmux_in_copy_mode: Arc` at line 40: FOUND
- src/pty/session.rs `pub tmux_drag_seen: Arc` at line 45: FOUND
- Commit 5666ad8 (RED test): FOUND in git log
- Commit ffc7d14 (Task 1 GREEN): FOUND in git log
- Commit ec2a651 (Task 2): FOUND in git log
- `cargo build --bin martins`: clean (1 pre-existing warning in events.rs out-of-scope)
- `cargo test --bin martins`: 138 passed, 0 failed (matches plan's projected count)

---
*Phase: 07-tmux-native-main-screen-selection*
*Completed: 2026-04-25*
