---
phase: 04-navigation-fluidity
plan: 03
status: complete
closed: 2026-04-24
requirements: [NAV-01, NAV-02, NAV-03, NAV-04]
---

# Plan 04-03 — Human UAT + Phase Close

## Outcome

User UAT on `target/release/martins` — four feel-tests against NAV-01..04:

- NAV-01 (sidebar Up/Down scroll on long list + large repo) → approved
- NAV-02 (click any sidebar item activates it + highlight follows) → approved **after** 04-03 highlight-sync fix
- NAV-03 (workspace switch paints PTY on next frame) → approved
- NAV-04 (tab switch via F1-F3, number keys, click-on-strip) → approved

## NAV-02 Gap Closure

User flagged during first UAT: "It's really fast, but the line highlight doesn't change to selected workspace."

Diagnosis: `src/ui/sidebar_left.rs::render` drives the visual highlight from ratatui's `ListState` (the keyboard-cursor position) and ignores `active_workspace_idx` (prefixed `_` and unused). The mouse click handler at `src/events.rs:174` was calling `app.select_active_workspace(idx)` (which updates the model) but never syncing `app.left_list.select(...)` (which the renderer actually reads). Pre-existing bug; the faster nav path simply made it more visible because activation and paint now land within the same frame.

Fix: one line added in the mouse click handler —

```rust
if let Some(item) = app.sidebar_items.get(local_row).cloned() {
    app.left_list.select(Some(local_row));  // ← added
    match item { ... }
}
```

Commit: `f89277a`. Re-UAT of NAV-02 → approved.

## Verification

- `cargo test` — 107/107 pass
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo build --release` — clean
- All grep invariants (nav hot-path awaits, legit awaits, 6th select branch, diff_tx/diff_rx fields, Phase 2/3 anchors) intact — see PHASE-SUMMARY.md.

## Artifacts

- `.planning/phases/04-navigation-fluidity/PHASE-SUMMARY.md` — phase close document + grep invariant snapshot + Phase 5 readiness note.

## Self-Check: PASSED

All must_haves.truths satisfied. Phase 4 closed.
