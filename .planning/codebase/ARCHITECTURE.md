# Architecture

**Analysis Date:** 2026-04-24

## Pattern Overview

**Overall:** Event-driven async TUI application with multi-layered architecture.

**Key Characteristics:**
- Async event loop driven by `tokio::select!` for concurrent I/O handling
- Stateful application model with persistent JSON state (GlobalState)
- Responsive 3-pane layout with toggleable sidebars
- Terminal session management via tmux integration
- File watching and git diff tracking for live file status

## Layers

**Presentation (UI):**
- Purpose: Render terminal user interface components and handle user input
- Location: `src/ui/` directory
- Contains: Layout computation, modal dialogs, sidebars, pickers, theme styling
- Depends on: Ratatui framework, crossterm for events, state data
- Used by: Main app event loop in `src/app.rs`

**Application Logic:**
- Purpose: Coordinate app state, event handling, and persistence
- Location: `src/app.rs` (primary coordinator)
- Contains: Event loop, session management, workspace/project navigation, keyboard input routing
- Depends on: PTY manager, git operations, configuration, UI modules
- Used by: Main entry point `src/main.rs`

**State Management:**
- Purpose: Define and persist application state (projects, workspaces, tabs)
- Location: `src/state.rs`
- Contains: GlobalState (multi-project model), Project, Workspace, TabSpec, Agent types
- Depends on: Serde for serialization
- Used by: App and CLI modules for load/save operations

**Terminal Session Management:**
- Purpose: Manage PTY sessions and tmux integration
- Location: `src/pty/`, `src/tmux.rs`
- Contains: PTY session spawning, tmux process wrapping, shell command handling
- Depends on: portable-pty, tokio async
- Used by: App for terminal display and input

**Git Integration:**
- Purpose: Repository operations, worktree management, diff tracking
- Location: `src/git/` (repo.rs, worktree.rs, diff.rs)
- Contains: Repository discovery, branch detection, file change tracking
- Depends on: git2 library
- Used by: App for workspace creation, diff display

**Configuration:**
- Purpose: Path resolution, state persistence locations, gitignore management
- Location: `src/config.rs`
- Contains: XDG-aware directory resolution, repo hashing, write probe testing
- Depends on: directories crate
- Used by: Main initialization, state save/load

**CLI Interface:**
- Purpose: Command-line subcommand handling
- Location: `src/cli.rs`
- Contains: Clap command definitions, workspace listing/archiving/removal
- Depends on: Clap for argument parsing
- Used by: Main entry point for non-TUI operations

## Data Flow

**Initialization Flow:**

1. `main()` parses CLI args via `Clap`
2. If subcommand: execute CLI command and exit
3. Initialize logging and load global state from `~/.martins/state.json`
4. Auto-discover git repo at current working directory if no projects exist
5. Initialize ratatui terminal with mouse/paste support
6. Create `App` struct with PtyManager, initialize tmux sessions for existing workspaces
7. Enter main event loop

**Event Loop:**

```
loop {
  - Render UI (all panes)
  - Sync PTY size
  - Check pending workspace creation
  
  tokio::select! {
    crossterm events       => handle_event (keyboard/mouse)
    PTY output ready       => [display refresh]
    status tick (1s)       => [status bar update]
    refresh tick (5s)      => refresh_diff()
    file system changes    => refresh_diff()
  }
  
  if should_quit => break
}

Save state and exit
```

**Workspace Creation Flow:**

1. User presses 'n' → show NewWorkspaceForm modal
2. User enters name → triggers `create_workspace(name)`
3. Git worktree created via `git worktree add`
4. Workspace struct added to active project
5. Tmux session spawned for new workspace
6. PTY session attached
7. State persisted to JSON

**File Status Update Flow:**

1. Periodic timer (5s) or file system watch triggers `refresh_diff()`
2. Get active workspace/project paths
3. Call `diff::modified_files()` (async spawned task)
4. Query git status against base branch
5. Update `app.modified_files` vec
6. Next draw cycle displays in right sidebar

**State Management & Persistence:**

```
GlobalState (v2)
├── projects: Vec<Project>
│   ├── id: String (SHA256 hash of repo path)
│   ├── name: String
│   ├── repo_root: PathBuf
│   ├── base_branch: String
│   ├── workspaces: Vec<Workspace>
│   │   ├── name: String (MPB-generated)
│   │   ├── worktree_path: PathBuf
│   │   ├── agent: Agent (Claude/Opencode/Codex)
│   │   ├── status: WorkspaceStatus (Active/Inactive/Archived/Deleted/Exited)
│   │   └── tabs: Vec<TabSpec>
│   │       ├── id: u32
│   │       └── command: String (agent command or "shell")
│   └── expanded: bool (UI state)
└── active_project_id: Option<String>
```

Saved atomically to `~/.martins/state.json` with backup (`state.json.bak`).

## Key Abstractions

**Modal Dialog System:**
- Purpose: Handle user input for forms (new workspace, new project, confirmations)
- Examples: `src/ui/modal.rs` contains AddProjectForm, NewWorkspaceForm, ConfirmDelete, CommandArgsForm
- Pattern: Modal enum variants hold form state, dispatched by app

**Sidebar Items:**
- Purpose: Represent navigable tree structure (projects → workspaces → archived sections)
- Examples: `src/app.rs` SidebarItem enum
- Pattern: Built during render, indexed for click/keyboard navigation

**PTY Manager:**
- Purpose: Abstract multiplexed terminal sessions keyed by (project_id, workspace_id, tab_id)
- Examples: `src/pty/manager.rs`
- Pattern: HashMap with SessionKey tuples, spawn/write/resize operations

**Picker:**
- Purpose: Generic file/directory picker modal
- Examples: `src/ui/picker.rs` PickerKind enum (File/Directory)
- Pattern: Async modal with search, keyboard navigation, preview

## Entry Points

**Terminal UI:**
- Location: `src/main.rs::main()` → `src/app.rs::App::run()`
- Triggers: Running `martins` with no args or with `--path`
- Responsibilities: Event loop, state coordination, UI rendering

**CLI Commands:**
- Location: `src/cli.rs::run()` with subcommands
- Triggers: `martins workspaces list|archive|remove|prune` or `martins keybinds`
- Responsibilities: Non-interactive operations on state/workspaces

**Event Handlers:**
- Location: `src/app.rs::handle_event()` and `handle_*` methods
- Dispatches: keyboard input, mouse clicks, modal input
- Routes to: workspace creation, navigation, project management

## Error Handling

**Strategy:** Layered error propagation with anyhow Result types, recovery where possible.

**Patterns:**
- Git operations wrapped in custom `GitError` enum with context
- State load failures fall back to backup then default (resilient)
- PTY spawn failures logged but don't crash app
- Modal forms show error messages to user and preserve form state
- File I/O wrapped with `?` operator and context added via `.context()`

## Cross-Cutting Concerns

**Logging:** Initialized via `tracing` subscriber with optional file output to `~/.martins/logs/`. Panic hook installed for crash logging.

**Validation:** Project discovery via git2, workspace names validated against existing names, path writability probed before persisting.

**Authentication:** Inherits from tmux and git (respects existing git credentials, SSH agent). No app-level auth.

**File System:**
- Workspaces stored in `~/.martins/workspaces/{project_hash}/`
- Git worktrees linked to main repo via `.git/worktrees/`
- State persisted in `~/.martins/state.json`
- Logs in `~/.martins/logs/`
