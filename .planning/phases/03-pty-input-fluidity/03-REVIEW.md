---
phase: 03-pty-input-fluidity
reviewed: 2026-04-24T00:00:00Z
depth: standard
files_reviewed: 3
files_reviewed_list:
  - src/pty_input_tests.rs
  - src/main.rs
  - src/pty/session.rs
findings:
  critical: 0
  warning: 0
  info: 3
  total: 3
status: clean
---

# Phase 03: Code Review Report

**Reviewed:** 2026-04-24
**Depth:** standard
**Files Reviewed:** 3
**Status:** clean (info-only observations)

## Summary

Phase 3 adds three PTY-input validation tests and a doc-comment on `PtySession::write_input`. Scope is minimal:

- `src/pty_input_tests.rs` — new file, three tests (`#[test]`, two `#[tokio::test]`) covering PTY-01 (keystroke -> echo -> parser -> rendered buffer) and PTY-02 (biased select prefers input over PTY-output notify).
- `src/main.rs` — adds `#[cfg(test)] mod pty_input_tests;` declaration only.
- `src/pty/session.rs` — adds a doc-comment above `write_input` explaining the synchronous-by-design contract. Function body unchanged.

No bugs, security issues, or quality regressions. All findings are informational. The tests correctly exercise Phase 2 primitives (synchronous `write_input`, `spawn_with_notify`, biased `select!`), assertions are specific with good failure messages, and the doc-comment is accurate and useful (correctly flags the "do not move onto `tokio::task::spawn`" trap and references the research doc).

`.unwrap()` calls on `RwLock::read()`/`Mutex::lock()` in test code are acceptable — panic-on-poison surfaces as a test failure, which is the desired behavior in tests. Not flagged.

No Critical or Warning findings. Info-level observations follow.

## Info

### IN-01: Poll-based deadline may flake under heavy CI load

**File:** `src/pty_input_tests.rs:30-38`, `src/pty_input_tests.rs:76-84`
**Issue:** Both PTY tests poll the parser buffer for up to 2 seconds waiting for `/bin/cat` to start, receive input, echo via the PTY line discipline, and for the reader thread to process it. 2s is generous for local dev on Apple Silicon, but a heavily loaded GitHub Actions `macos-latest` runner under contention could plausibly miss the window. This is a latent flake risk, not a current bug.
**Fix:** If the tests flake in CI, bump the deadline to 5s (matching the `spawn_echo` / `eof_exit_code` timeouts already used in `src/pty/session.rs:215, 237`) for consistency:
```rust
let deadline = std::time::Instant::now() + Duration::from_secs(5);
```
No change required now — record this as the first knob to turn if flakes appear.

### IN-02: `let _ = &notify;` pattern is unusual; comment carries the load

**File:** `src/pty_input_tests.rs:75`
**Issue:** `let _ = &notify;` exists purely to keep the `Arc<Notify>` in scope and visibly exercise the `spawn_with_notify` code path. The intent is explained clearly in the preceding 10-line block comment (lines 65-74), but the pattern will look odd to readers who skim. A `#[allow(...)]` or an explicit `drop(notify)` at end-of-test would be no clearer.
**Fix:** Leave as-is. The comment justifies it. Optionally, rename the binding to `_notify_wiring_guard` to make the intent self-evident without needing the comment:
```rust
let _notify_wiring_guard = notify; // keeps spawn_with_notify path exercised; see note above
```
Low priority — current form is acceptable.

### IN-03: `typing_appears_in_buffer` never actually asserts the `Notify` fires

**File:** `src/pty_input_tests.rs:47-116`
**Issue:** The test is named to suggest a round-trip through the notify-driven event loop, but the block comment on lines 65-75 correctly acknowledges the 8ms throttle in `session.rs:98` may coalesce the single-byte echo into a silent update, so the test intentionally does NOT await `notify.notified()` — it polls the parser buffer instead. This is a sound design choice (the parser IS the source of truth for PTY-01), but a future reader may add a `notify.notified().await` "to make the test stricter" and reintroduce flakes. The comment already warns against this; documenting it here for traceability.
**Fix:** No code change. Consider renaming the test to `typing_round_trips_pty_to_rendered_buffer` to drop the implicit promise about notify behavior, if a rename is cheap:
```rust
async fn typing_round_trips_pty_to_rendered_buffer() { ... }
```
Optional.

---

_Reviewed: 2026-04-24_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
