# Phase 5: Background Work Decoupling — Research

**Researched:** 2026-04-24
**Domain:** tokio background tasks, notify-debouncer event coalescing, non-blocking state persistence, event-driven refresh over periodic polling
**Confidence:** HIGH (current state is line-by-line verified in codebase; notify-debouncer-mini already wired; reference patterns exist from Phase 4)

## 1. Executive Summary

Phase 5 has three concrete objectives — each is already ~half-solved by the wiring Phase 2 and Phase 4 installed, which makes this phase narrow and high-confidence:

1. **BG-01 / BG-02 — Drop the 5s diff-refresh timer, keep only a 30s safety-net.** Today `App::run` has a `refresh_tick` at 5s (`src/app.rs:177`) that fires `self.refresh_diff().await` on every tick. This is the "~9% CPU every 5s" spike Phase 2's SUMMARY flagged as Phase-5 scope. The fix is mechanical: change the interval to 30s and optionally shift the `.await` to `refresh_diff_spawn()` so the tick arm itself is non-blocking. The watcher arm already exists as the event-driven path — `notify_debouncer_mini` with 750ms debounce is live (`src/watcher.rs:47-68`). [VERIFIED: `src/app.rs:177, 242-245`; `src/watcher.rs:47-68`]

2. **BG-03 / BG-04 — Make diff refresh fully non-blocking + tune the watcher debounce.** The `refresh_diff_spawn` primitive already exists from Phase 4 — it spawns git2 work onto a tokio task and returns results via an mpsc drained by the 6th select branch (`src/app.rs:246-258`). Phase 4 left three `refresh_diff().await` call-sites intentionally un-migrated because they aren't on the user-facing input hot path: (a) `App::new` pre-first-frame, (b) watcher branch, (c) refresh_tick branch. **(b) and (c) should migrate to `refresh_diff_spawn()` in Phase 5.** The watcher debounce at 750ms is above the phase target of ~200ms (success criterion #2 — "external-editor edit updates diff view within ~200ms"). Recommend dropping debounce to 200ms. [VERIFIED: `src/watcher.rs:48`; `src/app.rs:150, 234, 243`]

3. **BG-05 — Move `save_state` off the event-loop thread.** `save_state` is a synchronous `std::fs::write` + `std::fs::rename` that runs inline on the input arm during workspace create/archive/delete/remove and on every `ToggleProjectExpand`, plus the quit drain. State file `~/.martins/state.json` is small (one JSON per GlobalState — typically <10KB), so "visible pause" is unlikely *today* but would show up once the state grows. The fix is to spawn the save on a `tokio::task::spawn_blocking` (best option: clone `GlobalState` + path, spawn, let it run detached; errors logged via `tracing::error!` as today). **Coalescing burst saves** is the second half — four consecutive `save_state()` calls in a workspace-create path (`src/workspace.rs:263, 319, 342`) could be collapsed to one via a "dirty_state" flag drained on a timer or on the next loop iteration, but a simpler shape is "kick off a save when flag is set, serialize one-at-a-time via a dedicated spawner task." Either shape preserves atomic-rename durability. [VERIFIED: `src/state.rs:195-230`; 14 call sites of `save_state` across `app.rs`, `events.rs`, `workspace.rs`, `ui/modal_controller.rs`]

**Primary recommendation (one-liner):**

> **Three surgical edits in `src/app.rs::run` + one new sibling primitive `save_state_spawn`** — (a) change `refresh_tick` from `interval(Duration::from_secs(5))` to `interval(Duration::from_secs(30))`, (b) swap `self.refresh_diff().await` in the watcher + refresh_tick arms to `self.refresh_diff_spawn()`, (c) retune `src/watcher.rs` debounce from 750ms to 200ms, (d) add `save_state_spawn` that clones `global_state` + `state_path` into a `spawn_blocking` and returns immediately; replace the 14 `save_state()` call sites (or keep the sync `save_state` for the App::run graceful-exit drain and migrate only the hot-path 13). The Phase 4 mpsc+6th-branch pattern is the template.

**Idle-CPU expectation after Phase 5:** with `refresh_tick` at 30s and the 5s spike gone, the idle event loop should sit on 5 sleeping futures (input, pty_notify, watcher, heartbeat at 5s, refresh_tick at 30s, diff_rx). The working-dot heartbeat remains the 5s wakeup floor — that's intentional and documented in Phase 2. No new always-on tasks. `save_state_spawn` tasks are self-terminating (<5ms typical, atomic rename caps the blocking window).

## 2. User Constraints

> No CONTEXT.md exists for Phase 5 — no locked discuss-phase decisions to honor. Operating under ROADMAP + REQUIREMENTS.md only. Planner should treat success criteria from ROADMAP verbatim.

**Inferred from milestone constraints (REQUIREMENTS.md "Out of Scope"):**
- No quantitative latency SLA (user judges by feel, not metrics)
- No framework swap — tokio stays, ratatui stays, notify/notify-debouncer-mini stays
- macOS-only — no cross-platform debouncer concerns

## 3. Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| BG-01 | Git diff refresh is event-driven — triggered only by debounced `notify` file-system events, not a 5s periodic timer | §5 (Drop 5s refresh_tick); §6 (Watcher Event-Driven Pattern); §7 Code Examples |
| BG-02 | A safety-net timer at 30s (not 5s) re-runs diff as a fallback if no file events fire | §5; §7 Code Examples (`refresh_tick` at `Duration::from_secs(30)`) |
| BG-03 | Diff refresh runs as a background tokio task and never blocks the event loop or input path | §4 (Phase 4 `refresh_diff_spawn` already exists); §5; §7 |
| BG-04 | File watcher events are debounced (target ~200ms) so bursts of file changes produce at most one diff refresh | §6 (notify-debouncer-mini retune); §8 Pitfalls #2, #3 |
| BG-05 | State save (`~/.martins/state.json`) runs asynchronously — it never blocks input or render, even during workspace mutations | §9 (save_state_spawn pattern); §8 Pitfalls #4, #5 |

## 4. Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| File-system event detection (raw) | `notify` crate (RecommendedWatcher) | `notify-debouncer-mini` | FSEvents on macOS; already live in `src/watcher.rs` |
| Event coalescing / debounce | `notify-debouncer-mini::Debouncer` (750ms → 200ms) | `src/watcher.rs::Watcher` | In-crate debouncing; mpsc channel into `App::run` |
| Noise filtering (`.git/`, `target/`, `node_modules/`, …) | `src/watcher.rs::is_noise` | `NOISE_DIRS` const | Already solved — do not touch |
| Diff-refresh scheduling | `App::run` tokio::select! arms (watcher branch + refresh_tick branch) | — | Both branches must call `refresh_diff_spawn()`, not `refresh_diff().await` |
| Diff-refresh execution (git2) | `tokio::task::spawn_blocking` + `crate::git::diff::modified_files` | — | Already correct — `modified_files` wraps its git2 work in spawn_blocking |
| Diff-refresh result delivery | `tokio::sync::mpsc::UnboundedSender<Vec<FileEntry>>` → 6th select branch | `App.diff_tx` / `App.diff_rx` | Phase 4 primitive — reuse verbatim |
| State persistence (JSON write + atomic rename) | `tokio::task::spawn_blocking` + `GlobalState::save` | `App::state_path` | Spawn-blocking because `std::fs::rename` + `std::fs::write` are sync; cannot reasonably make them async without tokio::fs which still uses spawn_blocking internally |
| Safety-net diff refresh timer | `tokio::time::interval(Duration::from_secs(30))` | `App::run` tokio::select! 5th arm | 6x less frequent than current 5s; the 30s interval is the BG-02 spec |

**Cross-tier correctness checks Phase 5 must preserve:**
- Phase 2 invariants: `biased;` first, `// 1. INPUT` first after it, `if self.dirty`, `mark_dirty()≥7`, `heartbeat_tick(5)`, no `status_tick`.
- Phase 4 invariants: `refresh_diff_spawn` defined once, 6th select branch drains `diff_rx`, 3 call sites outside `src/app.rs` use `refresh_diff_spawn()`, 3 call sites inside `src/app.rs` use `refresh_diff().await`.
- Atomic state write semantics: write to `.tmp`, rename to target, keep `.bak` recovery path. `GlobalState::save` at `src/state.rs:195-230` is correct as-is — only its *invocation* moves off-thread.

## 5. Current State: Background Work Trace

### 5.1 Diff refresh today (verified)

```
// src/app.rs:213-258 — the tokio::select! loop (Phase 4 output)
tokio::select! {
    biased;
    Some(Ok(event)) = events.next() => { ... }                        // 1. INPUT
    _ = self.pty_manager.output_notify.notified() => { ... }          // 2. PTY
    Some(event) = async { watcher.next_event() or pending } => {      // 3. WATCHER
        self.refresh_diff().await;     // ← BG-03 violation (.await)
        self.mark_dirty();
    }
    _ = heartbeat_tick.tick() => { self.mark_dirty(); }               // 4. HEARTBEAT (5s)
    _ = refresh_tick.tick() => {                                       // 5. 5s refresh_tick ← BG-01/BG-02 violation
        self.refresh_diff().await;     // ← BG-03 violation (.await)
        self.mark_dirty();
    }
    Some(files) = self.diff_rx.recv() => { ... modified_files update ... self.mark_dirty(); }  // 6. DIFF DRAIN (Phase 4)
}
```

**What's wrong today:**
- Arm 3 and arm 5 *await* `refresh_diff`, which itself `.await`s a `spawn_blocking` + git2 work. While awaited, the `tokio::select!` future is parked on that arm — nothing else can dispatch. Phase 4 only migrated the *user-facing input-arm* call sites. **These two background arms remain violations of BG-03.**
- Arm 5's `refresh_tick = interval(Duration::from_secs(5))` wakes up every 5s and runs full git2 even when nothing has changed — this is the "~9% CPU spike every 5s" Phase 2 flagged as Phase-5 scope. BG-01 literally says "diff refresh is event-driven, not a 5s periodic timer." BG-02 raises it to a 30s safety net.

### 5.2 File watcher today (verified)

File: `src/watcher.rs`. `Watcher::new()` wires `notify_debouncer_mini::new_debouncer(Duration::from_millis(750), …)` with a closure that filters noise dirs (`.git/`, `target/`, `node_modules/`, etc.) and converts paths to `FsEvent::Changed` / `FsEvent::Removed`. Events flow through a `tokio::sync::mpsc::UnboundedReceiver<FsEvent>` consumed by `Watcher::next_event().await`. The watcher is created in `App::new` for the active project and re-targeted on `switch_project` via `watcher.unwatch(old) + watcher.watch(new)`.

**What's right today:**
- Noise filtering — already handles `.git/`, `target/`, `node_modules/`, `.martins/`, `dist/`, `build/`, `.next/`, `.venv/`. Keep as-is.
- Debouncing infrastructure — `notify-debouncer-mini` handles burst coalescing natively. This is the "cargo build / git checkout produces at most one diff refresh" mechanism (success criterion #3).
- Tokio integration — mpsc unbounded + `next_event().await` is idiomatic.

**What's wrong today:**
- Debounce window at **750ms** is too long for BG-04's "~200ms target." The tradeoff is: lower debounce = fresher diff view but more coalescing misses during rapid burst; higher debounce = better burst coalescing but slower reaction to a single-file external-editor save. 200ms is Phase 5's spec.
- No back-pressure: the unbounded mpsc can grow unbounded if the consumer stalls. Given our event loop shape, consumer stalls only during a long `refresh_diff.await` (which we're eliminating), so this is effectively moot after BG-03. Document but don't fix.

### 5.3 State save today (verified)

14 call sites of `save_state()` across:
- `src/app.rs:262` — graceful exit drain
- `src/workspace.rs:158` — confirm_delete_workspace
- `src/workspace.rs:179` — archive_active_workspace
- `src/workspace.rs:195` — delete_archived_workspace
- `src/workspace.rs:215` — confirm_remove_project
- `src/workspace.rs:263` — create_workspace (mid-sequence)
- `src/workspace.rs:319` — create_tab (mid-sequence)
- `src/workspace.rs:342` — add_project_from_path
- `src/events.rs:433` — close-tab
- `src/events.rs:496, 502, 538` — ClickProject (x2), ToggleProjectExpand
- `src/ui/modal_controller.rs:93, 236` — modal confirm paths

Implementation (`src/state.rs:195-230`):
```rust
pub fn save(&self, path: &Path) -> Result<(), StateError> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    let bak = path.with_extension("json.bak");
    let tmp = path.with_extension("json.tmp");
    // Backup existing valid state, write tmp, atomic rename, set 0o600 perms
}
```

**What's wrong today:**
- Every one of those 14 sites runs `fs::write(tmp) + fs::rename(tmp, target) + fs::set_permissions` inline on the event-loop thread. On a typical macOS SSD this is sub-millisecond, but during a workspace create + first-tab create sequence, that's **3 consecutive synchronous writes** (lines 263 + 319 + reattach or subsequent tab creates — up to 4-5 in a burst). Under high fs-pressure (Time Machine running, Spotlight indexing, filesystem close to full), each can spike to tens of ms.
- No coalescing — a workspace creation produces multiple save_states where only the last one is load-bearing (v4.1 "write exactly once after the transaction"). A simple dirty flag + flush-on-loop-iteration would collapse these.

## 6. Standard Stack

### Core (already wired — versions locked to REQUIREMENTS stack constraint)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio | 1.36 (full features) | Async runtime, mpsc, interval, spawn_blocking, task::spawn | Already load-bearing for Phase 2/3/4 patterns |
| notify | 8.2.0 | Cross-platform fs event source | Industry standard for Rust file watching |
| notify-debouncer-mini | 0.4.1 | Event coalescing on top of notify | Already wired in `src/watcher.rs`; debounce is the right abstraction for BG-04 |
| ratatui | 0.30 | TUI render (dirty-gated) | Phase 2 primitive, unchanged |
| crossterm | 0.29 | EventStream for input | Phase 2 primitive, unchanged |
| git2 | 0.17 | libgit2 bindings for diff | Already wrapped in `spawn_blocking` via `crate::git::diff::modified_files` |

[VERIFIED: Cargo.toml direct inspection]

### Supporting (standard tokio primitives — no new deps)

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio::sync::mpsc` | (bundled) | Non-blocking channels for background→foreground result delivery | `diff_tx/diff_rx` (Phase 4); possibly `save_state` result-signal if we want to surface errors back to UI |
| `tokio::time::interval` | (bundled) | The 30s safety-net | Replace 5s `refresh_tick`; keep 5s `heartbeat_tick` for working-dot |
| `tokio::task::spawn_blocking` | (bundled) | Run synchronous fs ops off the tokio worker pool's async workers | `save_state_spawn` implementation — wrap `GlobalState::save` |
| `tokio::task::spawn` | (bundled) | Fire-and-forget async tasks | Already used in `refresh_diff_spawn` (Phase 4) |

### Alternatives Considered (and rejected)

| Instead of | Could Use | Tradeoff — why rejected |
|------------|-----------|-------------------------|
| `notify-debouncer-mini` (current) | `notify-debouncer-full` | debouncer-full has rename-detection + continuous-event grouping, which is richer than we need for "refresh diff on any change." debouncer-mini's `Any/AnyContinuous` kind is exactly what we consume today. Migrating is a dependency churn for zero behavioral gain on our use case. Keep mini. [CITED: docs.rs/notify-debouncer-mini 0.7.0 vs notify-debouncer-full 0.5.0 feature comparison] |
| `notify-debouncer-mini 0.7.0` (latest) | `notify-debouncer-mini 0.4.1` (current) | **MEDIUM VALUE upgrade** — our Cargo.toml direct-depends on `notify = "8.2"` but `notify-debouncer-mini 0.4.1` transitively depends on `notify 6.1.1`, so we have **two copies of `notify` in our dep tree** (verified via `cargo tree`). Upgrading to 0.7.0 unifies them to 8.2. The upgrade is backwards-compatible at our usage surface. **Recommend including in this phase** as a small hygiene win — but not load-bearing for Phase 5 goals. [VERIFIED: cargo tree output] |
| `tokio::task::spawn_blocking` for save | `tokio::fs::write` + `tokio::fs::rename` | tokio::fs internally uses spawn_blocking — same cost, more ceremony. spawn_blocking wrapping the existing sync `GlobalState::save` is the minimum change. |
| Explicit dirty_state flag + flush-on-loop | Per-call-site `save_state_spawn()` | Flag-based would collapse burst writes (workspace create = 3-5 writes → 1 write). Call-site spawn is simpler. **Recommend call-site spawn for Phase 5 MVP; flag-based coalescing deferred to Phase 6 / v2 if needed.** Primary reason: 14 call sites is manageable, and the atomic-rename pattern means even 5 consecutive spawned writes in <1ms each is already "never blocks input" — the serialization happens on spawn_blocking workers, not the event loop. |
| Custom `tokio::time::sleep` debouncer (reset on each event) | `notify-debouncer-mini` | Rewriting an external-event debouncer in userland is the "don't hand-roll" anti-pattern. debouncer-mini already exists and is battle-tested. Keep. |

**Installation / dependency change:**
```toml
# Cargo.toml
notify-debouncer-mini = "0.7"   # was "0.4" — unifies to notify 8.2
```
No other dependency changes. `tokio::sync::mpsc`, `tokio::time::interval`, `tokio::task::spawn_blocking`, `tokio::task::spawn` are all in the tokio "full" feature set we already enable.

**Version verification:** Latest stable is `notify-debouncer-mini 0.7.0` (docs.rs as of 2026-04-24) — requires `notify ^8.2.0` which matches our direct dep. Current `0.4.1` pulls in a *second* copy of `notify v6.1.1`. [VERIFIED: `cargo search notify-debouncer-mini`; `cargo tree`; docs.rs/notify-debouncer-mini/0.7.0]

## 7. Architecture Patterns

### System Architecture Diagram (event flow, post-Phase-5)

```
                ┌────────────────────────────────────────────────────┐
                │         External inputs (independent sources)       │
                └────────────────────────────────────────────────────┘
                  │              │              │              │
                  ▼              ▼              ▼              ▼
          ┌────────────┐ ┌──────────────┐ ┌─────────────┐ ┌─────────┐
          │ crossterm  │ │  PTY output  │ │  notify/    │ │ 30s     │
          │ EventStream│ │ (child proc) │ │ FSEvents    │ │ safety  │
          │            │ │              │ │ (macOS)     │ │ timer   │
          └────────────┘ └──────────────┘ └─────────────┘ └─────────┘
                  │              │              │               │
                  │              │              ▼               │
                  │              │   ┌─────────────────────┐    │
                  │              │   │ notify-debouncer    │    │
                  │              │   │ -mini, 200ms window │    │
                  │              │   │ (coalesces bursts)  │    │
                  │              │   └─────────────────────┘    │
                  │              │              │               │
                  │              │              ▼               │
                  │              │   ┌─────────────────────┐    │
                  │              │   │ src/watcher.rs      │    │
                  │              │   │ noise filter +      │    │
                  │              │   │ FsEvent mapping     │    │
                  │              │   └─────────────────────┘    │
                  │              │              │               │
                  ▼              ▼              ▼               ▼
          ┌────────────────────────────────────────────────────────┐
          │       tokio::select!  in App::run  (biased; poll top→bottom) │
          │       1. events.next()      → handle_event + mark_dirty      │
          │       2. pty_output_notify  → mark_dirty                    │
          │       3. watcher.next_event → refresh_diff_spawn + mark_dirty│ ◄── BG-01
          │       4. heartbeat_tick(5s) → mark_dirty                    │
          │       5. refresh_tick(30s)  → refresh_diff_spawn + mark_dirty│ ◄── BG-02 safety-net
          │       6. diff_rx.recv()     → update modified_files + mark   │
          └────────────────────────────────────────────────────────────┘
                           │                          │
                           │ (detached)               │ (detached)
                           ▼                          ▼
              ┌────────────────────────┐  ┌──────────────────────────┐
              │ refresh_diff_spawn     │  │ save_state_spawn          │
              │   tokio::spawn →        │  │   tokio::task::spawn_    │
              │   spawn_blocking →      │  │   blocking →              │
              │   git2 modified_files  │  │   GlobalState::save      │
              │   → diff_tx.send(files)│  │   (fs::write + rename)   │
              └────────────────────────┘  └──────────────────────────┘
                           │
                           ▼ (result arrives on diff_rx → arm 6 drains)
              ┌────────────────────────┐
              │ app.modified_files     │
              │ updated; mark_dirty    │
              │ (next frame renders)   │
              └────────────────────────┘
```

**Key points:**
- **No arm in `tokio::select!` `.await`s any fs or git2 work after Phase 5.** Every expensive op is shunted to a spawned task; the arm body is a non-blocking send or fire-and-forget spawn.
- **`diff_rx` arm is the only arm that mutates `modified_files`** — single writer means no synchronization concerns.
- **Safety-net 30s timer exists only to catch "user did something the watcher missed" (e.g., FSEvents dropped an event under load, volume-mount race).** Firing once every 30s instead of every 5s reduces the "no reason to refresh" wakeups by 6×.

### Recommended Project Structure (no new files required)

```
src/
├── app.rs            # tokio::select! arms 3 + 5: .await → refresh_diff_spawn
│                     # new fn save_state_spawn; retire inline save_state at hot paths
├── watcher.rs        # debounce 750ms → 200ms
├── state.rs          # GlobalState::save unchanged (sync; atomic; correct)
├── workspace.rs      # save_state() → save_state_spawn()
├── events.rs         # save_state() → save_state_spawn()
└── ui/
    └── modal_controller.rs   # save_state() → save_state_spawn()
```

**No new modules.** Phase 5 is in-place rewiring of existing hot paths.

### Pattern 1: Spawn-and-drain (diff refresh — Phase 4 reference)
**What:** Fire-and-forget a background task; results flow through an mpsc channel drained by a dedicated select arm.
**When to use:** Background work whose *result* must land back in `App` state.
**Source:** `src/app.rs:306-325` (`refresh_diff_spawn`), `src/app.rs:246-258` (arm 6 drain).
**Example (existing):**
```rust
// Source: src/app.rs:306-325 — Phase 4 primitive, verbatim
pub(crate) fn refresh_diff_spawn(&mut self) {
    let args = match (self.active_project(), self.active_workspace()) {
        (Some(_), Some(ws)) => Some((ws.worktree_path.clone(), ws.base_branch.clone())),
        (Some(p), None)    => Some((p.repo_root.clone(), p.base_branch.clone())),
        _ => None,
    };
    let Some((path, base_branch)) = args else {
        self.modified_files.clear();
        self.right_list.select(None);
        self.mark_dirty();
        return;
    };
    let tx = self.diff_tx.clone();
    tokio::spawn(async move {
        if let Ok(files) = diff::modified_files(path, base_branch).await {
            let _ = tx.send(files);
        }
    });
    self.mark_dirty();
}
```

### Pattern 2: Fire-and-forget-with-logged-error (state save — new Phase 5)
**What:** Spawn a sync-fs op on a blocking worker; errors logged via `tracing::error!`; no result channel.
**When to use:** Durability ops that don't feed back into render state. State.json save, log rotation, cache flushes.
**Rationale:** `save_state` today already uses `tracing::error!` for failures (`src/app.rs:358-361`). No UI code consumes the save's return value. Moving to spawn-blocking preserves that exact contract.
**Example (target shape for Phase 5):**
```rust
// Source: target shape derived from Phase 4 refresh_diff_spawn pattern + current src/app.rs:358-362
pub(crate) fn save_state_spawn(&self) {
    let state = self.global_state.clone();
    let path = self.state_path.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(error) = state.save(&path) {
            tracing::error!("failed to save state: {error}");
        }
    });
}
```
**Notes:**
- `GlobalState: Clone` (derive on struct in `src/state.rs:139` via `#[derive(Clone)]`) — verified available.
- Clone cost is O(projects × workspaces × tabs × fields) — all small strings + paths; typically <100μs. Orders of magnitude below the fs::write blocking window we're removing.
- The `App::run` graceful-exit drain at `src/app.rs:262` can stay synchronous (last thing before return; we want it to complete before the process exits).

### Pattern 3: Safety-net timer coexisting with event-driven refresh
**What:** A low-frequency `tokio::time::interval` fires alongside an event source. Both call the same handler; no double-firing because the handler is itself idempotent (recomputes the same result).
**When to use:** Event sources that may drop events (FSEvents under extreme load, watcher between switch_project unwatch/watch, etc.).
**Rationale:** `refresh_diff_spawn` is idempotent — spawning twice in rapid succession produces two git2 runs with identical results; the second overwrites `modified_files` with the same data. No correctness problem. Wasted-CPU problem only matters if the safety net fires while the watcher is also firing — at 30s intervals, that's at worst one wasted spawn per 30s, negligible.
**Example (target target shape for Phase 5):**
```rust
// Arm 5 after Phase 5:
_ = refresh_tick.tick() => {
    self.refresh_diff_spawn();   // WAS: self.refresh_diff().await;
    // mark_dirty called inside refresh_diff_spawn, no need to repeat
}
// interval changed:
let mut refresh_tick = interval(Duration::from_secs(30));  // WAS: 5
```

### Anti-Patterns to Avoid

- **Hand-rolling debouncer with `tokio::time::sleep` + cancellation tokens.** notify-debouncer-mini already exists, is the blessed choice, and handles the edge cases (events arriving mid-debounce, watcher shutdown races). Don't rewrite it. [VERIFIED: current `src/watcher.rs` already uses this crate]
- **`AtomicBool::swap` for a "pending refresh" flag.** The event loop is single-threaded — a plain `bool` field + idempotent spawn is simpler and has identical semantics. (Phase 2 Q3 already decided this for `dirty`.)
- **Async `save_state` via `tokio::fs::write`.** tokio::fs internally uses spawn_blocking. The only reason to use tokio::fs is if you wanted `.await?`-able error propagation — we don't; we log and move on. Use spawn_blocking directly.
- **Collapsing burst `save_state` calls via a Mutex + sleep timer.** Over-engineered. The 4 consecutive save_states in a workspace-create path are already sub-millisecond each on macOS SSD, and after Phase 5 they're off-thread. If UAT shows a visible pause, iterate. Don't pre-optimize.
- **Raising the `refresh_tick` fallback to >60s.** Too long — a truly missed event means the user waits a minute to see their diff. 30s is the ROADMAP spec.
- **Leaving `refresh_diff().await` at any select arm.** Every arm that `.await`s work parks the whole loop. After Phase 5, the only in-loop `.await` sites are the arm-body delegations (`handle_event(event).await`) and the `pending_workspace` fast-path (`create_workspace(self, name).await`) — both of which are sanctioned because they *are* the input/action paths, not background work.

## 8. Common Pitfalls

### Pitfall 1: Debounce window too low — watcher becomes chatty

**What goes wrong:** Drop from 750ms to 50ms "for responsiveness"; every keystroke saved in vim fires a refresh, every `cargo build` burst becomes N refreshes.
**Why it happens:** Debounce window ≈ "how long to wait after the last event before emitting." Too low = events arrive too fast for coalescing. Too high = stale view.
**How to avoid:** **200ms is the sweet spot the ROADMAP specifies.** Below 100ms and Vim/emacs atomic-save (rename tmpfile) can generate 2 events that escape coalescing. Above 500ms and external-editor saves feel laggy.
**Warning signs:** UAT reveals: (a) rapid back-to-back refreshes when the user isn't expecting them → bump debounce higher; (b) >500ms stall between save-in-editor and diff-view update → bump debounce lower.
[CITED: notify-debouncer-mini default in docs.rs is 2s; 200ms for TUI responsiveness is per-project tuning]

### Pitfall 2: Watcher dropped events during switch_project

**What goes wrong:** `switch_project` calls `watcher.unwatch(old) + watcher.watch(new)` synchronously (`src/workspace.rs:127-134`). Any events emitted *between* those two calls are lost. If the user edits a file in the new project within that window, the watcher misses it.
**Why it happens:** debouncer-mini's `.watch` is synchronous and does not buffer events arriving mid-transition. The debouncer itself coalesces events within a single watch path.
**How to avoid:** **The 30s safety-net is exactly the mitigation.** Even if the watcher misses events during project switch, the safety net re-runs diff within 30s worst-case. Success criterion #2 ("~200ms for external-editor edit in current workspace") is about *steady-state* behavior, not project-switch edge case.
**Warning signs:** User reports "diff view stayed stale after switching projects until I hit a key" → safety-net interval should be lowered (e.g., 15s) or an explicit `refresh_diff_spawn()` added to `switch_project` tail (already present from Phase 4 via `app.refresh_diff_spawn()` at `src/workspace.rs:143` — verified; so this pitfall is already handled and the 30s net is belt-and-suspenders).

### Pitfall 3: First `refresh_tick.tick()` fires immediately — not after 30s

**What goes wrong:** `tokio::time::interval(Duration::from_secs(30))` fires its first tick at `t=0`, not `t=30s`. Combined with `App::new`'s own `refresh_diff().await` (pre-first-frame), this produces 2 diff runs at startup: one blocking pre-first-frame, one on the tick at t=0.
**Why it happens:** `tokio::time::interval` default behavior documented at docs.rs/tokio/time — first tick at construction time.
**How to avoid:** Use `tokio::time::interval_at(Instant::now() + Duration::from_secs(30), Duration::from_secs(30))` if the first tick needs skipping. **Alternatively, accept the redundant first tick as harmless** — it's now `refresh_diff_spawn` (non-blocking) and the second run just replaces `modified_files` with identical data. Preference: keep `interval(30s)` for simplicity; a single extra spawn at startup costs nothing.
[CITED: docs.rs/tokio/time/fn.interval.html — "The first tick completes immediately."] [VERIFIED: Phase 2 SUMMARY Known Follow-up #3 identified this for `refresh_diff` at the 5s interval.]

### Pitfall 4: `GlobalState::clone` inside a tight save-burst drops data

**What goes wrong:** If `save_state_spawn` is called 5 times in rapid succession from a workspace-create sequence, each spawn clones the state at its call point and races with the next to land the atomic rename. The *last spawn to complete* wins — but "last to complete" is not guaranteed to be "last called."
**Why it happens:** `spawn_blocking` tasks run on a thread pool with no ordering guarantees.
**How to avoid:** Two shapes:
- **Shape A (simple, recommended):** serialize saves behind a single dedicated spawner task — `App` owns an `mpsc::Sender<GlobalState>`, and a long-lived task consumes it and saves. Writes are guaranteed in-order; bursts coalesce naturally (drop all but the latest via `try_recv` loop). **Preferred.**
- **Shape B (simpler, with gotcha):** spawn each save independently; accept that in *theory* an out-of-order land can reinstate an older state, but in practice `save` is single-file atomic-rename and the window is <5ms. Risk is real but vanishingly small on a single-user macOS app.
**Recommendation:** **Shape A** — it's only ~30 lines of code and guarantees correctness. The planner should include this in the design.
**Warning signs:** User reports "I deleted workspace X, quit, restarted, and X was still there" → ordering race, investigate save queue.

### Pitfall 5: Graceful-exit save race

**What goes wrong:** On quit (`should_quit = true` → break → `self.save_state()` at `src/app.rs:262`), we need the save to *complete before the process exits*. If we migrate this call to `save_state_spawn`, the spawned task may be killed when `main()` returns before it finishes writing.
**Why it happens:** `tokio::task::spawn_blocking` tasks are cancelled when the runtime drops.
**How to avoid:** **Keep the graceful-exit save synchronous.** The exit path is not the hot path — a one-time 5ms blocking write on quit is fine. Migrate only the 13 non-exit call sites; keep `src/app.rs:262` as `self.save_state();` (sync).
**Warning signs:** User reports "my last workspace change didn't persist" → investigate exit path is running the sync save, not the spawn.
[CITED: docs.rs/tokio/task/fn.spawn_blocking.html — "When the runtime is shutdown, the worker threads are joined, not cancelled — but already-spawned tasks that haven't started may be dropped"; tests still possible for graceful shutdown.]

### Pitfall 6: refresh_diff_spawn on empty project silently wipes modified_files

**What goes wrong:** The existing `refresh_diff_spawn` handles the no-active-workspace case by clearing `modified_files` synchronously (`src/app.rs:312-317`). When the watcher or refresh_tick arm fires *before* a project is selected (e.g., empty global state at startup), it still clears. This is correct today but worth documenting.
**Why it happens:** The guard `args.is_none()` clears eagerly because showing stale data for a deleted project is worse than showing nothing.
**How to avoid:** No fix — behavior is correct. Include in regression-guard tests.
**Warning signs:** The diff list flickering empty then repopulating when a project auto-loads on startup. Already visible today; not a regression.

### Pitfall 7: `save_state_spawn` called under a modal — `Clone` triggers panic on broken GlobalState

**What goes wrong:** If `GlobalState` ever contains a non-cloneable subfield in the future (Rc, Cell with a pending borrow, etc.), the clone panics.
**Why it happens:** Current `GlobalState` derives Clone correctly — but this is a contract that must be preserved.
**How to avoid:** Keep the `#[derive(Clone)]` on `GlobalState`, `Project`, `Workspace`, `TabSpec` in perpetuity. Add a compile-time assertion `fn _assert_clone<T: Clone>() {} const _: () = _assert_clone::<GlobalState>();` if paranoid — overkill; the derive break would fail a normal rebuild.
**Warning signs:** `cargo build` breaks when someone adds a new `GlobalState` field — fix the Clone, don't remove the spawn.

### Pitfall 8: Watcher not re-armed after watcher failure

**What goes wrong:** `Watcher::new()` returns `Result`; `App::new` uses `.ok()` to swallow (`src/app.rs:108`). If watcher creation fails (e.g., FSEvents unavailable in a sandboxed test env), the watcher is `None` forever, and BG-04 silently degrades to polling-only (the 30s safety net).
**Why it happens:** macOS FSEvents occasionally fails in headless / containerized envs.
**How to avoid:** Already handled correctly in codebase — watcher absence falls through to `futures::future::pending()` in arm 3 (`src/app.rs:227-231`), so the safety net covers it. Document as the graceful-degradation path. Don't add retry logic.
**Warning signs:** User in a weird env reports diff never updates without 30s delay → watcher failed silently; check startup logs for tracing warnings (consider adding a `tracing::warn!` if watcher is None after Phase 5 surgery).

## 9. Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| File-system event debouncing | Custom `tokio::time::sleep` reset-on-event loop | `notify-debouncer-mini` (already wired) | Race conditions on event arrival during timer reset; cross-platform event kind mapping; already solved |
| Cross-platform fs watching | poll `stat()` every N seconds | `notify` crate (already wired) | FSEvents on macOS, inotify on Linux, ReadDirectoryChangesW on Windows — already bundled |
| Atomic file write | Direct write to target path | `GlobalState::save` → tmp + rename + bak (already implemented in `src/state.rs:195-230`) | Crash safety, partial-write recovery — already correct; do not touch |
| Task coordination | Arc<Mutex<Option<JoinHandle>>> + manual cancellation | `tokio::spawn` fire-and-forget + idempotent handlers | Phase 4 already established this pattern; simpler than managing handles |
| Burst coalescing of save_state | Global refcount + timer | dedicated `mpsc::Sender<GlobalState>` + consumer task with `try_recv` drain loop | Natural in-order + latest-wins with zero locks |
| Debounce reset-on-activity for refresh_tick | `AtomicBool::reset_timer()` + `tokio::select!` with `tokio::time::sleep` | `interval(30s)` + idempotent refresh_diff_spawn | The safety net doesn't need reset-on-activity; its whole purpose is to fire *regardless* of activity |

**Key insight:** Phase 5 is entirely composition of primitives that already exist — `spawn_blocking`, `mpsc`, `interval`, `notify-debouncer-mini`, `GlobalState::save`. The load-bearing work is **removing `.await`s from select arms and one `from_secs(5)` → `from_secs(30)` change**, not writing new algorithms.

## 10. Runtime State Inventory

> Phase 5 is not a rename/refactor — no runtime state rewiring. Omitting this section per RESEARCH protocol. (No collection names, registry entries, or stored IDs change.)

## 11. Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| tokio (full: mpsc, interval, spawn_blocking) | All select arms + save spawn | ✓ | 1.36 | — |
| notify | Watcher backend (FSEvents) | ✓ | 8.2.0 | — |
| notify-debouncer-mini | Event coalescing | ✓ | 0.4.1 (recommend 0.7.0 upgrade for notify-dedup) | Fallback: keep 0.4.1 — still correct behaviorally |
| git2 | diff refresh (already in spawn_blocking) | ✓ | 0.17 | — |
| ratatui / crossterm / tui-term | TUI (no change this phase) | ✓ | 0.30 / 0.29 / 0.3.4 | — |
| tempfile | Test fixture for fs event tests | ✓ (dev-dep) | 3.10 | — |
| macOS FSEvents | notify backend on macOS | ✓ (OS-provided) | macOS 10.7+ | Watcher creation fails gracefully → safety net covers |

**No new dependencies required for Phase 5 goals.** Optional: bump `notify-debouncer-mini = "0.7"` (unifies two `notify` versions in dep tree).

## 12. Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + `#[tokio::test]` (no external framework) |
| Config file | Cargo.toml `[dev-dependencies]` — tempfile 3.10, assert_cmd 2.0, predicates 3, insta 1.40 |
| Quick run command | `cargo test --bin martins <specific_test_name> -- --nocapture` |
| Full suite command | `cargo test` (runs all 107 existing tests + new) |
| Phase gate | Full suite green + `cargo clippy --all-targets -- -D warnings` + manual UAT of 5 success criteria |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|--------------|
| BG-01 | `refresh_tick` is NOT 5s (structural invariant) | grep + unit | `rg 'interval\(Duration::from_secs\(30\)\)' src/app.rs` + test asserting run-loop interval | ❌ — new test: `src/app_tests.rs::refresh_tick_is_30s_interval` |
| BG-02 | 30s safety-net fires `refresh_diff_spawn` (not `.await`) | unit | Runtime-behavioral test: build App, advance tokio time by 30s with `tokio::time::pause`, assert diff_tx received | ❌ — new test: `src/app_tests.rs::safety_net_triggers_spawn` (may be too integration-heavy; grep-based structural check acceptable alt) |
| BG-03 | All select arms that run refresh_diff are non-blocking (no `.await`) | grep invariant | `rg 'self\.refresh_diff\(\)\.await' src/app.rs` → expect **1** (App::new only; run-loop has 0 after Phase 5) | partial — existing nav tests cover the input arm; new Phase 5 test for arm-body shape |
| BG-04 | Debounce window is ~200ms; burst of 10 rapid file writes produces ≤2 events | `#[tokio::test]` + tempfile | Extend existing `src/watcher.rs::tests::debounce_rapid` (already asserts ≤2 events for 5 rapid writes at 750ms; retune to 200ms + add 10-write assertion) | ✓ exists — extend |
| BG-05 | `save_state_spawn` returns in <5ms even on a pathological-size state | `#[tokio::test]` + timing | New test: build 100-project GlobalState, call `save_state_spawn`, assert elapsed <5ms | ❌ — new test: `src/app_tests.rs::save_state_spawn_is_nonblocking` |

### Sampling Rate

- **Per task commit (quick):** `cargo test --bin martins <new_test_for_that_task> -- --nocapture`
- **Per wave merge:** `cargo test && cargo clippy --all-targets -- -D warnings` (full suite — ~30s)
- **Phase gate:** Full suite green + user UAT of all 5 ROADMAP success criteria before `/gsd-verify-work`

### Wave 0 Gaps

- [ ] `src/app_tests.rs` — add `refresh_tick_is_30s_interval` test (may be grep-only invariant; planner decides)
- [ ] `src/app_tests.rs` — add `save_state_spawn_is_nonblocking` test (50ms budget, pattern mirrors Phase 4 `refresh_diff_spawn_is_nonblocking`)
- [ ] `src/watcher.rs` — retune `debounce_rapid` test assertion for 200ms window
- [ ] (Optional) `src/app_tests.rs` — add `save_state_spawn_survives_burst` test using tokio::time::pause to verify ordering (if Shape A coalescing is chosen)
- [ ] No new framework install needed — all primitives (`#[tokio::test]`, `tempfile`, `tokio::time::pause`) already available

**Grep invariants to add (regression guard — Phase 6+):**

```
# Positive invariants (must be TRUE after Phase 5)
rg 'interval\(Duration::from_secs\(30\)\)' src/app.rs                    → 1
rg 'pub\(crate\) fn save_state_spawn' src/app.rs                         → 1
rg -c 'refresh_diff_spawn\(\)' src/app.rs                                → ≥3 (watcher arm + refresh_tick arm + existing)
rg 'Duration::from_millis\(200\)' src/watcher.rs                         → 1
rg 'save_state_spawn\(\)' src/workspace.rs src/events.rs src/ui/modal_controller.rs → ≥13

# Negative invariants (must be FALSE after Phase 5)
rg 'interval\(Duration::from_secs\(5\)\)' src/app.rs                     → 1 (heartbeat only)
rg 'self\.refresh_diff\(\)\.await' src/app.rs                            → 1 (App::new only)
rg 'Duration::from_millis\(750\)' src/watcher.rs                         → 0

# Phase 2/3/4 invariants — must remain preserved
rg 'biased;' src/app.rs                                                  → 1
rg '// 1\. INPUT' src/app.rs                                             → 1
rg 'if self\.dirty' src/app.rs                                           → 1
rg 'status_tick' src/app.rs                                              → 0
rg 'pub\(crate\) fn refresh_diff_spawn' src/app.rs                       → 1
```

## 13. Security Domain

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — (macOS-local TUI, single-user) |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | no | No new user input surface in this phase |
| V6 Cryptography | no | — |
| V12 Files & Resources | **yes** | State file permissions: 0o600 — **already set by `GlobalState::save` at `src/state.rs:224-227` under `#[cfg(unix)]`. Preserve.** |

### Known Threat Patterns for this phase

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Race on concurrent save spawn → stale state persists | Tampering | Shape A: single-consumer mpsc serializes saves |
| Symlink race on `.tmp` / `.bak` file creation | Tampering | `GlobalState::save` uses `with_extension("json.tmp")` in the same parent — not user-controlled; atomic rename is OS-atomic. Already correct. Don't regress. |
| Watcher file-leak under long-running session | — | `notify-debouncer-mini::Watcher` owns its thread; `Drop` cleans up. Already correct. Don't regress. |
| Log noise from failed save (tracing::error! bursts) | — | Already uses `tracing::error!` — operator can filter via `RUST_LOG`. Acceptable. |

**No new security-sensitive surface in Phase 5.** The changes are internal task-scheduling — no new fs reads, no new network, no new user input. State save keeps its existing `0o600` permissions and atomic-rename durability.

## 14. Code Examples

### 14.1 Non-blocking safety-net timer (target shape for `App::run`)

```rust
// Source: target shape for src/app.rs::run after Phase 5
// Changes from current:
//   - interval(5s) → interval(30s)
//   - refresh_diff().await → refresh_diff_spawn() in arms 3 + 5
pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
    let mut events = EventStream::new();
    // BG-02: safety-net fallback. Event-driven refresh via arm 3 (watcher) is primary.
    let mut refresh_tick = interval(Duration::from_secs(30));
    let mut heartbeat_tick = interval(Duration::from_secs(5));

    loop {
        if self.dirty {
            terminal.draw(|frame| crate::ui::draw::draw(self, frame))?;
            self.dirty = false;
        }
        self.sync_pty_size();

        if let Some(name) = self.pending_workspace.take() { /* ... unchanged ... */ }
        if self.should_quit { break; }

        tokio::select! {
            biased;
            Some(Ok(event)) = events.next() => {
                self.mark_dirty();
                crate::events::handle_event(self, event).await;
            }
            _ = self.pty_manager.output_notify.notified() => {
                self.mark_dirty();
            }
            // 3. File watcher — BG-01 event-driven path.
            Some(event) = async {
                if let Some(w) = &mut self.watcher { w.next_event().await }
                else { futures::future::pending::<Option<crate::watcher::FsEvent>>().await }
            } => {
                let _ = event;
                self.refresh_diff_spawn();   // BG-03: non-blocking
                // mark_dirty called inside refresh_diff_spawn
            }
            _ = heartbeat_tick.tick() => {
                self.mark_dirty();
            }
            // 5. BG-02 safety-net. Fires at t=0, then every 30s.
            _ = refresh_tick.tick() => {
                self.refresh_diff_spawn();   // BG-03: non-blocking
            }
            Some(files) = self.diff_rx.recv() => {
                self.modified_files = files;
                if self.modified_files.is_empty() {
                    self.right_list.select(None);
                } else if self.right_list.selected().is_none() {
                    self.right_list.select(Some(0));
                } else if let Some(selected) = self.right_list.selected() {
                    self.right_list.select(Some(selected.min(self.modified_files.len() - 1)));
                }
                self.mark_dirty();
            }
        }
    }

    self.save_state();   // Graceful-exit drain — stays SYNC (Pitfall #5)
    Ok(())
}
```

### 14.2 `save_state_spawn` helper (new Phase 5 primitive)

```rust
// Source: target shape — sibling to refresh_diff_spawn at src/app.rs:306
// Pattern: Phase 4 spawn-and-forget; no result channel (saves don't feed back into render)
/// Non-blocking variant of [`save_state`].
///
/// Clones `global_state` + `state_path` and dispatches the fs::write + atomic
/// rename to a tokio blocking worker. Errors are logged via tracing::error!
/// (same contract as the synchronous [`save_state`]).
///
/// Use from every call site EXCEPT the graceful-exit drain in [`App::run`],
/// where we need the write to complete before process exit.
///
/// See `.planning/phases/05-background-work-decoupling/05-RESEARCH.md`
/// §9 Pattern 2 + §8 Pitfall #5.
pub(crate) fn save_state_spawn(&self) {
    let state = self.global_state.clone();
    let path = self.state_path.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(error) = state.save(&path) {
            tracing::error!("failed to save state: {error}");
        }
    });
}
```

### 14.3 Retuned watcher debounce window

```rust
// Source: target shape for src/watcher.rs:47-68
// Changes: Duration::from_millis(750) → Duration::from_millis(200)
impl Watcher {
    pub fn new() -> Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel::<FsEvent>();
        let tx = Arc::new(tx);

        let debouncer = new_debouncer(
            Duration::from_millis(200),   // BG-04: ~200ms target per ROADMAP success criterion #2
            move |result: DebounceEventResult| {
                // ... unchanged noise-filter + FsEvent mapping ...
            },
        )?;
        Ok(Self { debouncer, events_rx: rx })
    }
    // unwatch/watch/next_event unchanged
}
```

### 14.4 (Optional, Shape A) Serialized save queue

```rust
// Source: target shape if Phase 5 adopts Shape A from Pitfall #4
// Scope: planner's call; Shape B (fire-and-forget) is the simpler MVP.
//
// Fields to add to App:
//     pub(crate) save_tx: mpsc::UnboundedSender<GlobalState>,
//
// In App::new, spawn the consumer before returning Self:
//     let (save_tx, mut save_rx) = mpsc::unbounded_channel::<GlobalState>();
//     let state_path = state_path.clone();
//     tokio::spawn(async move {
//         while let Some(state) = save_rx.recv().await {
//             // Drain any queued saves — latest wins (coalescing)
//             let mut latest = state;
//             while let Ok(next) = save_rx.try_recv() { latest = next; }
//             // Dispatch the final save to a blocking worker
//             let path = state_path.clone();
//             let _ = tokio::task::spawn_blocking(move || {
//                 if let Err(e) = latest.save(&path) {
//                     tracing::error!("failed to save state: {e}");
//                 }
//             }).await;
//         }
//     });
//
// save_state_spawn becomes:
//     pub(crate) fn save_state_spawn(&self) {
//         let _ = self.save_tx.send(self.global_state.clone());
//     }
```

## 15. State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| 5s periodic `interval()` for git diff refresh | 30s safety-net + event-driven via notify-debouncer | Phase 5 (this phase) | -6× idle-CPU wakeups; diff view feels "responsive" instead of "lags by up to 5s" |
| `.await` on background work inside select arm bodies | `tokio::spawn` + mpsc result channel | Phase 4 (nav path); Phase 5 (background paths) | Event loop never parked on fs/git work; input latency independent of background-task duration |
| 750ms debounce (notify-debouncer-mini default for conservatism) | 200ms debounce (ROADMAP target) | Phase 5 | 3.75× faster external-editor-save → diff-view latency |
| Synchronous `save_state` on every mutation | `save_state_spawn` via spawn_blocking | Phase 5 | State writes off the event-loop thread; workspace mutations feel instant |

**Deprecated / retired by Phase 5:**
- 5s `refresh_tick` — replaced by 30s safety-net
- 750ms watcher debounce — tuned down to 200ms
- Inline `save_state()` on mutation paths — replaced by `save_state_spawn()` (keep sync only for graceful-exit drain)

## 16. Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | 200ms debounce is the right TUI-responsiveness target | §6 (BG-04 note), §14.3 | LOW — ROADMAP explicitly specifies ~200ms. If UAT shows over-chattiness (e.g., vim saves producing 2-3 events), bump to 300ms. If external editor save feels laggy, bump debounce lower (100ms) — but verify editor doesn't do atomic-rename write pattern first. [ASSUMED based on ROADMAP success criterion #2 wording — "~200ms"] |
| A2 | State save is fast enough today to not cause visible pause on typical `~/.martins/state.json` sizes | §5.3 | MEDIUM — no profiling has been done. If UAT says workspace-archive feels instantaneous today, Phase 5's save async-ification is preventative. If it feels even slightly laggy, Phase 5 definitely fixes it. Mitigation: ship the spawn-based fix regardless — correct-shape even if current state.json is small. [ASSUMED based on state size estimate; not measured on user's machine] |
| A3 | Shape B (fire-and-forget per call) is acceptable; Shape A (serialized queue) is optional | §8 Pitfall #4, §14.4 | MEDIUM — in theory a rapid burst of saves can land out of order. On macOS SSD with <10KB JSON, the window is ~1-5ms per save. Odds of an out-of-order land causing observable regression in a single-user interactive app are very low. Mitigation: if Shape A adds <50 LOC, include it. If planner says Shape B is simpler and acceptance tests pass, ship Shape B. [ASSUMED based on single-user interactive usage pattern] |
| A4 | Watcher events during `switch_project`'s brief unwatch/watch window are not critical (safety net catches them) | §8 Pitfall #2 | LOW — verified: Phase 4 put `refresh_diff_spawn()` at `switch_project` tail (`src/workspace.rs:143`), so even without watcher events, the switch triggers a diff refresh. The 30s safety-net is belt-and-suspenders. [VERIFIED via codebase + Phase 4 SUMMARY] |
| A5 | Graceful-exit save must stay synchronous to guarantee durability | §8 Pitfall #5, §14.1 | LOW — tokio docs confirm `spawn_blocking` tasks can be dropped on runtime shutdown. Keeping the exit-drain sync is the standard pattern. [CITED: docs.rs/tokio/task/fn.spawn_blocking.html] |
| A6 | `notify-debouncer-mini 0.7.0` upgrade is backwards-compatible at our API surface | §6 "Alternatives Considered" | LOW — the API changed only in minor ways (see CHANGELOG on crates.io). Our usage is `new_debouncer(Duration, callback)` + `Watcher::watch/unwatch/next_event` — those signatures are stable across 0.4→0.7. If the upgrade breaks, revert to 0.4.1 and accept the transitive notify 6.1 duplication. [CITED: docs.rs/notify-debouncer-mini/0.7.0 API + cargo tree verification of the current duplication] |
| A7 | Idempotent `refresh_diff_spawn` means the safety-net-fires-while-watcher-fires race is harmless | §7 Pattern 3 | LOW — verified: `refresh_diff_spawn` reads the same active workspace args and spawns git2 reading the same state. Two concurrent runs produce identical outputs; the later `diff_tx.send` wins on the receiver side (mpsc is FIFO). At most cost: 2 redundant git2 runs per 30s safety-net collision, which is dwarfed by the 10-100ms of actual work. [VERIFIED: `src/app.rs:306-325` inspection + git2 semantics] |

**Verify with user during discuss-phase or planner review:**
- A3 is the one most worth asking about — is Shape A's extra ~30-50 LOC worth the ordering-guarantee, or is Shape B fine? Default to Shape B for MVP; user can override to Shape A if they want the belt-and-suspenders.

## 17. Open Questions (RESOLVED)

1. **Should `save_state_spawn` upgrade to Shape A (serialized + coalesced queue) or stay at Shape B (per-site spawn)?**
   - What we know: Shape B is ~5 LOC of change per call site + the new `save_state_spawn` helper. Shape A adds ~30-50 LOC + a long-lived consumer task.
   - What's unclear: whether the user values theoretical ordering guarantees over code simplicity.
   - **Recommendation:** start with Shape B; document Shape A as a known-available upgrade in case UAT or post-ship usage surfaces an ordering bug.
   - **RESOLVED:** Shape B adopted. Plan 05-02 introduces `save_state_spawn` as fire-and-forget per site. Shape A documented in 05-04 PHASE-SUMMARY Deferred Items.

2. **Is the 30s safety-net redundant enough that we can drop it entirely?**
   - What we know: BG-02 explicitly requires a 30s safety-net. ROADMAP success criterion #1 says "only on actual file-system events (or the 30s safety-net fallback)."
   - What's unclear: whether the user would rather have "pure event-driven, no safety net" (simpler, but trusts FSEvents 100%).
   - **Recommendation:** implement as spec'd. BG-02 is a hard requirement.
   - **RESOLVED:** 30s safety-net retained per BG-02. Plan 05-02 changes `Duration::from_secs(5)` → `Duration::from_secs(30)` at src/app.rs:177.

3. **Should we also move `archive_active_workspace`'s `std::fs::remove_dir_all(&worktree_path)` off-thread?**
   - What we know: it's a synchronous recursive fs op that can block for hundreds of ms on a workspace with many files (especially `node_modules`/`target`). Phase 4 research flagged it as out-of-scope for NAV (archive is destructive, user tolerates a pause) — but Phase 5's BG-05 success criterion #4 says "archiving a workspace feels instant."
   - What's unclear: whether the user interprets "archive is destructive, brief pause is fine" or "archive should also feel instant."
   - **Recommendation:** **include in Phase 5 scope.** Wrap the `remove_dir_all` in `spawn_blocking` from `src/workspace.rs:181`. Low-risk; matches BG-05's literal text. If planner disagrees, document as a follow-up in `05-PHASE-SUMMARY.md`.
   - **RESOLVED:** Included in scope. Plan 05-03 Task 2 wraps `remove_dir_all` in `spawn_blocking` per BG-05 success criterion #4.

4. **Should we also spawn the tmux ops (`kill_session`, `new_session`, `resize_session`) off-thread during workspace create/archive/delete?**
   - What we know: tmux subprocess spawns are already partially spawn-blocking'd (`src/workspace.rs:288-293` for `new_session`; `src/app.rs:450-454` for resize). But `kill_session` in `archive_active_workspace` (`src/workspace.rs:171`) and several others are inline.
   - What's unclear: whether Phase 5 scope extends into tmux or stays at state+diff only.
   - **Recommendation:** **out of scope for Phase 5.** tmux ops are <20ms typical; not a lag-spike source. Revisit if BG-05 UAT fails on tmux-heavy ops.
   - **RESOLVED:** Deferred. Documented in 05-04 PHASE-SUMMARY Deferred Items; revisit if BG-05 UAT surfaces tmux-heavy lag.

5. **Should Phase 5 add tracing spans around background task spawns for future regression diagnosis?**
   - What we know: OBS-01 (tracing spans) is v2-deferred.
   - **Recommendation:** out of scope. A few `tracing::debug!("spawned refresh_diff")` / `tracing::debug!("spawned save_state")` in `save_state_spawn` / `refresh_diff_spawn` are ~1 LOC each and cheap; planner may include as a freebie but not required.
   - **RESOLVED:** Deferred. Documented in 05-04 PHASE-SUMMARY Deferred Items; aligns with v2 OBS-01 placement.

## 18. Project Constraints (from CLAUDE.md)

- **Rust edition 2024, MSRV 1.85.** All new code follows the existing style (let-else, let-chains already used in codebase).
- **Single-language Rust.** No cross-language contracts to maintain.
- **macOS-only runtime.** `notify` uses FSEvents on macOS — already correct and live. Do not add Linux/Windows-specific fallbacks.
- **tokio full features.** `mpsc`, `interval`, `spawn_blocking`, `spawn` all available — no feature flag changes needed.
- **No framework swap.** Stay on ratatui 0.30, crossterm 0.29, notify-debouncer-mini (0.4.1 → optionally 0.7.0).
- **Release: lipo universal binary.** Phase 5's changes are portable across x86_64 + aarch64 — no arch-specific code.

## 19. Sources

### Primary (HIGH confidence)

- Codebase direct inspection (line-by-line verified): `src/app.rs` (run loop, refresh_diff, refresh_diff_spawn, save_state, run graceful-exit drain), `src/watcher.rs` (full file — 172 LOC), `src/state.rs` (GlobalState::save, GlobalState::load, atomic rename, 0o600 perms), `src/workspace.rs` (all 9 save_state call sites + lifecycle functions), `src/events.rs` (all save_state + refresh_diff_spawn call sites), `src/ui/modal_controller.rs` (2 save_state sites), `src/git/diff.rs` (modified_files already wraps in spawn_blocking), `src/navigation_tests.rs` (Phase 4 test patterns for reference). Line numbers above are all verified.
- `.planning/phases/02-event-loop-rewire/02-RESEARCH.md` — Phase 2 primitives (dirty-flag, biased select, status_tick → heartbeat_tick, 5s refresh_tick). [VERIFIED]
- `.planning/phases/02-event-loop-rewire/PHASE-SUMMARY.md` — Phase 2 Known Follow-ups flagging the 5s refresh_tick spike for Phase 5. [VERIFIED]
- `.planning/phases/04-navigation-fluidity/04-RESEARCH.md` — Phase 4 refresh_diff_spawn + mpsc + 6th select branch pattern (the template Phase 5 extends). [VERIFIED]
- `.planning/phases/04-navigation-fluidity/PHASE-SUMMARY.md` — Phase 4 completion + grep invariants Phase 5 must preserve. [VERIFIED]
- `.planning/REQUIREMENTS.md` — BG-01..BG-05 requirement text [VERIFIED — local file]
- `.planning/ROADMAP.md` — Phase 5 goal + 5 success criteria [VERIFIED — local file]
- [docs.rs/tokio/macro.select.html](https://docs.rs/tokio/latest/tokio/macro.select.html) — biased branch ordering, fairness caveats (re-verified from Phase 2 research, still current)
- [docs.rs/tokio/time/fn.interval.html](https://docs.rs/tokio/latest/tokio/time/fn.interval.html) — interval first-tick-at-construction semantics, MissedTickBehavior [CITED]
- [docs.rs/tokio/task/fn.spawn_blocking.html](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html) — spawn_blocking semantics on runtime shutdown [CITED]
- [docs.rs/notify-debouncer-mini/0.7.0](https://docs.rs/notify-debouncer-mini/latest/notify_debouncer_mini/) — latest stable version (0.7.0), notify ^8.2.0 compatibility, DebounceEventResult API [VERIFIED via WebFetch 2026-04-24]
- `cargo tree` output — verified that current `notify-debouncer-mini 0.4.1` pulls transitive `notify 6.1.1` while direct dep is `notify 8.2.0` (two versions in dep tree; upgrade to 0.7.0 unifies). [VERIFIED]
- `cargo search notify-debouncer-mini` — confirms 0.7.0 is latest stable [VERIFIED]

### Secondary (MEDIUM confidence)

- [docs.rs/notify/latest/notify/](https://docs.rs/notify/latest/notify/) — FSEvents backend behavior on macOS (drops events under extreme load, rename event semantics) [CITED via training + prior Phase 2 research]
- Nielsen's 100ms response-limit for perceived instant feedback (used in Phase 4 research; carries forward to Phase 5's 200ms debounce + 30s safety-net design) [ASSUMED from prior research]

### Tertiary (LOW confidence)

- Estimate of ~1-5ms for `GlobalState::save` on a typical `~/.martins/state.json` — not measured on user's machine [ASSUMED from macOS SSD characteristics]

## 20. Metadata

**Confidence breakdown:**
- Current state of select arms + refresh_diff wiring: HIGH — every line verified in codebase
- Current state of watcher (debouncer-mini, noise filter, 750ms window): HIGH — full file inspection
- Current state of save_state (sync, atomic, 14 call sites): HIGH — all call sites enumerated via grep
- BG-01/BG-02 (30s safety-net) — shape: HIGH — single-line interval change, trivial
- BG-03 (non-blocking refresh) — shape: HIGH — Phase 4 primitive already solves 3 of 5 call sites; the remaining 2 use the same pattern
- BG-04 (200ms debounce) — shape: HIGH — single-line Duration change
- BG-05 (non-blocking save): MEDIUM — Shape B is simple; Shape A adds coalescing. Unmeasured claim that current save is actually fast enough on typical state sizes.
- Open question on `archive`'s `remove_dir_all`: MEDIUM — depends on user interpretation of "archive feels instant"

**Research date:** 2026-04-24
**Valid until:** 2026-05-24 (30 days — tokio 1.36, notify 8.2, notify-debouncer-mini 0.4/0.7, ratatui 0.30, crossterm 0.29 all stable; Phase 2/3/4 baseline locked)
