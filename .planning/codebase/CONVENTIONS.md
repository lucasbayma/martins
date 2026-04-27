# Coding Conventions

**Analysis Date:** 2026-04-24

## Formatting

**Enforced by `rustfmt.toml`:**
```toml
edition = "2024"
max_width = 100
imports_granularity = "Crate"
```

- **Line width:** 100 characters max
- **Edition:** Rust 2024 (MSRV 1.85)
- **Imports:** Grouped at the crate level — all `use crate::foo::...` lines collapsed into `use crate::foo::{bar, baz};`
- **CI enforcement:** `cargo fmt --check` runs on every push/PR (`.github/workflows/ci.yml`)

## Linting

**Enforced by `clippy.toml`:**
```toml
avoid-breaking-exported-api = false
```

- **CI enforcement:** `cargo clippy --all-targets -- -D warnings` — warnings fail the build
- No custom Clippy lint allow/deny at crate root (no `#![deny(...)]` attributes observed)

## Naming Conventions

### Files & modules
- **Module files:** `snake_case.rs` (e.g., `sidebar_left.rs`, `pty/manager.rs`)
- **Module organization:** `mod.rs` re-exports with `pub mod foo;` + `pub use foo::Bar;`
- **Feature directories:** lowercase, descriptive (`ui/`, `pty/`, `git/`)

### Types
- **Structs/enums:** `PascalCase` (`App`, `GlobalState`, `Workspace`, `Project`, `TabSpec`)
- **Error types:** always suffixed `Error` (`AppError`, `GitError`, `ManagerError`, `DiffError`)
- **Enum variants:** `PascalCase` (`WorkspaceStatus::Active`, `Agent::Claude`, `InputMode::Normal`)

### Functions & variables
- **Functions:** `snake_case` (`refresh_diff`, `create_workspace`)
- **Async-wrapping pattern:** blocking functions exposed via `_async` suffix (`current_branch`, `current_branch_async`) in `src/git/repo.rs`
- **Constants:** `SCREAMING_SNAKE_CASE` (`ACCENT_GOLD` in `src/ui/theme.rs`)
- **Local bindings:** `snake_case`

### Struct fields
- All fields `snake_case`, no Hungarian prefixes

## Module Organization

**Crate root:** `src/main.rs` + module declarations in `main.rs`

**Re-export pattern:** In `mod.rs` files (e.g., `src/ui/mod.rs`, `src/git/mod.rs`, `src/pty/mod.rs`):
```rust
pub mod layout;
pub mod modal;
// ...
```

Dependents import via `crate::ui::modal::Modal` directly — no barrel re-exports of everything. Types are exported where declared.

## Import Ordering

Observed pattern (e.g., `src/app.rs` top of file):

```rust
// 1. Internal crate modules
use crate::git::{diff, repo};
use crate::keys::{Action, InputMode, Keymap};
use crate::pty::manager::PtyManager;
use crate::state::{Agent, GlobalState, Project, TabSpec, Workspace};

// 2. External crates (alphabetical)
use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent};
use futures::StreamExt;
use ratatui::{DefaultTerminal, Frame};

// 3. Standard library
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::interval;
```

**Rules:**
- Three groups, blank-line separated: `crate::` first, external crates next, `std` last
- `imports_granularity = "Crate"` flattens `use foo::{a, b};` automatically
- No path aliases

## Error Handling

**Two layers:**

### Application layer: `anyhow`
- Functions return `Result<T>` (= `anyhow::Result<T>`)
- Errors propagate with `?`
- Context added with `.context("msg")` or `.with_context(|| ...)`
- Used in: `src/app.rs`, `src/state.rs`, `src/cli.rs`, most async/IO paths

### Domain layer: `thiserror`
- Custom error enums per domain (`AppError`, `GitError`, `DiffError`, `ManagerError`)
- Declared with `#[derive(Debug, Error)]`
- `#[error("...")]` per variant
- `#[from]` for auto-conversion from wrapped errors

Example from `src/error.rs`:
```rust
#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),
    #[error("PTY error: {0}")]
    Pty(String),
    #[error("State error: {0}")]
    State(String),
    #[error("Config error: {0}")]
    Config(String),
}
```

### `.unwrap()` usage
Significant usage (~150 `.unwrap()` calls across the crate). Distribution:
- `src/app.rs`: 18
- `src/state.rs`: 21
- `src/git/worktree.rs`: 26
- `src/git/repo.rs`: 27

Many are in code paths where failure is catastrophic anyway (e.g., initial config). See `CONCERNS.md` for audit candidates.

## Logging

**Library:** `tracing` + `tracing-subscriber` + `tracing-appender`

**Setup:** `src/logging.rs` installs subscriber with env-filter + file appender.

**Usage pattern:** Standard `tracing::{info!, warn!, error!, debug!}` macros.

**Panic hook:** Installed in `logging.rs` to capture crash backtraces into the log file.

## Async & Concurrency

- **Runtime:** tokio (single runtime for the whole app, driven from `main.rs`)
- **Main loop:** `tokio::select!` multiplexes: crossterm events, PTY read-ready, status ticks (1s), refresh ticks (5s), file watch events
- **Background work:** `tokio::spawn` for per-task I/O (e.g., async git diff)
- **Sync→async bridge:** Blocking ops (git2 calls) wrapped with `tokio::task::spawn_blocking` or exposed via `_async` function pairs

## Comments & Documentation

**Convention:** minimal, targeted.

**Observed patterns:**
- Top-of-file `//!` module docs where rationale is non-obvious (e.g., `src/state.rs`: *"v2: multi-project model (Project → Workspace hierarchy)."*)
- Section dividers in long files: `// ── Workspace types ──...`
- Inline comments reserved for non-obvious *why* (e.g., noting version migration intent)

**No heavy rustdoc.** Public APIs do not have triple-slash `///` doc comments as a rule.

## Serde Conventions

- Struct fields serialized as-is (no `rename_all`); field names match JSON keys
- Defaults via `#[serde(default = "default_expanded")]` for backward compat
- `Option<T>` used for optional fields (no `#[serde(skip_serializing_if)]` observed)

## Derives

Common derive sets:
- Data types: `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]`
- Enums with a natural zero: add `Default` + `#[default]` on a variant (see `Agent`)
- Error enums: `#[derive(Debug, Error)]`

## Allowances

- `#![allow(dead_code)]` at top of `src/state.rs` — intentional, some variants/fields used only via serde or future-facing
- `#[allow(dead_code)]` on `AppError` enum in `src/error.rs`

## UI Conventions (ratatui)

- Layout computation isolated in `src/ui/layout.rs` as pure functions over `(area, state)` → `PaneRects`
- Widget drawing receives `&mut Frame` + pre-computed rects
- Modal state held in `App::modal: Option<Modal>`, rendered last (on top)
- Theme constants centralized in `src/ui/theme.rs`

## Testing Conventions

See [TESTING.md](TESTING.md). Summary:
- Inline `#[cfg(test)] mod tests { ... }` per source file
- No `tests/` integration directory
- Snapshot testing via `insta` available as dev-dep (limited usage)

## Commit & PR Conventions

From `CONTRIBUTING.md` and recent git history:
- Conventional Commits (`feat:`, `fix:`, `chore:`, `docs:`)
- CI (`fmt`, `clippy`, `test`) must pass before merge

## Where Conventions Are Documented

- `CONTRIBUTING.md` — contributor workflow
- `rustfmt.toml`, `clippy.toml` — enforced rules
- `.github/workflows/ci.yml` — CI gates
- This file — descriptive conventions observed in code
