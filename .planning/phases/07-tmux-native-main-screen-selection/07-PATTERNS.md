# Phase 7: tmux-native main-screen selection — Pattern Map

**Mapped:** 2026-04-25
**Files analyzed:** 5 modified + 0 new (SGR encoder lives inline in `events.rs` per RESEARCH §Recommended File Modification Surface — no `src/sgr.rs` extraction)
**Analogs found:** 5 / 5 (all in-codebase, no external fallback)

## File Classification

| Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---------------|------|-----------|----------------|---------------|
| `src/tmux.rs` (mod) | external-process integration / config writer | request-response (subprocess) | `src/tmux.rs::ensure_config` (lines 24-41) + `src/tmux.rs::send_key` / `pane_command` (lines 134-152) | exact (in-place extension; new free fn helpers) |
| `src/events.rs` (mod) | free-function event handler | request-response (event) + transform (mouse → SGR bytes) | `src/events.rs::handle_mouse` Drag/Up/Down (lines 38-101) + inline SGR encodes (lines 195-196, 256-257) + `handle_key` cmd+c/Esc precedence (lines 387-413) | exact (in-place extension) |
| `src/pty/session.rs` (mod) | PTY reader + session state | streaming (bytes→parser) + state-mutation | `src/pty/session.rs:18-31` (struct + Arc<AtomicU64> field pattern) | role-match (read-only helper, no drain-thread edit) |
| `src/app.rs` (mod) | session-state helpers + clipboard subprocess | request-response (subprocess) + state-query | `src/app.rs::copy_selection_to_clipboard` (lines 473-502) + `active_scroll_generation` (lines 526-537) + `set_active_tab` (lines 400-404) | exact (in-place extension; new sibling helpers) |
| `src/ui/terminal.rs` (mod, OPTIONAL) | ratatui render | transform (Buffer→Buffer) | `src/ui/terminal.rs:157-199` (overlay highlight pass) | exact (gate body, do not replace) |
| `src/selection_tests.rs` or new `src/tmux_native_selection_tests.rs` | inline unit-test module | test harness | `src/pty_input_tests.rs:1-43` + `src/selection_tests.rs` | exact (mirror structure) |

**No new files for production code.** Per RESEARCH §Recommended File Modification Surface, the SGR encoder is a free function in `events.rs` — extracting `src/sgr.rs` is not warranted at ~20 LOC and would diverge from the existing inline-encode convention at `events.rs:195/256`.

## Pattern Assignments

### `src/tmux.rs` — `ensure_config` extension + new helpers (`save_buffer_to_pbcopy`, `cancel_copy_mode`)

**Analog:** `src/tmux.rs::ensure_config` (lines 24-41) for the conf-write extension; `src/tmux.rs::send_key` (lines 146-152) and `pane_command` (lines 134-144) for the new subprocess helpers.

**Existing `ensure_config` body to extend in place** (`src/tmux.rs:24-41`):
```rust
fn ensure_config() -> PathBuf {
    let config_path = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join(".martins")
        .join("tmux.conf");

    let _ = std::fs::create_dir_all(config_path.parent().unwrap());
    let _ = std::fs::write(
        &config_path,
        "set -g mouse on\n\
         set -g default-terminal \"xterm-256color\"\n\
         set -g allow-passthrough off\n\
         set -g escape-time 0\n\
         setw -g alternate-screen off\n",
    );
    config_path
}
```

**Extension pattern** (RESEARCH §Tmux Defaults — only 3 lines, defaults already cover MouseDragEnd1Pane / DoubleClick1Pane / TripleClick1Pane):
```rust
let _ = std::fs::write(
    &config_path,
    "set -g mouse on\n\
     set -g default-terminal \"xterm-256color\"\n\
     set -g allow-passthrough off\n\
     set -g escape-time 0\n\
     setw -g alternate-screen off\n\
     # Phase 7: pipe selection-via-keyboard to macOS pbcopy (defaults only pipe MouseDragEnd1Pane).\n\
     bind-key -T copy-mode-vi y     send-keys -X copy-pipe-and-cancel \"pbcopy\"\n\
     bind-key -T copy-mode-vi Enter send-keys -X copy-pipe-and-cancel \"pbcopy\"\n\
     # Phase 7: vi-mode Esc defaults to clear-selection — override to cancel for single-press exit.\n\
     bind-key -T copy-mode-vi Escape send-keys -X cancel\n",
);
```

**Existing subprocess-fire-and-forget pattern to mirror** (`src/tmux.rs:146-152` — `send_key`):
```rust
pub fn send_key(name: &str, key: &str) {
    let _ = Command::new("tmux")
        .args(["send-keys", "-t", name, key])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}
```

**Existing piped-stdout subprocess pattern** (`src/tmux.rs:134-144` — `pane_command`):
```rust
pub fn pane_command(name: &str) -> Option<String> {
    Command::new("tmux")
        .args(["list-panes", "-t", name, "-F", "#{pane_current_command}"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
```

**New helpers to add (siblings of `send_key` / `pane_command`)** — RESEARCH §Pattern 3:
```rust
/// Read the latest tmux paste-buffer for `session` and pipe it to pbcopy.
/// Returns true on full success (buffer non-empty AND pbcopy succeeded).
/// Mirrors src/tmux.rs::pane_command (piped stdout) + src/app.rs:492-501
/// (pbcopy spawn-and-write).
pub fn save_buffer_to_pbcopy(session: &str) -> bool {
    use std::process::{Command, Stdio};
    let Ok(buf_proc) = Command::new("tmux")
        .args(["save-buffer", "-", "-t", session])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn() else { return false };
    let Ok(output) = buf_proc.wait_with_output() else { return false };
    if !output.status.success() || output.stdout.is_empty() {
        return false;
    }
    let Ok(mut pbcopy) = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn() else { return false };
    if let Some(stdin) = pbcopy.stdin.as_mut() {
        use std::io::Write;
        let _ = stdin.write_all(&output.stdout);
    }
    let _ = pbcopy.wait();
    true
}

/// Fire-and-forget `tmux send-keys -X cancel`. Idempotent: tmux exits 1 with
/// stderr "not in a mode" when no copy-mode is active — both stdout & stderr
/// are discarded (mirrors src/tmux.rs::send_key). Use on Esc / tab-switch.
pub fn cancel_copy_mode(session: &str) {
    let _ = Command::new("tmux")
        .args(["send-keys", "-X", "cancel", "-t", session])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}
```

---

### `src/events.rs` — `handle_mouse` conditional intercept + `encode_sgr_mouse` free fn

**Analog:**
- `src/events.rs::handle_mouse` Drag/Up/Down branches (lines 38-101) — current Phase 6 overlay path; the body Phase 7 must gate.
- Inline SGR encodes at `src/events.rs:195-196` (scroll wheel) and `src/events.rs:256-257` (sidebar click forward) — wire-format precedent the new `encode_sgr_mouse` mirrors.

**Existing in-terminal overlay branch to gate** (`src/events.rs:44-101`):
```rust
if in_terminal {
    match mouse.kind {
        MouseEventKind::Drag(MouseButton::Left) => {
            let inner = terminal_content_rect(app.last_panes.as_ref().unwrap().terminal);
            let col = mouse.column.saturating_sub(inner.x).min(inner.width.saturating_sub(1));
            let row = mouse.row.saturating_sub(inner.y).min(inner.height.saturating_sub(1));
            let current_gen = app.active_scroll_generation();
            if let Some(sel) = &mut app.selection {
                sel.end_col = col;
                sel.end_row = row;
            } else {
                app.selection = Some(SelectionState {
                    start_col: col, start_row: row, start_gen: current_gen,
                    end_col: col, end_row: row, end_gen: None,
                    dragging: true, text: None,
                });
            }
            app.mark_dirty();
            return;
        }
        MouseEventKind::Up(MouseButton::Left) => { /* ... mouse-up snapshot path ... */ }
        _ => {}
    }
}
```

**Existing inline SGR encode patterns** — wire-format precedent:

`src/events.rs:195-196` (scroll wheel forward — confirms `\x1b[<{button};{col};{row}M` shape and 1-based coords):
```rust
let button: u8 = if delta < 0 { 64 } else { 65 };
let seq = format!("\x1b[<{button};{local_col};{local_row}M");
app.write_active_tab_input(seq.as_bytes());
```

`src/events.rs:252-260` (sidebar click forward — confirms 1-based +1 conversion + press/release pair):
```rust
let inner = terminal_content_rect(panes.terminal);
if rect_contains(inner, col, row) {
    let local_col = col.saturating_sub(inner.x) + 1;
    let local_row = row.saturating_sub(inner.y) + 1;
    let press = format!("\x1b[<0;{local_col};{local_row}M");
    let release = format!("\x1b[<0;{local_col};{local_row}m");
    app.write_active_tab_input(press.as_bytes());
    app.write_active_tab_input(release.as_bytes());
}
```

**New `encode_sgr_mouse` pure function to add** (RESEARCH §SGR Mouse Encoding — pure fn, no IO):
```rust
use vt100::MouseProtocolMode;

/// Encode a crossterm MouseEvent into an SGR (1006) byte sequence for
/// forwarding into the tmux PTY. Coords are inner-pane-relative AND
/// 1-based per xterm convention. Returns None for events that should
/// not be forwarded.
pub(crate) fn encode_sgr_mouse(
    kind: MouseEventKind,
    modifiers: KeyModifiers,
    local_col: u16,
    local_row: u16,
) -> Option<Vec<u8>> {
    let (button_base, trailing) = match kind {
        MouseEventKind::Down(MouseButton::Left)  => (0u8, 'M'),
        MouseEventKind::Drag(MouseButton::Left)  => (32u8, 'M'),  // motion bit + left
        MouseEventKind::Up(MouseButton::Left)    => (0u8, 'm'),   // lowercase = release
        MouseEventKind::ScrollUp                 => (64u8, 'M'),
        MouseEventKind::ScrollDown               => (65u8, 'M'),
        _ => return None,
    };
    let mut cb = button_base;
    if modifiers.contains(KeyModifiers::SHIFT)   { cb += 4; }
    if modifiers.contains(KeyModifiers::ALT)     { cb += 8; }
    if modifiers.contains(KeyModifiers::CONTROL) { cb += 16; }
    let col = local_col + 1;  // 1-based per xterm SGR spec
    let row = local_row + 1;
    Some(format!("\x1b[<{cb};{col};{row}{trailing}").into_bytes())
}
```

**Conditional dispatch wrapping the existing match** (RESEARCH Example 1 — short-circuit to forward path BEFORE the overlay path):
```rust
pub async fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    let in_terminal = app.last_panes.as_ref().is_some_and(|p| {
        let inner = terminal_content_rect(p.terminal);
        rect_contains(inner, mouse.column, mouse.row)
    });

    // Phase 7: when delegating, forward Down/Drag/Up(Left) as SGR; skip overlay state mutation.
    if in_terminal && app.active_session_delegates_to_tmux() {
        let inner = terminal_content_rect(app.last_panes.as_ref().unwrap().terminal);
        let local_col = mouse.column.saturating_sub(inner.x);
        let local_row = mouse.row.saturating_sub(inner.y);
        let forwarded = matches!(
            mouse.kind,
            MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left)
        );
        if forwarded {
            if let Some(bytes) = encode_sgr_mouse(mouse.kind, mouse.modifiers, local_col, local_row) {
                app.write_active_tab_input(&bytes);
            }
            // Update tmux_in_copy_mode flag (per state machine in §State Source).
            // Do NOT mark_dirty — tmux's own PTY output triggers redraw via existing path.
            return;
        }
    }

    // [...existing Phase 6 overlay path runs unchanged when delegate == false ...]
}
```

---

### `src/events.rs::handle_key` — cmd+c precedence chain (3-tier) + Esc fallback

**Analog:** `src/events.rs:387-413` — current Phase 6 cmd+c (Tier 1 + Tier 3) + Esc-with-selection branches.

**Existing precedence chain to extend** (`src/events.rs:387-413`):
```rust
// D-02, D-03: cmd+c with selection re-copies; without selection in Terminal mode forwards SIGINT.
if key.code == KeyCode::Char('c')
    && key.modifiers.contains(KeyModifiers::SUPER)
{
    if let Some(sel) = &app.selection {
        if !sel.is_empty() {
            app.copy_selection_to_clipboard();
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

**Extension pattern** (RESEARCH Example 2 — insert Tier 2 between Tier 1 and Tier 3 of cmd+c; Esc forward when delegating):
```rust
if key.code == KeyCode::Char('c')
    && key.modifiers.contains(KeyModifiers::SUPER)
{
    // Tier 1 — overlay selection (Phase 6 D-02, unchanged):
    if let Some(sel) = &app.selection {
        if !sel.is_empty() {
            app.copy_selection_to_clipboard();
            return;
        }
    }
    // Tier 2 — NEW (Phase 7 D-10): tmux paste-buffer if delegating.
    if app.active_session_delegates_to_tmux() {
        if let Some(session_name) = app.active_tmux_session_name() {
            let session_name_clone = session_name.clone();
            tokio::task::spawn_blocking(move || {
                crate::tmux::save_buffer_to_pbcopy(&session_name_clone);
            });
            return;
        }
    }
    // Tier 3 — Phase 6 D-03 (unchanged): SIGINT in Terminal mode.
    if app.mode == InputMode::Terminal {
        app.write_active_tab_input(&[0x03]);
        return;
    }
}

// Esc: existing overlay-clear branch unchanged. Then NEW — forward Esc byte to PTY when
// delegating + tmux is in copy-mode (overrides default vi-mode clear-selection via our
// `bind-key -T copy-mode-vi Escape send-keys -X cancel` config addition).
if key.code == KeyCode::Esc
    && key.modifiers == KeyModifiers::NONE
{
    if app.selection.is_some() {
        app.selection = None;
        app.mark_dirty();
        return;
    }
    // Phase 7 D-14: forward Esc byte to delegating session in copy-mode.
    if app.active_session_delegates_to_tmux() && app.tmux_in_copy_mode() {
        app.write_active_tab_input(&[0x1b]);
        // Locally clear the flag — tmux's `cancel` will exit copy-mode.
        app.tmux_in_copy_mode_set(false);
        return;
    }
}
```

---

### `src/pty/session.rs` — optional `tmux_in_copy_mode: Arc<AtomicBool>` field

**Analog:** `src/pty/session.rs:18-31` — existing struct shape including the `scroll_generation: Arc<AtomicU64>` field added in Phase 6. The Phase 7 flag mirrors that exact pattern.

**Existing field-on-struct + Arc pattern** (`src/pty/session.rs:18-31`):
```rust
pub struct PtySession {
    pub id: u64,
    pub parser: Arc<RwLock<vt100::Parser>>,
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    status: Arc<Mutex<PtyStatus>>,
    pub exit_rx: Option<oneshot::Receiver<i32>>,
    pub last_output: Arc<Mutex<std::time::Instant>>,
    /// Per-session counter incremented by the PTY reader thread when a
    /// vertical scroll is inferred (see RESEARCH §Q1 SCROLLBACK-LEN
    /// heuristic). Plans 06-03 (drag anchor) and 06-05 (render
    /// translation) read this to keep selections stable across scroll.
    pub scroll_generation: Arc<std::sync::atomic::AtomicU64>,
}
```

**Existing init + clone pattern** (`src/pty/session.rs:75-76`):
```rust
let scroll_gen = Arc::new(std::sync::atomic::AtomicU64::new(0));
let scroll_gen_clone = Arc::clone(&scroll_gen);
```

**Extension pattern** (RESEARCH §State Source — Option (a) Martins-side state machine):
```rust
pub struct PtySession {
    // ...existing fields...
    pub scroll_generation: Arc<std::sync::atomic::AtomicU64>,
    /// Phase 7: set on forwarded Down(Left) when delegating; cleared on
    /// Up(Left) without prior Drag, on tab-switch, on Esc-cancel, and on
    /// observed copy-mode-exit. Read by handle_key Esc / tab-switch /
    /// click-outside handlers to decide whether to forward `\x1b` byte
    /// or run `tmux send-keys -X cancel`.
    pub tmux_in_copy_mode: Arc<std::sync::atomic::AtomicBool>,
    /// Phase 7: transient flag — set on forwarded Drag(Left); read+clear
    /// on Up(Left) to distinguish "click without drag" (release sets
    /// in_copy_mode=false) from "click→drag→release" (in_copy_mode stays
    /// true because tmux entered copy-mode on the drag).
    pub tmux_drag_seen: Arc<std::sync::atomic::AtomicBool>,
}
```

Init mirrors `scroll_gen` exactly:
```rust
let tmux_in_copy_mode = Arc::new(std::sync::atomic::AtomicBool::new(false));
let tmux_drag_seen = Arc::new(std::sync::atomic::AtomicBool::new(false));
// ... pass to Self at the end alongside scroll_generation ...
```

**Note:** RESEARCH OQ-1 recommends this Martins-side flag for hot-path Esc; tab-switch can use unconditional `cancel_copy_mode` (fire-and-forget) instead. Plan should adopt the flag for Esc; subprocess for tab-switch.

**No PTY-drain edits required.** vt100 already exposes mouse-mode via `screen.mouse_protocol_mode()` and `screen.alternate_screen()` (RESEARCH §State Source) — there is NO byte-scanner extension to the drain loop. CONTEXT.md D-02's proposed scanner is superseded.

---

### `src/app.rs` — new helpers: `active_session_delegates_to_tmux`, `active_tmux_session_name`, `tmux_in_copy_mode_*`, plus `set_active_tab` extension

**Analogs:**
- `src/app.rs::active_scroll_generation` (lines 526-537) — exact precedent for "read atomic field off active session".
- `src/app.rs::copy_selection_to_clipboard` (lines 473-502) — pbcopy subprocess pattern (already used by `tmux::save_buffer_to_pbcopy` mirror).
- `src/app.rs::set_active_tab` (lines 400-404) — tab-switch hook where Phase 7 cancel-on-outgoing-session lands.
- `src/app.rs::active_sessions` (lines 824-841) — multi-tab session lookup pattern.

**Existing `active_scroll_generation`** (the model for new vt100-state read helpers):
```rust
/// Read the active session's `scroll_generation` counter. Returns 0
/// when no active session is available — a safe default since gen=0 is
/// also the initial value at session spawn (Plan 02).
pub(crate) fn active_scroll_generation(&self) -> u64 {
    let sessions = self.active_sessions();
    let Some((_, session)) = sessions.get(self.active_tab) else {
        return 0;
    };
    session
        .scroll_generation
        .load(std::sync::atomic::Ordering::Relaxed)
}
```

**New helper `active_session_delegates_to_tmux`** (RESEARCH Example 4 — same shape as `active_scroll_generation`, gates on parser read):
```rust
/// Phase 7: returns true when the active session's inner program has NOT
/// requested mouse mode AND is NOT on alternate screen. In that state,
/// Down/Drag/Up(Left) events are forwarded as SGR bytes to the tmux
/// client, which owns visual feedback via its native copy-mode.
pub(crate) fn active_session_delegates_to_tmux(&self) -> bool {
    let sessions = self.active_sessions();
    let Some((_, session)) = sessions.get(self.active_tab) else {
        return false;
    };
    let Ok(parser) = session.parser.try_read() else {
        // Parser write-lock contention — fall back to overlay path for one
        // frame (harmless visual blip, if any).
        return false;
    };
    let screen = parser.screen();
    matches!(screen.mouse_protocol_mode(), vt100::MouseProtocolMode::None)
        && !screen.alternate_screen()
}
```

**New helper `active_tmux_session_name`** (RESEARCH Example 4 — wraps existing `tmux::tab_session_name`):
```rust
/// Phase 7: synthesize the `martins-{shortid}-{workspace}-{tab}` session
/// name for the active tab, for subprocess invocations
/// (`tmux save-buffer`, `tmux send-keys -X cancel`).
pub(crate) fn active_tmux_session_name(&self) -> Option<String> {
    let project = self.active_project()?;
    let workspace = self.active_workspace()?;
    let tab = workspace.tabs.get(self.active_tab)?;
    Some(crate::tmux::tab_session_name(&project.id, &workspace.name, tab.id))
}
```

**New helpers `tmux_in_copy_mode` / `tmux_in_copy_mode_set` / `tmux_drag_seen_*`** (mirror `active_scroll_generation` exactly — atomic load/store on active session):
```rust
pub(crate) fn tmux_in_copy_mode(&self) -> bool {
    let sessions = self.active_sessions();
    let Some((_, session)) = sessions.get(self.active_tab) else { return false };
    session.tmux_in_copy_mode.load(std::sync::atomic::Ordering::Relaxed)
}
pub(crate) fn tmux_in_copy_mode_set(&self, value: bool) {
    let sessions = self.active_sessions();
    if let Some((_, session)) = sessions.get(self.active_tab) {
        session.tmux_in_copy_mode.store(value, std::sync::atomic::Ordering::Relaxed);
    }
}
// (tmux_drag_seen_set / tmux_drag_seen_take follow the same shape.)
```

**Existing `set_active_tab` to extend** (`src/app.rs:400-404`):
```rust
pub(crate) fn set_active_tab(&mut self, index: usize) {
    self.clear_selection();
    self.active_tab = index;
    self.mark_dirty();
}
```

**Extension pattern** (RESEARCH §Pattern 3 + D-16 — fire-and-forget cancel on outgoing-active session BEFORE the active_tab assignment):
```rust
pub(crate) fn set_active_tab(&mut self, index: usize) {
    // Phase 7 D-16: cancel any tmux copy-mode selection on the OUTGOING
    // active session. Fire-and-forget; idempotent (exits 1 with stderr
    // "not in a mode" when no copy-mode active — discarded).
    if let Some(name) = self.active_tmux_session_name() {
        crate::tmux::cancel_copy_mode(&name);
    }
    self.clear_selection();
    self.active_tab = index;
    self.mark_dirty();
}
```

**Existing `copy_selection_to_clipboard` pbcopy pattern** (lines 492-501) — already followed by the mirror in `tmux::save_buffer_to_pbcopy`:
```rust
let _ = std::process::Command::new("pbcopy")
    .stdin(std::process::Stdio::piped())
    .spawn()
    .and_then(|mut child| {
        use std::io::Write;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(trimmed.as_bytes())?;
        }
        child.wait().map(|_| ())
    });
```

---

### `src/ui/terminal.rs` — render gate (OPTIONAL)

**Analog:** `src/ui/terminal.rs:157-199` — current Phase 6 overlay highlight pass with anchored-coord translation.

**Existing render branch** (`src/ui/terminal.rs:157-199`) — body Phase 7 considers gating:
```rust
if let Some(sel) = selection {
    if !sel.is_empty() {
        let ((sc_raw, sr_raw), (ec_raw, er_raw)) = sel.normalized();
        let start_delta = current_gen.saturating_sub(sel.start_gen);
        let end_delta = sel.end_gen
            .map(|g| current_gen.saturating_sub(g))
            .unwrap_or(0);
        // ... translate, clip, iterate cells, toggle Modifier::REVERSED ...
        for row in sr..=er {
            // ...
            for col in c_start..=c_end {
                if let Some(cell) = buf.cell_mut((inner.x + col, inner.y + row)) {
                    cell.modifier.toggle(Modifier::REVERSED);
                }
            }
        }
    }
}
```

**Phase 7 stance — DO NOT modify in tmux path.** Per CONTEXT.md D-13 and RESEARCH §State Source, when delegating to tmux, `app.selection` is `None` (no overlay mutation occurs in `handle_mouse`), so the existing `if let Some(sel) = selection` guard already short-circuits the overlay render. **No code change required** in `src/ui/terminal.rs` for the tmux path.

**Defensive option** (only if planner wants explicit safety against a stale `app.selection` from a vim→bash transition): gate the outer block on `mouse_requested == false` is unnecessary — Pitfall #2 in RESEARCH covers this via `app.clear_selection()` on mode transition, not via render-gating.

---

### `src/selection_tests.rs` (existing, extend) OR `src/tmux_native_selection_tests.rs` (new)

**Analog:** `src/pty_input_tests.rs:1-43` (real `PtySession::spawn` + `vt100` parser polling) + existing `src/selection_tests.rs` (registered at `src/main.rs:27`).

**Module registration** — already-active pattern at `src/main.rs:20-27`:
```rust
#[cfg(test)]
mod pty_input_tests;

#[cfg(test)]
mod navigation_tests;

#[cfg(test)]
mod selection_tests;
// + Phase 7 (if separate module):
// #[cfg(test)]
// mod tmux_native_selection_tests;
```

**Test header pattern** (mirror `src/pty_input_tests.rs:1-12`):
```rust
//! Phase 7 — tmux-native main-screen selection (TM-ENC-01..06, TM-DISPATCH-01..04,
//! TM-CMDC-01..03, TM-ESC-01..03, TM-CONF-01, TM-CANCEL-01).
//!
//! See `.planning/phases/07-tmux-native-main-screen-selection/07-RESEARCH.md` §Validation Architecture.

#![cfg(test)]
```

**Pure-fn unit-test pattern** (TM-ENC-01..06 — `encode_sgr_mouse` is pure, no fixture):
```rust
#[test]
fn encode_sgr_down_left_no_mods() {
    use crate::events::encode_sgr_mouse;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEventKind};
    let bytes = encode_sgr_mouse(
        MouseEventKind::Down(MouseButton::Left),
        KeyModifiers::NONE,
        9, 4,
    ).expect("forwarded event");
    assert_eq!(bytes, b"\x1b[<0;10;5M");  // 1-based: 9+1=10, 4+1=5
}

#[test]
fn encode_sgr_drag_left_shift_alt() {
    use crate::events::encode_sgr_mouse;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEventKind};
    let bytes = encode_sgr_mouse(
        MouseEventKind::Drag(MouseButton::Left),
        KeyModifiers::SHIFT | KeyModifiers::ALT,
        9, 4,
    ).expect("forwarded event");
    assert_eq!(bytes, b"\x1b[<44;10;5M");  // 32 (motion) + 4 (shift) + 8 (alt)
}
```

**Real-vt100 integration pattern** (TM-DISPATCH-01..04 — feeds `\x1b[?1006h` through real parser, then asserts `active_session_delegates_to_tmux` flips):
```rust
#[test]
fn delegate_flips_on_mouse_mode_set_reset() {
    use crate::pty::session::PtySession;
    let session = PtySession::spawn(
        std::env::temp_dir(), "/bin/cat", &[], 24, 80,
    ).expect("spawn /bin/cat");
    // Initially mouse mode == None — delegate.
    {
        let parser = session.parser.read().unwrap();
        assert_eq!(parser.screen().mouse_protocol_mode(), vt100::MouseProtocolMode::None);
    }
    // Feed DECSET 1006 directly into the parser.
    {
        let mut parser = session.parser.write().unwrap();
        parser.process(b"\x1b[?1006h");
    }
    // Now mouse_protocol_mode != None — overlay path.
    {
        let parser = session.parser.read().unwrap();
        assert_ne!(parser.screen().mouse_protocol_mode(), vt100::MouseProtocolMode::None);
    }
    // Reset.
    {
        let mut parser = session.parser.write().unwrap();
        parser.process(b"\x1b[?1006l");
    }
    {
        let parser = session.parser.read().unwrap();
        assert_eq!(parser.screen().mouse_protocol_mode(), vt100::MouseProtocolMode::None);
    }
}
```

**Inline `#[cfg(test)] mod tests`** for `src/tmux.rs::ensure_config` extension — mirror existing pattern at `src/tmux.rs:171-196`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    // existing tests: session_name_format, tab_session_name_format, ...

    #[test]
    fn ensure_config_writes_phase7_bindings() {
        let path = ensure_config();
        let conf = std::fs::read_to_string(&path).expect("read tmux.conf");
        assert!(conf.contains("bind-key -T copy-mode-vi y"));
        assert!(conf.contains("bind-key -T copy-mode-vi Enter"));
        assert!(conf.contains("bind-key -T copy-mode-vi Escape send-keys -X cancel"));
    }
}
```

---

## Shared Patterns

### Free-Function Event Handlers (Phase 1 convention)

**Source:** `src/events.rs:20-181` (`handle_event`, `handle_mouse`) + `src/events.rs:355-429` (`handle_key`).
**Apply to:** `encode_sgr_mouse` (free fn, pure); the conditional dispatch wrapper inside `handle_mouse`.

**Convention:** All event-routing logic is free `pub async fn` (or sync `pub(crate) fn` for pure helpers) in `src/events.rs`, taking `&mut App`. State-mutation helpers live as `impl App` methods in `src/app.rs`.

### Subprocess-Spawn — Fire-and-Forget vs Piped-Stdout

**Source:** `src/tmux.rs::send_key` (lines 146-152) — fire-and-forget `status()`; `src/tmux.rs::pane_command` (lines 134-144) — piped `stdout(Stdio::piped()).output()`. `src/app.rs:492-501` — pbcopy spawn-then-write-stdin.

**Apply to:**
- `tmux::cancel_copy_mode` — fire-and-forget on tab-switch / Esc fallback. **Mirror `send_key` exactly.**
- `tmux::save_buffer_to_pbcopy` — piped stdout → pbcopy stdin. **Mirror `pane_command` for the read; mirror `app.rs:492-501` for the pbcopy write.**

```rust
// Fire-and-forget (idempotent, discarding stderr):
let _ = Command::new("tmux")
    .args(["send-keys", "-X", "cancel", "-t", session])
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status();
```

### Off-Hot-Path Subprocess via `tokio::task::spawn_blocking`

**Source:** `src/app.rs::save_state_spawn` (lines 441-449) — fire-and-forget `tokio::task::spawn_blocking` for non-event-path I/O.

**Apply to:** cmd+c Tier 2 (`tmux save-buffer | pbcopy`) — user-initiated, ~5-15ms acceptable, but should not block the event loop. Wrap the synchronous `tmux::save_buffer_to_pbcopy(&name)` in `spawn_blocking`:
```rust
tokio::task::spawn_blocking(move || {
    crate::tmux::save_buffer_to_pbcopy(&session_name_clone);
});
```

### vt100-state Reads via `try_read()` + Conservative Fallback

**Source:** `src/app.rs::active_scroll_generation` (lines 526-537) + `src/app.rs::materialize_selection_text` (lines 510-524) + `src/ui/terminal.rs:147-150` (parser read with retry).

**Apply to:** `active_session_delegates_to_tmux` — wrap `parser.try_read()` in `let Ok(parser) = ... else { return false }`. On contention, return the safer default ("not delegating" = run overlay path = harmless one-frame blip). Never `parser.read().unwrap()` on the event path.

### Atomic-Counter Field on PtySession

**Source:** `src/pty/session.rs:30, 75-76, 143` — `Arc<AtomicU64>` field initialized once, cloned for each consumer; loaded via `Ordering::Relaxed`.

**Apply to:** `tmux_in_copy_mode: Arc<AtomicBool>` and `tmux_drag_seen: Arc<AtomicBool>` — same Arc-Atomic-Relaxed pattern. Initialize alongside `scroll_gen` in `PtySession::spawn_with_notify`; expose to App via `tmux_in_copy_mode_*` helpers that mirror `active_scroll_generation`.

### Dirty-Flag Discipline (Phase 6 D-23, carries forward)

**Source:** `src/app.rs::mark_dirty` (lines 170-173) + Phase 6 mutation sites.

**Apply to (Phase 7):**
- Esc forwarding to tmux PTY: **NO** `mark_dirty` — tmux's own PTY output triggers the existing drain → `output_notify` → `mark_dirty` path.
- Forwarded SGR bytes: **NO** `mark_dirty` — same reason.
- `set_active_tab`: existing `mark_dirty()` at line 403 covers tab-switch redraw.

### Real-vt100 Tests (no mocks)

**Source:** `src/pty_input_tests.rs:16-43` — real `PtySession::spawn("/bin/cat")` and parser polling.
**Apply to:** TM-DISPATCH-01..04 (mouse-mode flip) and TM-CANCEL-01 (tab-switch cancel). Use real `PtySession::spawn` + direct `parser.write().unwrap().process(bytes)` for DECSET sequences.

---

## No Analog Found

None. Every Phase 7 surface has a direct in-codebase analog from Phase 6 wiring or the existing `src/tmux.rs` subprocess patterns.

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| (none) | | | All Phase 7 surfaces extend existing code; no green-field modules. SGR encoder lives inline in `events.rs` per RESEARCH §Recommended File Modification Surface. |

---

## Metadata

**Analog search scope:**
- `src/events.rs`, `src/app.rs`, `src/tmux.rs`, `src/pty/session.rs`, `src/ui/terminal.rs`, `src/main.rs`
- `src/pty_input_tests.rs`, `src/selection_tests.rs`, `src/navigation_tests.rs`

**Files scanned:** 6 production files + 3 test files
**Pattern extraction date:** 2026-04-25
**Upstream docs:**
- `.planning/phases/07-tmux-native-main-screen-selection/07-CONTEXT.md`
- `.planning/phases/07-tmux-native-main-screen-selection/07-RESEARCH.md`
- `.planning/phases/06-text-selection/06-PATTERNS.md` (direct ancestor — same files, prior wave)

## PATTERN MAPPING COMPLETE

**Phase:** 07 — tmux-native main-screen selection
**Files classified:** 5 modified (no new files)
**Analogs found:** 5 / 5

### Coverage
- Files with exact analog: 5
- Files with role-match analog: 0
- Files with no analog: 0

### Key Patterns Identified
1. **Conditional dispatch in `handle_mouse`** — wrap existing Phase 6 overlay path with a `if delegate { forward SGR } else { ... existing ... }` short-circuit, gated on `app.active_session_delegates_to_tmux()`. No deletion of Phase 6 code.
2. **SGR encoder as pure free fn** in `src/events.rs` — mirrors existing inline encodes at lines 195-196 (scroll wheel) and 256-257 (sidebar click); no new module file.
3. **Subprocess pattern split** — fire-and-forget `cancel_copy_mode` mirrors `tmux::send_key`; piped-stdout `save_buffer_to_pbcopy` mirrors `tmux::pane_command` + `app.rs:492-501` pbcopy spawn. cmd+c Tier 2 wraps in `tokio::task::spawn_blocking` (mirrors `save_state_spawn`).
4. **vt100-state read** via `screen.mouse_protocol_mode()` + `screen.alternate_screen()` — replaces CONTEXT.md D-02's proposed PTY-drain byte scanner. Helper `active_session_delegates_to_tmux` mirrors `active_scroll_generation` shape exactly.
5. **`Arc<AtomicBool>` flag on PtySession** for `tmux_in_copy_mode` — mirrors `scroll_generation` field exactly (same Arc-clone-init, same Relaxed ordering, same `active_*` accessor pattern).

### File Created
`.planning/phases/07-tmux-native-main-screen-selection/07-PATTERNS.md`

### Ready for Planning
Pattern mapping complete. Planner can now reference analog files + line ranges in PLAN.md actions.
