use clap::{Parser, Subcommand};
use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::config;
use crate::state::{GlobalState, WorkspaceStatus};

#[derive(Parser)]
#[command(name = "martins", version, about = "Terminal workspace manager for AI coding agents orchestration")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Path to a git repository to open
    pub path: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Manage workspaces
    Workspaces {
        #[command(subcommand)]
        action: WorkspacesAction,
    },
    /// Show keyboard shortcuts
    Keybinds,
}

#[derive(Subcommand)]
pub enum WorkspacesAction {
    /// List all workspaces across projects
    List,
    /// Remove a workspace (delete from state and disk)
    Remove {
        /// Project name
        project: String,
        /// Workspace name
        name: String,
    },
    /// Archive a workspace
    Archive {
        /// Project name
        project: String,
        /// Workspace name
        name: String,
    },
    /// Unarchive a workspace
    Unarchive {
        /// Project name
        project: String,
        /// Workspace name
        name: String,
    },
    /// Delete all orphan workspace directories not tracked in state
    Prune,
}

pub fn run(cmd: Command) -> anyhow::Result<()> {
    match cmd {
        Command::Workspaces { action } => run_workspaces(action),
        Command::Keybinds => run_keybinds(),
    }
}

fn run_workspaces(action: WorkspacesAction) -> anyhow::Result<()> {
    let state_path = config::global_state_path();
    let mut state = GlobalState::load(&state_path).unwrap_or_default();
    let workspaces_dir = config::global_workspaces_dir();

    match action {
        WorkspacesAction::List => {
            let mut has_output = false;

            for project in &state.projects {
                println!("{}  {}", project.name, project.repo_root.display());
                has_output = true;

                let mut known_names: HashSet<String> = HashSet::new();
                for ws in &project.workspaces {
                    known_names.insert(ws.name.clone());
                    let status = match &ws.status {
                        WorkspaceStatus::Active => "active",
                        WorkspaceStatus::Inactive => "inactive",
                        WorkspaceStatus::Archived => "archived",
                        WorkspaceStatus::Deleted => "deleted",
                        WorkspaceStatus::Exited(code) => {
                            &format!("exited({})", code)
                        }
                    };
                    println!("  {:<20} {}", ws.name, status);
                }

                let project_ws_dir = workspaces_dir.join(&project.name);
                if let Ok(entries) = std::fs::read_dir(&project_ws_dir) {
                    for entry in entries.flatten() {
                        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if !known_names.contains(&name) {
                                println!("  {:<20} orphan", name);
                            }
                        }
                    }
                }

                if project.workspaces.is_empty() {
                    let project_ws_dir = workspaces_dir.join(&project.name);
                    let has_orphans = project_ws_dir.exists()
                        && std::fs::read_dir(&project_ws_dir)
                            .map(|mut d| d.next().is_some())
                            .unwrap_or(false);
                    if !has_orphans {
                        println!("  (no workspaces)");
                    }
                }
            }

            // Scan for project dirs not in state at all
            if let Ok(entries) = std::fs::read_dir(&workspaces_dir) {
                let known_projects: HashSet<String> =
                    state.projects.iter().map(|p| p.name.clone()).collect();
                for entry in entries.flatten() {
                    if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        continue;
                    }
                    let proj_name = entry.file_name().to_string_lossy().to_string();
                    if known_projects.contains(&proj_name) {
                        continue;
                    }
                    println!("{}  (not in state)", proj_name);
                    has_output = true;
                    if let Ok(ws_entries) = std::fs::read_dir(entry.path()) {
                        for ws_entry in ws_entries.flatten() {
                            if ws_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                let name = ws_entry.file_name().to_string_lossy().to_string();
                                println!("  {:<20} orphan", name);
                            }
                        }
                    }
                }
            }

            if !has_output {
                println!("No projects found.");
            }
        }
        WorkspacesAction::Remove { project, name } => {
            let found_in_state =
                mutate_workspace(&mut state, &project, &name, |proj, ws_name| {
                    proj.remove(ws_name);
                });
            if found_in_state {
                state.save(&state_path)?;
            }

            let ws_path = workspaces_dir.join(&project).join(&name);
            let removed_dir =
                ws_path.is_dir() && std::fs::remove_dir_all(&ws_path).is_ok();

            if found_in_state && removed_dir {
                println!("Removed workspace '{}/{}' from state and disk.", project, name);
            } else if found_in_state {
                println!("Removed workspace '{}/{}' from state.", project, name);
            } else if removed_dir {
                println!("Removed orphan workspace '{}/{}' from disk.", project, name);
            } else {
                eprintln!("Workspace '{}/{}' not found.", project, name);
                std::process::exit(1);
            }
        }
        WorkspacesAction::Archive { project, name } => {
            let found = mutate_workspace(&mut state, &project, &name, |proj, ws_name| {
                proj.archive(ws_name);
            });
            if found {
                state.save(&state_path)?;
                println!("Archived workspace '{}/{}'.", project, name);
            } else {
                eprintln!("Workspace '{}/{}' not found.", project, name);
                std::process::exit(1);
            }
        }
        WorkspacesAction::Unarchive { project, name } => {
            let found = mutate_workspace(&mut state, &project, &name, |proj, ws_name| {
                proj.unarchive(ws_name);
            });
            if found {
                state.save(&state_path)?;
                println!("Unarchived workspace '{}/{}'.", project, name);
            } else {
                eprintln!("Workspace '{}/{}' not found.", project, name);
                std::process::exit(1);
            }
        }
        WorkspacesAction::Prune => {
            let orphans = collect_orphans(&state, &workspaces_dir);

            if orphans.is_empty() {
                println!("No orphan workspaces found.");
                return Ok(());
            }

            println!("Found {} orphan workspace(s):", orphans.len());
            for path in &orphans {
                println!("  {}", path.display());
            }

            print!("\nDelete all? [y/N] ");
            io::stdout().flush()?;

            let mut answer = String::new();
            io::stdin().lock().read_line(&mut answer)?;

            if !matches!(answer.trim(), "y" | "Y" | "yes" | "Yes") {
                println!("Aborted.");
                return Ok(());
            }

            let mut deleted = 0u32;
            for path in &orphans {
                if std::fs::remove_dir_all(path).is_ok() {
                    deleted += 1;
                    println!("  deleted {}", path.display());
                } else {
                    eprintln!("  failed  {}", path.display());
                }
            }
            println!("Pruned {} orphan workspace(s).", deleted);
        }
    }

    Ok(())
}

fn collect_orphans(state: &GlobalState, workspaces_dir: &Path) -> Vec<PathBuf> {
    let mut orphans = Vec::new();

    for project in &state.projects {
        let known_names: HashSet<&str> = project.workspaces.iter().map(|w| w.name.as_str()).collect();
        let project_ws_dir = workspaces_dir.join(&project.name);
        if let Ok(entries) = std::fs::read_dir(&project_ws_dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !known_names.contains(name.as_str()) {
                        orphans.push(entry.path());
                    }
                }
            }
        }
    }

    let known_projects: HashSet<&str> = state.projects.iter().map(|p| p.name.as_str()).collect();
    if let Ok(entries) = std::fs::read_dir(workspaces_dir) {
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let proj_name = entry.file_name().to_string_lossy().to_string();
            if known_projects.contains(proj_name.as_str()) {
                continue;
            }
            if let Ok(ws_entries) = std::fs::read_dir(entry.path()) {
                for ws_entry in ws_entries.flatten() {
                    if ws_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        orphans.push(ws_entry.path());
                    }
                }
            }
            if entry.path().read_dir().map(|mut d| d.next().is_none()).unwrap_or(true) {
                orphans.push(entry.path());
            }
        }
    }

    orphans
}

fn mutate_workspace(
    state: &mut GlobalState,
    project_name: &str,
    ws_name: &str,
    f: impl FnOnce(&mut crate::state::Project, &str),
) -> bool {
    for project in &mut state.projects {
        if project.name == project_name && project.workspaces.iter().any(|w| w.name == ws_name) {
            f(project, ws_name);
            return true;
        }
    }
    false
}

fn run_keybinds() -> anyhow::Result<()> {
    println!("Martins — Keyboard Shortcuts\n");

    let binds: &[(&str, &[(&str, &str)])] = &[
        ("Navigation", &[
            ("j/k  ↑/↓", "Move selection"),
            ("1-9", "Switch tab (Normal mode)"),
            ("F1-F9", "Switch tab (any mode)"),
            ("Ctrl+B", "Switch to sidebar"),
        ]),
        ("Workspace", &[
            ("n", "New workspace"),
            ("d", "Delete workspace"),
            ("a", "Archive workspace"),
        ]),
        ("Tabs", &[
            ("t", "New tab (agent/shell)"),
            ("T", "Close current tab"),
        ]),
        ("Project", &[
            ("+", "Add project"),
        ]),
        ("View", &[
            ("[", "Toggle left sidebar"),
            ("]", "Toggle right sidebar"),
            ("/", "Fuzzy search workspaces"),
            ("p", "Preview file"),
        ]),
        ("Terminal Mode", &[
            ("i", "Enter terminal mode"),
            ("Esc Esc", "Exit terminal mode"),
            ("Ctrl+B", "Exit terminal mode"),
        ]),
        ("App", &[
            ("q / Ctrl+C", "Quit"),
            ("?", "Show help overlay"),
        ]),
    ];

    for (section, keys) in binds {
        println!("{section}");
        for (key, desc) in *keys {
            println!("  {key:<14} {desc}");
        }
        println!();
    }

    Ok(())
}
