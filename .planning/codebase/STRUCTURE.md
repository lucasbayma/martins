# Codebase Structure

**Analysis Date:** 2026-04-24

## Directory Layout

```
martins/
├── src/                       # Rust source code
│   ├── main.rs               # Entry point, async runtime setup
│   ├── app.rs                # Main application state, event loop (77KB)
│   ├── cli.rs                # CLI subcommands and argument parsing
│   ├── state.rs              # GlobalState, Project, Workspace data models
│   ├── config.rs             # Path resolution, state file locations
│   ├── error.rs              # AppError type definition
│   ├── keys.rs               # Keymap, input mode, action definitions
│   ├── editor.rs             # External editor spawning
│   ├── agents.rs             # Agent type definitions and utilities
│   ├── mpb.rs                # MPB (Brazilian music) name generation
│   ├── logging.rs            # Tracing subscriber setup, panic hook
│   ├── tools.rs              # Utility functions
│   ├── tmux.rs               # Tmux subprocess wrapper and control
│   ├── watcher.rs            # File system watcher for git changes
│   ├── ui/                   # Terminal UI rendering
│   │   ├── mod.rs            # Module exports
│   │   ├── layout.rs         # 3-pane layout computation (responsive)
│   │   ├── theme.rs          # Color scheme definitions
│   │   ├── modal.rs          # Modal dialogs (forms, confirmations)
│   │   ├── picker.rs         # File/directory picker UI
│   │   ├── preview.rs        # File preview rendering
│   │   ├── sidebar_left.rs   # Projects/workspaces tree sidebar
│   │   ├── sidebar_right.rs  # Git diff/files sidebar
│   │   └── terminal.rs       # Terminal pane with tmux session
│   ├── pty/                  # Pseudo-terminal session management
│   │   ├── mod.rs            # Module exports
│   │   ├── manager.rs        # Multi-session PTY manager
│   │   └── session.rs        # Individual session wrapper
│   └── git/                  # Git repository operations
│       ├── mod.rs            # Module exports
│       ├── repo.rs           # Repository discovery, branch operations
│       ├── worktree.rs       # Git worktree linking/unlinking
│       └── diff.rs           # File status vs base branch
├── Cargo.toml                # Rust dependencies and metadata
├── Cargo.lock                # Locked dependency versions
├── clippy.toml               # Clippy linter configuration
├── rustfmt.toml              # Code formatter configuration
├── README.md                 # Project overview and usage guide
├── CHANGELOG.md              # Version history
├── LICENSE                   # MIT license
├── CONTRIBUTING.md           # Contribution guidelines
├── CODE_OF_CONDUCT.md        # Community conduct standards
├── resources/                # Documentation and images
│   └── martins.png          # Project logo/screenshot
├── Formula/                  # Homebrew formula for installation
├── .github/                  # GitHub configuration
├── .gitignore               # Git ignore rules
├── .planning/               # GSD planning artifacts
│   └── codebase/           # Architecture documentation (this file)
└── .claude/                 # Claude-specific metadata
```

## Directory Purposes

**`src/`:**
- Purpose: All Rust source code
- Contains: 30 .rs files organized by concern (UI, PTY, Git, CLI)
- Key files: `main.rs`, `app.rs`, `state.rs`

**`src/ui/`:**
- Purpose: Terminal UI rendering and layout
- Contains: 9 files for different UI components
- Key files: `layout.rs` (responsive layout), `modal.rs` (forms), `sidebar_left.rs` (navigation tree)

**`src/pty/`:**
- Purpose: Terminal session management via tmux
- Contains: 3 files for PTY spawning and control
- Key files: `manager.rs` (session multiplexing), `session.rs` (individual session wrapping)

**`src/git/`:**
- Purpose: Git repository operations
- Contains: 4 files for git2 operations
- Key files: `repo.rs` (discovery/branches), `diff.rs` (file status tracking), `worktree.rs` (git worktree management)

**`resources/`:**
- Purpose: Non-code assets
- Contains: Marketing images and documentation
- Generated: No
- Committed: Yes

**`Formula/`:**
- Purpose: Homebrew installation formula
- Contains: Ruby formula for `brew install martins`
- Generated: No
- Committed: Yes

**`.planning/`:**
- Purpose: GSD (code navigation/planning) documentation
- Contains: Architecture, structure, conventions, testing, concerns analysis
- Generated: Yes (by GSD tools)
- Committed: Yes

**`.claude/`:**
- Purpose: Claude-specific metadata and project configuration
- Contains: Skills, memory, worktree state
- Generated: Yes
- Committed: No

## Key File Locations

**Entry Points:**
- `src/main.rs`: Application entry point, async runtime initialization, CLI dispatch
- `src/app.rs::App::run()`: Main event loop and TUI orchestration

**Configuration:**
- `src/config.rs`: Path resolution (XDG aware)
- `Cargo.toml`: Dependency declarations
- `rustfmt.toml`: Formatting rules
- `clippy.toml`: Linting rules

**Core Logic:**
- `src/state.rs`: Data model (GlobalState, Project, Workspace)
- `src/app.rs`: Application state machine and event dispatcher
- `src/cli.rs`: CLI subcommand implementations

**Terminal & PTY:**
- `src/tmux.rs`: Tmux process control
- `src/pty/manager.rs`: Multi-session terminal management
- `src/pty/session.rs`: Individual terminal session wrapper

**Git Integration:**
- `src/git/repo.rs`: Repository operations
- `src/git/diff.rs`: File change tracking
- `src/git/worktree.rs`: Worktree creation/deletion

**UI Components:**
- `src/ui/layout.rs`: 3-pane responsive layout
- `src/ui/sidebar_left.rs`: Project/workspace navigation tree
- `src/ui/sidebar_right.rs`: Modified files list
- `src/ui/modal.rs`: Modal dialogs and forms
- `src/ui/picker.rs`: File/directory picker
- `src/ui/theme.rs`: Color scheme

**Testing:**
- No dedicated test files; tests are inline with `#[cfg(test)]` modules

## Naming Conventions

**Files:**
- Rust module files: `snake_case.rs`
- Submodule organization: `mod.rs` with `pub mod xyz;` exports
- UI components: Descriptive names (sidebar_left, sidebar_right, terminal)

**Directories:**
- Feature modules: Plural or descriptive (`ui/`, `pty/`, `git/`)
- No underscore prefixes (all public modules)

**Types & Structs:**
- Domain types: `PascalCase` (App, GlobalState, Workspace, Project)
- Enums: `PascalCase` (WorkspaceStatus, Agent, InputMode)
- Error types: Suffix with `Error` (AppError, GitError, ManagerError, DiffError)

**Functions:**
- Private functions: `snake_case`
- Public functions: `snake_case`
- Async functions: `_async` suffix for blocking wrappers (e.g., `current_branch_async`)

**Variables:**
- Local bindings: `snake_case`
- Module constants: `SCREAMING_SNAKE_CASE` (e.g., ACCENT_GOLD in theme)

## Where to Add New Code

**New Feature (e.g., workspace search):**
- Primary code: Implement logic in relevant module (e.g., `state.rs` for filtering)
- UI: Add modal or sidebar component in `src/ui/`
- Event handling: Add action in `src/keys.rs` and route in `src/app.rs::handle_event()`
- Tests: Add inline `#[cfg(test)]` module in same file

**New Component/Module:**
- Implementation: Create `src/{feature}/mod.rs` with exports
- Sub-items: Create `src/{feature}/{item}.rs` for each concern
- Public API: Use `pub use` in `mod.rs` for clean imports
- Integration: Import and use in `src/app.rs` or other coordinating modules

**Utilities:**
- Shared helpers: Add to `src/tools.rs` or create focused module (e.g., `src/validation.rs`)
- Formatting: Use `rustfmt.toml` rules (already configured)
- Error handling: Use `thiserror` for custom error types

**Database/State:**
- State types: Add to `src/state.rs` (e.g., new Workspace field)
- Persistence: Implement Save/Load in GlobalState impl block
- Migration: Add version bump in `src/state.rs::GlobalState::version`

## Special Directories

**`.martins/` (User home):**
- Purpose: Runtime state and logs
- Contents:
  - `state.json`: Global state (projects, workspaces)
  - `state.json.bak`: Backup for recovery
  - `tmux.conf`: Generated tmux configuration
  - `logs/`: Application logs
  - `workspaces/`: Workspace directory structure (not used; git worktrees used instead)
- Generated: Yes (by app at runtime)
- Committed: No

**`.git/worktrees/`:**
- Purpose: Git linked worktree directories
- Created: When workspace is created (via `git worktree add`)
- Managed: App deletes worktree entry when workspace removed
- Committed: No (worktree metadata only)

## File Growth & Size Notes

**Large Files (potential for refactoring):**
- `src/app.rs` (77KB): Main application logic, event loop, state machine
  - Candidates for splitting: Modal handling, event routing, drawing logic
- `src/ui/modal.rs` (24KB): All modal form types and handling
  - Could split into separate form modules per domain
- `src/ui/sidebar_left.rs` (10KB): Left sidebar rendering and navigation tree
  - Mostly UI composition logic

**Thin/Focused Files:**
- `src/error.rs` (384 bytes): Simple error enum
- `src/editor.rs` (1.3KB): Minimal external editor wrapper
- `src/ui/theme.rs` (778 bytes): Color definitions

## Import Organization Pattern

**Observed pattern in `src/app.rs`:**
```rust
// Internal crate modules (relative imports)
use crate::git::{diff, repo};
use crate::keys::{Action, InputMode, Keymap};
use crate::pty::manager::PtyManager;
use crate::state::{Agent, GlobalState, Project, TabSpec, Workspace};
use crate::ui::layout::{self, LayoutState, PaneRects};
use crate::ui::modal::{self, AddProjectForm, CommandArgsForm, Modal, NewWorkspaceForm};

// External crate dependencies
use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, ...};
use futures::StreamExt;
use ratatui::{DefaultTerminal, Frame, ...};

// Standard library
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::interval;
```

**Rules:**
- Group by: internal crate, external crates, std library
- Within groups: alphabetical by module
- Re-export: Use `use` in `mod.rs` files to control public API
- No path aliases: Direct `crate::` style (XDG/non-standard)

## Default/Common Commands

**Building:**
```bash
cargo build --release     # Optimized binary
cargo build              # Debug build
```

**Code Quality:**
```bash
cargo fmt               # Format code
cargo clippy           # Lint with clippy
cargo test             # Run tests (inlined)
```

**Running:**
```bash
cargo run              # Run in debug mode
martins               # Run installed binary
martins /path/to/repo # Open specific project
```
