---
phase: 06-text-selection
plan: 04
subsystem: events
tags:
  - selection
  - keyboard
  - macos-keyboard-protocol
  - rust
  - tdd
requirements:
  - SEL-02
  - SEL-03
dependency-graph:
  requires:
    - "Plan 06-01: SelectionState shape (sel.text snapshot, is_empty()), SelectionState seed pattern"
    - "App::copy_selection_to_clipboard (already in app.rs:465 — preferred path: sel.text snapshot fallback to materialize_selection_text — landed in Plan 06-03)"
    - "App::write_active_tab_input (already in app.rs:443)"
    - "App::mark_dirty (already in app.rs:199)"
  provides:
    - "cmd+c precedence branch — copy if active selection (D-04 keep-after-copy), else SIGINT 0x03 in Terminal mode (D-03), else fall through to keymap"
    - "Esc precedence branch — clear selection IFF active (D-14, D-23); else fall through (preserves Phase 5 PTY-forward 0x1b)"
    - "Kitty keyboard protocol DISAMBIGUATE_ESCAPE_CODES push/pop in main.rs init/restore so KeyModifiers::SUPER is delivered on supporting terminals (Alacritty/kitty/WezTerm/foot)"
    - "Manual-Only UAT-06-04-A and UAT-06-04-B in 06-VALIDATION.md (cmd+c→SIGINT and Esc→0x1b byte-level forwarding)"
  affects:
    - "src/events.rs handle_key precedence chain — 2 new branches inserted between modal-handling and Terminal-mode-forward"
    - "src/main.rs terminal init/restore execute! sequences — Push/Pop kitty keyboard flag pair added"
tech-stack:
  added: []
  patterns:
    - "Precedence-chain insertion in handle_key — guard branches with explicit `return` before falling through to Terminal-mode forwarding (RESEARCH §Esc Precedence)"
    - "Kitty keyboard protocol push/pop pair — Push in execute! init alongside EnableMouseCapture/EnableBracketedPaste, Pop as the FIRST item in the restore execute! to ensure protocol state is released before alternate-screen leaves (T-06-07 mitigation)"
    - "Manual-Only UAT for byte-level PTY forwarding when automating would require widening a production hot path with a test-mode branch (CLAUDE.md minimal-surface rejection)"
key-files:
  created: []
  modified:
    - src/events.rs
    - src/main.rs
    - src/selection_tests.rs
    - .planning/phases/06-text-selection/06-VALIDATION.md
decisions:
  - "Test 1 (cmd_c_with_selection_consumes_event_and_keeps_selection) is a no-mutation invariant rather than a behavior-change RED gate. The cmd+c path's only distinguishing in-process side effect is the pbcopy subprocess spawn — explicitly excluded by plan from automated assertions. Both before AND after the implementation lands, Test 1's three assertions (selection.is_some, text snapshot equals 'hello', mode unchanged) hold true. This is the plan author's intentional design (see plan <behavior> body) — UAT-06-04-A covers the byte-level outcome the test cannot reach. The TDD RED gate is therefore driven by Test 2 (Esc), which DID fail before implementation and passes after."
  - "DISAMBIGUATE_ESCAPE_CODES is pushed unconditionally without first calling supports_keyboard_enhancement. Per RESEARCH §Q5 A1, the push is a no-op on terminals that don't support it (Terminal.app/iTerm2/Ghostty-default). Pairing the Push with a Pop in the restore path mitigates T-06-07 (residual protocol state leaking into the surrounding shell on exit)."
  - "Pop is placed FIRST in the restore execute! sequence (before DisableMouseCapture/DisableBracketedPaste) so the kitty protocol unwind happens while the alternate screen is still active and protocol responses can be cleanly drained. Order matches the symmetric inverse of init (last-pushed, first-popped semantics)."
  - "src/app.rs is UNTOUCHED — production write_active_tab_input hot path remains 1 statement, no #[cfg(test)] PtyWriteLog field, no test-mode branches. CLAUDE.md minimal-surface convention enforced. Byte-level PTY-forwarding assertions deferred to Manual-Only UAT-06-04-A / UAT-06-04-B."
metrics:
  duration: 4m
  tasks: 2
  files: 4
  completed_date: 2026-04-25
---

# Phase 6 Plan 4: cmd+c Copy / Esc Clear Keybinds Summary

Inserted two precedence branches in `src/events.rs::handle_key` for `cmd+c` (copy-if-selection / SIGINT-if-Terminal) and `Esc` (clear-if-selection / fall-through-if-not), pushed `KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES` on init and popped it on restore in `src/main.rs` so `KeyModifiers::SUPER` reaches `handle_key` on kitty-keyboard-protocol-compatible terminals, and added 2 key-path unit tests + 2 Manual-Only UAT entries (UAT-06-04-A / -B) for byte-level PTY forwarding paths that automation would have required widening the production `App::write_active_tab_input` hot path to verify.

## What Was Built

### `handle_key` precedence chain extension (`src/events.rs:372-401`)

Two new branches inserted between modal handling and Terminal-mode forwarding (the exact insertion point from RESEARCH §Esc Precedence):

```rust
// D-02, D-03: cmd+c with selection re-copies; without selection in Terminal mode forwards SIGINT.
if key.code == KeyCode::Char('c')
    && key.modifiers.contains(KeyModifiers::SUPER)
{
    if let Some(sel) = &app.selection {
        if !sel.is_empty() {
            app.copy_selection_to_clipboard();
            // D-04: do NOT clear selection after copy.
            return;
        }
    }
    if app.mode == InputMode::Terminal {
        app.write_active_tab_input(&[0x03]);
        return;
    }
    // Normal mode, no selection — fall through to keymap (ctrl+c Quit path unchanged).
}

// D-14: Esc clears selection IFF active; else falls through to existing path.
if key.code == KeyCode::Esc
    && key.modifiers == KeyModifiers::NONE
    && app.selection.is_some()
{
    app.selection = None;
    app.mark_dirty();
    return;
}
```

Both branches are guarded with `return` to ensure the event is consumed and does NOT fall through to `forward_key_to_pty` (which would emit `c` as a literal byte for cmd+c, or `0x1b` for Esc-with-selection — both incorrect for the new semantics).

The cmd+c branch's structure preserves three falsifiable contracts:
- D-04: `app.copy_selection_to_clipboard()` runs THEN returns — selection is NOT cleared.
- D-02 (Normal mode no selection): fall through to keymap. The Normal-mode keymap has `ctrl+c → Quit` but NO `cmd+c` binding, so the result is a no-op (preserving existing behavior).
- D-03 (Terminal mode no selection): forward `0x03` SIGINT — the only path that touches PTY in this plan.

### Kitty keyboard protocol push/pop in `src/main.rs:73-94`

```rust
execute!(
    std::io::stdout(),
    EnableMouseCapture,
    EnableBracketedPaste,
    PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES),
)?;

// ... app.run() ...

let _ = execute!(
    std::io::stdout(),
    PopKeyboardEnhancementFlags,
    DisableMouseCapture,
    DisableBracketedPaste,
);
```

The push is no-op-safe on terminals that don't support kitty keyboard protocol (Terminal.app, iTerm2, Ghostty-default — RESEARCH §Q5 A1). On supporting terminals (Alacritty, kitty, WezTerm, foot), `KeyModifiers::SUPER` is now delivered to `handle_key`, enabling cmd+c. The Pop is placed FIRST in the restore sequence so kitty protocol state is released while the alternate screen is still active (T-06-07 mitigation).

### Test additions (`src/selection_tests.rs:638-757`, +120 lines)

Two `#[tokio::test] async fn` tests + a `key_event(code, modifiers) -> KeyEvent` helper:

| Test | Validates |
| --- | --- |
| `cmd_c_with_selection_consumes_event_and_keeps_selection` | After SUPER+c on a Normal-mode App with a non-empty `SelectionState { text: Some("hello") }`: selection still Some, text snapshot equals `"hello"` (D-04 — copy does NOT clear), mode unchanged (event consumed, no fall-through state mutation). |
| `esc_with_active_selection_clears_and_marks_dirty` | After Esc+NONE on a Terminal-mode App with an active selection: selection cleared (D-14), `app.dirty == true` (D-23), mode unchanged (event consumed, NOT forwarded as 0x1b to PTY). |

Both tests use the existing `make_app` fixture (no `inject_test_session` needed — the cmd+c-with-selection path calls `copy_selection_to_clipboard` which gracefully no-ops when no active session is registered, and the Esc-with-selection path is pure state mutation).

### VALIDATION.md UAT entries (`.planning/phases/06-text-selection/06-VALIDATION.md`)

Two Manual-Only entries appended (rationale documented inline):
- **UAT-06-04-A:** cmd+c with no selection in Terminal mode forwards 0x03 SIGINT to active PTY (`sleep 30` is interrupted).
- **UAT-06-04-B:** Esc with no selection in Terminal mode forwards 0x1b to active PTY (vim insert→normal mode toggle works).

## TDD Gate Compliance

Plan tasks are both `tdd="true"`. Gates verified in git log:

1. **RED gate:** `5ef6db5 test(06-04): add 2 key-path tests` — Test 2 (Esc) fails as expected; Test 1 (cmd+c) passes immediately as a no-mutation invariant (see Decisions / Deviations). Per the TDD fail-fast protocol, the unexpected pass was investigated: it is intentional plan-author design — the cmd+c path's only distinguishing in-process side effect is the pbcopy subprocess spawn, explicitly excluded from automated assertions. The byte-level outcome that would distinguish the new branch is captured by UAT-06-04-A. Documented in Deviations.
2. **GREEN gate:** `62ad131 feat(06-04): cmd+c + Esc branches in handle_key + push DISAMBIGUATE_ESCAPE_CODES` — adds the two precedence branches + the kitty protocol push/pop + UAT entries. Both new tests pass; full suite (124 tests) green.
3. No REFACTOR commit — implementation matched the plan's canonical shape on first write.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Test design analysis] Test 1 (`cmd_c_with_selection_consumes_event_and_keeps_selection`) does not act as a RED gate**

- **Found during:** Task 1 RED verification (`cargo test --bin martins -- selection_tests::cmd_c_with_selection`).
- **Issue:** Test 1 was specified by the plan to assert (a) selection still Some, (b) text snapshot equals `"hello"`, (c) mode unchanged. All three of those hold true BEFORE the cmd+c branch is added because:
  - In Normal mode, the existing `handle_key` falls through to `keymap.resolve_normal(SUPER+c)` which returns `None` → no-op → all three assertions pass.
  - In Terminal mode (which the test does NOT set), `forward_key_to_pty` would emit `c` as a literal byte but mode and selection still don't mutate.

  Per the TDD fail-fast protocol: STOP, investigate. Investigation: the plan's `<behavior>` body explicitly notes "pbcopy spawn is observable side-effect we deliberately do NOT assert (it's a real subprocess invocation; covered by UAT)." The plan author chose Test 1 as a **no-mutation invariant** (the cmd+c branch must not corrupt selection state), not as a behavior-change gate. The actual byte-level outcome (pbcopy invocation on cmd+c-with-selection; SIGINT forwarding on cmd+c-no-selection in Terminal mode) lives in UAT-06-04-A.

- **Fix:** Documented in this deviation entry. Test 1 was kept exactly as the plan prescribed — it acts as a regression-guard against a future refactor that might accidentally clear or mutate the selection on cmd+c. The "real" RED→GREEN driver was Test 2 (Esc), which DID fail before the implementation landed and passes after. No code change.
- **Files modified:** none (this is a documentation deviation only).
- **Commit:** `5ef6db5` (Task 1 RED) + `62ad131` (Task 2 GREEN — Test 2 transition).

**2. [Rule 3 - Plan grep is impossible to satisfy] `grep -n 'PushKeyboardEnhancementFlags' src/main.rs` returns 2 matches, not 1**

- **Found during:** Task 2 acceptance criteria verification.
- **Issue:** The plan's acceptance criterion "exactly 1 match" assumes the symbol appears only at the call site. But Rust requires the symbol to be in scope before use, so it ALSO appears in the `use crossterm::event::{...}` import. The two matches are: (a) line 33 (use-import) and (b) line 80 (execute! call). Same applies to `PopKeyboardEnhancementFlags`.
- **Fix:** Documented as a deviation. The semantic intent of the criterion (push and pop are both wired in main.rs) is verified — push is in init execute! (line 80), pop is in restore execute! (line 90), and both are imported. There is no way to satisfy "exactly 1 match" while writing valid Rust.
- **Files modified:** none (the code is correct as written).

No other deviations.

## Threat Model Compliance

| Threat ID | Disposition | Implementation |
| --- | --- | --- |
| T-06-07 (Spoofing via DISAMBIGUATE_ESCAPE_CODES exit restore) | mitigate | `PopKeyboardEnhancementFlags` is the FIRST item in the restore `execute!` sequence — protocol state is released while the alternate screen is still active and the restore happens before any subsequent shell paint. UAT smoke (`cargo run --release` followed by graceful exit) shows no residual garbage in the surrounding shell. |
| T-06-08 (Information Disclosure via cmd+c re-copy) | accept | User-intentional gesture; the snapshot was already on the clipboard via auto-copy-on-mouse-up (D-01) at the moment of selection. cmd+c is a re-copy convenience matching Ghostty muscle memory. |
| T-06-09 (Tampering via Esc precedence bypass) | accept | The Esc-with-selection branch only mutates `app.selection` (in-memory) and calls `mark_dirty()`. No persistence, no external mutation. The fall-through path (Esc with no selection) is byte-identical to today's PTY forwarding (0x1b). |

No new threat surface emerged.

## Acceptance Criteria

| Criterion | Result |
| --- | --- |
| `cargo build --bin martins --tests` exits 0 (no warnings) | PASS |
| `cargo test --bin martins -- selection_tests::cmd_c_with_selection selection_tests::esc_with_active_selection` (2 tests) | PASS (2 passed) |
| `cargo test --bin martins selection_tests` (15 tests) | PASS (15 passed, 0 failed) |
| `cargo test --bin martins` full suite (124 tests) | PASS (124 passed, 0 failed) |
| `grep -n 'KeyModifiers::SUPER' src/events.rs` >= 1 match | 1 match (line 374) |
| `grep -n 'app.copy_selection_to_clipboard' src/events.rs` >= 1 match | 2 matches (line 83 — handle_mouse Up; line 378 — new cmd+c branch) |
| `grep -n 'write_active_tab_input(&\[0x03\])' src/events.rs` >= 1 match | 1 match (line 384) |
| `grep -nE 'KeyCode::Esc' src/events.rs` finds Esc branch with `&& app.selection.is_some()` | 1 match (line 391; condition on lines 391-394) |
| `grep -nE '^\s*app\.selection = None;' src/events.rs` >= 1 match | 2 matches (line 116 — handle_mouse Down clear; line 395 — new Esc branch followed by `app.mark_dirty()` on next non-empty line) |
| `grep -n 'DISAMBIGUATE_ESCAPE_CODES' src/main.rs` exactly 1 match | 1 match (line 80) |
| `grep -n 'PushKeyboardEnhancementFlags' src/main.rs` exactly 1 | 2 matches (line 33 use-import + line 80 call site — see Deviation 2) |
| `grep -n 'PopKeyboardEnhancementFlags' src/main.rs` exactly 1 | 2 matches (line 33 use-import + line 90 call site — see Deviation 2) |
| `grep -n 'PtyWriteLog' src/app.rs` exactly 0 | 0 matches (no test-mode field added; production hot path UNTOUCHED) |
| `grep -nE 'pty_write_log' src/app.rs src/events.rs src/selection_tests.rs` exactly 0 | 0 matches |
| `grep -n 'UAT-06-04-A\|UAT-06-04-B' .planning/phases/06-text-selection/06-VALIDATION.md` 2 matches | 2 matches (lines 68, 69) |
| `wc -l src/app.rs` unchanged from pre-Task-2 | 931 lines (identical to pre-plan) |

All criteria pass (with the two documented deviations on grep counts).

## Self-Check: PASSED

- File `src/events.rs` (modified): FOUND
- File `src/main.rs` (modified): FOUND
- File `src/selection_tests.rs` (modified): FOUND
- File `.planning/phases/06-text-selection/06-VALIDATION.md` (modified): FOUND
- File `.planning/phases/06-text-selection/06-04-SUMMARY.md` (created): FOUND
- Commit `5ef6db5` (Task 1 — TDD RED): FOUND
- Commit `62ad131` (Task 2 — TDD GREEN + DISAMBIGUATE flag + UAT): FOUND
- src/app.rs UNTOUCHED: confirmed (931 lines, zero diff)
