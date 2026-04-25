---
phase: 06-text-selection
plan: 02
subsystem: pty
tags:
  - selection
  - pty
  - scroll
  - rust
  - tdd
requirements:
  - SEL-04
dependency-graph:
  requires:
    - "PtySession + spawn_with_notify (Phase 1 / pty subsystem)"
    - "vt100::Screen API (size, cursor_position, cell, contents)"
  provides:
    - "PtySession.scroll_generation: Arc<AtomicU64> — read by Plan 06-03 (drag anchor) and Plan 06-05 (render translation)"
    - "row_hash(screen, row, cols) helper — local to src/pty/session.rs"
  affects:
    - "PtySession public surface (new pub field scroll_generation)"
    - "PTY reader thread inner loop (now captures cursor_position + row_hash before each parser.process)"
tech-stack:
  added: []
  patterns:
    - "SCROLLBACK-LEN heuristic per RESEARCH §Q1: cursor-at-bottom + top-row-hash-changed → infer scroll"
    - "Atomic counter passed across thread boundary via Arc::clone (mirrors parser_clone / status_clone / last_output_clone pattern)"
    - "DefaultHasher over screen.cell.contents() for cheap visible-row fingerprint (O(cols) per PTY read)"
key-files:
  created: []
  modified:
    - src/pty/session.rs
    - src/selection_tests.rs
decisions:
  - "Scroll detection lives in the PTY reader thread (where parser.process is called), not on render — keeps render path stateless and ensures the counter increments before any render observes the post-scroll screen"
  - "Ordering::Relaxed is sufficient — the render path tolerates a slightly-stale snapshot (T-06-04 mitigation)"
  - "row_hash is a free function at module level (not impl PtySession) — it operates on a borrowed vt100::Screen and has no per-session state"
  - "Renamed local var `gen` → `gen_count` in the test because Rust 2024 reserves `gen` as a keyword (deviation Rule 3)"
metrics:
  duration: 8m
  tasks: 2
  files: 2
  completed_date: 2026-04-25
---

# Phase 6 Plan 2: PTY Scroll-Generation Counter Summary

Added `scroll_generation: Arc<AtomicU64>` to `PtySession` and wrapped `parser.process()` in the PTY reader thread with the SCROLLBACK-LEN heuristic (RESEARCH §Q1) — produces the per-session scroll counter that Plans 06-03 (drag anchor) and 06-05 (render translation) depend on for selection-stability (SEL-04).

## What Was Built

### `PtySession.scroll_generation` field (`src/pty/session.rs:30`)

```rust
pub struct PtySession {
    pub id: u64,
    pub parser: Arc<RwLock<vt100::Parser>>,
    // ...
    pub last_output: Arc<Mutex<std::time::Instant>>,
    pub scroll_generation: Arc<std::sync::atomic::AtomicU64>,  // NEW
}
```

Initialized to 0 per session. Cloned across the thread boundary alongside the existing `parser_clone` / `status_clone` / `last_output_clone` (`src/pty/session.rs:75-76`).

### SCROLLBACK-LEN heuristic in reader thread (`src/pty/session.rs:91-118`)

The `Ok(n) =>` arm now captures cursor row + top-row hash **before** `parser.process(bytes)`, then re-hashes after, and increments the counter when both conditions hold:

```rust
let (rows, cols) = parser.screen().size();
let before_cursor_row = parser.screen().cursor_position().0;
let before_top_hash = row_hash(parser.screen(), 0, cols);
parser.process(&buf[..n]);
let after_top_hash = row_hash(parser.screen(), 0, cols);
let scrolled = before_cursor_row >= rows.saturating_sub(1)
    && before_top_hash != after_top_hash;
if scrolled {
    scroll_gen_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
```

The pre-existing `last_output_clone` update + 8ms `output_notify` throttle (Phase 2 dirty-wake + Phase 3 render throttle) are preserved verbatim — verified by grep + test pass.

### `row_hash` free function (`src/pty/session.rs:226-237`)

```rust
fn row_hash(screen: &vt100::Screen, row: u16, cols: u16) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for col in 0..cols {
        if let Some(cell) = screen.cell(row, col) {
            cell.contents().hash(&mut h);
        }
    }
    h.finish()
}
```

O(cols) ≈ ≤500 hash steps per 16KB PTY read — negligible compared to vt100 parse (T-06-03 disposition: accept).

### Integration test (`src/selection_tests.rs`, +37 lines)

`scroll_generation_increments_on_vertical_scroll` (`#[tokio::test]`):

- Spawns `/bin/cat` at 24×80.
- Feeds 30 newlined lines via `write_input` (overflows visible area).
- Polls `session.scroll_generation` for up to 500ms.
- Asserts counter > 0.

Test passes in ~30ms locally.

## TDD Gate Compliance

Plan tasks are both `tdd="true"`. Gates verified in git log:

1. **RED gate:** `12ce532 test(06-02): add failing scroll_generation integration test` — test fails to compile against unknown field `scroll_generation`.
2. **GREEN gate:** `b370434 feat(06-02): scroll_generation counter via SCROLLBACK-LEN heuristic` — adds the field, heuristic, and `row_hash`; test passes; full suite (114 tests) green.
3. No REFACTOR commit needed — implementation matches the plan's canonical shape one-shot.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Rust 2024 reserves `gen` as a keyword**

- **Found during:** Task 1 RED build.
- **Issue:** Plan's exact test body used `let mut gen = 0u64;` and `assert!(gen > 0, ...)`. Rust 2024 (edition declared in Cargo.toml) reserves `gen` for generator syntax — compilation fails with `expected identifier, found reserved keyword`.
- **Fix:** Renamed the local to `gen_count`. Same semantics; assert message updated. The plan's explicit test body was fixed forward — this is a Rule 3 blocking-issue resolution, not a plan deviation in semantics.
- **Files modified:** `src/selection_tests.rs`
- **Commit:** `12ce532`

**2. [Rule 3 - Stylistic] Single-line `fetch_add` to satisfy acceptance grep**

- **Found during:** Task 2 acceptance-criteria verification.
- **Issue:** First write of the increment used a wrapped two-line form `scroll_gen_clone\n.fetch_add(...)`. The plan's acceptance criterion `grep -n 'scroll_gen_clone.fetch_add' src/pty/session.rs` requires a single-line match. Test still passed either way.
- **Fix:** Reformatted to a single-line statement. Pure cosmetic change; semantics identical.
- **Files modified:** `src/pty/session.rs` (within Task 2 commit)
- **Commit:** `b370434`

No other deviations — the plan executed close to verbatim.

## Threat Model Compliance

| Threat ID | Disposition | Implementation |
| --- | --- | --- |
| T-06-03 (DoS via row_hash per read) | accept | row_hash is O(cols) ≤ 500 hash steps; runs once per 16KB PTY chunk; negligible vs vt100 parse cost. |
| T-06-04 (atomic counter integrity) | mitigate | `Arc<AtomicU64>` with `Ordering::Relaxed` on both store (fetch_add) and load (test reads). Reader/writer both observe a monotonic snapshot — sufficient because consumers tolerate a slightly-stale value. |

No new threat surface introduced. The PTY-bytes → vt100 → counter chain is already-trusted input (PTY child output).

## Acceptance Criteria

| Criterion | Result |
| --- | --- |
| `cargo build` (test profile) | PASS |
| `cargo test scroll_generation_increments_on_vertical_scroll` (1 test) | PASS (~30ms) |
| `cargo test` full suite (114 tests) | PASS (114 passed, 0 failed) |
| `pub scroll_generation: Arc<std::sync::atomic::AtomicU64>` in `src/pty/session.rs` | 1 match (line 30) |
| `fn row_hash` in `src/pty/session.rs` | 1 match (line 232) |
| `before_cursor_row >= rows.saturating_sub(1)` | 1 match (line 112) |
| `scroll_gen_clone.fetch_add` | 1 match (line 115) |
| `let mut session = PtySession::spawn` in `src/selection_tests.rs` | 1 match (test body) |
| `session.scroll_generation.load(Ordering::Relaxed)` in `src/selection_tests.rs` | 1 match |
| `*last_output_clone.lock().unwrap() = ...` preserved | 1 match (Phase 2 dirty-wake intact) |
| `output_notify` 8ms throttle preserved | confirmed (Phase 3 render throttle intact) |

All criteria pass.

## Self-Check: PASSED

- File `src/pty/session.rs` (modified): FOUND
- File `src/selection_tests.rs` (modified): FOUND
- File `.planning/phases/06-text-selection/06-02-SUMMARY.md` (created): FOUND
- Commit 12ce532 (Task 1 — TDD RED): FOUND
- Commit b370434 (Task 2 — TDD GREEN): FOUND
