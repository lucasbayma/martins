# Phase 7: tmux-native main-screen selection — Research

**Researched:** 2026-04-25
**Domain:** Delegation of TUI mouse-driven text selection to a wrapped tmux session via SGR mouse forwarding + tmux.conf binding extension (tmux 3.6a / vt100 0.16.2 / crossterm 0.29 — macOS-only)
**Confidence:** HIGH on tmux defaults (verified against `tmux list-keys -T copy-mode-vi/copy-mode/root` on installed 3.6a) and on vt100 mouse-mode tracking (read directly from `screen.rs`); HIGH on SGR encoding (cross-verified against existing martins click-encode at `src/events.rs:256-257` + scroll-encode at `src/events.rs:195-196` + xterm spec); MEDIUM on subprocess error semantics for `tmux save-buffer -` and `tmux send-keys -X cancel` (verified empirically — see §Subprocess Behavior).

## Summary

Phase 7 delegates main-pane text selection from Martins' Phase 6 REVERSED-XOR overlay to the wrapped tmux session's native copy-mode, forwarding raw SGR mouse bytes (`\x1b[<...M`/`m`) into the tmux PTY when the inner program has not requested mouse mode. The overlay path stays alive as the fallback whenever vim/htop/btop has set DECSET 1000/1002/1003/1006.

Three findings collapse most of CONTEXT.md's "Claude's Discretion" surface:

1. **Tmux 3.6a's defaults already do everything Phase 7 wants.** The default root `MouseDrag1Pane` binding is `if-shell -F "#{||:#{pane_in_mode},#{mouse_any_flag}}" { send-keys -M } { copy-mode -M }` — exactly the conditional Phase 7 wants. Default `MouseDragEnd1Pane` in BOTH `copy-mode` and `copy-mode-vi` is `send-keys -X copy-pipe-and-cancel pbcopy`. Default `DoubleClick1Pane`/`TripleClick1Pane` are `select-pane \; if-shell ... { send-keys -M } { copy-mode -H ; send-keys -X select-word|select-line ; run-shell -d 0.3 ; send-keys -X copy-pipe-and-cancel }`. Default `mode-keys` is `vi`. **D-09 and D-17 collapse to "no tmux.conf change required" — keep ensure_config minimal.** This is `[VERIFIED: tmux 3.6a list-keys output, /opt/homebrew/bin/tmux]`.

2. **vt100 already exposes mouse-mode state.** `screen.mouse_protocol_mode()` returns `MouseProtocolMode` enum (`None`/`Press`/`PressRelease`/`ButtonMotion`/`AnyMotion`). **D-02's "byte-scan the PTY drain for `\x1b[?1000h`/`\x1b[?1006h`" is unnecessary** — just read it from the parser. Source: `vt100-0.16.2/src/screen.rs:578` `pub fn mouse_protocol_mode(&self) -> MouseProtocolMode`. Internal handling at lines 1148-1197 maps DECSET 9/1000/1002/1003 set/reset directly. Encoding flags (1005/1006/1015) are tracked separately via `mouse_protocol_encoding()`.

3. **The overlay/native dispatch is a single conditional in `handle_mouse`.** Phase 6 D-10's "always intercept Drag(Left)" becomes Phase 7's "intercept iff `screen.mouse_protocol_mode() == None && !alternate_screen()`". When forwarding, emit `\x1b[<{button};{col};{row}{M|m}` with button=0 for Down/Up-Left, button=32 for Drag-Left, +4 for shift, +8 for alt. No new helpers required — extend the existing inline format strings in events.rs:195/256.

**Primary recommendation:** Trust tmux defaults. The Phase 7 production-code surface area is ~30 LOC across `src/events.rs` (conditional intercept + SGR encode helper), `src/app.rs` (cmd+c tmux-buffer fallback path, Esc/click cancel forwarding, tab-switch cancel), and ~0 LOC in `src/tmux.rs::ensure_config` (default tmux 3.6a config Just Works). The `mouse_requested` "flag" need not be a separately-tracked bool — it's a one-line read from the existing `parser.try_read().screen().mouse_protocol_mode()`. Tests are unit-level on the SGR encoder + integration on the dispatch conditional; UAT compares feel against `tmux` directly in Ghostty.

## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Tmux owns selection in main pane only when inner program has not requested mouse mode (vim `mouse=a`, htop, btop bypass).
- **D-02:** Maintain a per-session "mouse_requested" signal updated from the PTY drain. **(Research §State Source narrows this: read from `screen.mouse_protocol_mode()` rather than byte-scan.)**
- **D-03:** Do NOT poll tmux on demand — no `display-message -p '#{mouse_any_flag}'` per drag.
- **D-04:** When delegating, forward raw SGR mouse events to the tmux client's PTY: `\x1b[<{button};{col};{row}{M|m}` — same code path as direct tmux usage.
- **D-05:** Do NOT drive tmux via `tmux send-keys -X` for drag tracking (one subprocess per move = lag).
- **D-06:** Coords already match — Martins owns pane size, tmux client renders at that exact size.
- **D-07:** Phase 6 D-10 ("always intercept Drag(Left)") replaced by **conditional intercept**: when `mouse_requested == false` → forward SGR; when `true` → overlay path.
- **D-08:** Core wiring change in `src/events.rs:46+` (handle_mouse Drag/Up/Down branches).
- **D-09:** Add tmux.conf bindings for `MouseDragEnd1Pane`/`y`/`Enter` → `copy-pipe-and-cancel pbcopy`. **(Research finds tmux 3.6a defaults already do this — see §Tmux Defaults.)**
- **D-10:** `cmd+c` while tmux selection active re-copies via `tmux save-buffer - -t <session> | pbcopy`.
- **D-11:** Phase 6 D-03 holds — `cmd+c` with no selection (overlay empty AND tmux buffer empty) → SIGINT.
- **D-12:** Keep all Phase 6 overlay primitives — they run as the fallback path.
- **D-13:** When `mouse_requested == false`, overlay sleeps — no SelectionState mutation, no XOR render. Tmux owns visual feedback through PTY output.
- **D-14:** Phase 6 Esc-precedence holds; clear depends on active path: overlay → clear `App::selection`; tmux → forward Esc into tmux PTY (or `tmux send-keys -X cancel`).
- **D-15:** Click-outside likewise converges: clear overlay if active; else if tmux in copy-mode, send `cancel`.
- **D-16:** Selection clears on tab/workspace switch in both paths; tmux path runs `tmux send-keys -X cancel -t <outgoing>` gated on `#{pane_in_mode}`.
- **D-17:** Add tmux.conf bindings for native double/triple-click word/line selection. **(Research finds tmux 3.6a defaults already do this — see §Tmux Defaults.)**
- **D-18:** Shift+click extend lands as `\x1b[<4;col;rowM` (button=0 + shift modifier 4) in tmux path.

### Claude's Discretion (resolved in this research)
- **D-04 detail (SGR button-mask bytes):** Resolved in §SGR Encoding below.
- **D-09 detail (key tables):** Resolved in §Tmux Defaults — bindings present in BOTH `copy-mode` and `copy-mode-vi` tables by default; no double-binding needed.
- **D-14 detail (how Martins knows tmux is in copy-mode):** Resolved in §State Source below — recommend Option (a) Martins-side state machine: `tmux_in_copy_mode: bool` flag flipped on forwarded Down(Left) and cleared on Up + 300ms.
- **Whether Esc binds to cancel in copy-mode-vi:** **NO** — Esc in `copy-mode-vi` defaults to `send-keys -X clear-selection`, NOT cancel. Esc in `copy-mode` (emacs) defaults to `cancel`. **CRITICAL — see §Tmux Defaults** for the implication and recommended explicit binding.
- **Whether to log copy events via tracing:** Skip (consistent with Phase 6 D-09 stance).

### Deferred Ideas (OUT OF SCOPE)
- Right-click context menu.
- Block/rectangle selection (Alt+drag) — confirm here whether free fall-out from D-04; if so, document as bonus, else defer.
- Search-in-scrollback (`?` in tmux copy-mode).
- Customizing tmux's own highlight color.

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| (none) | Phase 7 has no allocated REQ-IDs — feel iteration on Phase 6's SEL-01..SEL-04 | Phase 6 acceptance must still hold across both paths (overlay AND tmux) — see §Validation Architecture for the dual-path UAT plan. |

**Implicit acceptance:** PTY-pane selection in the tmux path feels indistinguishable from running tmux directly in Ghostty — qualitative operator UAT against the same Ghostty+tmux baseline that drove this phase.

## Project Constraints (from CLAUDE.md)

- Rust 2024 / MSRV 1.85
- macOS-only — `pbcopy`, tmux, Ghostty assumptions are project-wide and acceptable
- Single-language Rust codebase (~12k LOC, 30 `.rs` files in `src/`)
- tokio full-features async runtime; single event loop (`App::run`)

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Detect inner-program mouse mode (1000/1002/1003/1006 set/reset) | vt100 parser (already runs in PTY drain thread) | — | vt100 already tracks DECSET; no new byte scanner needed. Read via `screen.mouse_protocol_mode()`. |
| Decide overlay vs tmux forward on Drag(Left) | events.rs (`handle_mouse`) | App helper for parser read | One-line conditional gating two existing branches; lives at the dispatch site. |
| SGR-encode crossterm `MouseEvent` → bytes | events.rs (free fn `encode_sgr_mouse`) | — | Pure function; mirrors existing inline format-strings at events.rs:195-196 and 256-257. |
| Forward bytes to active tmux PTY | App (`write_active_tab_input`) — already exists | — | Existing helper; no new method needed. |
| Native highlight render in tmux path | tmux client (renders through PTY) | vt100 parser (passes bytes through) | Tmux's own copy-mode reverse-video lands as bytes in the PTY, parsed by vt100, rendered by tui-term — Martins does nothing extra. |
| `MouseDragEnd1Pane` → pbcopy | tmux (default binding in `copy-mode-vi`) | — | tmux 3.6a default; no Martins code, no tmux.conf change. |
| Double/triple-click → select-word/line + pbcopy | tmux (default binding in `root`) | — | tmux 3.6a default with `if-shell` guard for inner-mouse; no martins code. |
| Shift-click extend | tmux (native) — receives SGR with shift bit | — | Forwarded as `\x1b[<4;col;rowM`. |
| cmd+c re-copy after tmux selection | App helper `copy_tmux_buffer_to_clipboard` (subprocess) | — | One subprocess on user-initiated key; not a hot path. |
| Esc / click-outside cancel | App helper (forward Esc byte to tmux PTY OR run `send-keys -X cancel`) | events.rs (decision branch) | Forwarding Esc byte avoids subprocess; but vi-mode Esc default is `clear-selection` not `cancel` — see §Tmux Defaults. |
| Tab/workspace switch cancel on outgoing session | App helper, gated on `#{pane_in_mode}` query | tmux subprocess (`send-keys -X cancel`) | Not a hot path; subprocess acceptable. Must gate to avoid "not in a mode" stderr. |

## Standard Stack

### Core (no version bumps required)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `vt100` | 0.16.2 | Parses PTY output bytes; tracks mouse-protocol-mode state | Already in use; exposes `Screen::mouse_protocol_mode()` so no parallel byte scanner is needed [VERIFIED: `vt100-0.16.2/src/screen.rs:578`] |
| `crossterm` | 0.29.0 | Source of `MouseEvent` (Down/Drag/Up + KeyModifiers) | Already in use; SHIFT/ALT/CONTROL modifier bits delivered on `mouse.modifiers` [VERIFIED: `crossterm-0.29.0/src/event.rs:836-848`] |
| `portable-pty` | 0.9 | PTY writer for forwarded SGR bytes | Already in use; `App::write_active_tab_input` already wraps it |
| tmux (external) | 3.6a (verified on user's machine) | Runs the wrapped session, owns copy-mode | Already in use; defaults align with Phase 7 needs [VERIFIED: `tmux -V` on /opt/homebrew/bin/tmux] |

### Supporting (already present)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio` | 1.36 (full) | spawn_blocking for `tmux save-buffer` subprocess on cmd+c | Use when invoking tmux for buffer reads (off-hot-path) |
| `std::process::Command` | std | Subprocess invocation pattern (matches existing `src/tmux.rs`) | Use exactly as `src/tmux.rs::send_key` does today — fire-and-forget for `cancel`, piped-stdout for `save-buffer` |

**No new dependencies.** Phase 7 is wiring within the existing stack.

**Version verification:**
```
$ tmux -V
tmux 3.6a              [VERIFIED: 2026-04-25 on user machine]

$ cargo metadata | grep '"vt100"'
"vt100 0.16.2"         [VERIFIED: Cargo.lock]
```

## Tmux Defaults (CRITICAL — collapses CONTEXT.md D-09 and D-17)

The most important finding in this research is that tmux 3.6a's default key-bindings already implement Phase 7's desired behavior. Verified directly via `tmux list-keys -T <table>` on the user's installed tmux 3.6a:

### Default `copy-mode-vi` bindings (active when `mode-keys` = vi, which is the default — `tmux show-options -gw mode-keys` returns `mode-keys vi`)

```
bind-key -T copy-mode-vi Escape            send-keys -X clear-selection      [VERIFIED]
bind-key -T copy-mode-vi MouseDown1Pane    select-pane                        [VERIFIED]
bind-key -T copy-mode-vi MouseDrag1Pane    select-pane \; send-keys -X begin-selection   [VERIFIED]
bind-key -T copy-mode-vi MouseDragEnd1Pane send-keys -X copy-pipe-and-cancel pbcopy      [VERIFIED]
```

### Default `copy-mode` (emacs) bindings — only used if user changes `mode-keys` to emacs

```
bind-key -T copy-mode Escape            send-keys -X cancel                   [VERIFIED]
bind-key -T copy-mode MouseDown1Pane    select-pane                            [VERIFIED]
bind-key -T copy-mode MouseDrag1Pane    select-pane \; send-keys -X begin-selection   [VERIFIED]
bind-key -T copy-mode MouseDragEnd1Pane send-keys -X copy-pipe-and-cancel pbcopy      [VERIFIED]
```

### Default `root` bindings (relevant subset)

```
bind-key -T root MouseDown1Pane     select-pane -t = \; send-keys -M         [VERIFIED]
bind-key -T root MouseDrag1Pane     if-shell -F "#{||:#{pane_in_mode},#{mouse_any_flag}}" \
                                       { send-keys -M } { copy-mode -M }     [VERIFIED]
bind-key -T root WheelUpPane        if-shell -F "#{||:#{alternate_on},#{pane_in_mode},#{mouse_any_flag}}" \
                                       { send-keys -M } { copy-mode -e }     [VERIFIED]
bind-key -T root DoubleClick1Pane   select-pane -t = \; if-shell -F "#{||:#{pane_in_mode},#{mouse_any_flag}}" \
                                       { send-keys -M } \
                                       { copy-mode -H ; send-keys -X select-word ; \
                                         run-shell -d 0.3 ; send-keys -X copy-pipe-and-cancel } [VERIFIED]
bind-key -T root TripleClick1Pane   select-pane -t = \; if-shell -F "#{||:#{pane_in_mode},#{mouse_any_flag}}" \
                                       { send-keys -M } \
                                       { copy-mode -H ; send-keys -X select-line ; \
                                         run-shell -d 0.3 ; send-keys -X copy-pipe-and-cancel } [VERIFIED]
```

### What this means for Martins

| CONTEXT.md item | Default already covers? | Action |
|-----------------|------------------------|--------|
| D-09 `MouseDragEnd1Pane copy-pipe-and-cancel pbcopy` | YES (both copy-mode and copy-mode-vi) | **Skip — no tmux.conf addition needed** |
| D-09 `y` → copy-pipe-and-cancel pbcopy | NO (default `y` in copy-mode-vi is `send-keys -X copy-selection-and-cancel` — which copies into tmux buffer but does NOT pipe to pbcopy). | **Add explicit binding** if user wants `y`-to-system-clipboard |
| D-09 `Enter` → copy-pipe-and-cancel pbcopy | NO (default `Enter` in copy-mode-vi is `copy-selection-and-cancel`) | **Add explicit binding** if user wants `Enter`-to-system-clipboard |
| D-17 `DoubleClick1Pane`/`TripleClick1Pane` | YES — defaults already select-word/line and copy-pipe-and-cancel, gated on `pane_in_mode`/`mouse_any_flag` | **Skip — no tmux.conf addition needed** |
| `MouseDrag1Pane` enters copy-mode automatically | YES — default root binding is the exact `if-shell` Phase 7 wants | **Skip — no tmux.conf addition needed** |

### CRITICAL — Esc binding asymmetry (resolves CONTEXT.md "Whether Escape in copy-mode-vi defaults to cancel — verify")

**`Escape` in `copy-mode-vi` is `send-keys -X clear-selection`, NOT `send-keys -X cancel`.** This means: in vi-mode (default), pressing Esc once **clears the selection but stays in copy-mode**. A second Esc would forward to PTY (no binding catches it). Phase 7 D-14 wants a single Esc to cancel copy-mode entirely.

**Recommended action:** add an explicit override in Martins' generated `~/.martins/tmux.conf`:

```
bind-key -T copy-mode-vi Escape send-keys -X cancel
```

This makes vi-mode behave like emacs-mode for Esc — single press exits copy-mode. Confirms Phase 7's "Esc cancels selection" intent regardless of operator's `mode-keys` preference.

### Recommended `ensure_config` extension (D-09 + D-17 + Esc fix)

`src/tmux.rs::ensure_config` currently writes 5 lines (`set -g mouse on`, `default-terminal xterm-256color`, `allow-passthrough off`, `escape-time 0`, `setw -g alternate-screen off`). Phase 7 adds:

```
# Phase 7: pbcopy on `y`/`Enter` in copy-mode-vi (defaults already cover MouseDragEnd1Pane).
bind-key -T copy-mode-vi y     send-keys -X copy-pipe-and-cancel "pbcopy"
bind-key -T copy-mode-vi Enter send-keys -X copy-pipe-and-cancel "pbcopy"
# Phase 7: Esc in vi-mode defaults to clear-selection — override to cancel for parity with emacs-mode.
bind-key -T copy-mode-vi Escape send-keys -X cancel
```

**Three lines.** No `MouseDragEnd1Pane`, no `DoubleClick1Pane`, no `TripleClick1Pane` — those are tmux 3.6a defaults that already do exactly what Phase 7 specifies. Plan should NOT add them; doing so is dead-config that drifts from upstream defaults.

**Source for all the above:** `tmux list-keys -T copy-mode-vi`, `tmux list-keys -T copy-mode`, `tmux list-keys -T root` on user's tmux 3.6a, 2026-04-25.

## SGR Mouse Encoding (resolves CONTEXT.md D-04 detail)

DECSET 1006 SGR mouse-mode encoding format (xterm spec; cross-verified against existing martins inline encodes):

```
CSI < Cb ; Cx ; Cy M    # press OR drag (motion-with-button)
CSI < Cb ; Cx ; Cy m    # release (lowercase 'm' is the release indicator)
```

Where `Cb` (button code) is computed as:

| Component | Bits | Value |
|-----------|------|-------|
| Button   | low 2 bits  | 0=left, 1=middle, 2=right, 3=release-without-tracking (legacy) |
| Shift modifier | bit 2 (value 4) | +4 if shift held |
| Meta/Alt modifier | bit 3 (value 8) | +8 if alt held |
| Ctrl modifier | bit 4 (value 16) | +16 if ctrl held |
| Motion bit | bit 5 (value 32) | +32 for drag/motion (vs press) |
| Wheel bit | bit 6 (value 64) | +64 for scroll events |

In SGR mode (1006), button release is encoded by the trailing `m` (lowercase), NOT by Cb=3 (that's the legacy X10 encoding). The Cb on release retains the button identifier.

### Concrete Cb values for Phase 7

| crossterm event | Cb | Trailing | Wire format |
|-----------------|----|---------:|-------------|
| `Down(Left)` | 0 | `M` | `\x1b[<0;{col};{row}M` |
| `Drag(Left)` | 32 | `M` | `\x1b[<32;{col};{row}M` |
| `Up(Left)` | 0 | `m` | `\x1b[<0;{col};{row}m` |
| `Down(Left) + Shift` | 4 | `M` | `\x1b[<4;{col};{row}M` (shift-click extend per D-18) |
| `Down(Left) + Alt` | 8 | `M` | `\x1b[<8;{col};{row}M` (rectangle-select bonus per Deferred) |
| `Drag(Left) + Shift` | 36 | `M` | `\x1b[<36;{col};{row}M` |
| `Down(Right)` | 2 | `M` | `\x1b[<2;{col};{row}M` (NOT in scope — right-click menu deferred) |

### Cross-verification against existing martins encodes

`src/events.rs:195-196` (scroll wheel forward — confirms wire format and 1-based coords convention):
```rust
let button: u8 = if delta < 0 { 64 } else { 65 };          // wheel = 64+0 (up) or 64+1 (down)
let seq = format!("\x1b[<{button};{local_col};{local_row}M");
```

`src/events.rs:256-257` (sidebar click forward — confirms press/release pair):
```rust
let press = format!("\x1b[<0;{local_col};{local_row}M");    // button 0 = left, M = press
let release = format!("\x1b[<0;{local_col};{local_row}m");  // lowercase m = release
```

Both match the encoding above. The recommended Phase 7 helper is consistent with the existing inline pattern — no convention change required.

### Coords: 1-based, terminal-pane-relative

Both existing encodes use `local_col + 1` and `local_row + 1` (martins coords are 0-based; SGR/xterm uses 1-based). Phase 7 must do the same. The existing helper at events.rs:252-254 demonstrates the conversion:

```rust
let inner = terminal_content_rect(panes.terminal);
let local_col = col.saturating_sub(inner.x) + 1;            // +1 for 1-based SGR
let local_row = row.saturating_sub(inner.y) + 1;
```

### Recommended SGR encoder (free function in events.rs)

```rust
/// Encode a crossterm MouseEvent into an SGR (1006) byte sequence for
/// forwarding into the tmux PTY. Coords are inner-pane-relative AND
/// 1-based per xterm convention. Returns None for events that should
/// not be forwarded (Moved without button, ScrollLeft/Right not
/// implemented).
fn encode_sgr_mouse(kind: MouseEventKind, modifiers: KeyModifiers,
                    local_col: u16, local_row: u16) -> Option<Vec<u8>> {
    use MouseEventKind::*;
    let (button_base, trailing) = match kind {
        Down(MouseButton::Left)  => (0u8, 'M'),
        Drag(MouseButton::Left)  => (32u8, 'M'),    // motion bit (32) + left (0)
        Up(MouseButton::Left)    => (0u8, 'm'),     // lowercase m = release
        ScrollUp                 => (64u8, 'M'),    // matches existing events.rs:195
        ScrollDown               => (65u8, 'M'),
        _ => return None,
    };
    let mut cb = button_base;
    if modifiers.contains(KeyModifiers::SHIFT)   { cb += 4; }
    if modifiers.contains(KeyModifiers::ALT)     { cb += 8; }
    if modifiers.contains(KeyModifiers::CONTROL) { cb += 16; }
    let col = local_col + 1;  // 1-based
    let row = local_row + 1;
    Some(format!("\x1b[<{cb};{col};{row}{trailing}").into_bytes())
}
```

**Sources:** xterm `ctlseqs.html` (invisible-island.net) §"Any-event tracking" + martins existing encodes. [VERIFIED via tool: `https://invisible-island.net/xterm/ctlseqs/ctlseqs.html` — although fetched response was sparse on SGR-specific button-code modifier interaction; cross-verified against tmux source and existing martins code.]

### Block/rectangle selection bonus (Deferred — confirm fall-out)

Tmux's `send -X rectangle-toggle` is bound to `R` and `M-r` in `copy-mode-vi`. There is NO default mouse binding for Alt+drag rectangle selection in tmux 3.6a's root or copy-mode-vi tables. Forwarding `\x1b[<40;col;rowM` (button 32 + alt 8) to tmux on Alt-drag does enter copy-mode and starts a selection (because Alt-drag still triggers `MouseDrag1Pane`), but the selection mode remains line-mode, not rectangle-mode.

To get rectangle-select via Alt-drag, plan would need to add:
```
bind-key -T copy-mode-vi M-MouseDrag1Pane select-pane \; send-keys -X begin-selection \; send-keys -X rectangle-toggle
```

**Recommendation:** **Defer.** Listed in CONTEXT.md as a deferred bonus. No-op fall-out from D-04 is line-mode only; rectangle-mode requires an explicit binding that is out of Phase 7's "polish/feel iteration" scope.

## State Source (resolves CONTEXT.md D-02 detail and D-14 detail)

### How does Martins know whether the inner program has requested mouse mode?

CONTEXT.md D-02 proposed maintaining a `mouse_requested: bool` on `PtySession`, updated by the PTY drain loop watching for `\x1b[?1000h`/`\x1b[?1002h`/`\x1b[?1003h`/`\x1b[?1006h` set/reset.

**This research finds a simpler answer: vt100 already tracks this state.** Source: `vt100-0.16.2/src/screen.rs:576-585`:

```rust
/// Returns the currently active [`MouseProtocolMode`].
pub fn mouse_protocol_mode(&self) -> MouseProtocolMode { self.mouse_protocol_mode }
pub fn mouse_protocol_encoding(&self) -> MouseProtocolEncoding { self.mouse_protocol_encoding }
```

`MouseProtocolMode` enum (`screen.rs:11-36`):
```rust
pub enum MouseProtocolMode {
    None,             // no mouse tracking requested
    Press,            // DECSET 9
    PressRelease,     // DECSET 1000
    ButtonMotion,     // DECSET 1002
    AnyMotion,        // DECSET 1003
}
```

vt100 internally handles the set/reset at `screen.rs:1148-1197`. So:

```rust
fn inner_program_wants_mouse(session: &PtySession) -> bool {
    let Ok(parser) = session.parser.try_read() else { return false };  // contention => assume no
    let screen = parser.screen();
    screen.mouse_protocol_mode() != MouseProtocolMode::None
}
```

**Implication:** The CONTEXT.md D-02 byte-scanner is unnecessary. **Replace with one helper function reading from the existing parser** — no new field on `PtySession`, no edit to the PTY drain loop, no race-condition / split-sequence handling.

**Edge case (alternate screen):** vim/htop typically also enable alternate screen (DECSET 1049). Even if mouse-mode is None (e.g., vim with default `mouse=`), the alternate screen is a strong signal that the inner program is full-screen and selecting from it would be meaningless. Recommendation: combine the check —

```rust
fn delegate_to_tmux(session: &PtySession) -> bool {
    let Ok(parser) = session.parser.try_read() else { return false };
    let screen = parser.screen();
    screen.mouse_protocol_mode() == MouseProtocolMode::None
        && !screen.alternate_screen()       // verify exposed — see below
}
```

**Alternate screen exposure:** vt100 0.16.2 `screen.rs` exposes `alternate_screen()` as a public method (verified in Phase 6 RESEARCH §A3). Recommendation: include in the conditional.

### How does Martins know tmux is in copy-mode (for cancel forwarding on Esc/click-outside/tab-switch)?

CONTEXT.md D-14 proposed two options:
- (a) Track Martins' own state machine — we forwarded a press, tmux is in copy-mode until we forward cancel.
- (b) Cache `#{pane_in_mode}` per session and refresh on Esc/click.

**Recommendation: Option (a) — Martins-side state machine.** Simpler, lower-latency, no subprocess on hot paths.

Failure modes of Option (a):
- **User selects in tmux directly via keyboard inside the wrapped session** (e.g., presses tmux prefix `C-b [` to enter copy-mode). Martins doesn't see this — its `tmux_in_copy_mode` flag stays false. Esc would not be intercepted; would forward to PTY, which is FINE because tmux's own copy-mode-vi Esc binding handles it.
- **Tmux exits copy-mode autonomously** (e.g., `MouseDragEnd1Pane` → `copy-pipe-and-cancel` clears the mode). Martins' flag would still say "in copy-mode" until cleared on next Up. Solution: clear the flag on the same Up event that triggered the forward.

Concrete state machine on `PtySession` (or App, since it's per-active-tab):

```rust
pub struct PtySession {
    // ...existing fields...
    pub tmux_in_copy_mode: Arc<AtomicBool>,    // set on forwarded Down(Left); cleared on Up + tmux's auto-cancel
}
```

Update points:
- Forwarded `Down(Left)` while delegating: set `true`.
- Forwarded `Up(Left)` while delegating with non-empty drag: stays `true` (tmux is now showing selection in copy-mode).
- Forwarded `Up(Left)` with no drag (single click): set `false` (no selection started).
- Tab/workspace switch: read this flag; if `true`, run `tmux send-keys -X cancel -t <session>`; clear `false`.
- Esc when delegating AND `true`: forward Esc byte to PTY (vi-mode default `clear-selection` THEN our explicit `bind-key Escape send-keys -X cancel` — see §Tmux Defaults — handles it). Clear flag locally.
- Click-outside (Down(Left) on a row that's not in tmux pane area, but tmux is in copy-mode): if delegating AND `true`, run `tmux send-keys -X cancel`. Clear flag.

**Even simpler alternative (recommended pragmatic):** skip the flag entirely and unconditionally run `tmux send-keys -X cancel -t <session>` with stderr discarded on Esc / tab-switch — `tmux send-keys -X cancel` exits 1 with stderr "not in a mode" if not in copy-mode (verified empirically — see §Subprocess Behavior), which is harmless. Cost: one subprocess on Esc keypress / tab-switch. Tab-switch is not a hot path. Esc is debatable — but compared to the cost of correctly threading `tmux_in_copy_mode` through every code path, the subprocess is cheaper to maintain.

**Final recommendation:** Use the Martins-side flag for Esc (hot-ish, want sub-50ms), and unconditional fire-and-forget `tmux send-keys -X cancel -t <outgoing>` for tab/workspace switch (not hot, simpler).

## Subprocess Behavior (verified empirically)

| Command | When | Exit code | Stderr | Action in plan |
|---------|------|-----------|--------|----------------|
| `tmux send-keys -X cancel -t <session>` | Session exists and IS in copy-mode | 0 | (empty) | succeed |
| `tmux send-keys -X cancel -t <session>` | Session exists, NOT in copy-mode | **1** | `not in a mode` | discard stderr; treat as no-op [VERIFIED: 2026-04-25 on tmux 3.6a] |
| `tmux send-keys -X cancel -t <session>` | Session does NOT exist | 1 | `can't find session: <name>` | discard; defensive — should not happen if we only call on outgoing-active sessions |
| `tmux save-buffer - -t <session>` | Buffer exists for session | 0 | (empty) | pipe stdout to `pbcopy` [VERIFIED — exit 0, stdout = buffer content] |
| `tmux save-buffer -` (no arg, server up, no buffers anywhere) | No buffers exist | **1** | `no buffers` | fall through to D-11 SIGINT path [VERIFIED: 2026-04-25] |
| `tmux save-buffer -` | Server not running | 1 | `no server running on /tmp/tmux-NNN/default` | should not happen during normal Martins use; defensive: treat as "no buffer" |
| `tmux list-buffers` | Server up, no buffers | 0 | (empty stdout) | distinguishable from "buffers present" via empty stdout; useful as gate |
| `tmux list-buffers` | Server not running | 1 | `no server running...` | defensive: treat as "no buffers" |

**Implication for D-10 / D-11 cmd+c precedence:** The plan can use **`tmux save-buffer -` exit code as the buffer-exists signal**. No need for a separate `tmux list-buffers` query first — `save-buffer` returns exit 0 on hit, exit 1 on miss, in a single subprocess.

Recommended flow in `App::handle_cmd_c`:
```
1. If app.selection (overlay path) is non-empty → copy_selection_to_clipboard (existing Phase 6 path); return.
2. Else if active session is delegating to tmux:
     spawn_blocking { tmux save-buffer - -t <session> }
       → exit 0, stdout non-empty: pipe stdout to pbcopy via std::process::Command (existing pattern in src/app.rs:492-499); return.
       → exit 1 OR empty stdout: fall through.
3. Else (Terminal mode) → write_active_tab_input(&[0x03]) (SIGINT); return.
```

The save-buffer subprocess is on the cmd+c key path — the user's perceived latency target is sub-50ms. `tmux save-buffer -` is process-fork + tmux client connect + buffer write to stdout + exit; on M-series macOS this is typically 5-15ms. Acceptable. If profiling later shows it's the bottleneck, can be moved behind `tmux list-buffers` cache invalidated on each `MouseDragEnd1Pane` — defer.

## Architecture Patterns

### System Architecture Diagram

```
+---------------------------+
| crossterm event stream    |
| (KeyEvent, MouseEvent)    |
+---------------------------+
              |
              v
+---------------------------+         +-----------------------------+
| events::handle_event      | <-----> | App state                   |
|  - handle_mouse           |         |  - selection: Option<...>    |
|  - handle_key             |         |  - pty_manager: PtyManager   |
+---------------------------+         |  - mode: InputMode           |
              |                        +-----------------------------+
              v
+---------------------------+
| Phase 7 dispatch:         |
| read parser.screen()      |
| .mouse_protocol_mode()    |
+---------------------------+
       |              |
       | == None      | != None
       | (delegate)   | (overlay)
       v              v
+---------------+   +------------------------+
| encode_sgr    |   | Phase 6 path:          |
| _mouse(...)   |   |  - SelectionState      |
| Vec<u8>       |   |  - REVERSED-XOR render |
+---------------+   |  - mouse-up snapshot   |
       |            +------------------------+
       v
+---------------+
| App::write_   |
| active_tab_   |
| input(&bytes) |
+---------------+
       |
       v
+---------------+      +-------------------+
| portable-pty  |----->| tmux client       |
| writer        |      | (in spawned PTY)  |
+---------------+      +-------------------+
                              |
                              | (interprets via default copy-mode-vi
                              |  bindings: MouseDrag1Pane, MouseDragEnd1Pane,
                              |  DoubleClick1Pane, etc.)
                              v
                       +-------------------+
                       | tmux server       |
                       |  - copy-mode      |
                       |  - selection      |
                       |  - paste buffer   |
                       +-------------------+
                              |
                              | (renders REVERSE-VIDEO highlight + copy-pipe to pbcopy)
                              v
                       +-------------------+
                       | PTY output bytes  |
                       +-------------------+
                              |
                              v
                       +-------------------+
                       | vt100 parser      |
                       | (PTY drain thread)|
                       +-------------------+
                              |
                              v
                       +-------------------+
                       | tui-term + ratatui|
                       | (Martins' renderer)|
                       +-------------------+

Special paths:
  cmd+c key    -> handle_key -> [overlay sel? copy_selection_to_clipboard]
                              -> [delegating? tmux save-buffer - | pbcopy]
                              -> [neither? SIGINT to PTY]
  Esc key      -> handle_key -> [overlay sel? clear locally]
                              -> [delegating + tmux_in_copy_mode? forward Esc byte]
                              -> [neither? forward Esc to PTY]
  tab switch   -> set_active_tab -> [delegating? unconditional `tmux send-keys -X cancel`]
                                 -> [overlay? clear_selection (existing Phase 6)]
```

### Recommended File Modification Surface

```
src/
├── tmux.rs           # ensure_config: +3 lines (y/Enter/Escape bindings)
├── pty/session.rs    # +1 helper method on PtySession (delegate_to_tmux)
│                     # +1 Arc<AtomicBool> field (tmux_in_copy_mode) — only if Option (a) state machine adopted
├── events.rs         # handle_mouse: conditional intercept (~15 LOC)
│                     # encode_sgr_mouse: new free fn (~20 LOC)
│                     # handle_key: cmd+c extension (~10 LOC), Esc extension (~5 LOC)
└── app.rs            # +1 helper: copy_tmux_buffer_to_clipboard (~15 LOC)
                      # +1 helper or modify set_active_tab: cancel_outgoing_tmux_selection (~5 LOC)
```

**Estimated production LOC:** ~70 lines added, ~3 lines modified in `ensure_config`. No deletions (Phase 6 overlay code stays whole).

### Pattern 1: Conditional Intercept in handle_mouse

**What:** Read mouse-mode from vt100 parser; branch overlay vs SGR-forward.
**When to use:** All Down(Left)/Drag(Left)/Up(Left) events in the terminal pane — these three branches in `handle_mouse` must each gate on the same condition.
**Source:** This research, applied to existing structure at `src/events.rs:38-181`.

**Concrete shape:**
```rust
pub async fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    let in_terminal = app.last_panes.as_ref().is_some_and(|p| {
        let inner = terminal_content_rect(p.terminal);
        rect_contains(inner, mouse.column, mouse.row)
    });

    if in_terminal {
        // Decide path ONCE per event. Use a snapshot to avoid re-acquiring
        // the parser lock multiple times within the same dispatch.
        let delegate = app.active_session_delegates_to_tmux();
        match (mouse.kind, delegate) {
            (MouseEventKind::Down(MouseButton::Left), true)
            | (MouseEventKind::Drag(MouseButton::Left), true)
            | (MouseEventKind::Up(MouseButton::Left), true) => {
                // Forward as SGR. Skip overlay state mutation entirely.
                let inner = terminal_content_rect(app.last_panes.as_ref().unwrap().terminal);
                let local_col = mouse.column.saturating_sub(inner.x);
                let local_row = mouse.row.saturating_sub(inner.y);
                if let Some(bytes) = encode_sgr_mouse(mouse.kind, mouse.modifiers, local_col, local_row) {
                    app.write_active_tab_input(&bytes);
                }
                // Update tmux_in_copy_mode flag (Down=true; Up with no drag=false).
                // NOTE: Do NOT mark_dirty — tmux's own PTY output triggers redraw
                // through the existing PTY drain → output_notify → mark_dirty path.
                return;
            }
            _ => { /* fall through to overlay path */ }
        }
    }

    // Existing Phase 6 overlay path runs unchanged when delegate == false.
    match mouse.kind {
        MouseEventKind::Drag(MouseButton::Left) if in_terminal => { /* existing */ }
        MouseEventKind::Up(MouseButton::Left) if in_terminal => { /* existing */ }
        MouseEventKind::Down(MouseButton::Left) => { /* existing — incl. shift-extend, click-counter */ }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => { /* existing — already SGR-forwards */ }
        _ => {}
    }
}
```

**Helper on App:**
```rust
pub(crate) fn active_session_delegates_to_tmux(&self) -> bool {
    let Some(session) = self.active_session() else { return false };
    let Ok(parser) = session.parser.try_read() else { return false };
    let screen = parser.screen();
    screen.mouse_protocol_mode() == vt100::MouseProtocolMode::None
        && !screen.alternate_screen()
}
```

### Pattern 2: SGR Encoder as Pure Function

**What:** A free function that takes `(MouseEventKind, KeyModifiers, local_col, local_row)` and returns `Option<Vec<u8>>`.
**When to use:** Any forwarded mouse byte stream — Phase 7 Down/Drag/Up + (future) right-click forwarding.
**Why pure fn:** Trivially unit-testable; mirrors existing scroll-encode at `events.rs:195-196` and click-encode at `events.rs:256-257`.

See body in §SGR Mouse Encoding.

### Pattern 3: Subprocess on User-Initiated Key, Fire-and-Forget on Switch

**What:** `tmux save-buffer -` subprocess on cmd+c (user-initiated, accepts ~10ms latency); `tmux send-keys -X cancel` fire-and-forget on tab-switch (not hot).
**When to use:** Any tmux interaction off the per-event hot path.
**Source:** Phase 5 lesson + existing `src/tmux.rs` patterns (`send_key`, `pane_command`). Match these patterns exactly.

**Concrete:**
```rust
// In src/tmux.rs — extend with new helpers:

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
    // Pipe stdout to pbcopy — match existing src/app.rs:492-499 pattern.
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

pub fn cancel_copy_mode(session: &str) {
    let _ = Command::new("tmux")
        .args(["send-keys", "-X", "cancel", "-t", session])
        .stdout(Stdio::null())
        .stderr(Stdio::null())   // discard "not in a mode" stderr
        .status();
}
```

### Anti-Patterns to Avoid

- **Don't byte-scan PTY drain for `\x1b[?1006h` etc.** vt100 already does this. Adding a parallel scanner is duplicate state with race conditions.
- **Don't run `tmux display-message -p '#{pane_in_mode}'` per Drag event.** Subprocess per mouse-move is exactly the perf trap that drove the Phase 5 background-decoupling work. Either track state locally or don't track at all.
- **Don't add `bind-key` for tmux defaults.** tmux 3.6a's `MouseDragEnd1Pane copy-pipe-and-cancel pbcopy` is already there; re-binding it in `~/.martins/tmux.conf` is dead config and a maintenance liability if upstream defaults shift in tmux 4.x.
- **Don't try to render Martins' overlay on top of tmux's native highlight.** They will fight each other. The whole point of Phase 7 is "let tmux own the visual feedback" in the delegating path.
- **Don't increment `scroll_generation` (Phase 6 D-05) on PTY output that's just tmux's copy-mode highlight repaint.** The existing SCROLLBACK-LEN heuristic at `src/pty/session.rs:101-116` checks "cursor at bottom AND top row hash changed" — copy-mode highlight repaints don't meet this gate (cursor isn't at bottom), so we're fine. But verify in plan: feed a session in tmux copy-mode some highlight bytes and assert `scroll_generation` does not bump.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Detect inner-program mouse-mode requests | A byte scanner watching for `\x1b[?1000h`/`\x1b[?1006h` etc. in the PTY drain | `screen.mouse_protocol_mode() != MouseProtocolMode::None` | vt100 parses these CSI sequences as part of its normal job. Duplicating the state risks race conditions. |
| Detect alternate screen state | Track our own bool keyed off `\x1b[?1049h`/`\x1b[?47h` etc. | `screen.alternate_screen()` | Same reason — vt100 already tracks. |
| Encode mouse events as SGR bytes | Build a stateful encoder | One pure function `encode_sgr_mouse(kind, mods, col, row) -> Option<Vec<u8>>` | Stateless mapping; existing inline format-strings at events.rs:195/256 already work the same way. |
| Track tmux's copy-mode state via subprocess polling | Loop `tmux display-message -p '#{pane_in_mode}'` every N ms | Either Martins-side state machine on Down/Up, OR fire-and-forget `tmux send-keys -X cancel` (idempotent — exits 1 with stderr if not in mode) | Polling is the Phase 5 perf trap. Cancel-anyway is simpler than tracking. |
| Click-counter for double/triple-click in tmux path | Re-implement Phase 6's `(last_click_at, click_count)` for tmux events | tmux's own default `DoubleClick1Pane`/`TripleClick1Pane` bindings | Tmux owns click timing in 3.6a defaults — no Martins code, no tmux.conf change. |
| Pipe selection text to clipboard | Subprocess pbcopy from a Martins-side text materialization | tmux's `copy-pipe-and-cancel pbcopy` (auto-fires on `MouseDragEnd1Pane`) | Tmux pipes natively. Martins' job is just to forward the SGR Up event. |

**Key insight:** The headline Phase 7 finding is "tmux 3.6a defaults already do almost everything." The temptation to "be explicit" by re-binding the defaults in `~/.martins/tmux.conf` should be resisted — it adds drift risk for zero behavior change today.

## Runtime State Inventory

> Phase 7 modifies behavior wiring in src/events.rs / src/app.rs / src/tmux.rs::ensure_config and may add fields to PtySession. No data migrations, no rename, no string-replacement. Inventory completeness check:

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | None — Phase 7 writes no new data to disk. `~/.martins/state.json` schema unchanged. `~/.martins/tmux.conf` rewritten on each session create (3 lines added, in line with existing pattern). Existing tmux.conf contents are deterministically regenerated from `ensure_config` — no migration needed. | none |
| Live service config | Tmux: existing sessions running under prior `~/.martins/tmux.conf` will NOT pick up the new bindings until the session is restarted. Action: document that running `tmux source-file ~/.martins/tmux.conf` per session would refresh, OR accept that bindings take effect on next Martins restart for new sessions. **For Phase 7 plan: prefer the latter (accept restart) — simpler, no per-session source-file subprocess.** | document; no automated migration |
| OS-registered state | None — no launchd, no Homebrew brew services. | none |
| Secrets/env vars | None. | none |
| Build artifacts | `target/` rebuild triggered by Rust changes (normal). No stale artifacts. | none |

**Nothing else found.** Verified by grep on `~/.martins`, code surface map matches CONTEXT.md `<canonical_refs>`.

## Common Pitfalls

### Pitfall 1: Forwarding mouse bytes to tmux when terminal pane is occluded by a modal

**What goes wrong:** User opens a modal (Help, ConfirmQuit, etc.) — modal occupies center of screen. User clicks/drags inside the modal area. If `handle_mouse` doesn't filter by `in_terminal`, the click forwards as SGR to the wrapped tmux session, which interprets it as a clipboard click.
**Why it happens:** The `in_terminal` check at `events.rs:39-42` uses `terminal_content_rect` which is the static border-inset rect — it does NOT subtract any modal overlay.
**How to avoid:** The Phase 7 dispatch must be gated on `in_terminal AND modal == Modal::None AND picker.is_none()`. The existing `handle_mouse` structure already gates the overlay-path mouse handling on `in_terminal`; Phase 7 must also gate the new SGR-forward branch on the modal/picker absence.
**Warning signs:** Drag in a modal selects text in the underlying tmux pane.

### Pitfall 2: Double-handling Down(Left) — cleared selection AND forward to tmux

**What goes wrong:** Phase 6 D-12/D-13 says any Down(Left) clears overlay selection. If Phase 7 forwards Down(Left) to tmux WITHOUT first clearing the (stale) overlay selection from a previous overlay-path session, the user sees both the residual overlay highlight AND tmux's new copy-mode highlight.
**Why it happens:** When the user runs vim (overlay path) then quits vim (mouse-mode resets to None), the leftover `app.selection` from the vim session would render on the new tmux-path frame.
**How to avoid:** When `mouse_protocol_mode()` transitions from non-None back to None (i.e., inner program just released mouse), proactively clear `app.selection`. Detection: cache last-known mouse mode on the session; on Drag/Down arrival, if cached != current AND current == None, `app.clear_selection()`.
**Warning signs:** Stale gold/reversed cells visible after quitting vim/htop.

### Pitfall 3: Cmd+c during a tmux drag (mouse button held down) — confused state

**What goes wrong:** User starts dragging in tmux path; mid-drag, presses cmd+c (with the mouse still held). Martins' cmd+c handler runs `tmux save-buffer - -t <session>`, but tmux's selection isn't finalized yet (no MouseDragEnd1Pane has fired). Buffer is empty or stale.
**Why it happens:** Tmux only writes to the buffer at copy-pipe time, which is on `MouseDragEnd1Pane`. Mid-drag has no buffer.
**How to avoid:** Document this as expected behavior; cmd+c during an in-flight drag returns either (a) the previous buffer's contents if any, or (b) falls through to SIGINT. Either is acceptable — the user fixed-up by releasing the mouse first. Plan should NOT try to interrupt the drag to force-finalize.
**Warning signs:** "cmd+c copied wrong text" complaint — likely the user pressed it mid-drag.

### Pitfall 4: tmux 3.6a defaults change in tmux 4.x

**What goes wrong:** Phase 7's "trust the defaults" relies on tmux 3.6a behavior. If the user upgrades to tmux 4.x and defaults shift, Martins behavior degrades silently.
**Why it happens:** Homebrew auto-updates tmux; users won't notice the major version bump.
**How to avoid:** Document the assumed default bindings (Recommended in `tmux.conf` as comments; OR explicit guard in `is_available()` checking version). Reasonable middle ground: if `tmux -V` returns major version != 3, log a warning at startup. Defer hard guard.
**Warning signs:** copy-pipe-and-cancel doesn't fire on MouseDragEnd1Pane after tmux upgrade.

### Pitfall 5: Esc handling diverges between vi-mode and emacs-mode users

**What goes wrong:** A user with `set -g mode-keys emacs` in their personal `~/.tmux.conf` would not be affected by Martins' `bind-key -T copy-mode-vi Escape send-keys -X cancel` override. Esc would behave differently than for the default (vi) user.
**Why it happens:** Martins' generated `~/.martins/tmux.conf` is loaded with `tmux -f`. User-personal tmux.conf is NOT loaded (per existing martins design — `tmux -f` replaces, not augments). So all martins users get vi-mode regardless of their personal preference. This is a forced choice that's already been made in Phase 1+.
**How to avoid:** No action needed — martins always loads its own tmux.conf which sets vi-mode (via tmux's default). Users who want emacs-mode would need to modify martins source. Document in plan if asked.
**Warning signs:** Power user who normally uses emacs-mode tmux complains about `b`/`B`/`w`/`W`/`y` behavior in martins.

### Pitfall 6: scroll_generation (Phase 6 D-05) firing during tmux copy-mode highlight repaint

**What goes wrong:** Tmux's copy-mode draws a REVERSE-VIDEO highlight over selected cells. Each repaint emits bytes through the PTY. If these bytes happen to repaint the top row, the SCROLLBACK-LEN heuristic at `src/pty/session.rs:101-116` would falsely increment `scroll_generation`, causing the (sleeping) overlay's anchored coords to drift.
**Why it happens:** The heuristic's gate is "cursor at bottom AND top row hash changed". Copy-mode highlight repaints typically position the cursor inside the highlight, NOT at the bottom — so the gate should hold. But edge case: tmux's copy-mode status line (`[1/3]` etc.) renders at the bottom row; a repaint that updates this line could pin the cursor there transiently.
**How to avoid:** During Phase 7 plan UAT, scroll while a tmux copy-mode selection is active and verify `scroll_generation` doesn't increment at non-scroll moments. If false-positive observed, gate the heuristic additionally on `!screen.alternate_screen()` (tmux copy-mode does not enable alternate screen — verify) OR on `mouse_protocol_mode() == None`.
**Warning signs:** When user switches back from tmux path to overlay path (e.g., re-enters vim), the overlay selection (if any) renders at wrong rows.

## Code Examples

### Example 1: SGR encoder, conditional dispatch, full handle_mouse skeleton

```rust
// src/events.rs

use vt100::MouseProtocolMode;

/// Pure SGR encoder — no state, no IO.
pub(crate) fn encode_sgr_mouse(
    kind: MouseEventKind,
    modifiers: KeyModifiers,
    local_col: u16,
    local_row: u16,
) -> Option<Vec<u8>> {
    let (button_base, trailing) = match kind {
        MouseEventKind::Down(MouseButton::Left)  => (0u8, 'M'),
        MouseEventKind::Drag(MouseButton::Left)  => (32u8, 'M'),
        MouseEventKind::Up(MouseButton::Left)    => (0u8, 'm'),
        MouseEventKind::ScrollUp                 => (64u8, 'M'),
        MouseEventKind::ScrollDown               => (65u8, 'M'),
        _ => return None,
    };
    let mut cb = button_base;
    if modifiers.contains(KeyModifiers::SHIFT)   { cb += 4; }
    if modifiers.contains(KeyModifiers::ALT)     { cb += 8; }
    if modifiers.contains(KeyModifiers::CONTROL) { cb += 16; }
    let col = local_col + 1; // 1-based per xterm SGR spec
    let row = local_row + 1;
    Some(format!("\x1b[<{cb};{col};{row}{trailing}").into_bytes())
}

pub async fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    let in_terminal = app.last_panes.as_ref().is_some_and(|p| {
        let inner = terminal_content_rect(p.terminal);
        rect_contains(inner, mouse.column, mouse.row)
    });

    // Phase 7: conditional intercept of Left button events when delegating to tmux.
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
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => app.tmux_in_copy_mode_set(true),
                MouseEventKind::Up(MouseButton::Left)   => {
                    // If this Up follows a drag (selection started), tmux remains in
                    // copy-mode; if no drag, copy-mode never started. Distinguish via
                    // a "drag_seen" flag also tracked on PtySession.
                    if !app.tmux_drag_seen_take() {
                        app.tmux_in_copy_mode_set(false);
                    }
                }
                MouseEventKind::Drag(MouseButton::Left) => app.tmux_drag_seen_set(true),
                _ => {}
            }
            // Do NOT mark_dirty — tmux's PTY output triggers redraw via existing path.
            return;
        }
    }

    // Phase 6 overlay path — runs unchanged when delegate == false.
    // [...existing src/events.rs:44-181 body...]
}
```

### Example 2: cmd+c precedence chain — overlay → tmux buffer → SIGINT

```rust
// src/events.rs handle_key, replacing the existing cmd+c branch at lines 388-403:

if key.code == KeyCode::Char('c')
    && key.modifiers.contains(KeyModifiers::SUPER)
{
    // Tier 1: overlay selection (Phase 6 D-02).
    if let Some(sel) = &app.selection {
        if !sel.is_empty() {
            app.copy_selection_to_clipboard();
            return;
        }
    }
    // Tier 2 (Phase 7 D-10): tmux buffer if delegating + selection done.
    if app.active_session_delegates_to_tmux() {
        if let Some(session_name) = app.active_tmux_session_name() {
            // Off-thread to keep cmd+c sub-50ms even on first invocation when
            // the tmux client connection is cold.
            let session_name = session_name.clone();
            tokio::task::spawn_blocking(move || {
                crate::tmux::save_buffer_to_pbcopy(&session_name);
            });
            return;
        }
    }
    // Tier 3 (Phase 6 D-03 + Phase 7 D-11): SIGINT in Terminal mode.
    if app.mode == InputMode::Terminal {
        app.write_active_tab_input(&[0x03]);
        return;
    }
    // Normal mode + no overlay + no tmux session — fall through to keymap (ctrl+c Quit unchanged).
}
```

### Example 3: tmux.conf extension in ensure_config

```rust
// src/tmux.rs ensure_config — replace existing 5-line config string with:

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

### Example 4: vt100 mouse-mode helper on App

```rust
// src/app.rs — new helper alongside existing parser-readers (analog: copy_selection_to_clipboard at line 473).

pub(crate) fn active_session_delegates_to_tmux(&self) -> bool {
    let Some(session) = self.active_session() else { return false };
    let Ok(parser) = session.parser.try_read() else {
        // Contention: the PTY drain holds the write lock briefly. Treat
        // contention as "not delegating" (conservative — falls back to
        // overlay path for one frame; harmless visual blip if any).
        return false;
    };
    let screen = parser.screen();
    matches!(screen.mouse_protocol_mode(), vt100::MouseProtocolMode::None)
        && !screen.alternate_screen()
}

pub(crate) fn active_tmux_session_name(&self) -> Option<String> {
    let project = self.active_project()?;
    let workspace = self.active_workspace()?;
    let tab = workspace.tabs.get(self.active_tab)?;
    Some(crate::tmux::tab_session_name(&project.id, &workspace.name, tab.id))
}
```

## State of the Art

| Old (Phase 6) | New (Phase 7) | When Changed | Impact |
|---------------|---------------|--------------|--------|
| Always intercept Drag(Left), build SelectionState, render REVERSED-XOR overlay | Conditional intercept: delegate to tmux when inner program hasn't requested mouse mode; overlay path retained as fallback | Phase 7 (this) | Selection feel matches `tmux` direct usage in Ghostty when not running mouse-aware programs |
| Hand-rolled click-counter for double/triple-click in martins | Tmux's default `DoubleClick1Pane`/`TripleClick1Pane` bindings handle word/line selection in tmux path | Phase 7 (this) | Click-counter code in events.rs:130-167 still runs in overlay path; tmux owns it natively in delegate path |
| `cmd+c` reads `App::selection` snapshot text | `cmd+c` precedence: overlay → tmux buffer → SIGINT | Phase 7 (this) | Same user-visible behavior; new Tier 2 covers "I selected via tmux native then pressed cmd+c" |
| (Phase 6 ignored vt100's `mouse_protocol_mode()` per Phase 6 D-10's "always intercept") | Use vt100's `mouse_protocol_mode()` as the dispatch signal | Phase 7 (this) | Removes need for any new byte scanner; trusts existing parser state |

**Deprecated/outdated:**
- CONTEXT.md D-02's "byte-scan PTY drain for `\x1b[?1006h`" approach — superseded by direct vt100 read.
- CONTEXT.md D-09's literal `MouseDragEnd1Pane` and `DoubleClick1Pane` bindings — superseded by tmux 3.6a defaults already covering them.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | tmux 3.6a's default `MouseDragEnd1Pane copy-pipe-and-cancel pbcopy` will continue in tmux 4.x | §Tmux Defaults | If tmux 4.x changes the default, Phase 7's "skip the binding" decision breaks silently. Mitigation: document the assumed default; consider runtime-detect via `tmux list-keys -T copy-mode-vi MouseDragEnd1Pane` at startup and warn if missing. [ASSUMED — based on tmux's stability commitment to default bindings] |
| A2 | `vt100::Screen::alternate_screen()` is publicly exposed in 0.16.2 | §State Source | If not exposed, fall back to `mouse_protocol_mode() == None` only; minor degradation (vim with `mouse=` empty would delegate to tmux even though it shouldn't). Verify before plan implementation: `grep alternate_screen ~/.cargo/registry/src/.../vt100-0.16.2/src/screen.rs`. [VERIFIED in Phase 6 RESEARCH §A3 — re-verify in Phase 7 plan Wave 0] |
| A3 | `KeyModifiers::ALT` is delivered on macOS for `MouseEvent.modifiers` (for the rectangle-select bonus) | §SGR Encoding | If macOS terminals consume Alt (Option) for native input methods, Alt-drag never reaches Martins. Mitigation: rectangle-select is already in CONTEXT.md Deferred — accept as deferred. [ASSUMED — depends on terminal emulator + Option-as-Meta config] |
| A4 | `tokio::task::spawn_blocking` for `tmux save-buffer -` keeps cmd+c perceived latency under 50ms | §Subprocess Behavior | If spawn_blocking pool is contended, latency could spike. Mitigation: `tokio::task::spawn_blocking` runs on a dedicated thread pool (default 512 threads); under typical load the spawn is ~1ms. [ASSUMED — based on tokio default pool sizing] |
| A5 | tmux's PTY output for copy-mode highlight repaints does NOT trigger Phase 6's `scroll_generation` SCROLLBACK-LEN heuristic | §Pitfall 6 | If it does, overlay-path coords drift on switch back from tmux path. Mitigation: UAT in plan; if observed, gate scroll_gen on `mouse_protocol_mode() == None && !alternate_screen()`. [ASSUMED — heuristic gate "cursor at bottom AND top row changed" should hold] |
| A6 | `tmux send-keys -X cancel -t <session>` exits 1 with stderr "not in a mode" when not in copy-mode (verified once empirically — assumption is this is stable across tmux 3.x) | §Subprocess Behavior | If stderr-on-not-in-mode changes to silent exit-0, the "fire-and-forget on tab switch" pattern still works. If exit-code semantics change to exit-0-always, also fine. The risk is only if tmux were to PRINT the error message on stdout instead of stderr (would not pollute Martins' UI either way since we redirect both). [VERIFIED 2026-04-25 on tmux 3.6a; stable across 3.x per release notes review] |
| A7 | `screen.mouse_protocol_mode()` updates synchronously on `parser.process(bytes)` containing the DECSET sequence — i.e., a Drag event arriving immediately after the inner program emits `\x1b[?1006h` will see the NEW mode, not the old one | §State Source | If updates lag (e.g., async parsing), there's a single-frame mis-dispatch — first Drag goes to overlay, subsequent Drags go to tmux. Mitigation: vt100 is synchronous (no async) — verified by reading screen.rs. [VERIFIED — vt100 0.16.2 is synchronous] |

## Open Questions

1. **Should Esc in delegate path forward Esc-byte to PTY, or call `tmux send-keys -X cancel`?**
   - What we know: tmux copy-mode-vi Esc default is `clear-selection` (not cancel). Our `ensure_config` extension overrides this to `cancel` (§Tmux Defaults).
   - What's unclear: with the override in place, is forwarding `\x1b` byte sufficient? Or do we also need the `tmux send-keys -X cancel` subprocess path as a guard?
   - Recommendation: forward Esc byte (zero subprocess on hot key), rely on the override. If UAT shows Esc doesn't always exit copy-mode, add `tmux send-keys -X cancel` as fallback after a 100ms timeout.

2. **Should the "delegate decision" be cached per drag-start, or re-read per event?**
   - What we know: re-reading per event costs one `try_read` lock acquisition (negligible). Caching avoids the lock entirely.
   - What's unclear: race condition where the inner program enables mouse mode mid-drag — re-read would switch paths halfway through a drag (broken). Cache-on-Down would commit to a path for the whole drag (correct).
   - Recommendation: cache the `delegate` decision on `MouseEventKind::Down(Left)` into a per-session field; subsequent Drag/Up events read the cached value. Reset on next Down.

3. **What happens if `tmux send-keys -X cancel` runs against a session whose tmux client process is not yet attached (race during Martins startup)?**
   - What we know: `tmux send-keys -X` operates against the server, not a specific client. As long as the session exists in the server, the operation succeeds.
   - What's unclear: if the session name is correct but Martins-side `PtySession` hasn't yet attached (pre-attach race), is the cancel still effective?
   - Recommendation: defensive — gate the cancel on `tmux::session_exists(&name)` (existing helper at `src/tmux.rs:15`). If session doesn't exist, no-op. Cost: one extra subprocess on tab switch — still off-hot-path.

4. **Should Wheel scroll bytes be re-encoded by `encode_sgr_mouse`, or kept inline at events.rs:195-196?**
   - What we know: Phase 7 doesn't change wheel handling — the existing inline encode at events.rs:195-196 already does the right thing.
   - What's unclear: code-cleanliness — having two SGR encoders (inline for wheel, helper for left-button) is duplicated knowledge.
   - Recommendation: in the Phase 7 plan, opportunistically migrate wheel to use `encode_sgr_mouse` IF the helper signature accommodates it cleanly (it does — Wheel variants are in the helper above). Single source of truth for SGR encoding. Marginal LOC win, real correctness win if future modifier handling changes.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| tmux | All Phase 7 paths | ✓ | 3.6a [VERIFIED 2026-04-25] | — (Martins core dep, not Phase 7-specific) |
| `pbcopy` | tmux's copy-pipe-and-cancel + Martins' subprocess | ✓ | macOS built-in | — |
| `vt100` | mouse-mode read on Screen | ✓ | 0.16.2 | — |
| `crossterm` | KeyModifiers + MouseEvent | ✓ | 0.29.0 | — |
| `portable-pty` | write_input pathway | ✓ | 0.9 | — |

**Missing dependencies with no fallback:** None.
**Missing dependencies with fallback:** None.

## Validation Architecture

**Source:** `workflow.nyquist_validation` — not set in `.planning/config.json` (treat as enabled per CONTEXT.md precedent).

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` (Rust built-in `#[test]` + `tokio::test` for async) |
| Config file | `Cargo.toml` (`[dev-dependencies]` — insta, tempfile, assert_cmd, predicates) |
| Quick run command | `cargo test --lib tmux_native_selection` (filter on new module) |
| Full suite command | `cargo test` |

### Phase Requirements → Test Map

Phase 7 has no allocated REQ-IDs — the validation surface is "Phase 6 SEL-01..SEL-04 still hold across BOTH paths" plus three new behavior gates derived from D-02/D-04/D-07/D-09/D-10/D-14/D-16:

| Gate ID | Behavior | Test Type | Automated Command | File |
|---------|----------|-----------|-------------------|------|
| TM-ENC-01 | `encode_sgr_mouse(Down(Left), NONE, 9, 4)` returns `b"\x1b[<0;10;5M"` | unit (pure fn) | `cargo test --lib encode_sgr_down_left_no_mods` | ❌ Wave 0 — `src/tmux_native_selection_tests.rs` |
| TM-ENC-02 | `encode_sgr_mouse(Drag(Left), NONE, 9, 4)` returns `b"\x1b[<32;10;5M"` (motion bit) | unit | `cargo test --lib encode_sgr_drag_left_no_mods` | ❌ Wave 0 |
| TM-ENC-03 | `encode_sgr_mouse(Up(Left), NONE, 9, 4)` returns `b"\x1b[<0;10;5m"` (lowercase) | unit | `cargo test --lib encode_sgr_up_left_release` | ❌ Wave 0 |
| TM-ENC-04 | `encode_sgr_mouse(Down(Left), SHIFT, 9, 4)` returns `b"\x1b[<4;10;5M"` (D-18 shift extend) | unit | `cargo test --lib encode_sgr_down_left_shift` | ❌ Wave 0 |
| TM-ENC-05 | `encode_sgr_mouse(Down(Left), ALT, 9, 4)` returns `b"\x1b[<8;10;5M"` (rectangle bonus) | unit | `cargo test --lib encode_sgr_down_left_alt` | ❌ Wave 0 |
| TM-ENC-06 | `encode_sgr_mouse(Drag(Left), SHIFT|ALT, 9, 4)` returns `b"\x1b[<44;10;5M"` (32+4+8) | unit | `cargo test --lib encode_sgr_drag_left_shift_alt` | ❌ Wave 0 |
| TM-DISPATCH-01 | When `screen.mouse_protocol_mode() == None` and not alternate-screen, `Drag(Left)` writes SGR bytes to active PTY (NOT mutating `app.selection`) | integration (real PtySession + parser) | `cargo test --lib drag_delegates_to_tmux_when_no_mouse_mode` | ❌ Wave 0 |
| TM-DISPATCH-02 | When `screen.mouse_protocol_mode() != None`, `Drag(Left)` mutates `app.selection` (Phase 6 path) and does NOT write SGR | integration | `cargo test --lib drag_uses_overlay_when_inner_mouse_mode` | ❌ Wave 0 |
| TM-DISPATCH-03 | When `screen.alternate_screen() == true`, `Drag(Left)` uses overlay path (vim/htop case) | integration | `cargo test --lib drag_uses_overlay_when_alternate_screen` | ❌ Wave 0 |
| TM-DISPATCH-04 | Mouse-mode transition in vt100 (feed `\x1b[?1006h` then `\x1b[?1006l`) flips delegate decision | integration | `cargo test --lib delegate_flips_on_mouse_mode_set_reset` | ❌ Wave 0 |
| TM-CMDC-01 | cmd+c with overlay non-empty → calls `copy_selection_to_clipboard` (Tier 1 — Phase 6 path unchanged) | unit (existing test pattern) | `cargo test --lib cmd_c_tier1_overlay_selection` | ❌ Wave 0 (extend Phase 6 cmd+c test) |
| TM-CMDC-02 | cmd+c with empty overlay + delegating session → spawns `tmux save-buffer` subprocess (mock by checking spawn invoked) | integration | `cargo test --lib cmd_c_tier2_tmux_buffer` | ❌ Wave 0 |
| TM-CMDC-03 | cmd+c with no overlay + no tmux + Terminal mode → SIGINT 0x03 (Tier 3 — Phase 6 D-03 path unchanged) | unit | `cargo test --lib cmd_c_tier3_sigint` | ❌ Wave 0 (existing Phase 6 test should still pass) |
| TM-ESC-01 | Esc with overlay selection → clears overlay (Phase 6 path unchanged) | unit | `cargo test --lib esc_tier1_overlay_clear` | ❌ Wave 0 (existing Phase 6 test should still pass) |
| TM-ESC-02 | Esc with no overlay + delegating + tmux_in_copy_mode → forwards `\x1b` byte to PTY | unit | `cargo test --lib esc_tier2_forwards_to_tmux` | ❌ Wave 0 |
| TM-ESC-03 | Esc with no overlay + not delegating → Phase 6 forward-to-PTY path unchanged | unit | `cargo test --lib esc_tier3_pty_forward` | ❌ Wave 0 (existing) |
| TM-CONF-01 | `ensure_config()` writes the 3 new bindings (y, Enter, Escape) at the expected paths | unit | `cargo test --lib ensure_config_writes_phase7_bindings` | ❌ Wave 0 (extend existing tmux tests in `src/tmux.rs` `mod tests`) |
| TM-CANCEL-01 | Tab switch on a delegating session calls `tmux send-keys -X cancel` (mock subprocess) | integration | `cargo test --lib tab_switch_cancels_outgoing_tmux` | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo test --lib tmux_native_selection` (targeted filter — fast, ~5s)
- **Per wave merge:** `cargo test` (full suite, includes pty_input_tests + navigation_tests + selection_tests + tmux_native_selection_tests)
- **Phase gate:** Full suite green BEFORE `/gsd-verify-work`; PLUS dual-path manual UAT — see below.

### Wave 0 Gaps

- [ ] `src/tmux_native_selection_tests.rs` — new test module for the 18 gates above
- [ ] `#[cfg(test)] mod tmux_native_selection_tests;` registration in `src/main.rs` (follow precedent at line 21 / 24 / 27)
- [ ] Re-verify `vt100::Screen::alternate_screen()` is publicly exposed in 0.16.2 (`grep -n alternate_screen ~/.cargo/registry/src/.../vt100-0.16.2/src/screen.rs`) — may already be confirmed by Phase 6 RESEARCH §A3
- [ ] No framework install needed — `cargo test` works today

### Manual UAT (dual-path Ghostty parity)

The headline qualitative target is "PTY-pane selection in tmux path feels indistinguishable from running tmux directly in Ghostty". The UAT must compare both paths against the Ghostty+tmux baseline:

**Setup:**
- Ghostty terminal A (top half): run `tmux` directly (`tmux new -s baseline`).
- Ghostty terminal B (bottom half): run `cargo run --release` (Martins).

**Cross-path UAT cases:**

| ID | Path | Procedure | Pass Criterion |
|----|------|-----------|----------------|
| UAT-7-A | tmux native (delegate) | In Martins active tab running `bash` (no inner mouse), drag-select a line. | Highlight shows in tmux's own reverse-video; on release, `pbpaste` in another terminal returns the selected text. Feel matches Ghostty terminal A side-by-side. |
| UAT-7-B | tmux native (delegate) | Double-click a word in `bash`. | Word highlights and is on clipboard immediately (matches `tmux` direct in Ghostty). |
| UAT-7-C | tmux native (delegate) | Triple-click a line. | Line highlights and is on clipboard. |
| UAT-7-D | tmux native (delegate) | Drag-select then press `Esc`. | Selection clears, copy-mode exits in single press (verifies the Esc-cancel override). |
| UAT-7-E | tmux native (delegate) | Drag-select then click outside the selection. | Selection clears (tmux's own behavior). |
| UAT-7-F | tmux native (delegate) | Drag-select then press `cmd+c`. | `pbpaste` in another terminal returns the selected text (TM-CMDC-02 path). |
| UAT-7-G | overlay (mouse-app) | Run `vim` in a tab, then `:set mouse=a`, then drag-select. | Phase 6 overlay highlight (REVERSED) appears, NOT tmux's. SEL-01..04 still hold. |
| UAT-7-H | overlay (htop) | Run `htop`, then drag-select. | Same as UAT-7-G — overlay path active because htop sets DECSET 1003. |
| UAT-7-I | overlay → tmux transition | In tab: run `vim`, drag-select (overlay), Esc, `:q`, then drag-select again. | After `:q`, vim resets mouse mode → next drag uses tmux native path. No stale overlay highlight. |
| UAT-7-J | tab switch with active tmux selection | In tab 1 (delegate path), drag-select, leave selected. Press F2 to switch to tab 2. | Tab 1's tmux selection is canceled (verifiable by switching back: no highlight). |
| UAT-7-K | cmd+c precedence | (a) overlay sel + cmd+c → overlay text; (b) clear, tmux sel + cmd+c → tmux buffer text; (c) clear all, in Terminal mode, cmd+c → SIGINT (interrupts a `sleep 30`). | All three tiers fire correctly. |

**Reference baseline:** Operator's qualitative comparison against Ghostty's `cmd+option+c` / drag-select UX. Phase 6 UAT 2026-04-25 captured "feels non-native" as the bug; this UAT is the gate that closes it.

## Sources

### Primary (HIGH confidence — empirically verified or read directly)
- `tmux 3.6a list-keys -T copy-mode-vi` / `-T copy-mode` / `-T root` — verified on `/opt/homebrew/bin/tmux`, 2026-04-25
- `tmux save-buffer -` exit code semantics — verified empirically on tmux 3.6a, 2026-04-25
- `tmux send-keys -X cancel -t <session>` exit code + stderr — verified empirically, 2026-04-25
- `~/.cargo/registry/src/index.crates.io-*/vt100-0.16.2/src/screen.rs:11-36, 576-585, 1148-1197` — `MouseProtocolMode` enum + `mouse_protocol_mode()` accessor + DECSET handling
- `~/.cargo/registry/src/index.crates.io-*/crossterm-0.29.0/src/event.rs:836-848` — `KeyModifiers` bits including `SHIFT`/`ALT`/`CONTROL`
- `src/events.rs:195-196, 256-257` — existing martins SGR encodes (cross-validates encoding convention)
- `src/tmux.rs:24-41` — existing `ensure_config` pattern
- `src/pty/session.rs:18-145` — existing PtySession structure + parser access pattern
- `.planning/phases/06-text-selection/06-RESEARCH.md` (esp. §Q1 SCROLLBACK-LEN heuristic, §Q5 cmd+c delivery) — direct ancestor

### Secondary (MEDIUM confidence — official docs but partial)
- xterm `ctlseqs.html` (invisible-island.net) — SGR 1006 format `CSI < Cb ; Cx ; Cy M/m`, button bits, modifier bits; partial coverage of motion bit (32) and SGR-vs-X10 differences (verified via cross-check against tmux source + martins existing encodes)
- tmux man page `tmux(1)` §COPY MODE, §MOUSE SUPPORT — referenced for `MouseDragEnd1Pane`, `copy-pipe-and-cancel`, `pane_in_mode`, `mouse_any_flag` (not directly fetched in this session — these are well-known)

### Tertiary (LOW confidence — community sources, used only for context)
- GitHub `tmux/tmux Discussion #3831` — community customization examples for MouseDragEnd1Pane
- `wbk.one`, `dev.to`, various tmux config blog posts — community patterns for clipboard piping (cross-checked against verified tmux defaults, no contradictions found)

## Metadata

**Confidence breakdown:**
- Tmux 3.6a defaults: HIGH — verified directly via `list-keys` on user machine
- vt100 mouse-mode tracking API: HIGH — read source directly
- SGR encoding: HIGH — cross-verified against existing martins inline encodes + xterm spec
- Subprocess error semantics (save-buffer exit codes, cancel "not in a mode"): HIGH — verified empirically on user machine
- "Trust tmux defaults" architectural choice: MEDIUM — relies on stability across tmux versions (A1 in Assumptions Log)
- Esc-byte forwarding vs `send-keys -X cancel` subprocess: MEDIUM — recommendation made (forward byte) but UAT-dependent; OQ-1 calls out the verification path
- Block/rectangle selection bonus: LOW — Alt-modifier delivery on macOS is terminal-emulator-dependent; Deferred per CONTEXT.md

**Research date:** 2026-04-25
**Valid until:** 2026-05-25 (tmux defaults stable across 3.x; if user upgrades to tmux 4.x, re-verify §Tmux Defaults)

## RESEARCH COMPLETE
