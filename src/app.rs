//! Application state and main event loop.

use crate::git::diff;
use crate::keys::{InputMode, Keymap};
use crate::pty::manager::PtyManager;
use crate::state::{GlobalState, Project, Workspace};
use crate::ui::layout::{LayoutState, PaneRects};
use crate::ui::modal::{Modal, NewWorkspaceForm};
use crate::ui::picker::{Picker, PickerKind};
use anyhow::Result;
use crossterm::event::{EventStream, KeyEvent};
use futures::StreamExt;
use ratatui::{DefaultTerminal, layout::Rect, widgets::ListState};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::interval;

#[derive(Debug, Clone)]
pub enum SidebarItem {
    RemoveProject(usize),
    Workspace(usize, usize),
    AddProject,
    NewWorkspace(usize),
    ArchivedHeader(usize),
    ArchivedWorkspace(usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionState {
    pub start_col: u16,
    pub start_row: u16,
    /// Anchored scroll generation at drag-start (D-06). Lives on PtySession
    /// (Plan 02) — captured here at mouse-down so the highlight stays
    /// pinned to text content even as new PTY output scrolls the screen.
    pub start_gen: u64,
    pub end_col: u16,
    pub end_row: u16,
    /// `None` while the user is mid-drag (end is cursor-relative); `Some`
    /// once the user releases the button (D-07). Anchoring the end after
    /// mouse-up means subsequent scroll events translate both endpoints.
    pub end_gen: Option<u64>,
    pub dragging: bool,
    /// Snapshot of the selected text captured at mouse-up so cmd+c can
    /// re-copy the same content even after the originally-selected rows
    /// have scrolled off the visible area (RESEARCH §Q2). vt100's
    /// `contents_between` only iterates visible rows, so without this
    /// snapshot a post-scroll-off cmd+c would yield only the surviving
    /// portion.
    pub text: Option<String>,
}

impl SelectionState {
    pub fn normalized(&self) -> ((u16, u16), (u16, u16)) {
        if self.start_row < self.end_row
            || (self.start_row == self.end_row && self.start_col <= self.end_col)
        {
            ((self.start_col, self.start_row), (self.end_col, self.end_row))
        } else {
            ((self.end_col, self.end_row), (self.start_col, self.start_row))
        }
    }

    pub fn is_empty(&self) -> bool {
        self.start_col == self.end_col && self.start_row == self.end_row
    }
}

pub struct App {
    pub global_state: GlobalState,
    pub active_project_idx: Option<usize>,
    pub layout: LayoutState,
    pub mode: InputMode,
    pub modal: Modal,
    pub picker: Option<Picker>,
    pub preview_lines: Option<(PathBuf, Vec<String>)>,
    pub left_list: ListState,
    pub right_list: ListState,
    pub pty_manager: PtyManager,
    pub active_workspace_idx: Option<usize>,
    pub active_tab: usize,
    pub modified_files: Vec<crate::git::diff::FileEntry>,

    pub keymap: Keymap,
    pub should_quit: bool,
    pub(crate) dirty: bool,
    pub watcher: Option<crate::watcher::Watcher>,
    pub(crate) diff_tx: tokio::sync::mpsc::UnboundedSender<Vec<crate::git::diff::FileEntry>>,
    pub(crate) diff_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<crate::git::diff::FileEntry>>,
    pub last_panes: Option<PaneRects>,
    pub sidebar_items: Vec<SidebarItem>,
    pub state_path: PathBuf,
    pub last_pty_size: (u16, u16),
    pub selection: Option<SelectionState>,
    /// Multi-click cluster tracking for double/triple-click detection (D-16).
    /// `last_click_at` captures the timestamp of the most recent
    /// `MouseEventKind::Down(Left)`; if the next click lands within 300ms
    /// at the same row/col, `last_click_count` increments. Otherwise the
    /// counter resets to 1.
    pub last_click_at: Option<std::time::Instant>,
    pub last_click_count: u8,
    pub last_click_row: u16,
    pub last_click_col: u16,
    pub last_frame_area: Rect,
    pub pending_workspace: Option<Option<String>>,
    pub archived_expanded: std::collections::HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabClick {
    Select(usize),
    Close(usize),
    Add,
}

impl App {
    pub async fn new(mut global_state: GlobalState, state_path: PathBuf) -> Result<Self> {
        let active_project_idx = global_state
            .active_project_id
            .as_ref()
            .and_then(|id| global_state.projects.iter().position(|project| &project.id == id))
            .or_else(|| (!global_state.projects.is_empty()).then_some(0));

        if let Some(idx) = active_project_idx {
            global_state.active_project_id = Some(global_state.projects[idx].id.clone());
        }

        let active_workspace_idx = active_project_idx
            .and_then(|idx| global_state.projects.get(idx))
            .and_then(|project| project.active().next().map(|_| 0));

        let watcher = if let Some(project) = active_project_idx.and_then(|idx| global_state.projects.get(idx)) {
            let mut watcher = crate::watcher::Watcher::new().ok();
            if let Some(w) = &mut watcher {
                let _ = w.watch(&project.repo_root);
            }
            watcher
        } else {
            None
        };

        let (diff_tx, diff_rx) =
            tokio::sync::mpsc::unbounded_channel::<Vec<crate::git::diff::FileEntry>>();

        let mut app = Self {
            global_state,
            active_project_idx,
            layout: LayoutState::new(),
            mode: InputMode::Terminal,
            modal: Modal::None,
            picker: None,
            preview_lines: None,
            left_list: ListState::default(),
            right_list: ListState::default(),
            pty_manager: PtyManager::new(),
            active_workspace_idx,
            active_tab: 0,
            modified_files: Vec::new(),

            keymap: Keymap::default_keymap(),
            should_quit: false,
            dirty: true,
            watcher,
            diff_tx,
            diff_rx,
            last_panes: None,
            sidebar_items: Vec::new(),
            state_path,
            last_pty_size: (24, 80),
            selection: None,
            last_click_at: None,
            last_click_count: 0,
            last_click_row: 0,
            last_click_col: 0,
            last_frame_area: Rect::default(),
            pending_workspace: None,
            archived_expanded: std::collections::HashSet::new(),
        };
        app.refresh_diff().await;
        crate::workspace::reattach_tmux_sessions(&mut app);
        Ok(app)
    }

    pub fn active_project(&self) -> Option<&Project> {
        self.active_project_idx
            .and_then(|idx| self.global_state.projects.get(idx))
    }

    pub(crate) fn active_project_mut(&mut self) -> Option<&mut Project> {
        self.active_project_idx
            .and_then(|idx| self.global_state.projects.get_mut(idx))
    }

    pub(crate) fn active_workspace(&self) -> Option<&Workspace> {
        self.active_project()
            .and_then(|project| self.active_workspace_idx.and_then(|idx| project.active().nth(idx)))
    }

    #[inline]
    pub(crate) fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Clear any active text selection and mark the frame dirty IFF a
    /// selection actually existed (D-22, D-23). Called from
    /// `set_active_tab`, `select_active_workspace`, and the four
    /// `active_workspace_idx`-write sites in `src/workspace.rs`.
    ///
    /// Avoids spurious redraws via the `take().is_some()` guard — if the
    /// user has no active selection, calling this is a no-op.
    #[inline]
    pub(crate) fn clear_selection(&mut self) {
        if self.selection.take().is_some() {
            self.mark_dirty();
        }
    }

    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let mut events = EventStream::new();
        // BG-02: safety-net fallback. Event-driven refresh via arm 3 (watcher)
        // is primary. 30s is RESEARCH §1 + ROADMAP success criterion #1.
        let mut refresh_tick = interval(Duration::from_secs(30));
        // Heartbeat: keeps the sidebar "working dot" animation advancing
        // without a high-frequency wakeup. Dropped from 1s to 5s now that
        // draw is dirty-gated. (See 02-RESEARCH §2 pitfall #5.)
        let mut heartbeat_tick = interval(Duration::from_secs(5));

        loop {
            if self.dirty {
                terminal.draw(|frame| crate::ui::draw::draw(self, frame))?;
                self.dirty = false;
            }
            self.sync_pty_size();

            if let Some(name) = self.pending_workspace.take() {
                match crate::workspace::create_workspace(self, name).await {
                    Ok(()) => self.modal = Modal::None,
                    Err(error) => {
                        self.modal = Modal::NewWorkspace(NewWorkspaceForm {
                            name_input: String::new(),
                            error: Some(error),
                        });
                    }
                }
                self.mark_dirty();
                continue;
            }

            if self.should_quit {
                break;
            }

            // Input-priority event loop (ARCH-03): the `biased` directive
            // below forces select! to poll futures top-to-bottom. The
            // `events.next()` branch sits first, so keyboard/mouse input
            // is processed on the very next iteration after a keystroke
            // becomes ready — PTY output and timers cannot starve input.
            tokio::select! {
                biased;

                // 1. INPUT — highest priority. Keyboard, mouse, paste, resize.
                Some(Ok(event)) = events.next() => {
                    self.mark_dirty();
                    crate::events::handle_event(self, event).await;
                }
                // 2. PTY output — high-volume under streaming agents.
                _ = self.pty_manager.output_notify.notified() => {
                    self.mark_dirty();
                }
                // 3. File watcher — debounced filesystem events.
                Some(event) = async {
                    if let Some(w) = &mut self.watcher {
                        w.next_event().await
                    } else {
                        futures::future::pending::<Option<crate::watcher::FsEvent>>().await
                    }
                } => {
                    let _ = event;
                    // BG-03: non-blocking. refresh_diff_spawn marks dirty internally.
                    self.refresh_diff_spawn();
                }
                // 4. Heartbeat — 5s tick to advance sidebar working-dot.
                _ = heartbeat_tick.tick() => {
                    self.mark_dirty();
                }
                // 5. BG-02 safety-net. Fires at t=0 (harmless — refresh_diff_spawn
                //    is idempotent and non-blocking; Pitfall #3), then every 30s.
                _ = refresh_tick.tick() => {
                    self.refresh_diff_spawn();
                }
                // 6. Diff-refresh results — drain background refresh_diff_spawn outputs.
                Some(files) = self.diff_rx.recv() => {
                    self.modified_files = files;
                    if self.modified_files.is_empty() {
                        self.right_list.select(None);
                    } else if self.right_list.selected().is_none() {
                        self.right_list.select(Some(0));
                    } else if let Some(selected) = self.right_list.selected() {
                        self.right_list
                            .select(Some(selected.min(self.modified_files.len() - 1)));
                    }
                    self.mark_dirty();
                }
            }
        }

        self.save_state();
        Ok(())
    }

    pub(crate) async fn refresh_diff(&mut self) {
        let (path, base_branch) = match (self.active_project(), self.active_workspace()) {
            (Some(_), Some(ws)) => (ws.worktree_path.clone(), ws.base_branch.clone()),
            (Some(p), None) => (p.repo_root.clone(), p.base_branch.clone()),
            _ => {
                self.modified_files.clear();
                self.right_list.select(None);
                return;
            }
        };

        if let Ok(files) = diff::modified_files(path, base_branch).await {
            self.modified_files = files;

            if self.modified_files.is_empty() {
                self.right_list.select(None);
            } else if self.right_list.selected().is_none() {
                self.right_list.select(Some(0));
            } else if let Some(selected) = self.right_list.selected() {
                self.right_list.select(Some(selected.min(self.modified_files.len() - 1)));
            }
        }
    }

    /// Non-blocking variant of [`refresh_diff`] for the navigation hot path.
    ///
    /// Spawns the git2 work onto a tokio task and returns immediately. Results
    /// arrive on `diff_rx` and are applied by the 6th select branch in
    /// `App::run`. Use from nav call-sites (workspace switch, sidebar
    /// activate) where blocking the input arm on git2 causes perceptible
    /// stutter. See
    /// `.planning/phases/04-navigation-fluidity/04-RESEARCH.md` §3 + §7.
    ///
    /// The existing async [`refresh_diff`] stays in use for the `App::new`
    /// pre-first-frame call and the watcher / refresh_tick branches — those
    /// are not on the user-facing input hot path.
    ///
    /// Do NOT collapse with [`refresh_diff`] — the sync-by-design return
    /// shape is load-bearing for the nav hot path. Plan 04-01's test
    /// `refresh_diff_spawn_is_nonblocking` locks in a <50ms return budget.
    pub(crate) fn refresh_diff_spawn(&mut self) {
        let args = match (self.active_project(), self.active_workspace()) {
            (Some(_), Some(ws)) => Some((ws.worktree_path.clone(), ws.base_branch.clone())),
            (Some(p), None) => Some((p.repo_root.clone(), p.base_branch.clone())),
            _ => None,
        };
        let Some((path, base_branch)) = args else {
            self.modified_files.clear();
            self.right_list.select(None);
            self.mark_dirty();
            return;
        };
        let tx = self.diff_tx.clone();
        tokio::spawn(async move {
            if let Ok(files) = diff::modified_files(path, base_branch).await {
                let _ = tx.send(files);
            }
        });
        self.mark_dirty();
    }

    pub(crate) fn open_new_tab_picker(&mut self) {
        self.picker = Some(Picker::new(
            vec![
                "opencode".to_string(),
                "claude".to_string(),
                "codex".to_string(),
                "aider".to_string(),
                "gemini".to_string(),
                "amp".to_string(),
                "goose".to_string(),
                "cline".to_string(),
                "shell".to_string(),
            ],
            PickerKind::NewTab,
        ));
    }

    pub(crate) fn select_active_workspace(&mut self, index: usize) {
        // D-22: per-session anchored gen — cross-workspace highlight is meaningless.
        self.clear_selection();
        self.active_workspace_idx = Some(index);
        self.right_list.select(None);
    }

    /// Canonical tab-switch primitive. D-22: clears any active selection
    /// first, because anchored (gen, row, col) coords are per-session and
    /// meaningless across tabs. D-23: unconditionally marks dirty so the
    /// tab-strip repaints regardless of whether a selection existed.
    /// Downstream call sites in `src/workspace.rs` and `src/events.rs`
    /// route through this instead of writing `self.active_tab = ...`
    /// directly.
    pub(crate) fn set_active_tab(&mut self, index: usize) {
        self.clear_selection();
        self.active_tab = index;
        self.mark_dirty();
    }

    pub(crate) fn refresh_active_workspace_after_change(&mut self) {
        let active_count = self.active_project().map(|project| project.active().count()).unwrap_or(0);
        self.active_workspace_idx = if active_count == 0 {
            None
        } else {
            Some(self.active_workspace_idx.unwrap_or(0).min(active_count - 1))
        };
    }

    pub(crate) fn save_state(&self) {
        if let Err(error) = self.global_state.save(&self.state_path) {
            tracing::error!("failed to save state: {error}");
        }
    }

    /// Non-blocking variant of [`save_state`].
    ///
    /// Clones `global_state` + `state_path` and dispatches the fs::write +
    /// atomic rename to a tokio blocking worker. Errors are logged via
    /// `tracing::error!` (same contract as the synchronous [`save_state`]).
    ///
    /// Use from every call site EXCEPT the graceful-exit drain in
    /// [`App::run`], where we need the write to complete before process
    /// exit (see Pitfall #5 below).
    ///
    /// Do NOT add `self.mark_dirty()` here — state save does not affect
    /// render (unlike `refresh_diff_spawn` which drives the right-pane
    /// file list). Do NOT `.await` the `spawn_blocking` JoinHandle —
    /// the fire-and-forget shape is load-bearing for BG-05.
    ///
    /// See `.planning/phases/05-background-work-decoupling/05-RESEARCH.md`
    /// §9 Pattern 2 + §8 Pitfall #5.
    ///
    /// Production call sites (Plan 05-03): 4 in `events.rs`, 7 in
    /// `workspace.rs`, 2 in `modal_controller.rs` — 13 total.
    pub(crate) fn save_state_spawn(&self) {
        let state = self.global_state.clone();
        let path = self.state_path.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(error) = state.save(&path) {
                tracing::error!("failed to save state: {error}");
            }
        });
    }

    pub(crate) fn write_active_tab_input(&mut self, bytes: &[u8]) {
        let Some(project) = self.active_project() else { return };
        let Some(workspace) = self.active_workspace() else { return };

        let sessions = self.active_sessions();
        let Some((tab_id, session)) = sessions.get(self.active_tab) else {
            self.mode = InputMode::Normal;
            return;
        };

        if session.is_exited() {
            self.mode = InputMode::Normal;
            return;
        }

        let project_id = project.id.clone();
        let ws_name = workspace.name.clone();
        let tab_id = *tab_id;

        let _ = self.pty_manager.write_input(&project_id, &ws_name, tab_id, bytes);
    }

    pub(crate) fn copy_selection_to_clipboard(&self) {
        let Some(sel) = &self.selection else { return };
        if sel.is_empty() {
            return;
        }

        // Prefer the text snapshot captured at mouse-up (sel.text from D-02).
        // It survives scroll-off because vt100's `contents_between` only
        // iterates visible rows. Fall back to live materialization when no
        // snapshot exists (e.g. selection seeded programmatically).
        let text = sel
            .text
            .clone()
            .unwrap_or_else(|| self.materialize_selection_text(sel));
        let trimmed = text.trim_end().to_string();
        if trimmed.is_empty() {
            return;
        }

        let _ = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(trimmed.as_bytes())?;
                }
                child.wait().map(|_| ())
            });
    }

    /// Materialize the visible text inside `sel` from the active session's
    /// vt100 screen. Returns the empty string when the active session can't
    /// be acquired or no rows match.
    ///
    /// Called from `copy_selection_to_clipboard` (snapshot fallback) AND
    /// from `handle_mouse::Up(Left)` (snapshot capture into `sel.text`).
    pub(crate) fn materialize_selection_text(&self, sel: &SelectionState) -> String {
        let sessions = self.active_sessions();
        let Some((_, session)) = sessions.get(self.active_tab) else {
            return String::new();
        };
        let Ok(parser) = session.parser.try_read() else {
            return String::new();
        };
        let screen = parser.screen();
        let ((sc, sr), (ec, er)) = sel.normalized();
        screen
            .contents_between(sr, sc, er, ec.saturating_add(1))
            .trim_end()
            .to_string()
    }

    /// Read the active session's `scroll_generation` counter. Returns 0
    /// when no active session is available — a safe default since gen=0 is
    /// also the initial value at session spawn (Plan 02).
    pub(crate) fn active_scroll_generation(&self) -> u64 {
        let sessions = self.active_sessions();
        let Some((_, session)) = sessions.get(self.active_tab) else {
            return 0;
        };
        session
            .scroll_generation
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Compute the word-boundary `(start_col, end_col)` containing `col` on
    /// `row`. Word chars = non-whitespace AND not in the punctuation
    /// blacklist (see RESEARCH §Q3). Skips `is_wide_continuation()` cells
    /// when walking left/right so wide CJK / emoji glyphs are treated as
    /// a single character. Returns `None` if no active session.
    fn word_boundary_at(&self, row: u16, col: u16) -> Option<(u16, u16)> {
        let sessions = self.active_sessions();
        let (_, session) = sessions.get(self.active_tab)?;
        let parser = session.parser.try_read().ok()?;
        let screen = parser.screen();
        let (_, cols) = screen.size();
        let is_word_char = |s: &str| -> bool {
            s.chars().next().is_some_and(|c| {
                !c.is_whitespace()
                    && !matches!(
                        c,
                        '[' | ']'
                            | '('
                            | ')'
                            | '<'
                            | '>'
                            | '{'
                            | '}'
                            | '.'
                            | ','
                            | ';'
                            | ':'
                            | '!'
                            | '?'
                            | '\''
                            | '"'
                            | '`'
                            | '/'
                            | '\\'
                            | '|'
                            | '@'
                            | '#'
                            | '$'
                            | '%'
                            | '^'
                            | '&'
                            | '*'
                            | '='
                            | '+'
                            | '~'
                    )
            })
        };
        let prev_col = |c: u16| {
            if c == 0 {
                return 0;
            }
            let mut nc = c - 1;
            while nc > 0
                && screen
                    .cell(row, nc)
                    .is_some_and(|cl| cl.is_wide_continuation())
            {
                nc -= 1;
            }
            nc
        };
        let next_col = |c: u16| {
            let mut nc = c + 1;
            while nc < cols
                && screen
                    .cell(row, nc)
                    .is_some_and(|cl| cl.is_wide_continuation())
            {
                nc += 1;
            }
            nc
        };
        let mut start = col;
        while start > 0 {
            let p = prev_col(start);
            let Some(cell) = screen.cell(row, p) else { break };
            if !cell.has_contents() || !is_word_char(cell.contents()) {
                break;
            }
            start = p;
        }
        let mut end = col;
        while end + 1 < cols {
            let n = next_col(end);
            let Some(cell) = screen.cell(row, n) else { break };
            if !cell.has_contents() || !is_word_char(cell.contents()) {
                break;
            }
            end = n;
        }
        Some((start, end))
    }

    /// D-15 — Select the word containing `(row, col)` on the active
    /// session's screen. Anchored to the current `scroll_generation` for
    /// both endpoints. Snapshots text immediately so cmd+c works post
    /// scroll-off.
    ///
    /// Uses the "compute-read-only-first, then mutate" pattern: all
    /// `&self` reads happen before the single `&mut self.selection` write,
    /// avoiding borrow-checker conflicts between `active_sessions()` /
    /// `materialize_selection_text` (both `&self`) and the mutation.
    pub(crate) fn select_word_at(&mut self, row: u16, col: u16) {
        // Compute all read-only values BEFORE any &mut self.selection borrow.
        let Some((start, end)) = self.word_boundary_at(row, col) else {
            return;
        };
        let current_gen = self.active_scroll_generation();

        // Build the SelectionState (no borrow yet).
        let new_sel = SelectionState {
            start_col: start,
            start_row: row,
            start_gen: current_gen,
            end_col: end,
            end_row: row,
            end_gen: Some(current_gen),
            dragging: false,
            text: None,
        };

        // Snapshot text via &self call (still no &mut borrow).
        let text = self.materialize_selection_text(&new_sel);

        // Single &mut self.selection borrow at end.
        self.selection = Some(SelectionState {
            text: Some(text),
            ..new_sel
        });
        self.mark_dirty();
    }

    /// D-15 / D-18 — Select the visible row containing `row` from col=0 to
    /// the last non-whitespace column. Wrapped lines are NOT joined (D-18
    /// scope decision: vt100 visible row only).
    pub(crate) fn select_line_at(&mut self, row: u16) {
        // Block-scope the parser read so it drops at the inner block end.
        let (end, current_gen) = {
            let sessions = self.active_sessions();
            let Some((_, session)) = sessions.get(self.active_tab) else {
                return;
            };
            let Ok(parser) = session.parser.try_read() else {
                return;
            };
            let screen = parser.screen();
            let (_, cols) = screen.size();
            let mut end = 0u16;
            for c in 0..cols {
                if let Some(cell) = screen.cell(row, c)
                    && cell.has_contents()
                    && cell
                        .contents()
                        .chars()
                        .next()
                        .is_some_and(|ch| !ch.is_whitespace())
                {
                    end = c;
                }
            }
            let gen_count = session
                .scroll_generation
                .load(std::sync::atomic::Ordering::Relaxed);
            (end, gen_count)
            // `parser` and `sessions` drop here at block end.
        };

        let new_sel = SelectionState {
            start_col: 0,
            start_row: row,
            start_gen: current_gen,
            end_col: end,
            end_row: row,
            end_gen: Some(current_gen),
            dragging: false,
            text: None,
        };
        let text = self.materialize_selection_text(&new_sel);
        self.selection = Some(SelectionState {
            text: Some(text),
            ..new_sel
        });
        self.mark_dirty();
    }

    /// D-19 — Extend the END endpoint of the active selection to
    /// `(row, col)`. No-op if no selection exists. Re-anchors `end_gen`
    /// to the current `scroll_generation` and refreshes the text snapshot.
    ///
    /// Uses the "compute-read-only-first, then &mut borrow" pattern to
    /// avoid borrow-checker conflicts (active_scroll_generation +
    /// materialize_selection_text are &self).
    pub(crate) fn extend_selection_to(&mut self, row: u16, col: u16) {
        // Compute read-only values BEFORE taking any &mut self.selection borrow.
        let current_gen = self.active_scroll_generation();
        let Some(sel_snapshot) = self.selection.as_ref().cloned() else {
            return;
        };

        // Build the post-mutation snapshot to feed materialize.
        let mut next_sel = sel_snapshot;
        next_sel.end_col = col;
        next_sel.end_row = row;
        next_sel.end_gen = Some(current_gen);

        // Materialize text via &self before taking &mut borrow.
        let text = self.materialize_selection_text(&next_sel);
        next_sel.text = Some(text);

        // Single &mut self.selection write.
        self.selection = Some(next_sel);
        self.mark_dirty();
    }

    pub(crate) fn forward_key_to_pty(&mut self, key: &KeyEvent) {
        let Some(bytes) = crate::events::key_to_bytes(key) else { return };
        self.write_active_tab_input(&bytes);
    }

    fn sync_pty_size(&mut self) {
        let Some(panes) = &self.last_panes else { return };

        let rows = panes.terminal.height.saturating_sub(3);
        let cols = panes.terminal.width.saturating_sub(2);

        if rows == 0 || cols == 0 {
            return;
        }

        if (rows, cols) == self.last_pty_size {
            return;
        }

        self.last_pty_size = (rows, cols);

        let Some(project) = self.active_project() else { return };
        let Some(workspace) = self.active_workspace() else { return };

        let project_id = project.id.clone();
        let ws_name = workspace.name.clone();
        let _ = self.pty_manager.resize_all_for(&project_id, &ws_name, rows, cols);

        let tab_tmux_names: Vec<String> = workspace
            .tabs
            .iter()
            .map(|tab| crate::tmux::tab_session_name(&project_id, &ws_name, tab.id))
            .collect();
        tokio::task::spawn_blocking(move || {
            for name in tab_tmux_names {
                crate::tmux::resize_session(&name, cols, rows);
            }
        });
    }

    pub(crate) fn build_working_map(&self) -> std::collections::HashMap<(String, String), bool> {
        use std::time::Duration;
        let threshold = Duration::from_secs(2);
        let mut map = std::collections::HashMap::new();

        for project in &self.global_state.projects {
            for workspace in project.active() {
                if workspace.tabs.is_empty() {
                    continue;
                }
                let any_working = workspace.tabs.iter().any(|tab| {
                    self.pty_manager
                        .get_session(&project.id, &workspace.name, tab.id)
                        .map(|s| s.is_working(threshold))
                        .unwrap_or(false)
                });
                map.insert((project.id.clone(), workspace.name.clone()), any_working);
            }
        }
        map
    }

    pub(crate) fn active_sessions(&self) -> Vec<(u32, &crate::pty::session::PtySession)> {
        let Some(project) = self.active_project() else {
            return Vec::new();
        };
        let Some(workspace) = self.active_workspace() else {
            return Vec::new();
        };

        workspace
            .tabs
            .iter()
            .filter_map(|tab| {
                self.pty_manager
                    .get_session(&project.id, &workspace.name, tab.id)
                    .map(|session| (tab.id, session))
            })
            .collect()
    }

    pub(crate) fn tab_at_column(&self, terminal: Rect, col: u16) -> Option<TabClick> {
        let workspace = self.active_workspace()?;
        let mut current_col = terminal.x;
        for (idx, tab) in workspace.tabs.iter().enumerate() {
            let label = crate::ui::terminal::tab_label(&tab.command);
            let width = format!(" {label} ✕ ").chars().count() as u16;
            if col >= current_col && col < current_col + width {
                let close_start = current_col + width.saturating_sub(2);
                return if col >= close_start {
                    Some(TabClick::Close(idx))
                } else {
                    Some(TabClick::Select(idx))
                };
            }
            current_col += width;
        }

        let add_width = " [+] ".chars().count() as u16;
        if col >= current_col && col < current_col + add_width {
            return Some(TabClick::Add);
        }

        None
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

#[cfg(test)]
impl App {
    /// Test-only: register a pre-spawned `PtySession` as the active tab
    /// of the active workspace of the active project. Creates a minimal
    /// `Project` + `Workspace` + `TabSpec` stub if none exist. Returns
    /// the assigned tab id.
    ///
    /// Downstream selection tests (Plans 03, 04, 06) use this seam to
    /// inspect `session.scroll_generation` and PTY parser state without
    /// going through the full `PtyManager::spawn_tab` path. Gated
    /// `#[cfg(test)]` so production binary surface is unchanged.
    #[allow(dead_code)]
    pub(crate) fn inject_test_session(
        &mut self,
        session: crate::pty::session::PtySession,
    ) -> u32 {
        use crate::state::{Agent, Project, TabSpec, Workspace, WorkspaceStatus};

        // Ensure a project exists.
        if self.global_state.projects.is_empty() {
            let project = Project {
                id: "test-project".to_string(),
                name: "test-project".to_string(),
                repo_root: std::env::temp_dir().join("martins-test-project"),
                base_branch: "main".to_string(),
                workspaces: Vec::new(),
                added_at: "2026-04-24T00:00:00Z".to_string(),
                expanded: true,
            };
            self.global_state.projects.push(project);
        }
        self.active_project_idx = Some(0);
        self.global_state.active_project_id =
            Some(self.global_state.projects[0].id.clone());

        // Ensure a workspace exists in the active project.
        let project = &mut self.global_state.projects[0];
        if project.workspaces.is_empty() {
            project.workspaces.push(Workspace {
                name: "test-ws".to_string(),
                worktree_path: std::env::temp_dir().join("martins-test-ws"),
                base_branch: "main".to_string(),
                agent: Agent::default(),
                status: WorkspaceStatus::Inactive,
                created_at: "2026-04-24T00:00:00Z".to_string(),
                tabs: Vec::new(),
            });
        }
        self.active_workspace_idx = Some(0);

        // Append a tab to the active workspace.
        let workspace = &mut self.global_state.projects[0].workspaces[0];
        let tab_id: u32 = workspace
            .tabs
            .iter()
            .map(|t| t.id)
            .max()
            .map(|m| m + 1)
            .unwrap_or(1);
        workspace.tabs.push(TabSpec {
            id: tab_id,
            command: "/bin/cat".to_string(),
        });
        self.active_tab = workspace.tabs.len() - 1;

        // Insert the session into pty_manager keyed on
        // (project_id, ws_name, tab_id).
        let project_id = self.global_state.projects[0].id.clone();
        let ws_name = self.global_state.projects[0].workspaces[0].name.clone();
        self.pty_manager
            .insert_for_test(project_id, ws_name, tab_id, session);

        tab_id
    }
}
