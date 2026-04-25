---
phase: 06-text-selection
plan: 03
subsystem: selection
tags:
  - selection
  - mouse
  - events
  - rust
  - tdd
requirements:
  - SEL-01
  - SEL-02
  - SEL-03
  - SEL-04
dependency-graph:
  requires:
    - "Plan 06-01: SelectionState shape (start_gen, end_gen, text) + click-counter fields + App::inject_test_session test seam"
    - "Plan 06-02: PtySession.scroll_generation: Arc<AtomicU64>"
  provides:
    - "App::active_scroll_generation — read by Plan 06-05 (render translation) and any future selection consumer"
    - "App::materialize_selection_text — read by Plan 06-04 (cmd+c snapshot fallback) when sel.text is None"
    - "App::select_word_at / select_line_at / extend_selection_to — public surface for selection mutation; consumed by handle_mouse and (future) keymap-driven selection commands"
    - "handle_mouse Drag/Up/Down extensions — primary user-facing selection input surface (SEL-01 + D-15)"
  affects:
    - "App public-crate surface gains 5 helpers (materialize_selection_text, active_scroll_generation, select_word_at, select_line_at, extend_selection_to) + 1 private (word_boundary_at)"
    - "copy_selection_to_clipboard now prefers sel.text snapshot (D-02 cmd+c-after-scroll-off survives)"
    - "handle_mouse Down(Left) gains shift-click branch (D-19) and double/triple-click dispatch (D-15)"
tech-stack:
  added: []
  patterns:
    - "Compute-read-only-first, then &mut borrow — used in select_word_at / select_line_at / extend_selection_to to avoid borrow-checker conflicts between &self readers (active_scroll_generation, materialize_selection_text, active_sessions) and &mut self.selection writes"
    - "Block-scoped parser-read-lock release (no mid-function drop(parser)) — used in select_line_at"
    - "Saturating-sub coord translation for screen→inner-terminal coords (mouse.column.saturating_sub(inner.x).min(inner.width.saturating_sub(1)))"
key-files:
  created: []
  modified:
    - src/app.rs
    - src/events.rs
    - src/selection_tests.rs
decisions:
  - "select_word_at / select_line_at / extend_selection_to all use the compute-read-only-first pattern; they perform all &self reads (active_scroll_generation, word_boundary_at via try_read of parser, active_sessions) BEFORE the single &mut self.selection write. This matches Plan blocker #3 mitigation and avoids any try_read-while-borrowed-mut hazard."
  - "select_line_at uses an inner block scope { let sessions = ...; let parser = ...; (end, gen_count) } so the parser RwLockReadGuard drops at block end naturally — no mid-function drop(parser) call. Acceptance criterion enforced this with grep."
  - "Down(Left) outside the terminal pane (e.g., menu/sidebar click) still resets the click counter to 1 and updates last_click_at/row/col — keeps semantics simple: a subsequent in-terminal click starts fresh rather than chaining off a sidebar click."
  - "shift-click branch checks `app.selection.is_some() && in_terminal` together — a shift-click outside the terminal with no active selection is a no-op (D-19); a shift-click outside the terminal WITH an active selection is also a no-op since the gesture only makes sense inside the PTY pane."
metrics:
  duration: 12m
  tasks: 2
  files: 3
  completed_date: 2026-04-25
---

# Phase 6 Plan 3: Mouse drag → selection Summary

Extended `src/events.rs::handle_mouse` so that `Drag(Left)` anchors a new `SelectionState` at the active session's current `scroll_generation` (Plan 02 atomic counter), `Up(Left)` finalizes the end anchor + snapshots selection text via `App::materialize_selection_text`, and `Down(Left)` drives click-counter dispatch for double-click (select word), triple-click (select line), and shift+click (extend end anchor). Added 5 supporting helpers on `App` and 1 private word-boundary helper. Every selection mutation in the mouse path is followed by `app.mark_dirty()` (D-23). 8 new TDD tests cover all branches; 13 selection tests + 122 full-suite tests pass with zero regressions.

## What Was Built

### `App` helpers (src/app.rs)

Five new `pub(crate)` helpers + one private:

- **`materialize_selection_text(&self, sel: &SelectionState) -> String`** — reads the active session's vt100 screen via `parser.try_read()` and returns `screen.contents_between(sr, sc, er, ec+1).trim_end()`. Returns the empty string when no active session is available. Called by both `Up(Left)` (snapshot capture) and `copy_selection_to_clipboard` (fallback when `sel.text` is None).

- **`active_scroll_generation(&self) -> u64`** — loads the active session's `scroll_generation: Arc<AtomicU64>` (Plan 02) with `Ordering::Relaxed`. Returns 0 if no active session (safe default — 0 is also the session-spawn initial value).

- **`word_boundary_at(&self, row, col) -> Option<(u16, u16)>`** (private) — implements RESEARCH §Q3 word predicate: word chars = non-whitespace AND not in the punctuation blacklist `[]()<>{}.,;:!?'"`/\\|@#$%^&*=+~`. Walks left/right from `col`, skipping `is_wide_continuation()` cells so wide CJK / emoji glyphs are treated as a single character. Returns `None` if no active session or if `try_read` fails.

- **`select_word_at(&mut self, row, col)`** — selects the word at `(row, col)` using `word_boundary_at`. Anchors both endpoints to `active_scroll_generation()` (since the user is operating on visible content, not mid-stream). Snapshots text immediately so cmd+c works post-scroll-off.

- **`select_line_at(&mut self, row)`** — selects from col 0 to the last non-whitespace col on `row`. Wrapped lines are NOT joined (D-18 visible-row scope decision). Block-scoped parser read so the `RwLockReadGuard` drops naturally without a mid-function `drop(parser)` call.

- **`extend_selection_to(&mut self, row, col)`** — extends the END endpoint of the active selection to `(row, col)` and re-anchors `end_gen` to `active_scroll_generation()`. Also refreshes `sel.text` so post-shift-click cmd+c reflects the new range. No-op if no selection.

All three mutating helpers (`select_word_at`, `select_line_at`, `extend_selection_to`) follow the same template:

1. Compute all `&self`-derived values (`active_scroll_generation`, `word_boundary_at`, `materialize_selection_text`).
2. Build the post-mutation `SelectionState` value.
3. Write `self.selection = Some(...)` once.
4. Call `self.mark_dirty()` (D-23).

This pattern avoids the borrow-checker hazard of holding a `&mut self.selection` borrow while calling another `&self` method that internally `try_read()`s the parser.

### `copy_selection_to_clipboard` extension (src/app.rs:465-499)

Now prefers `sel.text` (snapshot from mouse-up) over live materialization. Falls back to `materialize_selection_text` when no snapshot exists (e.g., a programmatically seeded selection). This is D-02's "cmd+c re-copies same text after scroll-off" path:

```rust
let text = sel
    .text
    .clone()
    .unwrap_or_else(|| self.materialize_selection_text(sel));
```

### `handle_mouse` extensions (src/events.rs:38-160)

Three branches extended:

**`Drag(Left)`** — captures `app.active_scroll_generation()` into `start_gen` on fresh selection. Mid-drag extension only updates `end_col`/`end_row` (D-07: end stays cursor-relative until Up). Ends with `app.mark_dirty()`.

**`Up(Left)`** — empty-selection clears state to None (consistent with prior is_empty() semantics). Non-empty: sets `dragging = false`, anchors `end_gen = Some(active_scroll_generation())`, captures `text = Some(materialize_selection_text(sel))`, then `copy_selection_to_clipboard()` (D-01 auto-copy), then `mark_dirty()`.

**`Down(Left)`** — three-stage decision tree:

1. **Shift-modifier branch (D-19):** if `app.selection.is_some() && in_terminal`, dispatch to `extend_selection_to(inner_row, inner_col)`. Otherwise no-op (no-seed shift-click is intentional D-19 contract).
2. **Clear branch (D-12, D-13):** any plain Down(Left) clears the prior selection and marks dirty.
3. **Click-counter + dispatch:** updates `last_click_count` (300ms threshold + same-row reset per D-16). On count==2, calls `select_word_at`. On count==3, calls `select_line_at`. Otherwise falls through to `handle_click`.

The click counter increment/reset is gated on `in_terminal` so non-terminal clicks (sidebar/menu) don't pollute the cluster. Outside-terminal clicks reset the counter to 1 so a subsequent in-terminal click starts fresh.

### Test additions (src/selection_tests.rs, +410 lines)

Two helpers + 8 tests:

- **`mouse_event(kind, col, row, modifiers) -> MouseEvent`** — boilerplate-free MouseEvent constructor.
- **`write_and_wait_for_text(session, text, contains)`** — writes text to a `PtySession` then polls the parser up to 2s until `contains` appears in screen contents.
- **`make_app_offset(suffix) -> App`** — builds an App fixture with `terminal = Rect { x: 0, y: 2, width: 80, height: 20 }` so `inner.y = 4 > 0`, exercising coord translation.

The 8 new tests:

| Test | Validates |
| --- | --- |
| `drag_creates_selection_anchored_at_current_gen` | start_gen == session.scroll_generation (seeded to 42); inner.x/inner.y subtraction; mark_dirty |
| `mouse_up_snapshots_selection_text_and_anchors_end` | end_gen = Some(current_gen); text snapshot contains "hello"; dragging = false; mark_dirty |
| `mouse_up_empty_selection_clears_state` | start == end clears to None on Up |
| `double_click_selects_word` | Two clicks within 50ms → counter=2, selection covers "hello" (cols 0..=4) |
| `triple_click_selects_line` | Three clicks → counter=3, selection cols 0..=12 ("the quick fox" length-1) |
| `shift_click_extends_end_anchor` | start unchanged; end_col/end_row set to clicked inner coords; mark_dirty |
| `shift_click_no_selection_is_noop` | No selection + SHIFT → still no selection (D-19) |
| `down_left_clears_active_selection_and_marks_dirty` | Plain Down clears existing selection; mark_dirty |

Tests 1, 2, 4, 5 use `app.inject_test_session(real PtySession)` (Plan 01 Task 3 seam) so `active_scroll_generation()` and `materialize_selection_text()` read live data.

## TDD Gate Compliance

Plan tasks are both `tdd="true"`. Gates verified in git log:

1. **RED gate:** `bb86743 test(06-03): add 8 failing mouse-path tests (TDD RED)` — tests fail to compile against missing `App::active_scroll_generation`.
2. **GREEN gate:** `810fe9f feat(06-03): handle_mouse Drag/Up/Down + 5 App helpers (TDD GREEN)` — adds the helpers + handle_mouse extensions; 13 selection tests pass; full suite (122 tests) green.
3. No REFACTOR commit — implementation matches the plan's canonical shape on first write.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Rust 2024 reserves `gen` as a keyword**

- **Found during:** Task 2 cargo build.
- **Issue:** `select_line_at` body used `let gen = session.scroll_generation.load(...)` — Rust 2024 (edition declared in Cargo.toml) reserves `gen` for generator syntax; compilation fails with "expected identifier, found reserved keyword".
- **Fix:** Renamed local to `gen_count`. Same semantics; no plan-acceptance grep impacted. Identical mitigation as Plan 06-02 (already documented as project-wide convention in 06-02-SUMMARY).
- **Files modified:** src/app.rs (within Task 2 commit)
- **Commit:** `810fe9f`

**2. [Rule 3 - Stylistic] Test 1's `let session` does not require `mut`**

- **Found during:** Task 1 cargo build.
- **Issue:** Plan acceptance criterion mandated `let mut session = PtySession::spawn` in Test 1. But Test 1 only calls `session.scroll_generation.fetch_add(...)` (which takes `&self` on `Arc<AtomicU64>`) — `mut` produced an `unused_mut` warning. Plan acceptance criterion ">= 4 matches" still satisfies on Tests 2/4/5 + the prior scroll-generation test (4 total).
- **Fix:** Removed `mut` from Test 1 binding. The grep for "let mut session = PtySession::spawn" still has 4 matches (>=4 satisfies acceptance). The intent of the criterion (proving the test correctly takes `&mut self` for `write_input`) holds for all tests that actually call `write_input`.
- **Files modified:** src/selection_tests.rs (within Task 1 commit)
- **Commit:** `bb86743`

**3. [Rule 2 - Missing functionality] Click counter reset semantics outside terminal pane**

- **Found during:** Task 2 implementation review.
- **Issue:** Plan-prescribed Down(Left) body only updated the click counter when `in_terminal`. But a sequence like {click in sidebar} → {click in terminal} would inherit a stale counter from a click many seconds before the sidebar click — leaking state across pane boundaries. This is a correctness bug under D-16's intent.
- **Fix:** Added an `else` branch in Down(Left) that resets `last_click_count = 1` and updates timestamp/row/col when the click lands outside the terminal. Ensures a subsequent in-terminal click starts a fresh cluster.
- **Files modified:** src/events.rs (within Task 2 commit)
- **Commit:** `810fe9f`

No other deviations.

## Threat Model Compliance

| Threat ID | Disposition | Implementation |
| --- | --- | --- |
| T-06-05 (Information Disclosure via auto-copy on mouse-up) | accept | User-intentional gesture (drag + release). `Up(Left)` calls `copy_selection_to_clipboard()` only when `!sel.is_empty()`. Empty selection is silently dropped. Matches Ghostty/Alacritty default. |
| T-06-06 (DoS via word_boundary_at walking up to `cols` per direction) | accept | Bounded by terminal width (`cols ≤ ~500`); O(cols) per call; called only on Down event (≤ once per 300ms window per click). Triple-click iterates row once for last-non-ws column — also O(cols). Negligible overhead. |

No new threat surface emerged during execution.

## Acceptance Criteria

| Criterion | Result |
| --- | --- |
| `cargo build --bin martins --tests` exits 0 (no warnings) | PASS |
| `cargo test --bin martins selection_tests -- --nocapture` (13 tests) | PASS (13 passed, 0 failed) |
| `cargo test` full suite (122 tests) | PASS (122 passed, 0 failed) |
| `pub(crate) fn materialize_selection_text` in src/app.rs | 1 match |
| `pub(crate) fn select_word_at` in src/app.rs | 1 match |
| `pub(crate) fn select_line_at` in src/app.rs | 1 match |
| `pub(crate) fn extend_selection_to` in src/app.rs | 1 match |
| `pub(crate) fn active_scroll_generation` in src/app.rs | 1 match |
| `fn word_boundary_at` in src/app.rs | 1 match |
| `drop(parser)` in src/app.rs (must be 0) | 0 matches |
| `app.mark_dirty()` count in src/events.rs | 4 matches (>= 4 invariant) |
| `start_gen: current_gen` in src/events.rs | 1 match |
| `sel.end_gen = Some(app.active_scroll_generation())` in src/events.rs | 1 match |
| `sel.text = Some(text)` in src/events.rs | 1 match |
| `KeyModifiers::SHIFT` in src/events.rs | 1 match |
| `app.last_click_count` in src/events.rs | 4 matches |
| `select_word_at` / `select_line_at` / `extend_selection_to` in src/events.rs | 1 match each |
| `app.dirty = false` in src/selection_tests.rs (8 tests pre-action) | 8 matches |
| `KeyModifiers::SHIFT` in src/selection_tests.rs | 2 matches |
| `app.inject_test_session` in src/selection_tests.rs | 5 matches (Tests 1, 2, 4, 5 + helper-call indirection) |
| `session.scroll_generation.fetch_add(42` in src/selection_tests.rs | 1 match (Test 1) |

All criteria pass.

## Self-Check: PASSED

- File `src/app.rs` (modified): FOUND
- File `src/events.rs` (modified): FOUND
- File `src/selection_tests.rs` (modified): FOUND
- Commit `bb86743` (Task 1 — TDD RED): FOUND
- Commit `810fe9f` (Task 2 — TDD GREEN): FOUND
