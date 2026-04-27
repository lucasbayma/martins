---
phase: 06-text-selection
reviewed: 2026-04-24T00:00:00Z
depth: standard
files_reviewed: 9
files_reviewed_list:
  - src/app.rs
  - src/events.rs
  - src/main.rs
  - src/pty/manager.rs
  - src/pty/session.rs
  - src/selection_tests.rs
  - src/ui/terminal.rs
  - src/ui/draw.rs
  - src/workspace.rs
findings:
  blocker: 0
  major: 0
  minor: 4
  nit: 5
  total: 9
status: issues_found
---

# Phase 6: Code Review Report

**Reviewed:** 2026-04-24
**Depth:** standard (per workflow.code_review_depth)
**Files Reviewed:** 9
**Status:** issues_found (0 blockers, 0 majors — all findings are quality-tier)

## Summary

Phase 6 (text-selection) implements Ghostty-style drag-select / cmd+c / Esc /
multi-click / shift-click in the PTY pane, with REVERSED-XOR highlight, anchored
(gen, row, col) endpoints, and per-session `scroll_generation: Arc<AtomicU64>`
for scroll-stable selections (SEL-01..SEL-04). The 9 source files were examined
for the seven special-focus areas called out in the review brief:

1. **Atomic ordering (Relaxed) — VERIFIED SAFE.** `scroll_generation` is a pure
   monotonic version stamp. Reader (PTY thread) does `fetch_add(1, Relaxed)`;
   consumers (events.rs anchor capture, draw.rs render translation, render
   `current_gen` parameter) do `load(Relaxed)`. No causal dependency on other
   shared memory must be observed in lock-step with this counter — anchored
   selection coords carry the gen value at capture time and translation is
   monotonically eventually consistent. Relaxed is the correct ordering and
   does not introduce a data race.
2. **Borrow-checker `try_read`-while-`&mut`-borrowed hazards — VERIFIED CLEAN.**
   The compute-read-only-first pattern in `App::select_word_at`,
   `select_line_at`, and `extend_selection_to` (src/app.rs:644-754) holds
   parser read guards inside short scopes and drops them BEFORE the single
   `&mut self.selection` write. `extend_selection_to` clones the selection
   snapshot before invoking `materialize_selection_text(&self)` to sidestep
   the conflict. No `try_read`-while-`&mut`-borrowed hazard observed.
3. **Flaky `scroll_generation_increments_on_vertical_scroll` test — production
   code is CORRECT; flake is test-environmental.** The SCROLLBACK-LEN
   heuristic in `src/pty/session.rs:100-116` is sound for the documented
   trade-offs (RESEARCH §A3). The 500ms polling deadline against a real
   `/bin/cat` PTY plus 30 newlines is a timing budget, not a correctness gate
   — slow CI / heavy-load shells can exceed it without indicating a bug. See
   MINOR-04 below for a hardening suggestion.
4. **`#[cfg(test)]` test seams — VERIFIED CLEAN.** All three seams
   (`App::inject_test_session` at app.rs:867-941, `PtyManager::insert_for_test`
   at pty/manager.rs:128-138, `ui::terminal::render_with_selection_for_test`
   at ui/terminal.rs:209-252) are gated behind `#[cfg(test)]` and excluded
   from release builds. `inject_test_session` carries an `#[allow(dead_code)]`
   to suppress warnings in test builds where only some modules invoke it —
   acceptable.
5. **`mark_dirty` discipline (D-23) — VERIFIED CLEAN.** Every selection
   mutation in events.rs is followed by `app.mark_dirty()`: Drag (line 68),
   Up (line 75, 84), Down clear (line 117), shift-click via
   `extend_selection_to` (app.rs:753), double-click via `select_word_at`
   (app.rs:671), triple-click via `select_line_at` (app.rs:724), Esc clear
   (events.rs:402). `set_active_tab` and `clear_selection` mark dirty in
   app.rs.
6. **unsafe / FFI — NONE.** Zero `unsafe` blocks, zero direct FFI calls in
   any of the 9 reviewed files.
7. **Unbounded growth of `Option<String>` snapshot — BOUNDED IN PRACTICE.**
   `SelectionState.text` snapshot is bounded by the visible terminal area
   (rows × cols × ~4 bytes per UTF-8 cell ≈ 80 × 24 × 4 = ~7.6 KiB worst
   case for a 24-row screen). One snapshot lives at a time inside
   `Option<SelectionState>`. No unbounded growth path: every new selection
   replaces the prior snapshot via `app.selection = Some(...)`. See NIT-04
   below for a robustness suggestion.

The 9 findings below are all quality-tier (Minor / Nit). None gate phase
completion. The implementation faithfully realizes the locked CONTEXT
decisions (D-01..D-23) and the RESEARCH recommendations.

## Minor Issues

### MINOR-01: Empty snapshot stored as `Some("")` defeats fallback re-materialization

**File:** `src/events.rs:80-81` and `src/app.rs:485-488`
**Issue:** When `MouseEventKind::Up(Left)` fires at line 79-81, the code calls
`app.materialize_selection_text(&sel)` and unconditionally stores the result
into `sel.text = Some(text)`. If the parser's `try_read()` happens to fail at
that instant (write contention with the PTY reader thread on a busy pane),
`materialize_selection_text` returns `String::new()` (app.rs:512-526). The
selection then holds `text: Some("")`. A subsequent `cmd+c` reads
`sel.text.clone().unwrap_or_else(...)` (app.rs:485-488) — because the
`Option` is `Some`, the `unwrap_or_else` fallback never fires, so live
re-materialization is skipped, and `pbcopy` ends up writing nothing. The user
selected text but the clipboard is empty.

**Fix:**

```rust
// src/events.rs:80-82 — only store snapshot when materialization succeeded.
let text = app.materialize_selection_text(&sel);
if !text.is_empty() {
    sel.text = Some(text);
}
// else: leave sel.text = None so cmd+c falls back to live re-materialization.
```

Alternatively, distinguish the two failure modes in `materialize_selection_text`
by returning `Option<String>` (None = could not read; `Some("")` = valid empty
selection). Single-call-site change limits blast radius.

### MINOR-02: `select_line_at` on a blank/whitespace-only row produces a non-empty `Option<SelectionState>` that `is_empty()`

**File:** `src/app.rs:677-725`
**Issue:** `select_line_at` initializes `end = 0u16` and only updates it when
a non-whitespace cell is found. If the row is blank or all-whitespace, `end`
stays at 0. The constructed `SelectionState` then has `start_col == end_col ==
0` and `start_row == end_row == row`, so `sel.is_empty()` is `true`. The
function still calls `app.mark_dirty()` and stores `Some(SelectionState)`.
Downstream:
- `terminal.rs:158` short-circuits highlight rendering on `is_empty()` — the
  triple-click on a blank line produces no visual highlight, but
- `app.selection.is_some()` is true, so the next plain `Down(Left)` clears it
  (events.rs:115-118) — the user has to click once to "dismiss" an invisible
  selection that they didn't perceive existing.
- A subsequent `cmd+c` enters the `if !sel.is_empty()` guard at events.rs:383
  and skips the copy (correct), but the empty selection persists in state.

This is a UX papercut, not a correctness bug. It does NOT match Ghostty
semantics (Ghostty's triple-click on a blank line is a no-op — no selection
created).

**Fix:**

```rust
// src/app.rs:709-724 — bail when the row is blank.
let new_sel = SelectionState {
    start_col: 0,
    start_row: row,
    start_gen: current_gen,
    end_col: end,
    end_row: row,
    end_gen: Some(current_gen),
    dragging: false,
    text: None,
};
if new_sel.is_empty() {
    // Blank line — leave any prior selection untouched and skip mark_dirty.
    return;
}
let text = self.materialize_selection_text(&new_sel);
self.selection = Some(SelectionState { text: Some(text), ..new_sel });
self.mark_dirty();
```

### MINOR-03: `last_click_col` is written but never read in same-cluster comparison

**File:** `src/app.rs:102, 173` (field decl + init), `src/events.rs:144, 164`
(write), `src/events.rs:136` (read site uses only `last_click_row`)
**Issue:** Same-cluster detection at events.rs:136 only compares
`mouse.row == app.last_click_row`. The `last_click_col` field is updated on
every Down(Left) (line 144 inside terminal, line 164 outside terminal) but
never read. Per RESEARCH §Q4 / D-16, the same-row check is the spec ("If a
click lands outside the same word/line region as the previous click, reset
the counter") — column-level region check was deemed sufficient at row level.
Storing the column without using it is dead bookkeeping.

Two interpretations:
- **(a)** The field is reserved for a future "same-word" tightening of the
  cluster check (CONTEXT D-16: "If a click lands outside the same word/line
  region as the previous click, reset the counter"). Keep the field, document
  intent.
- **(b)** Strip the field — it's unused state.

**Fix:** Either delete the field and its writes, or add a comment explaining
the reservation:

```rust
// src/app.rs:102
/// Reserved for a future same-word cluster check (D-16). Currently only
/// `last_click_row` participates in the within-cluster comparison —
/// row-level granularity matches Ghostty's default.
pub last_click_col: u16,
```

Recommendation: option (b) — add the comment. Stripping the field would force
an API churn if the same-word check is ever wanted.

### MINOR-04: Flaky test relies on 500ms wall-clock polling against real /bin/cat PTY

**File:** `src/selection_tests.rs:263-294`
**Issue:** `scroll_generation_increments_on_vertical_scroll` spawns a real
`/bin/cat` PTY, writes 30 newlines synchronously, then polls
`session.scroll_generation` for up to 500ms. The 500ms budget is tight on
slow CI runners or heavily-loaded macOS systems where PTY reader-thread
scheduling latency + cat's stdin echo + vt100 parse can plausibly exceed
500ms for the first scroll-detection. Production code is correct (verified by
direct read of `pty/session.rs:100-116` against the SCROLLBACK-LEN heuristic
in RESEARCH §Q1); the flake is a test-environment timing budget.

**Fix:** Increase the deadline to 2s (matches `write_and_wait_for_text` which
already uses 2s, file:48), and re-poll on each iteration without exiting on
the first non-zero load (so a transient false-positive doesn't pass the test
incorrectly):

```rust
// src/selection_tests.rs:280-289 — extend deadline to match other PTY tests.
let deadline = Instant::now() + Duration::from_millis(2000);
let mut gen_count = 0u64;
while Instant::now() < deadline {
    gen_count = session.scroll_generation.load(Ordering::Relaxed);
    if gen_count > 0 {
        break;
    }
    tokio::time::sleep(Duration::from_millis(20)).await;
}
assert!(gen_count > 0, "scroll_generation never incremented; got {gen_count}");
```

Alternative: refactor the test to use a synthetic `vt100::Parser` directly,
write enough bytes to trigger scroll, and assert on a hash-detection helper
extracted from the reader thread. This bypasses real-PTY timing entirely. Out
of scope for a quick fix.

## Nit Issues

### NIT-01: Inconsistent `mark_dirty` semantics between `set_active_tab` and `select_active_workspace`

**File:** `src/app.rs:388-406`
**Issue:** `set_active_tab` (line 402) unconditionally calls `mark_dirty()`
after `clear_selection()` (which itself only marks dirty when a selection
existed). `select_active_workspace` (line 388-393) calls only
`clear_selection()` — so when no selection is active, switching workspaces
does not mark dirty. The asymmetry is deliberate per the source comment ("tab
strip repaints regardless of whether a selection existed") but worth a
matching comment on `select_active_workspace` explaining why workspace switch
relies on the upstream event-handler dirty-mark instead.

**Fix:** Add a one-line comment to `select_active_workspace`:

```rust
// src/app.rs:388
pub(crate) fn select_active_workspace(&mut self, index: usize) {
    // D-22: per-session anchored gen — cross-workspace highlight is meaningless.
    // Note: unlike `set_active_tab`, this does NOT unconditionally mark_dirty —
    // upstream call sites (events.rs handle_key/handle_click,
    // workspace::switch_project) already mark_dirty for the originating event.
    self.clear_selection();
    self.active_workspace_idx = Some(index);
    self.right_list.select(None);
}
```

### NIT-02: `materialize_selection_text` silently returns empty string on parser-lock failure or no session

**File:** `src/app.rs:512-526`
**Issue:** Three independent failure modes (`active_sessions().get(...)` =
None; `session.parser.try_read()` = Err) all collapse to returning
`String::new()`. Callers cannot distinguish "no text in selection" from
"could not read parser." Caller `copy_selection_to_clipboard` then
`trim_end().is_empty()`-guards and silently no-ops; caller mouse-up Up handler
stores the empty string into the snapshot (see MINOR-01). Logging or
returning `Option<String>` would help diagnostics.

**Fix:** Either log the failure path:

```rust
let Ok(parser) = session.parser.try_read() else {
    tracing::trace!("materialize_selection_text: parser write-locked, returning empty");
    return String::new();
};
```

Or change the return type to `Option<String>` and let callers pattern-match
None vs `Some("")`. The trace-only fix is the lowest-risk addition.

### NIT-03: `App::inject_test_session` carries `#[allow(dead_code)]` despite being exercised by selection_tests

**File:** `src/app.rs:867-941`
**Issue:** Line 878 has `#[allow(dead_code)]` on `inject_test_session`. The
function IS called from `src/selection_tests.rs` (lines 324, 364, 459, 512).
The attribute is likely a holdover from an earlier checkpoint where the seam
existed before any test invoked it (Plan 06-01 TDD-RED gate per STATE.md
2026-04-25). Now redundant — `cargo test` should not emit a dead-code warning
because the function has live test-build callers.

**Fix:** Remove the attribute and verify with `cargo test`:

```rust
// src/app.rs:878 — drop the allow.
#[cfg(test)]
impl App {
    /// ... (doc comment unchanged)
    pub(crate) fn inject_test_session(
```

If a future refactor removes all callers, the warning resurfaces and the
attribute can be re-added at that time. Keeping unused `#[allow]` lints
masks future regressions of the same kind.

### NIT-04: SelectionState `text` snapshot is bounded but has no documented size invariant

**File:** `src/app.rs:43-49`
**Issue:** The doc comment on `SelectionState.text` explains why the snapshot
exists (D-02 + scroll-off survival) but does not state the size bound. In
practice the snapshot is bounded by `vt100::Screen::contents_between(sr, sc,
er, ec)` which iterates `visible_rows()` (RESEARCH §Q2) — at most rows × cols
× UTF-8 max width ≈ 24 × 80 × 4 = ~7.6 KiB for a default-size pane, or up to
~50 KiB on a maximized 50-row × 250-col terminal. One snapshot lives at a time
(replaced on each new selection or cleared on tab/workspace switch). No
unbounded growth.

A defensive size-clamp is unnecessary, but a doc-comment statement of the
invariant prevents a future contributor from accidentally widening the
snapshot to scrollback (which IS unbounded — vt100's scrollback is configured
at 1000 rows in `pty/session.rs:66`).

**Fix:** Extend the doc comment:

```rust
/// Snapshot of the selected text captured at mouse-up so cmd+c can
/// re-copy the same content even after the originally-selected rows
/// have scrolled off the visible area (RESEARCH §Q2). vt100's
/// `contents_between` only iterates visible rows, so without this
/// snapshot a post-scroll-off cmd+c would yield only the surviving
/// portion.
///
/// Size invariant: bounded by visible-area dimensions
/// (≤ rows × cols × 4 bytes ≈ 8 KiB for a 24×80 pane, ≤ 50 KiB at
/// realistic max). DO NOT widen to include scrollback rows — the
/// vt100 scrollback (1000 rows, see `pty/session.rs:66`) would push
/// per-snapshot cost into the MiB range.
pub text: Option<String>,
```

### NIT-05: `row_hash` uses non-stable DefaultHasher; cross-process / cross-restart determinism not guaranteed

**File:** `src/pty/session.rs:231-240`
**Issue:** `DefaultHasher::new()` returns a SipHash-1-3 instance with a
process-randomized seed. The hash is only ever compared within the same
process (before vs after a single `parser.process()` call inside one PTY
reader thread iteration), so determinism is fine. But the choice deserves a
brief comment: a future contributor seeing `DefaultHasher` in a "scroll
detection" path might wonder if persistence or cross-thread comparison was
intended.

**Fix:** Add a one-line clarification:

```rust
/// Hash the contents of one visible row in the vt100 screen. Used by the
/// PTY reader thread's SCROLLBACK-LEN heuristic (RESEARCH §Q1) to detect
/// whether the top row's text changed across a `parser.process()` call —
/// a strong signal that a vertical scroll happened. Cost is O(cols),
/// negligible compared to the vt100 parse itself.
///
/// `DefaultHasher` is process-local (SipHash with a randomized seed) —
/// fine here because we only compare two hashes back-to-back inside the
/// same thread iteration. Do NOT persist the result or compare across
/// processes.
fn row_hash(screen: &vt100::Screen, row: u16, cols: u16) -> u64 {
```

---

## Verification Notes

### Atomic ordering audit (Special Focus #1)

| Site | Operation | Ordering | Justification |
|------|-----------|----------|--------------|
| `pty/session.rs:115` | `fetch_add(1)` | Relaxed | Pure version-stamp counter; no other shared memory must be observed in lock-step |
| `app.rs:538` (`active_scroll_generation`) | `load` | Relaxed | Read for snapshotting at drag-anchor / mouse-up; eventually-consistent translation tolerates stale reads |
| `app.rs:704` (`select_line_at`) | `load` | Relaxed | Same as above |
| `ui/draw.rs:77` | `load` | Relaxed | Per-frame snapshot for render-time translation; consistent within frame |

All four sites use Relaxed. No SeqCst / Acquire / Release needed because the
counter does not coordinate access to other shared memory — it IS the
synchronization point and the value itself is the entire signal.

### Borrow-checker audit (Special Focus #2)

The `compute-read-only-first, then &mut borrow` pattern is correctly applied
in three call sites:
- `App::select_word_at` (app.rs:644-672) — `word_boundary_at` (`&self`),
  `active_scroll_generation` (`&self`), and `materialize_selection_text`
  (`&self`) are all evaluated before the single `self.selection = Some(...)`
  write at line 667.
- `App::select_line_at` (app.rs:677-725) — parser read guard scoped inside a
  block at lines 679-707 so it drops before the `self.selection = Some(...)`
  write at line 720.
- `App::extend_selection_to` (app.rs:734-754) — clones the existing
  selection (line 737) so the materialize call at line 748 can run via
  `&self` without conflicting with the eventual `&mut self.selection`
  write at line 752.

No borrow-checker hazard observed. The existing patterns are the canonical
way to thread these reads through Rust's aliasing rules.

### Test seam audit (Special Focus #4)

| Seam | File | Line | Gating | Production binary impact |
|------|------|------|--------|--------------------------|
| `App::inject_test_session` | `src/app.rs` | 879-940 | `#[cfg(test)] impl App { ... }` | Excluded from release |
| `PtyManager::insert_for_test` | `src/pty/manager.rs` | 128-138 | `#[cfg(test)]` on the fn | Excluded from release |
| `ui::terminal::render_with_selection_for_test` | `src/ui/terminal.rs` | 209-252 | `#[cfg(test)]` on the fn | Excluded from release |

All three are properly gated. Confirmed by direct re-read of each guard.

### `mark_dirty` discipline audit (Special Focus #5)

Selection-mutating events (with mark_dirty site):
- `Drag(Left)` create/extend — `events.rs:68`
- `Up(Left)` finalize — `events.rs:75` (empty-clear path), `events.rs:84`
  (snapshot-and-keep path)
- `Down(Left)` clear — `events.rs:117`
- Shift+`Down(Left)` extend — `app.rs:753` via `extend_selection_to`
- Double-click word — `app.rs:671` via `select_word_at`
- Triple-click line — `app.rs:724` via `select_line_at`
- `Esc` clear — `events.rs:402`
- Tab switch — `app.rs:405` via `set_active_tab`
- Workspace switch — `app.rs:213` via `clear_selection` (only if a selection
  existed; see NIT-01 for the asymmetry note)

Every selection mutation has a paired `mark_dirty` call. Discipline holds.

---

_Reviewed: 2026-04-24_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
