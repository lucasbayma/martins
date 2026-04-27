---
phase: 4
slug: navigation-fluidity
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-24
---

# Phase 4 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test --all-targets` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib` (scoped to modified module if reasonable)
- **After every plan wave:** Run `cargo test --all-targets`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

> Populated by planner. Each task gets one row mapping to a NAV requirement and a test command.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | TBD | TBD | NAV-01..04 | — | N/A | unit/integration | `cargo test ...` | ⬜ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Establish behavioral test harness for navigation paths (sidebar key events, click dispatch, workspace switch, tab switch) — see RESEARCH.md §6 Validation Architecture
- [ ] Stubs for NAV-01, NAV-02, NAV-03, NAV-04 verifying no `.await` on hot paths
- [ ] Regression anchor: grep-based assertion that `refresh_diff().await` does NOT appear in nav input-arm body

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Perceived "instantaneous" feel during sidebar hold-key scroll | NAV-01 | Perceptual — no automated proxy for "feels instant" beyond frame-count proxy | Launch app, open a workspace with long project list, hold Up/Down; observe no visible stutter |
| Workspace switch shows PTY view in single frame | NAV-03 | Requires running app against real tmux | Switch between 2 active workspaces; observe no blank/loading frame |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
