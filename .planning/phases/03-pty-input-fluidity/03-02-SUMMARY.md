---
phase: 03-pty-input-fluidity
plan: 02
status: skipped
subsystem: pty
tags: [pty, conditional, not-executed, frame-budget-gate]

# Dependency graph
requires: []
provides: []
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns: []

key-files:
  created: []
  modified: []

# Self-Check: N/A (plan skipped per conditional gate)
---

## Plan 03-02 — SKIPPED (conditional plan, UAT gate did not trigger)

**Status:** NOT EXECUTED. Plan remains on disk as evidenced alternative.

## Why skipped

Plan 03-02 is a conditional frame-budget-gate plan. Per its own objective: "Only executed if Plan 03-01 Task 3 manual UAT flags PTY-01 or PTY-02 as failing. If 03-01 UAT passes, this plan stays on disk unexecuted as the evidenced alternative."

Plan 03-01 UAT resolved `approved` (2026-04-24): all four feel-tests passed (PTY-01 Ghostty-equivalent keystroke feel, PTY-02 input-under-heavy-output, PTY-03 idle CPU < 1%, PTY-03 no warmup stall). The structural primitives from Phase 2 (biased select, synchronous `write_input`, dirty-gated draw, 8ms output throttle) were sufficient; the frame-budget gate was not needed.

## What this plan would have done (for future reference)

Had UAT flagged PTY-01 or PTY-02 failing, 03-02 would have introduced a per-frame input-drain budget in `src/app.rs::run` to guarantee input branch preemption even under worst-case output storms. See `03-02-PLAN.md` for the full design — it remains on disk as the prepared fallback.

## Requirements disposition

- PTY-01, PTY-02, PTY-03 — all closed at Plan 03-01. Not carried forward to 03-02.
