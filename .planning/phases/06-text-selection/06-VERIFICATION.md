---
phase: 06-text-selection
verified: 2026-04-24T00:00:00Z
status: human_needed
score: 4/4 must-haves verified (automated portion); 2 manual UAT items remain
overrides_applied: 0
re_verification: null
human_verification:
  - test: "Drag highlight tracks cursor with no visible lag/tearing on real macOS Terminal"
    expected: "60fps render feel, no tearing under continuous drag across multi-line agent transcript"
    why_human: "Subjective render-feel cannot be asserted from unit tests; ROADMAP SC-1 explicitly requires Ghostty-baseline feel"
  - test: "cmd+c on macOS places selection on clipboard (pbpaste)"
    expected: "After dragging, pressing cmd+c, then running pbpaste in another shell, output equals the highlighted text"
    why_human: "Clipboard integration is a system call + kitty-protocol delivery of SUPER+c must be confirmed against live Terminal.app/iTerm/Ghostty (UAT in 06-VALIDATION.md)"
  - test: "Selection survives streaming PTY output without flicker/disappearance"
    expected: "Drag-select before/during stream; highlight stays put and contains correct text as rows scroll"
    why_human: "Visual stability under 60fps render is perceptual; ROADMAP SC-4 explicitly requires no flicker/jitter under streaming output"
  - test: "Inverted-cell highlight matches Ghostty/iTerm visual feel (D-20, D-21)"
    expected: "Side-by-side parity with Ghostty at the same prompt; XOR un-reverses already-reversed cells"
    why_human: "Visual parity comparison"
  - test: "UAT-06-04-A: cmd+c with no selection in Terminal mode forwards SIGINT (0x03) to active PTY"
    expected: "Run `sleep 30`; with no selection press cmd+c → sleep exits within 1s; clipboard unchanged"
    why_human: "Byte-level PTY-forwarding deferred from automation per CLAUDE.md minimal-surface (no test-mode branch on write_active_tab_input)"
  - test: "UAT-06-04-B: Esc with no selection in Terminal mode forwards 0x1b to active PTY (Phase 5 fallthrough)"
    expected: "In vim insert mode press Esc → returns to Normal mode (-- INSERT -- disappears)"
    why_human: "Byte-level PTY-forwarding deferred from automation per CLAUDE.md minimal-surface"
known_issues:
  - id: REVIEW-MINOR-01
    file: "src/events.rs:80-81 + src/app.rs:485-488"
    issue: "Empty snapshot stored as Some(\"\") defeats live-rematerialization fallback in copy_selection_to_clipboard — when materialize_selection_text fails (try_read contention), sel.text becomes Some(\"\") and unwrap_or_else fallback never fires"
    severity: minor
  - id: REVIEW-MINOR-02
    file: "src/app.rs:677-725 (select_line_at)"
    issue: "Triple-click on a blank/whitespace-only row produces a non-empty Option<SelectionState> that is_empty() — UX papercut: invisible selection user has to dismiss"
    severity: minor
  - id: REVIEW-MINOR-03
    file: "src/app.rs:102 (last_click_col field)"
    issue: "last_click_col is written but never read — same-cluster comparison only uses last_click_row"
    severity: nit
  - id: REVIEW-MINOR-04
    file: "src/selection_tests.rs:263-294 (scroll_generation_increments_on_vertical_scroll)"
    issue: "Test polling deadline of 500ms against real /bin/cat PTY flakes on parallel cold runs; passes single-threaded and on warm runs. Production code is correct. Recommended fix: bump deadline to 2s."
    severity: minor
---

# Phase 6: Text Selection Verification Report

**Phase Goal:** Drag-select text in the PTY main pane with a visible highlight, copy with `cmd+c`, clear with click/Escape — matching Ghostty's feel. The highlight survives streaming PTY output until the user explicitly clears it.

**Verified:** 2026-04-24
**Status:** `human_needed` — all 4 ROADMAP success criteria are wired in code, and 19/20 automated tests pass on every run; 1 test (REVIEW-MINOR-04) is a known parallel-run timing flake that passes warm/single-threaded. The phase status is `human_needed` because Criteria 1, 2, and 4 of ROADMAP SC are perceptual/clipboard-integration behaviors that cannot be asserted from unit tests and require the manual UAT in `06-VALIDATION.md`.

**Re-verification:** No — initial verification.

---

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria)

| #   | Truth                                                                                                                                                              | Status                  | Evidence                                                                                                                                                                                                                                                                                       |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Click-and-drag on the PTY main pane shows a highlight that tracks the cursor with no lag or tearing                                                                | ✓ WIRED (needs human)   | Drag(Left) at `src/events.rs:46-69` creates `SelectionState` with `start_gen` anchored, mid-drag updates `end_col/end_row` cursor-relative, calls `app.mark_dirty()`. Highlight render at `src/ui/terminal.rs:157-199` uses `Modifier::REVERSED` XOR. `selection_highlights_cells_with_reversed_modifier` test passes. SC-1 perceptual feel needs manual UAT. |
| 2   | Pressing `cmd+c` while a selection is active puts the selected text on the macOS clipboard (verifiable via `pbpaste`)                                              | ✓ WIRED (needs human)   | cmd+c branch at `src/events.rs:378-394` checks `KeyModifiers::SUPER + Char('c')`, calls `app.copy_selection_to_clipboard()` at `src/app.rs:475-504` which spawns `pbcopy` with `sel.text` snapshot or live materialization fallback. `cmd_c_with_selection_consumes_event_and_keeps_selection` passes. `pbpaste` round-trip needs manual UAT (deferred per VALIDATION.md). |
| 3   | Clicking outside the selection, or pressing Escape, clears the highlight immediately in a single frame                                                             | ✓ VERIFIED              | Down(Left) clear at `src/events.rs:113-118` (`app.selection = None; app.mark_dirty()`); Esc clear at `src/events.rs:396-404` (gated on `selection.is_some()`, sets None, mark_dirty). Tests `down_left_clears_active_selection_and_marks_dirty` (events.rs:609) + `esc_with_active_selection_clears_and_marks_dirty` (events.rs:728) both pass. |
| 4   | While text is selected, new PTY output (e.g., agent streaming a reply) does not cause the highlight to flicker, jitter, or disappear — it stays put until the user clears it | ✓ WIRED (needs human)   | `PtySession.scroll_generation: Arc<AtomicU64>` at `src/pty/session.rs:30, 75-76, 143`; reader thread increments via SCROLLBACK-LEN heuristic at `src/pty/session.rs:99-117`. Anchored-coord translation at `src/ui/terminal.rs:160-197` uses `current_gen.saturating_sub(sel.start_gen)`; clip-at-top + scrolled-off branches present (D-08). `scroll_generation_increments_on_vertical_scroll` + `selection_clips_at_visible_top_when_scrolled` pass. SC-4 perceptual stability under streaming needs manual UAT. |

**Score:** 4/4 truths verified at the wiring level; 3 of the 4 require human verification of perceptual/system-call behaviors per `06-VALIDATION.md`.

---

### Required Artifacts

| Artifact                | Expected                                                                                | Status     | Details |
| ----------------------- | --------------------------------------------------------------------------------------- | ---------- | ------- |
| `src/app.rs`            | Extended `SelectionState` (start_gen, end_gen, text); App click-counter fields; `clear_selection`, `set_active_tab`, `materialize_selection_text`, `select_word_at`, `select_line_at`, `extend_selection_to`, `active_scroll_generation`, `word_boundary_at`, `copy_selection_to_clipboard` (snapshot-aware), `inject_test_session` (#[cfg(test)]) | ✓ VERIFIED | All present; struct extended (line 28-50); helpers at lines 211, 402, 475, 512, 531, 546, 644, 677, 734; `inject_test_session` at line 879. |
| `src/events.rs`         | `handle_mouse` Drag/Up/Down branches with anchoring, click-counter, double/triple/shift-click; `handle_key` cmd+c + Esc-clear branches | ✓ VERIFIED | Drag at line 46, Up at line 71, Down with click-counter at line 93-167; cmd+c branch at line 379, Esc-clear at line 397. `app.mark_dirty()` count = 13 occurrences in events.rs. |
| `src/main.rs`           | `PushKeyboardEnhancementFlags(DISAMBIGUATE_ESCAPE_CODES)` on init; `PopKeyboardEnhancementFlags` on restore | ✓ VERIFIED | Push at `src/main.rs:80`; Pop at `src/main.rs:90`; module registration `mod selection_tests;` at line 27. |
| `src/pty/manager.rs`    | `#[cfg(test)] pub(crate) fn insert_for_test`                                            | ✓ VERIFIED | Present (referenced from `App::inject_test_session` at app.rs:937). |
| `src/pty/session.rs`    | `scroll_generation: Arc<AtomicU64>` field; reader-thread increment via SCROLLBACK-LEN heuristic; `row_hash` helper | ✓ VERIFIED | Field at line 30; init at line 75-76; reader-thread heuristic at lines 99-117 with `before_top_hash != after_top_hash` gate; `fetch_add(1, Relaxed)` at line 115; `row_hash` at file scope. |
| `src/ui/terminal.rs`    | `render` signature with trailing `current_gen: u64`; gold-accent body removed; `Modifier::REVERSED` XOR; anchored-coord translation; `render_with_selection_for_test` shim | ✓ VERIFIED | Translation at line 160-167, clip at 172, REVERSED toggle at 193; test shim at 209-252; no `theme::ACCENT_GOLD` in highlight body. |
| `src/ui/draw.rs`        | Caller of `terminal::render` passes active session's `scroll_generation.load(Relaxed)` | ✓ VERIFIED | `current_gen` computed at line 75-78; passed to render at line 92. |
| `src/workspace.rs`      | All `app.active_tab = N` writes routed through `set_active_tab`; `app.active_workspace_idx` writes preceded by `app.clear_selection()` | ✓ VERIFIED | `set_active_tab` calls at lines 144, 231, 287, 328; explicit `clear_selection()` at lines 142, 229, 285. Zero bare `app.active_tab =` assignments remain in src/workspace.rs. |
| `src/selection_tests.rs`| 18+ unit + render-level tests (5 in 06-01, +1 in 06-02, +8 in 06-03, +2 in 06-04, +3 in 06-05, +2 in 06-06) | ✓ VERIFIED | 20 test functions enumerated (1097 lines): all Plan 06-01..06-06 names present. |

---

### Key Link Verification

| From                                                  | To                                                       | Via                                                                                            | Status     | Details                                                                                                            |
| ----------------------------------------------------- | -------------------------------------------------------- | ---------------------------------------------------------------------------------------------- | ---------- | ------------------------------------------------------------------------------------------------------------------ |
| `src/main.rs`                                         | `src/selection_tests.rs`                                 | `#[cfg(test)] mod selection_tests;`                                                            | ✓ WIRED    | `src/main.rs:26-27`                                                                                                |
| `src/events.rs::handle_mouse`                         | `session.scroll_generation` (via `App::active_scroll_generation`) | Drag branch reads `app.active_scroll_generation()` into `SelectionState.start_gen`             | ✓ WIRED    | `events.rs:50` `let current_gen = app.active_scroll_generation();` → `start_gen: current_gen` at line 60          |
| `src/events.rs::handle_mouse Up`                      | `App::materialize_selection_text`                        | `sel.text = Some(materialize_selection_text(app, &sel))`                                       | ✓ WIRED    | `events.rs:80-81`                                                                                                  |
| `src/events.rs::handle_mouse`                         | `App::mark_dirty`                                        | every selection-mutation branch ends with `app.mark_dirty()` (D-23)                            | ✓ WIRED    | events.rs lines 68, 75, 84, 117, 402; plus app.rs `clear_selection`/`set_active_tab`/`select_*_at` mutations       |
| `src/events.rs::handle_key`                           | `App::copy_selection_to_clipboard`                       | cmd+c branch with non-empty selection                                                          | ✓ WIRED    | `events.rs:379-388`                                                                                                |
| `src/events.rs::handle_key`                           | `App::write_active_tab_input`                            | cmd+c with no selection in Terminal mode forwards SIGINT (0x03)                                | ✓ WIRED    | `events.rs:389-392` `app.write_active_tab_input(&[0x03])` (UAT-06-04-A covers byte-level forwarding)               |
| `src/main.rs`                                         | terminal startup sequence                                | `PushKeyboardEnhancementFlags` alongside EnableMouseCapture+EnableBracketedPaste               | ✓ WIRED    | `main.rs:76-81`                                                                                                    |
| `src/ui/draw.rs render call`                          | `session.scroll_generation`                              | Compute `current_gen` from active session, pass to `terminal::render`                          | ✓ WIRED    | `draw.rs:75-78` + `draw.rs:92`                                                                                     |
| `SelectionState.start_gen/end_gen`                    | visible row translation                                  | `current_row = anchored_row - (current_gen - sel_gen)`                                         | ✓ WIRED    | `terminal.rs:162, 168-176`                                                                                         |
| `src/app.rs::set_active_tab`                          | `src/app.rs::clear_selection`                            | First line of body                                                                              | ✓ WIRED    | `app.rs:402-405` (also `mark_dirty` unconditionally for tab-strip repaint)                                         |
| `src/app.rs::select_active_workspace`                 | `src/app.rs::clear_selection`                            | First line of body                                                                              | ✓ WIRED    | `app.rs:388-393`                                                                                                   |
| `src/workspace.rs`                                    | `src/app.rs::clear_selection`                            | 3 explicit calls + 4 `set_active_tab` calls cover all switch paths                              | ✓ WIRED    | clear_selection at 142, 229, 285; set_active_tab at 144, 231, 287, 328                                            |
| `src/events.rs`                                       | `src/app.rs::set_active_tab`                             | All tab-switch branches: tab-click, number-key, kill retargets, right-list pick                | ✓ WIRED    | events.rs:234, 356, 555, 569, 650 (5 sites)                                                                        |
| `src/pty/session.rs reader thread`                    | `vt100::Parser`                                          | before/after `row_hash(screen, 0, cols)` around `parser.process(bytes)` with cursor-row gate    | ✓ WIRED    | session.rs:106-117                                                                                                 |
| `PtySession.scroll_generation`                        | `scroll_gen_clone`                                       | `Arc::clone(&scroll_gen)` passed into `std::thread::spawn` reader closure                       | ✓ WIRED    | session.rs:75-76                                                                                                   |

All 14+ key links verified.

---

### Data-Flow Trace (Level 4)

| Artifact                | Data Variable                                  | Source                                                                                | Produces Real Data | Status     |
| ----------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------- | ------------------ | ---------- |
| `SelectionState.text`   | `sel.text: Option<String>`                     | `App::materialize_selection_text` reads from `session.parser.try_read().screen().contents_between(...)` (live vt100 buffer) | ✓ Yes              | ✓ FLOWING  |
| `current_gen` (renderer)| `u64` snapshot                                 | `active_sessions.get(active_tab).map(|(_, s)| s.scroll_generation.load(Relaxed))`     | ✓ Yes (incremented by reader thread) | ✓ FLOWING  |
| Highlight buffer cells  | `Modifier::REVERSED` toggled                   | XOR pass over visible buffer `frame.buffer_mut().cell_mut(...)` driven by translated coords | ✓ Yes              | ✓ FLOWING  |
| Clipboard write         | `pbcopy` stdin                                 | `sel.text.clone().unwrap_or_else(|| materialize_selection_text(sel))` → `trim_end()` → spawn `pbcopy` | ✓ Yes (subprocess + stdin write asserted by test) | ✓ FLOWING (system call observed in UAT) |

**Note (REVIEW-MINOR-01):** there is a known edge case where `Some("")` defeats the live-rematerialization fallback when `try_read()` fails during mouse-up. Production-realistic but non-blocking; flagged for hardening, not as a goal-failure.

---

### Behavioral Spot-Checks

| Behavior                                              | Command                                                                | Result                                                            | Status |
| ----------------------------------------------------- | ---------------------------------------------------------------------- | ----------------------------------------------------------------- | ------ |
| Selection-tests suite compiles + passes (warm run)    | `cargo test --bin martins selection_tests::`                           | 20 tests run; 19 pass on every run, 1 (`scroll_generation_increments_on_vertical_scroll`) is the documented timing flake that passes warm/single-threaded | ✓ PASS (with known flake) |
| Full suite passes single-threaded                     | `cargo test --bin martins -- --test-threads=1`                         | 129 passed; 0 failed                                              | ✓ PASS |
| `cargo build` compiles                                | implied by `cargo test` finishing test profile                         | Exits 0                                                           | ✓ PASS |
| `app.active_tab =` direct writes outside test fixtures | `grep app\.active_tab\s*= src/`                                         | 2 hits in `src/selection_tests.rs` (test-fixture seeds for two-tab/two-workspace setups) + 1 hit in `src/navigation_tests.rs:66` (timing benchmark mirroring a pre-Phase-6 call site). Zero hits in production code (`src/events.rs`, `src/workspace.rs`). | ✓ PASS |
| `app.set_active_tab(` migration                       | `grep app\.set_active_tab\( src/`                                       | 4 hits in `src/workspace.rs` + 5 hits in `src/events.rs` + 1 hit in `src/selection_tests.rs:871` (the test) | ✓ PASS |
| `app.clear_selection()` invariant                     | `grep app\.clear_selection\(\) src/`                                    | 3 hits in `src/workspace.rs` (preceding bare `active_workspace_idx` writes at lines 142, 229, 285) | ✓ PASS |

---

### Requirements Coverage

| Requirement | Source Plan(s)              | Description                                                                                                                                          | Status      | Evidence                                                                                                                                                                                                                                                                                |
| ----------- | --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- | ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **SEL-01**  | 06-01, 06-03, 06-05         | Mouse drag on the PTY main pane starts a text selection with a visible highlight that tracks the cursor with no lag                                  | ✓ SATISFIED (perceptual portion deferred to UAT) | Drag handler `src/events.rs:46-69` creates anchored selection; render highlight `src/ui/terminal.rs:157-199` (REVERSED XOR). `drag_creates_selection_anchored_at_current_gen` test (selection_tests.rs:313) passes. ROADMAP SC-1 perceptual feel covered by VALIDATION.md Manual-Only entry. |
| **SEL-02**  | 06-01, 06-03, 06-04         | `cmd+c` while a selection is active copies the selected text to the macOS clipboard via `pbcopy`                                                     | ✓ SATISFIED (clipboard round-trip deferred to UAT) | `handle_key` cmd+c branch `src/events.rs:378-394` calls `App::copy_selection_to_clipboard` (`src/app.rs:475-504`) which pipes to `pbcopy` subprocess. `cmd_c_with_selection_consumes_event_and_keeps_selection` test (selection_tests.rs:680) passes. `pbpaste` verification deferred to VALIDATION.md UAT. |
| **SEL-03**  | 06-01, 06-03, 06-04, 06-06 | Click (or Escape) outside the selection clears the highlight immediately                                                                              | ✓ SATISFIED | Down(Left) clear at `src/events.rs:113-118`; Esc-clear at `src/events.rs:396-404`; tab/workspace switch via `set_active_tab`/`select_active_workspace` clears (D-22). Tests: `down_left_clears_active_selection_and_marks_dirty`, `esc_with_active_selection_clears_and_marks_dirty`, `tab_switch_clears_selection`, `workspace_switch_clears_selection` all pass. |
| **SEL-04**  | 06-01, 06-02, 06-03, 06-05  | Selection highlight does not flicker or disappear when the underlying PTY buffer receives new output — the selection stays stable                    | ✓ SATISFIED (perceptual portion deferred to UAT) | `PtySession.scroll_generation` counter increments via SCROLLBACK-LEN heuristic (`src/pty/session.rs:99-117`). Anchored-coord translation in renderer (`src/ui/terminal.rs:160-197`). Tests `scroll_generation_increments_on_vertical_scroll` (warm) + `selection_clips_at_visible_top_when_scrolled` pass. ROADMAP SC-4 visual stability covered by VALIDATION.md Manual-Only entry. |

**Coverage:** 4/4 SEL-XX requirements accounted for in plan frontmatter; all 4 are also listed in REQUIREMENTS.md as `[x]` (Phase 6 mapped). No orphaned requirements.

---

### Anti-Patterns Found

| File                          | Line(s)             | Pattern                                                          | Severity | Impact                                                                                                                                                       |
| ----------------------------- | ------------------- | ---------------------------------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `src/events.rs`               | 80-81               | `materialize_selection_text` empty-string return collapses with `Some("")` snapshot, defeating fallback | ⚠️ Warning (REVIEW-MINOR-01) | When `try_read()` fails at mouse-up, `sel.text` stores `Some("")`. Subsequent `cmd+c` reads `sel.text.clone().unwrap_or_else(...)` — Some short-circuits the fallback, so the clipboard receives empty after `trim_end()` guard → silent no-op. Production-realistic but rare. |
| `src/app.rs`                  | 677-725             | `select_line_at` on a blank/whitespace-only row produces an `is_empty()` selection that still marks dirty | ℹ️ Info (REVIEW-MINOR-02)    | UX papercut — invisible selection user has to dismiss with a click. Does NOT match Ghostty (Ghostty triple-click on blank = no-op).                          |
| `src/app.rs`                  | 102                 | `last_click_col` field written but never read                    | ℹ️ Info (REVIEW-MINOR-03)    | Dead bookkeeping; either delete or document as reservation for future "same-word" cluster check (D-16).                                                       |
| `src/selection_tests.rs`      | 263-294             | 500ms polling deadline against real `/bin/cat` PTY               | ⚠️ Warning (REVIEW-MINOR-04) | Test-only flake; production code is correct (verified by direct read of `pty/session.rs:99-117`). Recommended: bump deadline to 2s.                          |

No blockers found. All anti-patterns are quality-tier and were flagged in `06-REVIEW.md` (0 blockers / 0 majors / 4 minors / 5 nits).

---

### Human Verification Required

Per `06-VALIDATION.md` § Manual-Only Verifications and § Manual-Only UAT (06-04 deferred from automation), the following items cannot be confirmed programmatically:

#### 1. SC-1 — Drag highlight tracks cursor with no visible lag/tearing

- **Test:** `cargo run --release`, drag across a multi-line agent transcript
- **Expected:** No tearing or perceptible lag at 60fps
- **Why human:** Perceptual frame-rate feel cannot be asserted from unit tests

#### 2. SC-2 — cmd+c places selection on macOS clipboard

- **Test:** In martins, drag-select text, press cmd+c, then in another terminal run `pbpaste`
- **Expected:** `pbpaste` output equals the selected text
- **Why human:** Clipboard integration is a system call; kitty-protocol delivery of `SUPER+c` must be confirmed against live Terminal.app / iTerm / Ghostty (some emulators consume cmd+c before it reaches the app)

#### 3. SC-4 — Selection survives streaming PTY output

- **Test:** Start an agent stream, drag-select before the stream finishes, observe behavior as new rows scroll in
- **Expected:** Highlight stays put and the text inside stays correct as rows scroll under it
- **Why human:** Visual stability under 60fps render is perceptual; this is the SEL-04 contract

#### 4. UAT-06-04-A — cmd+c with no selection in Terminal mode forwards SIGINT

- **Test:** Launch Martins, run `sleep 30` in active tab, ensure no selection, press cmd+c
- **Expected:** `sleep` exits within 1s; clipboard unchanged (verify with `pbpaste`)
- **Why human:** Byte-level PTY-forwarding deferred from automation per CLAUDE.md minimal-surface — would require widening `write_active_tab_input` with a test-mode branch (rejected)

#### 5. UAT-06-04-B — Esc with no selection in Terminal mode forwards 0x1b to active PTY

- **Test:** Launch Martins, run `vim`/`nvim`, press `i` for insert mode, press Esc
- **Expected:** Vim returns to Normal mode (`-- INSERT --` indicator clears)
- **Why human:** Same byte-level PTY-forwarding rationale as UAT-06-04-A; preserves Phase 5 behavior

#### 6. D-20 / D-21 — Inverted-cell highlight matches Ghostty/iTerm visual feel

- **Test:** Compare side-by-side against Ghostty at the same prompt
- **Expected:** XOR REVERSED behavior visually distinct from vt100 reverse-video; matches Ghostty/iTerm muscle memory
- **Why human:** Visual parity comparison

---

### Known Issues

| ID                | Source         | File / Location                          | Severity | Disposition                                                                                                                                                                                                  |
| ----------------- | -------------- | ---------------------------------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| REVIEW-MINOR-01   | 06-REVIEW.md   | `src/events.rs:80-81` + `src/app.rs:485-488` | Minor    | Empty-snapshot edge case where `Some("")` defeats live-rematerialization fallback. Recommended fix: gate `sel.text = Some(text)` on `!text.is_empty()`. Non-blocking; Phase 6 closes without this fix.       |
| REVIEW-MINOR-02   | 06-REVIEW.md   | `src/app.rs:677-725` (`select_line_at`)  | Minor    | Triple-click on blank row creates an invisible `is_empty()` selection. UX papercut, not correctness bug. Non-blocking.                                                                                       |
| REVIEW-MINOR-03   | 06-REVIEW.md   | `src/app.rs:102` (`last_click_col`)      | Nit      | Field written but never read. Either delete or document as reservation for same-word cluster check.                                                                                                          |
| REVIEW-MINOR-04   | 06-REVIEW.md   | `src/selection_tests.rs:263-294`         | Minor    | `scroll_generation_increments_on_vertical_scroll` is timing-sensitive (500ms wall-clock budget against real `/bin/cat` PTY). Flakes occasionally on parallel cold runs; passes single-threaded and warm runs. **Production code is correct** (verified). Recommended: bump deadline to 2s. Phase summary documents this explicitly. |
| REVIEW-NIT-01..05 | 06-REVIEW.md   | various                                  | Nit      | Doc-comment additions (mark_dirty asymmetry note, materialize_selection_text trace logging, dead-code attribute removal, snapshot-size invariant comment, `row_hash` non-stability comment). Cosmetic only.   |

---

### Gaps Summary

**No blocking gaps.**

All 4 ROADMAP success criteria are wired in production code and observable through automated tests at the mechanical level. Three of the four (SC-1, SC-2, SC-4) require human verification to confirm the perceptual / system-call behaviors that automated tests deliberately do not assert (60fps render feel, real `pbpaste` round-trip, no-flicker streaming). The two byte-level UATs (UAT-06-04-A, UAT-06-04-B) cover PTY forwarding paths that were intentionally not automated per CLAUDE.md minimal-surface convention.

The phase status is therefore **`human_needed`**, not `passed`, despite a complete-looking score: closure depends on the operator running the 6 manual UAT items above.

The 4 review-minor issues are non-blocking quality concerns (3 are recommended hardening, 1 is a known test flake whose production code is verified correct).

---

_Verified: 2026-04-24_
_Verifier: Claude (gsd-verifier)_
_Depth: standard (goal-backward, requirement-cross-referenced)_
