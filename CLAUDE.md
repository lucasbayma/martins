## Project

Martins
## Technology Stack

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

