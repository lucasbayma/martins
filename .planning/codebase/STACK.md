# Technology Stack

**Analysis Date:** 2026-04-24

## Languages

**Primary:** Rust (edition 2024, MSRV 1.85)
- Single-language codebase, 100% Rust source
- Configured in `Cargo.toml` with `edition = "2024"` and `rust-version = "1.85"`
- Line count: ~12,065 lines across 30 `.rs` files in `src/`

## Runtime & Platform

**Target:** macOS only (Apple Silicon + Intel, universal binary)
- Declared in `README.md`: "macOS only. Martins is built and tested exclusively for macOS. Linux and Windows are not supported at this time."
- Release builds produce a universal binary via `lipo` combining `aarch64-apple-darwin` + `x86_64-apple-darwin` (`.github/workflows/release.yml`)
- CI matrix at `.github/workflows/ci.yml` runs on both `macos-latest` and `ubuntu-latest` for cross-platform compile guarantees, but runtime is macOS-only (tmux, pbcopy dependencies)

**Async runtime:** tokio (full features)
- Single async runtime drives the event loop
- Used for event streaming, PTY I/O, file watching, timers

## Frameworks & Core Libraries

**Terminal UI:**
- `ratatui = "0.30"` — terminal UI framework (renders widgets/frames)
- `crossterm = "0.29"` with `event-stream` feature — cross-platform terminal input/output backend
- `tui-term = "0.3.4"` (with `unstable` feature) — integrates VT100 emulator output into Ratatui
- `vt100 = "0.16"` — VT100/xterm terminal emulator parser

**PTY & Sessions:**
- `portable-pty = "0.9"` — pseudo-terminal spawning, cross-platform abstraction
- tmux (external binary, not a crate) — session persistence backend, shelled out via subprocess

**Async & Concurrency:**
- `tokio = "1.36"` with `full` feature — async runtime, tasks, timers, channels
- `futures = "0.3"` — `StreamExt` and combinators

**Git Integration:**
- `git2 = "0.17"` — libgit2 bindings for repo discovery, diff, branches
- git worktrees managed by shelling out to `git` CLI (see `src/git/worktree.rs`)

**File Watching:**
- `notify = "8.2"` — file system event watching
- `notify-debouncer-mini = "0.4"` — debounced file events

**Serialization:**
- `serde = "1"` with `derive` — struct (de)serialization
- `serde_json = "1"` — JSON persistence for `state.json`
- `chrono = "0.4"` with `serde` — RFC3339 timestamps

**CLI:**
- `clap = "4"` with `derive` — subcommand parsing (`martins workspaces list`, `prune`, etc.)

**Utility & Platform:**
- `directories = "5"` — XDG-aware path resolution for `~/.martins/`
- `which = "6"` — binary existence checks (tmux, git)
- `sha2 = "0.10"` — SHA-256 for repo-path hashing (stable project IDs)
- `fastrand = "2"` — lightweight RNG for MPB name generation
- `unicode-normalization = "0.1"` — name/string normalization
- `nucleo-matcher = "0.3"` — fuzzy matching in picker

**Logging & Errors:**
- `tracing = "0.1"` — structured logging
- `tracing-subscriber = "0.3"` with `env-filter` — log filtering via env vars
- `tracing-appender = "0.2"` — file-based log rotation
- `anyhow = "1"` — application-level error propagation
- `thiserror = "1"` — custom error enum derivation

**Crypto (transitive):**
- `openssl = "0.10"` with `vendored` — vendored OpenSSL for git2 TLS

## Dev Dependencies

From `Cargo.toml`:
- `insta = "1.40"` — snapshot testing (present but minimal usage observed)
- `tempfile = "3.10"` — temporary directories in tests
- `assert_cmd = "2.0"` — CLI binary testing
- `predicates = "3"` — assertion helpers for `assert_cmd`

## Build & Tooling

**Build system:** Cargo (standard Rust)
- `Cargo.toml` — dependency manifest
- `Cargo.lock` — pinned versions (committed)
- Target release binary: ~4MB static (per README)

**Linting & formatting:**
- `rustfmt.toml` — `edition = "2024"`, `max_width = 100`, `imports_granularity = "Crate"`
- `clippy.toml` — `avoid-breaking-exported-api = false`
- CI enforces `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`

**Distribution:**
- `Formula/` — Homebrew formula for `brew install lucasbayma/martins/martins`
- `.github/workflows/release.yml` — tag-triggered GitHub release pipeline
- `.github/workflows/update-tap.yml` — auto-updates Homebrew tap on release

## Runtime Dependencies (external, not crates)

Required on the host system:
- `tmux` — session backend (spawned as subprocess, see `src/tmux.rs`)
- `git` CLI — for worktree operations (see `src/git/worktree.rs`)
- `pbcopy` — clipboard integration (macOS-native)
- At least one AI agent CLI: `opencode`, `claude`, or `codex` (configurable per-tab)

## Configuration Files

| File | Purpose |
|---|---|
| `Cargo.toml` | Package metadata + dependencies |
| `rustfmt.toml` | Format rules |
| `clippy.toml` | Lint rules |
| `.gitignore` | Git ignore |
| `Formula/martins.rb` | Homebrew formula |
| `.github/workflows/ci.yml` | CI pipeline (fmt/clippy/test) |
| `.github/workflows/release.yml` | Release binary build |
| `.github/workflows/update-tap.yml` | Homebrew tap sync |

Runtime config (generated at `~/.martins/`):
- `state.json` — serialized `GlobalState` (projects, workspaces, tabs)
- `state.json.bak` — atomic backup
- `tmux.conf` — generated tmux config for all sessions
- `logs/martins.log` — tracing log output
