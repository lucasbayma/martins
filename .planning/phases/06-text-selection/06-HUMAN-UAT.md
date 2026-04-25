---
status: partial
phase: 06-text-selection
source: [06-VERIFICATION.md, 06-VALIDATION.md]
started: 2026-04-25T00:00:00Z
updated: 2026-04-25T00:00:00Z
---

## Current Test

[awaiting human testing]

## Tests

### 1. Drag-highlight tracks cursor with no visible lag/tearing (SEL-01)
expected: dragging across a multi-line agent transcript shows a REVERSED-XOR highlight that follows the cursor frame-by-frame at 60fps with no tearing or trailing artifacts
procedure: `cargo run --release`, open a tab, run something with multi-line output (e.g. `cat ~/.zshrc` or an agent transcript), drag-select across the visible rows
result: [pending]

### 2. cmd+c places selection on macOS clipboard (SEL-02)
expected: selected text is on the system clipboard and retrievable from another terminal via `pbpaste`
procedure: in Martins drag-select some text, press cmd+c, open Terminal.app, run `pbpaste` — output must equal the selection exactly
result: [pending]

### 3. Click outside selection / Esc clears highlight in single frame (SEL-03)
expected: highlight disappears immediately, no flicker
procedure: drag-select, then (a) click in an empty area outside the selection — highlight clears; (b) drag-select again, press Esc — highlight clears
result: [pending]

### 4. Selection survives streaming PTY output (SEL-04)
expected: while text is selected, new PTY output below/beside it does not cause the highlight to flicker, jitter, or disappear; the highlight stays anchored to the original text content as rows scroll
procedure: start a streaming command (e.g. `claude --verbose`, `tail -f log`, or `yes | head -200`), drag-select before it finishes, watch the highlight as rows scroll
result: [pending]

### 5. UAT-06-04-A — cmd+c with no selection in Terminal mode forwards SIGINT
expected: `sleep` exits within ~1s; no clipboard write occurred
procedure: launch Martins; in active tab run `sleep 30`; ensure no selection is active (drag-select then Esc); press cmd+c
result: [pending]

### 6. UAT-06-04-B — Esc with no selection forwards 0x1b to active PTY
expected: Vim returns from Insert to Normal mode (status bar `-- INSERT --` → empty/`-- NORMAL --`)
procedure: launch Martins; run `vim` in active tab; press `i` to enter insert mode; press Esc
result: [pending]

## Summary

total: 6
passed: 0
issues: 0
pending: 6
skipped: 0
blocked: 0

## Gaps
