---
status: partial
phase: 01-architectural-split
source: [01-VERIFICATION.md]
started: 2026-04-24T10:30:00Z
updated: 2026-04-24T10:30:00Z
---

## Current Test

[awaiting human testing]

## Tests

### 1. Basic TUI render
expected: Identical layout, colors, and modal overlays to pre-refactor. `cargo run --release`, verify sidebar + terminal + status bar + menu bar all appear; press `?` for Help; press `q` for ConfirmQuit; resize below 80x24 shows "Terminal too small".
result: [pending]

### 2. Every modal flow
expected: Each modal opens, accepts input, and closes with identical behavior to pre-refactor. Test NewWorkspace, AddProject, ConfirmDelete, ConfirmQuit, ConfirmArchive, ConfirmRemoveProject, CommandArgs, Help, Loading — via both keyboard (Enter/Escape) and mouse (click/click-outside).
result: [pending]

### 3. All 28 event-routing paths (01-03 Task 4)
expected: Every path behaves identically to pre-refactor. NORMAL arrows/Enter/n/t/d/?/q/F1-F9; TERMINAL typing + arrow forward + Ctrl-B/C/D + bracketed paste; mouse on sidebar/terminal/tabs/menu/status including drag-select with pbpaste clipboard verification; picker type/nav/select.
result: [pending]

### 4. Workspace + project lifecycle — 16 paths (01-04 Task 3)
expected: Each mutation hits git CLI + tmux + filesystem in the same order as pre-refactor; state.json reflects completed mutations; no orphaned tmux sessions after archive. Tests project create/switch/remove; workspace create + reattach; tab create/switch/close; archive + delete archived; name-uniqueness error; partial-failure rollback; crash-recovery state consistency.
result: [pending]

### 5. Final end-to-end composite pass (01-05 Task 3)
expected: Identical behavior to pre-refactor across the full extracted surface. All 16 cumulative checks across render, PTY typing, mode toggle, workspace switching, tab lifecycle, file-click diff preview, archive, quit.
result: [pending]

## Summary

total: 5
passed: 0
issues: 0
pending: 5
skipped: 0
blocked: 0

## Gaps
