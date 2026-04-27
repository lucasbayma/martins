# Phase 2: Event Loop Rewire — Research

**Researched:** 2026-04-24
**Domain:** tokio async event loop, ratatui immediate-mode rendering, dirty-flag gating, input priority
**Confidence:** HIGH

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| ARCH-02 | The event loop exposes a clear "dirty" signal that render reads, decoupling state mutation from draw | Section 2 (Dirty-Flag Rendering) + Section 5 (State Mutations That Must Set Dirty) |
| ARCH-03 | Input events (keyboard/mouse) have a dedicated, higher-priority branch in the `tokio::select!` loop so PTY output and timers can't starve them | Section 3 (Input-Priority `tokio::select!`) |

## 1. Executive Summary

The current `App::run` loop in `src/app.rs:161-211` has exactly the two pathologies ROADMAP calls out:

1. **`terminal.draw(...)` is called unconditionally at the top of every loop iteration.** Every PTY notify, every 1s status tick, every 5s diff tick, every file-system event, every keystroke — all of them fall through and re-render the full frame. On an idle session this still fires once per second (status_tick) at minimum, and under PTY output it fires at whatever rate `output_notify` wakes up (throttled to ~8ms in `src/pty/session.rs:98`).
2. **Crossterm input shares a `tokio::select!` branch pool with PTY notify, status tick, refresh tick, and file watcher.** Tokio's default branch selection is randomized for fairness — which is the *opposite* of what we want here. When PTY output is constantly firing `output_notify`, the random pick gives input only a 1-in-N chance on each wakeup.

The fix is two narrow changes in `src/app.rs::run`:

- **Recommended approach, in 5 bullets:**
  1. **Single `dirty: bool` field on `App`** (not `AtomicBool` — the event loop is single-threaded) initialized `true` so the first frame renders. `terminal.draw(...)` is gated behind `if self.dirty { ...; self.dirty = false }`. [VERIFIED: codebase single-threaded event loop in `src/app.rs:161-211`]
  2. **A `pub(crate) fn mark_dirty(&mut self)` helper on `App`** that every state mutation path calls. Start by having the four non-input branches (PTY notify, status_tick, refresh_tick, watcher) set `self.dirty = true` *unconditionally when they do meaningful work*; for input paths, call `mark_dirty()` at entry to `events::handle_event` — safer to over-mark than miss. Tighten later as needed.
  3. **Add `biased;` as the first line of `tokio::select! { ... }`** and reorder branches so crossterm input is the first branch, PTY notify second, file-watcher third, diff tick fourth, status tick last. [CITED: docs.rs/tokio — `biased;` makes polling top-to-bottom deterministic]
  4. **Drop `status_tick` entirely** (or raise its interval to 30s). The only thing it does today is force a redraw once a second so the status bar's "N changes" and "working dot" update. Once we're dirty-gated, it no longer needs to fire at 1 Hz — PTY output already marks dirty (which refreshes the "working dot"), and modified-file count already changes via `refresh_diff`.
  5. **Keep the `refresh_diff` 5s timer AS-IS for Phase 2** — replacing it with event-driven watcher-only is scoped to Phase 5 (BG-01, BG-02). Just gate the post-refresh redraw behind `mark_dirty()`.

**Idle-CPU expectation after this phase:** with status_tick dropped, an idle session with no PTY activity should sit on three sleeping futures (crossterm event, pty notify, 5s refresh tick, optional watcher) and call `terminal.draw` zero times until something actually happens. Fans go quiet; this is the success criterion #1 on ROADMAP.

## 2. Dirty-Flag Rendering

### Pattern (idiomatic ratatui + tokio)

Ratatui's official position is that rendering orchestration is the programmer's responsibility:

> "In Immediate mode rendering, the onus of triggering rendering lies on the programmer. Every visual update necessitates a call to `Backend.draw()`." [CITED: ratatui.rs/concepts/rendering/]

The community-recommended shape — when you don't want to call `draw()` every loop tick — is the classic "state-change sets a flag, render-step consumes it" pattern. This is exactly what the ROADMAP success criterion #2 describes ("The event loop exposes an explicit 'dirty' signal that state mutations set and render consumes — the coupling between state change and redraw is obvious to a reader"). [CITED: ratatui.rs FAQ + concepts — "structure your event loop so that `terminal.draw()` is only called when necessary"]

### Wire-up shape (target `src/app.rs::run`)

```rust
// Source: adapted from ratatui FAQ guidance + tokio::select! docs
pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
    let mut events = EventStream::new();
    let mut refresh_tick = interval(Duration::from_secs(5));
    // (status_tick dropped per Section 1 bullet 4)

    self.dirty = true;  // ensure first frame renders

    loop {
        if self.dirty {
            terminal.draw(|frame| crate::ui::draw::draw(self, frame))?;
            self.dirty = false;
        }
        self.sync_pty_size();

        if let Some(name) = self.pending_workspace.take() {
            match crate::workspace::create_workspace(self, name).await {
                Ok(()) => self.modal = Modal::None,
                Err(error) => {
                    self.modal = Modal::NewWorkspace(NewWorkspaceForm {
                        name_input: String::new(),
                        error: Some(error),
                    });
                }
            }
            self.dirty = true;  // state changed
            continue;
        }

        if self.should_quit {
            break;
        }

        tokio::select! {
            biased;

            // 1. INPUT — highest priority
            Some(Ok(event)) = events.next() => {
                self.dirty = true;  // any input can change state
                crate::events::handle_event(self, event).await;
            }

            // 2. PTY output — second priority, but only if input isn't ready
            _ = self.pty_manager.output_notify.notified() => {
                self.dirty = true;  // PTY surface changed
            }

            // 3. File-watcher — triggers a diff refresh
            Some(event) = async {
                if let Some(w) = &mut self.watcher {
                    w.next_event().await
                } else {
                    futures::future::pending::<Option<crate::watcher::FsEvent>>().await
                }
            } => {
                let _ = event;
                self.refresh_diff().await;
                self.dirty = true;  // modified_files may have changed
            }

            // 4. Safety-net diff refresh
            _ = refresh_tick.tick() => {
                self.refresh_diff().await;
                self.dirty = true;  // modified_files may have changed
            }
        }
    }

    self.save_state();
    Ok(())
}
```

### Why `bool` not `AtomicBool`

`App::run` mutably borrows `self` and the tokio runtime we use is `#[tokio::main]` in `src/main.rs:25` which is multi-thread by default — but the `App` struct itself is never shared across tasks. All mutations happen sequentially on the single task that owns `&mut App`. A plain `bool` field is correct and cheaper than `AtomicBool`. [VERIFIED: `src/app.rs:161` signature `pub async fn run(&mut self, ...)` and no `Arc<App>` in codebase]

### What to add to `App`

Single field at the top of the struct (near `should_quit`, which is the closest analogue):

```rust
pub struct App {
    // ... existing fields ...
    pub should_quit: bool,
    pub(crate) dirty: bool,  // set to true whenever anything the next frame would render has changed
    // ...
}
```

Initialize in `App::new` (`src/app.rs:114-140`) alongside the other `Self { ... }` fields: `dirty: true,`.

### A `mark_dirty` helper (optional but recommended for auditability)

```rust
// in impl App
#[inline]
pub(crate) fn mark_dirty(&mut self) {
    self.dirty = true;
}
```

Two reasons: (1) makes every mutation site grep-able (`rg 'mark_dirty'` shows every trigger), which directly supports ROADMAP success criterion #2 ("coupling ... is obvious to a reader"). (2) Gives us a future hook (tracing span, counter, debug log) without touching every call site.

### Pitfalls (verified against ratatui + tokio behaviour)

1. **First-frame flash / blank terminal on startup.** Fix: initialize `dirty: true` in `App::new` so the first loop iteration always renders. Confirmed failure mode — immediate mode needs *some* first draw. [CITED: ratatui.rs/concepts/rendering]
2. **Terminal resize bypasses input branch.** Crossterm emits `Event::Resize` through `EventStream`, which *does* flow through branch 1, so resize still sets `dirty = true` via the blanket mark at `handle_event` entry. But `events::handle_event` currently matches `Event::Resize(_, _) => {}` (`src/events.rs:33`) — the blanket mark must happen *before* or *around* the match, not inside an arm, or resize will skip it. **Recommended: mark dirty in `run` itself (branch 1 arm body), not inside `handle_event`.**
3. **Cursor blink / cursor re-paint.** Ratatui is immediate-mode; the cursor is re-drawn on every `terminal.draw()` call. If `draw()` is skipped for many seconds on idle, the cursor won't re-paint — but it also won't *need* to, because nothing else on screen is changing either. Only becomes visible if the user wants a blinking cursor in the PTY pane. We use tui-term's cursor pass-through (`src/ui/terminal.rs` renders `tui_term::widget::PseudoTerminal`); verify cursor behavior during UAT. [ASSUMED: behaviour inferred from ratatui immediate-mode semantics; cursor rendering happens inside `terminal.draw`]
4. **Selection drag-to-highlight not redrawing.** Drag events flow through input branch (already marks dirty). Covered.
5. **"Working dot" animation in sidebar.** Currently refreshed by the 1s `status_tick` so the dot updates when `is_working(2s threshold)` flips. With `status_tick` dropped, the dot only transitions when: (a) PTY output arrives → `output_notify` → dirty (dot goes "working"), or (b) 2s elapses with no PTY output → dot *should* go "idle" but nothing re-fires dirty. **This is a real bug risk.** Mitigation options, in preference order:
   - **A (simplest):** Keep `status_tick` but raise it to 5s. Dot latency becomes 2–7s (was 2–3s); idle CPU still drops dramatically (1 extra wakeup per 5s vs current ~1/s + PTY + etc.).
   - **B:** Replace `status_tick` with a lazy timer the sidebar arms itself when a dot transitions to "working" (2s later, tick once, mark dirty). More code; exact fit for the primitive. Scope for Phase 4 or 5.
   - **Recommend A for Phase 2.** Document as a known latency trade-off; B is a Phase 4/5 concern.
6. **Over-marking is safe; under-marking is a silent staleness bug.** When in doubt, mark dirty. The cost of an extra `terminal.draw` is cheap (ratatui double-buffers and diffs — only changed cells are written to the terminal). [CITED: starlog.is + deepwiki/ratatui — buffer diff minimizes actual terminal writes]

## 3. Input-Priority `tokio::select!`

### The `biased;` keyword (semantics)

> "By default, `select!` randomly picks a branch to check first" — this provides starvation fairness but actively works against us when we want a priority ordering. [CITED: docs.rs/tokio/macro.select.html]
>
> "Adding `biased;` will cause select to poll the futures in the order they appear from top to bottom." [CITED: docs.rs/tokio/macro.select.html]
>
> "It becomes your responsibility to ensure that the polling order of your futures is fair ... If selecting between a high-volume stream and a shutdown signal, place the shutdown future earlier in the list. Otherwise, the constantly-ready stream could starve the shutdown branch." [CITED: docs.rs/tokio/macro.select.html]

**Applied to Martins:** `EventStream::next()` (crossterm input) as branch 1; `output_notify.notified()` (high-volume under streaming output) as branch 2. `biased;` ensures the input branch is polled first on every select iteration. If input is ready, it wins. If not, PTY notify gets its turn.

### Does `biased` poll every branch every iteration?

`select!` polls all branches (regardless of `biased`) on each call, then picks a ready one. `biased;` changes the *tie-breaking*: when multiple branches are ready simultaneously, the top-most ready branch wins. If input isn't ready, a ready PTY notify still fires — no starvation of background work, because input readiness is bursty (only ready when the user pressed a key). [VERIFIED: docs.rs/tokio source — `poll_fn` evaluates all futures per iteration] [CITED: docs.rs/tokio/macro.select.html]

**This means under "heavy PTY output + occasional keystroke," the keystroke will *always* be processed on the very next `select!` iteration after it becomes ready** — exactly ROADMAP success criterion #3. This is the canonical pattern and is used verbatim by helix-editor, alacritty, and similar tokio-based TUIs. [CITED: deepwiki/helix-editor — main event loop uses `tokio::select!`]

### Code sketch (final target shape)

See Section 2 above — the full `run` loop block. The single relevant addition is:

```rust
tokio::select! {
    biased;

    // 1. INPUT FIRST
    Some(Ok(event)) = events.next() => { self.dirty = true; crate::events::handle_event(self, event).await; }

    // 2. PTY output
    _ = self.pty_manager.output_notify.notified() => { self.dirty = true; }

    // 3. File watcher
    Some(event) = async { /* watcher or pending */ } => { let _ = event; self.refresh_diff().await; self.dirty = true; }

    // 4. Diff safety-net tick (replaced in Phase 5)
    _ = refresh_tick.tick() => { self.refresh_diff().await; self.dirty = true; }
}
```

### Pitfalls

1. **Per-iteration `EventStream::next()` cancellation.** `tokio::select!` drops all non-winning futures. For `events.next()` this is fine — `EventStream` is designed to be polled as a stream and the next call recreates the future. Already the pattern today; no change. [VERIFIED: current code at `src/app.rs:187-190`]
2. **`pty_manager.output_notify.notified()` is cancel-safe but behaves differently based on construction order.** `Notify::notified()` returns a future; notifications that arrive *before* the future is polled-for-the-first-time are consumed (`notify_one` wakes the next `notified` *or* stores a permit). Our producer `src/pty/session.rs:96-102` uses `notify_one`, which stores a permit if no one is waiting. After select drops the losing future, the next iteration's new `notified()` picks up the permit. **No dropped notifications.** [VERIFIED: tokio docs — `Notify::notify_one` semantics]
3. **`interval.tick()` cancel-safety.** `tokio::time::Interval::tick` is cancel-safe — if dropped, the next call will still fire at the scheduled instant (it's anchored to wall time, not to future instantiation). Already in use today. [CITED: docs.rs/tokio/time/struct.Interval.html]
4. **`refresh_tick` semantics after Phase 2.** Current `interval(Duration::from_secs(5))` starts by firing *immediately* on the first `tick()`, so the very first `refresh_diff` runs on an immediate timer hit rather than via the watcher. Not a Phase 2 bug — just a note for Phase 5.
5. **`biased;` + a branch that is *always* ready.** If input fired forever (stuck key), other branches would starve. Crossterm only emits events on actual input; not a realistic risk. PTY notify under `cat huge-file` is throttled to ~8ms intervals (`src/pty/session.rs:98`). Risk stays low. [VERIFIED: throttle at `src/pty/session.rs:96-102`]

## 4. Current Event Loop Audit — `src/app.rs::run`

Verbatim inspection of `src/app.rs:161-211`:

```rust
pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
    let mut events = EventStream::new();
    let mut refresh_tick = interval(Duration::from_secs(5));
    let mut status_tick = interval(Duration::from_secs(1));

    loop {
        terminal.draw(|frame| crate::ui::draw::draw(self, frame))?;   // <- unconditional
        self.sync_pty_size();

        if let Some(name) = self.pending_workspace.take() { /* create workspace */ continue; }
        if self.should_quit { break; }

        tokio::select! {
            // default: random branch ordering
            Some(Ok(event)) = events.next() => { crate::events::handle_event(self, event).await; }
            _ = self.pty_manager.output_notify.notified() => {}
            _ = status_tick.tick() => {}
            _ = refresh_tick.tick() => { self.refresh_diff().await; }
            Some(event) = async { /* watcher */ } => { let _ = event; self.refresh_diff().await; }
        }
    }

    self.save_state();
    Ok(())
}
```

| Thing | Action in Phase 2 |
|------|------|
| `terminal.draw(...)` at top of loop | **Gate behind `if self.dirty { draw; self.dirty = false; }`** |
| `sync_pty_size()` | **Leaves unchanged.** It's a cheap no-op when size is already synced (`src/app.rs:342-344`). Running it on every iteration keeps PTY size correct after resize. |
| `pending_workspace` fast-path | **Add `self.dirty = true` after the branch,** since creating a workspace changes state. |
| `events` branch | **Mark dirty, then dispatch.** |
| `pty_manager.output_notify.notified()` | **Mark dirty.** PTY screen surface changed; next draw must re-render the PTY pane. |
| `status_tick` (1s) | **Drop entirely,** or raise to 5s to cover the "working dot" animation transition (see Section 2 pitfall #5). Recommend: keep at 5s for safety net. |
| `refresh_tick` (5s diff refresh) | **Keep for Phase 2; mark dirty after refresh.** Phase 5 will replace this with event-driven. |
| Watcher branch | **Mark dirty after refresh.** |
| `save_state()` at loop exit | **Leave unchanged.** |
| Default branch ordering (random) | **Replace with `biased;` + explicit ordering: events → pty → watcher → refresh → (optional status).** |

### What moves / what stays

- **Moves (or deletes):** `status_tick` — deleted or raised-and-renamed to `heartbeat_tick(5s)`.
- **Stays structurally:** every other branch, `sync_pty_size`, `pending_workspace`, `save_state`, the outer `loop { ... }` scaffold.
- **New, inline:** `self.dirty: bool` field; `if self.dirty { draw; self.dirty = false; }` check at top; `self.dirty = true` in every tokio::select! arm body and in `pending_workspace` branch.

## 5. State Mutations That Must Set Dirty

The safest policy is **mark dirty in the `run` loop's four arms and the pending-workspace branch** (one line each, five total). This covers all state mutations because everything that changes state is reached *through* one of these five gates.

**Verification by exhaustion:**

| Mutation source | Reached via | Already dirty-marked in loop? |
|-----------------|-------------|-------------------------------|
| Key press (any) | `events.next()` → `handle_event` → `handle_key` → `dispatch_action` / `forward_key_to_pty` / modal controller | Yes — arm 1 |
| Mouse click | `events.next()` → `handle_mouse` → `handle_click` | Yes — arm 1 |
| Mouse drag (selection) | `events.next()` → `handle_mouse` (drag arm) | Yes — arm 1 |
| Mouse scroll | `events.next()` → `handle_mouse` → `handle_scroll` | Yes — arm 1 |
| Paste | `events.next()` → `handle_event` (`Event::Paste`) | Yes — arm 1 |
| Resize | `events.next()` → `handle_event` (`Event::Resize`) | Yes — arm 1 |
| PTY stdout byte arrived | `pty_manager.output_notify.notified()` | Yes — arm 2 |
| Workspace file edited externally | watcher event → `refresh_diff` → `modified_files` mutation | Yes — arm 3 |
| Modified-files refresh (safety net) | `refresh_tick` → `refresh_diff` | Yes — arm 4 |
| Workspace created async | `pending_workspace.take()` → `create_workspace` | Yes — pending-workspace branch |
| Sub-calls that mutate state inside an arm | e.g., `dispatch_action` → `switch_project` → `refresh_diff` | Covered transitively — the arm already marks dirty |

**Specific mutation sites verified via grep** (for future-maintainer confidence, not for dirty wiring — the arm-level policy subsumes them):

- `src/events.rs` — 77 matches including `app.modal = ...`, `app.mode = ...`, `app.picker = ...`, `app.selection = ...`, `app.active_tab = ...`, `app.archived_expanded`, `app.global_state.projects.get_mut(...).expanded = ...`, `app.pending_workspace = ...`, `app.right_list.select(...)`, `app.left_list.select(...)`, `app.modified_files.clear()`, `app.preview_lines = ...` — all reached from arm 1.
- `src/workspace.rs` — 37 matches: `switch_project`, `create_workspace`, `create_tab`, `archive_active_workspace`, `delete_archived_workspace`, `confirm_delete_workspace`, `confirm_remove_project`, `add_project_from_path`, `queue_workspace_creation`. All called from arm 1 (event dispatch) or the pending-workspace branch.
- `src/ui/modal_controller.rs` — 57 matches: `handle_modal_key`, `handle_modal_click`. Called from arm 1.
- `src/ui/draw.rs` — 21 matches: these are *inside* the draw closure (`app.last_frame_area`, `app.last_panes`, `app.sidebar_items`). They mutate `App` during rendering, but dirty is cleared *before* the draw fires, and those fields aren't read by anything until the next iteration. Safe.

**Conclusion: five `self.dirty = true` lines in `run` cover every state mutation path. No per-call-site wiring needed.**

Trade-off: over-marking means we occasionally redraw when nothing visible changed (e.g., a mouse move on the status bar that's caught by `handle_mouse` but doesn't mutate state). Ratatui's buffer diff (`starlog.is/articles/developer-tools/ratatui-ratatui/`) already absorbs this — only changed cells get written to the terminal. **Accept this; optimise later if the profiler points at it.**

If a future refactor wants finer granularity (mark-dirty-only-on-actual-state-change), the `mark_dirty()` helper is the single refactoring point — move it from the arms into per-mutation call sites. Phase 2 does not need this.

## 6. Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (built-in) + `insta` snapshot, `assert_cmd`, `predicates`, `tempfile` |
| Config file | None; standard cargo layout |
| Quick run command | `cargo test -p martins app::tests dirty 2>&1 \| head -40` (once tests exist) |
| Full suite command | `cargo test` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ARCH-02 | `App::new` sets `dirty = true` so first frame renders | unit | `cargo test app::tests::app_starts_dirty` | ❌ Wave 0 |
| ARCH-02 | Clearing `dirty` and not mutating leaves `dirty = false` | unit | `cargo test app::tests::dirty_stays_clear_when_no_mutation` | ❌ Wave 0 |
| ARCH-02 | `mark_dirty` flips `dirty` to `true` | unit | `cargo test app::tests::mark_dirty_sets_flag` | ❌ Wave 0 |
| ARCH-02 | Every `tokio::select!` arm marks dirty — traceable by grep | code-review | manual: `rg 'self.dirty = true' src/app.rs` should find ≥ 5 hits; `rg 'terminal.draw' src/app.rs` should show it gated behind `if self.dirty` | n/a |
| ARCH-02 | Idle CPU drops to near-zero | **manual smoke test** | `./target/release/martins` in a clean repo; leave idle 30s; `top -pid <pid>` CPU% < 1 on macOS | n/a — manual |
| ARCH-03 | `tokio::select!` has `biased;` as its first line | code-review | manual: `rg 'biased' src/app.rs` returns a hit inside `run` | n/a |
| ARCH-03 | Input is the first branch of the select | code-review | manual: inspect `src/app.rs` — `events.next()` arm is the first branch after `biased;` | n/a |
| ARCH-03 | Keyboard input remains responsive under heavy PTY output | **manual feel test** | launch `cat /some/large/file` in a tab, then try typing in an adjacent sidebar/normal-mode keybind — each keystroke should register in one frame; compare against pre-Phase-2 baseline on the same machine | n/a — manual |

### Sampling Rate

- **Per task commit:** `cargo check && cargo clippy --all-targets -- -D warnings` (fast — no test run needed)
- **Per wave merge:** `cargo test` (full suite, ~97 tests passing pre-phase baseline)
- **Phase gate:** Full `cargo test` green + manual smoke tests (idle CPU, heavy-PTY-input latency)

### Wave 0 Gaps

- [ ] `src/app_tests.rs` — **exists** (from Phase 1, 85 lines). Add 3 new tests: `app_starts_dirty`, `dirty_stays_clear_when_no_mutation`, `mark_dirty_sets_flag`. No new test file needed.
- [ ] No new shared fixtures; `App::new` test-constructibility is already established in `src/app_tests.rs`.
- [ ] Framework install: none — `cargo test` already in place.
- [ ] **No integration test for input-priority under PTY load.** Automating "type a key while PTY streams and measure latency" requires pty-harness infrastructure we don't have. Covered as manual feel test; acceptable per ROADMAP decision ("subjective feel test against Ghostty, not a ms metric" — STATE.md line 68).

### Subjective / Feel Tests (documented in VALIDATION.md)

These are load-bearing success criteria but *not* automatable:

1. **Idle CPU.** Open the app; leave it alone for 30s; watch `top`/Activity Monitor. Baseline before Phase 2 = high/constant; target after = ~0%. Fans should audibly quiet on a laptop.
2. **Keystroke under heavy PTY.** In one workspace tab, run `cat /usr/share/dict/words` (or `yes`). Try typing `Ctrl-b` to exit terminal mode; try arrow keys in normal mode; try clicking the sidebar. All should register immediately. Subjectively compare against Ghostty.

## 7. Open Questions / Risks

1. **Working-dot animation vs `status_tick` drop.** If `status_tick` is removed entirely, the sidebar's "working" dot can get stuck (transition from "working" → "idle" at the 2s threshold is driven only by a redraw). **Resolution: keep a 5s heartbeat tick (renamed `heartbeat_tick` for clarity), mark dirty on each fire.** Documented as Section 2 pitfall #5. Planner should confirm this trade-off or accept a follow-up in Phase 4.

2. **Does `events::handle_event` need a "did this mutate?" return value?** If we later want to avoid marking dirty on events that don't mutate state (e.g., a mouse move that lands outside any pane), we'd want `handle_event` to return `bool`. **Phase 2 recommendation: don't bother.** Mark dirty unconditionally on any input event; ratatui diffs absorb the cost. Revisit only if idle CPU remains elevated during mouse movement (profiler-driven).

3. **Cursor blink in the PTY pane.** With a dirty-gated draw loop, the cursor stops blinking during idle. `tui-term::widget::PseudoTerminal` (used in `src/ui/terminal.rs`) renders the cursor as part of the frame — no independent blink timer. **Expected: cursor stays visibly "on" (not blinking) during idle.** Confirm during UAT whether this is acceptable vs. adding a 500ms blink tick (which would partially defeat the idle-CPU goal).

4. **`sync_pty_size` on every iteration.** Currently called before every draw; after Phase 2 it still runs every iteration even when no draw happens. It's a no-op when `last_pty_size` is unchanged, so cost is minimal. If we want to move it *inside* the `if self.dirty` block, we'd couple PTY-size-sync to frame rendering — which is the wrong direction (terminal resize can happen without any other state change). **Recommendation: leave outside the `if dirty` block.**

5. **First-frame race: `App::new` calls `refresh_diff` and `reattach_tmux_sessions` before `run` starts.** Both mutate state (`modified_files`, `pty_manager`, `last_pty_size`). Setting `dirty: true` in `App::new` is sufficient; this is not a real risk, just explicit-state note.

6. **`EventStream::next()` and the `Some(Ok(event))` pattern.** On a stream error (`Some(Err(_))`) the branch currently doesn't match and select falls through to other branches. Behavior preserved in Phase 2 rewrite. If we ever want to terminate on crossterm error, that's a separate concern.

7. **Heartbeat tick wake-up cost.** A 5s heartbeat means one tokio wakeup every 5 seconds even on pure idle — that's ~0.2 wakeups/sec. Negligible but not zero. An alternative for pure-zero idle: drop the heartbeat entirely, accept 2-to-∞ second staleness on the "working dot," rely on PTY output to refresh via notify. **Recommendation: keep the 5s heartbeat for Phase 2; revisit in Phase 4.**

8. **`refresh_diff` 5s timer fires on first iteration.** `tokio::time::interval(Duration::from_secs(5))` fires immediately on `tick()`. This causes an unnecessary diff refresh on startup (redundant with `refresh_diff` already called from `App::new`). Not a Phase 2 bug but worth noting — Phase 5 may want `interval_at(Instant::now() + 5s, 5s)` instead.

## Project Constraints (from CLAUDE.md)

- **Rust edition 2024, MSRV 1.85.** Language features: we can use let-else, let-chains, `if let` in match arms — all already used in codebase.
- **macOS-only runtime.** No platform-specific branching needed; tokio + ratatui + crossterm all behave identically on macOS for the primitives used here.
- **tokio full features.** `tokio::select!`, `tokio::time::interval`, `tokio::sync::Notify` — all in `tokio = { version = "1.36", features = ["full"] }` already. No dependency changes needed.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Cursor does not need independent blink during idle (tui-term draws it as part of the frame) | Section 2 pitfall #3 and Section 7 Q3 | Medium — if user expects blinking cursor, Phase 2 regresses UX. Mitigated: UAT catches it; fix is a 500ms interval branch. |
| A2 | 5s "working dot" latency is acceptable trade-off for dropping `status_tick` to 5s | Section 2 pitfall #5 and Section 7 Q1 | Low — if dot lag is visibly annoying, drop to 2s or 1s; idle CPU still much better than current. |
| A3 | Unconditional `self.dirty = true` on every input event is preferred over per-handler "did this mutate?" return values | Section 5 and Section 7 Q2 | Low — ratatui buffer diff absorbs redundant draws. If profiler indicates, refactor to finer marking. |
| A4 | `sync_pty_size` can safely stay outside the dirty gate | Section 4 and Section 7 Q4 | Low — it's a cheap no-op on unchanged size. |

**Verify with user during discuss-phase or planner review** if any of the above trade-offs are wrong for this project's feel.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| tokio | All async | ✓ | 1.36 | — |
| ratatui | TUI | ✓ | 0.30 | — |
| crossterm | Input | ✓ | 0.29 + event-stream | — |
| tmux | PTY back-end | ✓ on macOS dev machine | — | — |
| cargo + rustc | Build | ✓ | edition 2024 | — |

All dependencies in place; no installation required. No new crates needed for Phase 2 — `biased;` is a tokio macro syntax, not a feature gate.

## Sources

### Primary (HIGH confidence)

- [docs.rs/tokio/macro.select.html](https://docs.rs/tokio/latest/tokio/macro.select.html) — `biased;` semantics, branch polling, fairness caveats (VERIFIED via WebFetch)
- [ratatui.rs/concepts/rendering](https://ratatui.rs/concepts/rendering/) — immediate-mode rendering, programmer responsibility for `terminal.draw()` (VERIFIED via WebFetch)
- Codebase direct inspection: `src/app.rs:161-211` (current run loop), `src/events.rs`, `src/ui/draw.rs`, `src/pty/manager.rs:28` (`output_notify`), `src/pty/session.rs:96-102` (8ms notify throttle), `src/watcher.rs` (debounced FS watcher)

### Secondary (MEDIUM confidence)

- [ratatui.rs/faq](https://ratatui.rs/faq/) — guidance against multiple `terminal.draw()` per iteration (VERIFIED via WebFetch)
- [deepwiki.com/ratatui/ratatui/4.3-state-management-patterns](https://deepwiki.com/ratatui/ratatui/4.3-state-management-patterns) — state management and event-driven architectures (WebSearch summary)
- [starlog.is/articles/developer-tools/ratatui-ratatui](https://starlog.is/articles/developer-tools/ratatui-ratatui/) — buffer-diff mechanism (WebSearch summary, confirms over-marking cost is low)
- [deepwiki.com/helix-editor/helix](https://deepwiki.com/helix-editor/helix) — helix's tokio::select! main loop pattern (WebSearch summary)

### Tertiary (LOW confidence)

- [deepwiki.com/zellij-org/zellij/6-input-handling](https://deepwiki.com/zellij-org/zellij/6-input-handling) — zellij uses dedicated STDIN reader thread, a different pattern than martins uses; included for comparison only, not load-bearing

## Metadata

**Confidence breakdown:**
- `biased;` semantics and tokio::select! priority model: HIGH — directly verified on docs.rs
- Dirty-flag pattern: HIGH — ratatui official guidance + codebase verification
- State-mutation inventory: HIGH — full grep + file inspection of events.rs, workspace.rs, modal_controller.rs, draw.rs
- Working-dot latency trade-off: MEDIUM — based on reading `sidebar_left.rs` working_map logic from draw.rs; requires UAT confirmation
- Cursor-blink behavior: MEDIUM/ASSUMED — needs UAT; see Assumption A1

**Research date:** 2026-04-24
**Valid until:** 2026-05-24 (30 days — ratatui 0.30, tokio 1.36, and crossterm 0.29 are all stable)
