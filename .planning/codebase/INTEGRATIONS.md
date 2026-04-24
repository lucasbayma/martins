# External Integrations

**Analysis Date:** 2026-04-24

## Overview

Martins integrates with external processes and the local filesystem rather than network APIs. There are **no HTTP APIs, databases, auth providers, or webhooks** in this codebase. All "integrations" are local: subprocess management (tmux, git, AI agents), filesystem persistence, and PTY I/O.

## Subprocess Integrations

### tmux

**Purpose:** Session persistence — each tab runs inside a tmux session, so agents survive app restarts.

**Integration file:** `src/tmux.rs`

**How it's invoked:**
- Shelled out as `tmux` subprocess (requires `tmux` on `PATH`, checked with `which`)
- Generated `~/.martins/tmux.conf` loaded via `tmux -f <conf>`
- Commands: `new-session`, `attach-session`, `has-session`, `kill-session`, `send-keys`, `resize-window`
- Session names keyed by `(project_id, workspace_name, tab_id)`

**Data flow:**
1. App spawns tmux session per tab
2. PTY wraps the tmux client, not the agent directly
3. tmux server persists across app exits
4. Next `martins` run reattaches to existing sessions

**Failure modes:**
- Missing tmux binary → startup error
- tmux server crash → sessions lost, user sees empty panes
- Version incompatibility not explicitly handled

### git (CLI)

**Purpose:** Worktree creation/removal (operations not well-covered by libgit2).

**Integration file:** `src/git/worktree.rs`

**How it's invoked:**
- Shelled out as `git` subprocess
- Commands: `git worktree add`, `git worktree remove`, `git worktree list`, `git worktree prune`
- Branches auto-created per workspace (`workspace/<name>` pattern)

**Data flow:**
1. User creates workspace → `git worktree add <path> -b <branch>`
2. Worktree directory placed under `~/.martins/workspaces/{project_hash}/{workspace_name}/`
3. On delete → `git worktree remove` and branch cleanup

### AI Agent CLIs

**Purpose:** Each tab runs a user-selected AI agent binary inside its PTY.

**Integration file:** `src/agents.rs`, `src/state.rs` (Agent enum)

**Supported agents:**
- `opencode` (default)
- `claude` (Claude Code CLI)
- `codex` (OpenAI Codex CLI)
- Plain shell (user's `$SHELL`)

**How it's invoked:**
- Agent command string stored in `TabSpec::command`
- Launched inside the tmux session spawned for the tab
- User-provided argument forms via `CommandArgsForm` modal (`src/ui/modal.rs`)

**Data flow:**
- User presses `t` → picks agent → command string composed → tmux session runs it
- No API calls; the agent itself handles its own network I/O

### External Editor

**Purpose:** Spawn user's editor for file editing (e.g., from preview pane).

**Integration file:** `src/editor.rs`

**How it's invoked:**
- Reads `$EDITOR` environment variable
- Spawns as subprocess with file path argument
- TUI is suspended/restored around the editor session

### pbcopy (macOS clipboard)

**Purpose:** Copy selected text to clipboard.

**How it's invoked:** Subprocess `pbcopy` piped stdin with selected text. macOS-only.

## libgit2 (git2 crate)

**Purpose:** Repository introspection and diff tracking (where subprocess overhead is too high).

**Integration files:** `src/git/repo.rs`, `src/git/diff.rs`

**Operations:**
- Repository discovery (walk up from CWD to find `.git`)
- Current branch detection
- Default/base branch detection
- Diff between working tree and base branch (for `modified_files` list)

**Error wrapping:**
- `GitError` thiserror enum wraps `git2::Error` with contextual variants (see `src/error.rs` for `AppError::Git`)

## Filesystem "Integrations"

### State persistence

**Location:** `~/.martins/state.json`

**File:** `src/state.rs`

**Strategy:**
- Atomic write: write to `state.json.tmp` → rename → keep `state.json.bak`
- JSON-serialized `GlobalState` via serde_json
- Version field for migrations (currently v2: multi-project model)
- Fallback chain: `state.json` → `state.json.bak` → default empty state

### File system watcher

**Purpose:** Detect modifications in active workspace to refresh git diff.

**Integration file:** `src/watcher.rs`

**Library:** `notify` + `notify-debouncer-mini` (`notify-debouncer-mini = "0.4"`)

**Behavior:**
- Watches workspace worktree path recursively
- Debounced events (~500ms) feed into the tokio `select!` in `App::run`
- Triggers `refresh_diff()` to update modified files sidebar

### Logging

**Purpose:** Structured tracing output for debugging.

**Integration file:** `src/logging.rs`

**Destination:** `~/.martins/logs/martins.log` (via `tracing-appender`)

**Controls:**
- Filter via `RUST_LOG` env var (`tracing-subscriber`'s `env-filter` feature)
- Panic hook installed to log crashes with backtrace

## Platform Integrations

### XDG / directories

**Library:** `directories = "5"`

**File:** `src/config.rs`

**Paths resolved:**
- Home dir → `~/.martins/` (state, logs, tmux.conf)
- Cache / data dirs follow XDG spec where applicable

**Write probe:** `src/config.rs` tests directory writability before persisting.

## Environment Variables Consumed

| Variable | Purpose | File |
|---|---|---|
| `EDITOR` | External editor command | `src/editor.rs` |
| `SHELL` | Default shell for shell tabs | `src/pty/` / `src/state.rs` |
| `RUST_LOG` | Log filter | `src/logging.rs` |
| `PATH` | Locate `tmux`, `git`, agent binaries | via `which` crate |

## Authentication

**No app-level auth.**

Authentication is fully delegated to:
- `git` credentials and SSH agent (for git operations on private repos)
- AI agent CLIs (each handles its own auth — e.g., Claude Code, Codex, OpenCode)
- User's shell environment (inherited by spawned PTYs)

## Network

**No direct network I/O from Martins itself.** All network traffic happens inside spawned agent subprocesses (Claude Code, Codex, OpenCode), which manage their own API calls.

## Risk Surface

- **Subprocess trust:** Martins launches tmux/git/agent binaries from `$PATH`. A compromised `$PATH` could redirect to malicious binaries.
- **Path validation:** Workspace names are used to construct filesystem paths. See `CONCERNS.md` for path-traversal notes.
- **TLS:** Inherited from `openssl = { features = ["vendored"] }` via git2 — vendored libcrypto, patching requires Martins rebuild.
