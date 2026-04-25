---
phase: 06-text-selection
plan: 05
subsystem: ui
tags:
  - selection
  - render
  - ratatui
  - rust
  - tdd
requirements:
  - SEL-01
  - SEL-04
dependency-graph:
  requires:
    - "Plan 06-01: SelectionState shape (start_gen, end_gen, text)"
    - "Plan 06-02: PtySession.scroll_generation: Arc<AtomicU64>"
    - "Plan 06-03: handle_mouse populates SelectionState with anchored gens"
  provides:
    - "ui::terminal::render(..., current_gen: u64) — extended signature consumed by ui::draw"
    - "render_with_selection_for_test #[cfg(test)] shim — exercised by render tests; not part of production surface"
    - "REVERSED-XOR highlight body — every selected cell on the visible buffer; D-20 + D-21 satisfied by single Modifier::REVERSED toggle"
    - "Anchored→visible row translation with clip-at-top — start clipped to row 0 when translated < 0; entire selection skipped when end translated < 0 (D-08)"
  affects:
    - "ui::terminal::render public signature gains current_gen: u64 (last parameter)"
    - "ui::draw caller now reads PtySession.scroll_generation per-frame to compute current_gen"
tech-stack:
  added: []
  patterns:
    - "ratatui Modifier::REVERSED XOR via cell.modifier.toggle — pure XOR replaces fg/bg swap; terminal emulators implement REVERSED as the actual fg<->bg swap so Color::Reset edge cases are handled by the protocol layer (RESEARCH §Q7 / OQ-4)"
    - "i64 cast for saturating coord translation — (row as i64) - (delta as i64) avoids u16 underflow per T-06-11; .max(0) as u16 clips at top"
    - "#[cfg(test)] render shim mirrors production body — avoids constructing the full sessions/tabs argument fan-out for buffer-level assertions"
key-files:
  created: []
  modified:
    - src/ui/terminal.rs
    - src/ui/draw.rs
    - src/selection_tests.rs
decisions:
  - "Adopted RESEARCH §Q7 OQ-4 recommendation: pure Modifier::REVERSED XOR replaces the literal D-20 fg/bg swap. The simpler form satisfies both D-20 (terminal protocol implements REVERSED as the actual swap) and D-21 (XOR un-reverses already-reversed cells, producing visual contrast against vt100 reverse-video). Color::Reset is handled correctly by the terminal emulator at draw time."
  - "current_gen passed as the LAST parameter to render(...) — minimizes diff to the existing 9-argument signature; #[allow(clippy::too_many_arguments)] already present, so adding a 10th parameter incurs no new lint suppression."
  - "Anchored-coord translation lives BEFORE the row/column iteration loop. The full block is gated on `er_translated >= 0` (D-08 fully-scrolled-off short-circuit) — this preserves the prior `if let Some(sel)` / `if !sel.is_empty()` precedent and keeps the diff minimal."
  - "render_with_selection_for_test is `pub(crate)` not `pub` — only the in-crate selection_tests module needs it. Production callers go through render() with the real current_gen snapshot from PtySession.scroll_generation."
metrics:
  duration: 3m
  tasks: 2
  files: 3
  completed_date: 2026-04-25
---

# Phase 6 Plan 5: Highlight Render — REVERSED XOR + Anchored-Coord Translation Summary

Replaced the gold-accent highlight body in `src/ui/terminal.rs` with `cell.modifier.toggle(Modifier::REVERSED)` (D-20 + D-21), added anchored-coord translation per D-06 / D-08 so endpoints survive scroll, extended `render(..., current_gen: u64)` signature, and updated the sole caller in `src/ui/draw.rs` to load `session.scroll_generation` per-frame. Three new TDD render tests (REVERSED-toggle baseline, REVERSED-XOR-un-reverses, clip-at-top-on-scroll) drive a `ratatui::backend::TestBackend`. Full suite: 129 tests, all green; zero warnings.

## What Was Built

### REVERSED-XOR highlight body (`src/ui/terminal.rs:156-198`)

```rust
if let Some(sel) = selection {
    if !sel.is_empty() {
        let ((sc_raw, sr_raw), (ec_raw, er_raw)) = sel.normalized();
        // D-06: translate anchored rows to current-screen rows.
        let start_delta = current_gen.saturating_sub(sel.start_gen);
        let end_delta = sel
            .end_gen
            .map(|g| current_gen.saturating_sub(g))
            .unwrap_or(0);
        let sr_translated = (sr_raw as i64) - (start_delta as i64);
        let er_translated = (er_raw as i64) - (end_delta as i64);
        // D-08: fully-scrolled-off => render nothing; SelectionState stays in app state.
        if er_translated >= 0 {
            let sr = sr_translated.max(0) as u16;
            let er = er_translated.max(0) as u16;
            // D-08: clip start column to 0 if start row was clipped.
            let sc = if sr_translated < 0 { 0 } else { sc_raw };
            let ec = ec_raw;
            let buf = frame.buffer_mut();
            for row in sr..=er {
                if row >= inner.height { break; }
                let c_start = if row == sr { sc } else { 0 };
                let c_end = if row == er { ec } else { inner.width.saturating_sub(1) };
                for col in c_start..=c_end {
                    if col >= inner.width { break; }
                    if let Some(cell) = buf.cell_mut((inner.x + col, inner.y + row)) {
                        // D-20 + D-21: XOR REVERSED — already-reversed cells
                        // un-reverse, making the highlight visually distinct
                        // from surrounding vt100 reverse-video.
                        cell.modifier.toggle(Modifier::REVERSED);
                    }
                }
            }
        }
    }
}
```

Replaces the prior `cell.set_bg(theme::ACCENT_GOLD); cell.set_fg(theme::BG_SURFACE);` body. The single-line toggle relies on the terminal emulator's protocol-level swap of fg↔bg when REVERSED is set, which handles `Color::Reset` correctly without a fallback branch (RESEARCH §Q7 OQ-4).

### Render signature extension (`src/ui/terminal.rs:38-49`)

```rust
pub fn render(
    frame: &mut Frame,
    area: Rect,
    sessions: &[(u32, &PtySession)],
    tab_specs: &[TabSpec],
    active_tab: usize,
    mode: InputMode,
    focused: bool,
    workspace_info: Option<&WorkspaceInfo>,
    selection: Option<&SelectionState>,
    current_gen: u64,                    // NEW (last parameter)
) {
```

### Caller update (`src/ui/draw.rs:67-90`)

```rust
let current_gen = active_sessions
    .get(active_tab)
    .map(|(_, s)| s.scroll_generation.load(std::sync::atomic::Ordering::Relaxed))
    .unwrap_or(0);

terminal::render(
    frame,
    panes.terminal,
    &active_sessions,
    /* tab_specs */ ...,
    active_tab,
    app.mode,
    true,
    ws_info.as_ref(),
    app.selection.as_ref(),
    current_gen,
);
```

`scroll_generation.load(Relaxed)` matches the per-cell hot-path access pattern from Plan 02 — the renderer tolerates a slightly-stale snapshot (T-06-04 disposition).

### `render_with_selection_for_test` shim (`src/ui/terminal.rs:200-243`)

`#[cfg(test)] pub(crate)` — never compiled into release binaries. Mirrors the production translation + toggle body verbatim, so the render tests assert the same code path the production renderer executes (modulo the surrounding tab-bar / pseudo-terminal widget composition, which the tests don't need).

### Tests (`src/selection_tests.rs`, +194 lines, 3 tests)

All three drive a `ratatui::backend::TestBackend` at 80×24 via `Terminal::new`:

| Test | Validates |
|------|-----------|
| `selection_highlights_cells_with_reversed_modifier` | D-20 baseline: cells (5..=10, 3) have REVERSED toggled on; cell (0, 3) outside selection does NOT. |
| `already_reversed_cell_un_reverses_under_selection` | D-21: pre-populating cells (7..=9, 2) with `Modifier::REVERSED` then running the highlight pass REMOVES the flag (XOR). |
| `selection_clips_at_visible_top_when_scrolled` | D-08: `sel.start_row=2, end_row=5, gens=0`; `current_gen=3`; rows 0..=2 of the column span are REVERSED (clipped at top); rows 3..=5 are NOT. |

All three pass on first run after Task 2.

## TDD Gate Compliance

Plan tasks are both `tdd="true"`. Gates verified in git log:

1. **RED gate:** `58c8548 test(06-05): add 3 failing render tests for REVERSED highlight + clip-at-top (TDD RED)` — `cargo build --bin martins --tests` produces 3 `cannot find function render_with_selection_for_test` errors.
2. **GREEN gate:** `fde0658 feat(06-05): REVERSED-XOR highlight + anchored-coord translation (TDD GREEN)` — adds the production body + signature extension + caller update + `#[cfg(test)]` test shim. All 3 new tests pass; full suite 129/129 green.
3. No REFACTOR commit needed — implementation matches the plan's canonical shape on first write.

## Deviations from Plan

None. The plan executed verbatim:

- All 3 tests landed exactly as the plan's `<action>` body specified.
- Production highlight body replaced with single-line `cell.modifier.toggle(Modifier::REVERSED)` (the simpler OQ-4-recommended form, which the plan also explicitly endorses on lines 240-248).
- Anchored-coord translation matches the plan's snippet exactly (i64 cast, `er_translated < 0` short-circuit, clip-start-col-to-0).
- `current_gen: u64` added as the LAST parameter (plan-prescribed).
- Sole caller in `src/ui/draw.rs:72` updated; `grep -rn 'ui::terminal::render(\|terminal::render(' src/` confirms no other production call sites.

The plan's two existing tests in `src/ui/terminal.rs:182-214` were updated to pass `0` as the new `current_gen` argument (mechanical fix, no semantic change — those tests render an empty pane and don't exercise selection).

## Threat Model Compliance

| Threat ID | Disposition | Implementation |
|-----------|-------------|----------------|
| T-06-10 (DoS via per-cell modifier toggle) | accept | Bounded by selection area (≤ rows × cols within `inner`). The added work per highlighted cell is one bitflag XOR — negligible vs ratatui's existing per-frame buffer diff. |
| T-06-11 (Tampering via integer underflow in coord translation) | mitigate | `current_gen.saturating_sub(sel.start_gen)` (u64 — saturates to 0); explicit `(row as i64) - (delta as i64)` cast avoids u16 underflow before the `.max(0) as u16` re-cast; explicit `er_translated < 0` and `sr_translated < 0` checks gate negative-row paths. No panic surface. |

No new threat surface emerged.

## Acceptance Criteria

| Criterion | Result |
|-----------|--------|
| `cargo build --bin martins --tests` exits 0 (no warnings) | PASS |
| `cargo test --bin martins -- selection_tests::selection_highlights_cells_with_reversed_modifier selection_tests::already_reversed_cell_un_reverses_under_selection selection_tests::selection_clips_at_visible_top_when_scrolled` | PASS (3 passed) |
| `cargo test --bin martins` full suite | PASS (129 passed, 0 failed) |
| `cargo build --release --bin martins` | PASS |
| `grep -cE '^\s*fn (selection_highlights_cells_with_reversed_modifier\|already_reversed_cell_un_reverses_under_selection\|selection_clips_at_visible_top_when_scrolled)\b' src/selection_tests.rs` | 3 |
| `grep -c TestBackend src/selection_tests.rs` | 7 (≥1 required) |
| `grep -c 'Modifier::REVERSED' src/selection_tests.rs` | 10 (≥3 required) |
| `grep -c current_gen src/selection_tests.rs` | 11 (≥3 required) |
| `theme::ACCENT_GOLD` in highlight body (`src/ui/terminal.rs` lines 156-200) | 0 (gold-accent body removed) |
| `grep -c 'cell.modifier.toggle(Modifier::REVERSED)' src/ui/terminal.rs` | 2 (production + test shim) |
| `grep -c 'saturating_sub(sel.start_gen)' src/ui/terminal.rs` | 2 (production + test shim) |
| `grep -c 'er_translated < 0' src/ui/terminal.rs` | 1 (test shim short-circuit; production uses inverted `er_translated >= 0` gate) |
| `grep -c 'current_gen: u64' src/ui/terminal.rs` | 2 (render signature + test shim signature) |
| `grep -c 'scroll_generation.load' src/ui/draw.rs` | 1 |
| caller passes `current_gen` argument | YES |
| number of production `terminal::render(` call sites | 1 (`src/ui/draw.rs:80`) |

All criteria pass.

## Self-Check: PASSED

- File `src/ui/terminal.rs` (modified): FOUND
- File `src/ui/draw.rs` (modified): FOUND
- File `src/selection_tests.rs` (modified): FOUND
- File `.planning/phases/06-text-selection/06-05-SUMMARY.md` (created): FOUND
- Commit `58c8548` (Task 1 — TDD RED): FOUND
- Commit `fde0658` (Task 2 — TDD GREEN): FOUND
