use crate::app::git_worktrees::scan_worktrees;
use crate::app::model::{
    PersistedState, ProjectRecord, TerminalRecord, TreeStateRecord, UiState, WorktreeRecord,
    create_id, infer_project_name, next_project_name, next_terminal_name,
};
use crate::app::persistence;
use crate::app::runtime::RuntimeSession;
use crate::ghostty_embed::{
    GhosttyEmbed, host_view_new, host_view_set_frame, host_view_set_hidden, ns_view_ptr,
};
use iced::{
    Point, Size, Subscription, Task, keyboard,
    window::{self},
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub(crate) const SIDEBAR_WIDTH_EXPANDED: f32 = 248.0;
pub(crate) const HEADER_HEIGHT: f32 = 32.0;
const BRANCH_REFRESH_INTERVAL: Duration = Duration::from_millis(350);

#[derive(Debug, Clone)]
pub(crate) enum RenameTarget {
    Project {
        project_id: String,
    },
    Terminal {
        project_id: String,
        worktree_id: String,
        terminal_id: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct RenameDialog {
    pub(crate) target: RenameTarget,
    pub(crate) value: String,
}

#[derive(Debug, Clone)]
pub(crate) struct QuickOpenEntry {
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    pub(crate) worktree_name: String,
    pub(crate) terminal_id: String,
    pub(crate) terminal_name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct TerminalLocator {
    pub(crate) project_idx: usize,
    pub(crate) worktree_idx: usize,
    pub(crate) terminal_idx: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveTerminalContext {
    pub(crate) project_name: String,
    pub(crate) worktree_name: String,
    pub(crate) terminal_name: String,
    pub(crate) terminal_id: String,
    pub(crate) worktree_path: String,
}

pub(crate) struct App {
    pub(crate) title: String,
    pub(crate) window_id: Option<window::Id>,
    pub(crate) window_size: Size,
    pub(crate) window_scale_factor: f32,
    pub(crate) cursor_position_logical: Option<Point>,
    pub(crate) keyboard_modifiers: keyboard::Modifiers,
    pub(crate) host_ns_view: Option<usize>,
    pub(crate) runtimes: HashMap<String, RuntimeSession>,
    pub(crate) persisted: PersistedState,
    pub(crate) status: String,
    pub(crate) filter_query: String,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) preferences_open: bool,
    pub(crate) quick_open_open: bool,
    pub(crate) quick_open_query: String,
    pub(crate) rename_dialog: Option<RenameDialog>,
    pub(crate) suppress_next_key_release: bool,
    pub(crate) branch_by_terminal: HashMap<String, String>,
    pub(crate) last_branch_refresh_terminal_id: Option<String>,
    pub(crate) last_branch_refresh_at: Option<Instant>,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    WindowLocated(Option<window::Id>),
    HostViewResolved(Option<usize>),
    WindowSizeResolved(Size),
    WindowScaleResolved(f32),
    WindowEvent(window::Id, window::Event),
    StateLoaded(Result<PersistedState, String>),
    StateSaved(Result<(), String>),
    GhosttyTick,
    Keyboard(iced::keyboard::Event),
    Mouse(iced::mouse::Event),
    ToggleSidebar,
    FilterChanged(String),
    AddProject,
    ProjectRescan(String),
    SelectProject(String),
    ToggleProjectCollapsed(String),
    ToggleWorktreeCollapsed {
        project_id: String,
        worktree_id: String,
    },
    AddTerminal {
        project_id: String,
        worktree_id: String,
    },
    SelectTerminal {
        project_id: String,
        terminal_id: String,
    },
    RemoveTerminal {
        project_id: String,
        worktree_id: String,
        terminal_id: String,
    },
    OpenPreferences(bool),
    OpenQuickOpen(bool),
    QuickOpenQueryChanged(String),
    QuickOpenSubmit,
    QuickOpenSelect(String),
    StartRenameFocused,
    StartRenameTerminal,
    RenameValueChanged(String),
    RenameCommit,
    RenameCancel,
    SwitchTerminalByOffset(i32),
    ActiveBranchResolved {
        terminal_id: String,
        branch: Option<String>,
    },
}

impl App {
    pub(crate) fn boot() -> (Self, Task<Message>) {
        let app = Self {
            title: String::from("Iced + Ghostty Projects"),
            window_id: None,
            window_size: Size::new(1280.0, 820.0),
            window_scale_factor: 1.0,
            cursor_position_logical: None,
            keyboard_modifiers: keyboard::Modifiers::default(),
            host_ns_view: None,
            runtimes: HashMap::new(),
            persisted: PersistedState::default(),
            status: String::from("Ready"),
            filter_query: String::new(),
            sidebar_collapsed: false,
            preferences_open: false,
            quick_open_open: false,
            quick_open_query: String::new(),
            rename_dialog: None,
            suppress_next_key_release: false,
            branch_by_terminal: HashMap::new(),
            last_branch_refresh_terminal_id: None,
            last_branch_refresh_at: None,
        };

        (
            app,
            Task::batch([
                window::latest().map(Message::WindowLocated),
                Task::perform(async { persistence::load_state() }, Message::StateLoaded),
            ]),
        )
    }

    pub(crate) fn title(&self) -> String {
        self.title.clone()
    }

    pub(crate) fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            window::events().map(|(id, event)| Message::WindowEvent(id, event)),
            iced::event::listen_with(|event, _status, _window| match event {
                iced::Event::Keyboard(event) => Some(Message::Keyboard(event)),
                iced::Event::Mouse(event) => Some(Message::Mouse(event)),
                _ => None,
            }),
        ];

        if !self.runtimes.is_empty() {
            subscriptions.push(window::frames().map(|_| Message::GhosttyTick));
        }

        Subscription::batch(subscriptions)
    }

    pub(crate) fn app_ns_view(window_id: window::Id) -> Task<Message> {
        Task::batch([
            window::run(window_id, ns_view_ptr).map(Message::HostViewResolved),
            window::size(window_id).map(Message::WindowSizeResolved),
            window::scale_factor(window_id).map(Message::WindowScaleResolved),
        ])
    }

    pub(crate) fn apply_loaded_state(&mut self, loaded: PersistedState) {
        self.persisted = loaded;
        self.sidebar_collapsed = self.persisted.ui.sidebar_collapsed;
        self.filter_query.clear();
        self.normalize_selection();
    }

    pub(crate) fn save_task(&self) -> Task<Message> {
        let mut snapshot = self.persisted.clone();
        snapshot.ui = UiState {
            sidebar_collapsed: self.sidebar_collapsed,
        };

        Task::perform(
            async move { persistence::save_state(&snapshot) },
            Message::StateSaved,
        )
    }

    pub(crate) fn window_size_px(&self) -> (u32, u32) {
        let scale = self.window_scale_factor.max(0.1);
        let width_px = (self.window_size.width * scale).max(1.0).round() as u32;
        let height_px = (self.window_size.height * scale).max(1.0).round() as u32;
        (width_px, height_px)
    }

    pub(crate) fn sidebar_width_logical(&self) -> f32 {
        if self.sidebar_collapsed {
            0.0
        } else {
            SIDEBAR_WIDTH_EXPANDED
                .max(0.0)
                .min(self.window_size.width.max(1.0) - 1.0)
        }
    }

    pub(crate) fn terminal_frame_logical(&self) -> (f32, f32, f32, f32) {
        let sidebar_width = self.sidebar_width_logical();
        let header_height = self.header_height_logical();
        let terminal_width = (self.window_size.width - sidebar_width).max(1.0);
        let terminal_height = (self.window_size.height - header_height).max(1.0);
        (
            sidebar_width,
            header_height,
            terminal_width,
            terminal_height,
        )
    }

    pub(crate) fn header_height_logical(&self) -> f32 {
        HEADER_HEIGHT
            .max(0.0)
            .min(self.window_size.height.max(1.0) - 1.0)
    }

    pub(crate) fn terminal_frame_px(&self) -> (u32, u32, u32, u32) {
        let (window_width_px, window_height_px) = self.window_size_px();
        let scale = self.window_scale_factor.max(0.1);
        let mut sidebar_width_px = (self.sidebar_width_logical() * scale).max(0.0).round() as u32;
        sidebar_width_px = sidebar_width_px.min(window_width_px.saturating_sub(1));
        let mut header_height_px = (self.header_height_logical() * scale).max(0.0).round() as u32;
        header_height_px = header_height_px.min(window_height_px.saturating_sub(1));

        let terminal_width_px = window_width_px.saturating_sub(sidebar_width_px).max(1);
        let terminal_height_px = window_height_px.saturating_sub(header_height_px).max(1);
        (
            sidebar_width_px,
            header_height_px,
            terminal_width_px,
            terminal_height_px,
        )
    }

    pub(crate) fn terminal_local_from_position(&self, position: Point) -> Option<(f64, f64)> {
        let (x, y, width, height) = self.terminal_frame_logical();
        let within_x = position.x >= x && position.x < x + width;
        let within_y = position.y >= y && position.y < y + height;

        if !(within_x && within_y) {
            return None;
        }

        Some(((position.x - x) as f64, (position.y - y) as f64))
    }

    pub(crate) fn normalize_selection(&mut self) {
        if self.persisted.projects.is_empty() {
            self.persisted.active_project_id = None;
            return;
        }

        if self.persisted.active_project_id.as_ref().is_none_or(|id| {
            !self
                .persisted
                .projects
                .iter()
                .any(|project| &project.id == id)
        }) {
            self.persisted.active_project_id = self
                .persisted
                .projects
                .first()
                .map(|project| project.id.clone());
        }

        for project in &mut self.persisted.projects {
            let has_selected = project
                .selected_terminal_id
                .as_ref()
                .is_some_and(|id| project_terminal_exists(project, id));

            if has_selected {
                continue;
            }

            project.selected_terminal_id = project.worktrees.iter().find_map(|worktree| {
                worktree
                    .terminals
                    .first()
                    .map(|terminal| terminal.id.clone())
            });
        }
    }

    pub(crate) fn active_project_index(&self) -> Option<usize> {
        self.persisted
            .active_project_id
            .as_ref()
            .and_then(|project_id| {
                self.persisted
                    .projects
                    .iter()
                    .position(|project| &project.id == project_id)
            })
    }

    pub(crate) fn active_project(&self) -> Option<&ProjectRecord> {
        self.active_project_index()
            .and_then(|index| self.persisted.projects.get(index))
    }

    pub(crate) fn active_terminal_id(&self) -> Option<String> {
        let project = self.active_project()?;
        if let Some(selected) = &project.selected_terminal_id
            && project_terminal_exists(project, selected)
        {
            return Some(selected.clone());
        }

        project.worktrees.iter().find_map(|worktree| {
            worktree
                .terminals
                .first()
                .map(|terminal| terminal.id.clone())
        })
    }

    pub(crate) fn active_terminal_context(&self) -> Option<ActiveTerminalContext> {
        let active_terminal_id = self.active_terminal_id()?;
        let locator = self.find_terminal_locator(&active_terminal_id)?;

        let project = self.persisted.projects.get(locator.project_idx)?;
        let worktree = project.worktrees.get(locator.worktree_idx)?;
        let terminal = worktree.terminals.get(locator.terminal_idx)?;

        Some(ActiveTerminalContext {
            project_name: project.name.clone(),
            worktree_name: worktree.name.clone(),
            terminal_name: terminal.name.clone(),
            terminal_id: terminal.id.clone(),
            worktree_path: worktree.path.clone(),
        })
    }

    pub(crate) fn active_branch(&self) -> Option<String> {
        let terminal_id = self.active_terminal_id()?;
        self.branch_by_terminal.get(&terminal_id).cloned()
    }

    pub(crate) fn refresh_active_branch_task(&mut self) -> Task<Message> {
        let Some(context) = self.active_terminal_context() else {
            return Task::none();
        };

        let now = Instant::now();
        let too_soon = self
            .last_branch_refresh_terminal_id
            .as_ref()
            .is_some_and(|terminal_id| terminal_id == &context.terminal_id)
            && self
                .last_branch_refresh_at
                .is_some_and(|last| now.duration_since(last) < BRANCH_REFRESH_INTERVAL);

        if too_soon {
            return Task::none();
        }

        let terminal_id = context.terminal_id.clone();
        let worktree_path = context.worktree_path.clone();

        self.last_branch_refresh_terminal_id = Some(terminal_id.clone());
        self.last_branch_refresh_at = Some(now);

        Task::perform(
            async move { crate::app::git_branch::resolve_branch(&worktree_path) },
            move |branch| Message::ActiveBranchResolved {
                terminal_id,
                branch,
            },
        )
    }

    pub(crate) fn find_terminal_locator(&self, terminal_id: &str) -> Option<TerminalLocator> {
        for (project_idx, project) in self.persisted.projects.iter().enumerate() {
            for (worktree_idx, worktree) in project.worktrees.iter().enumerate() {
                for (terminal_idx, terminal) in worktree.terminals.iter().enumerate() {
                    if terminal.id == terminal_id {
                        return Some(TerminalLocator {
                            project_idx,
                            worktree_idx,
                            terminal_idx,
                        });
                    }
                }
            }
        }

        None
    }

    pub(crate) fn global_terminal_sequence(&self) -> Vec<String> {
        let mut result = Vec::new();
        for project in &self.persisted.projects {
            for worktree in &project.worktrees {
                for terminal in &worktree.terminals {
                    result.push(terminal.id.clone());
                }
            }
        }
        result
    }

    pub(crate) fn quick_open_entries(&self) -> Vec<QuickOpenEntry> {
        let query = self.quick_open_query.trim().to_lowercase();
        let mut entries = Vec::new();

        for project in &self.persisted.projects {
            for worktree in &project.worktrees {
                for terminal in &worktree.terminals {
                    let text = format!(
                        "{} {} {}",
                        project.name.to_lowercase(),
                        worktree.name.to_lowercase(),
                        terminal.name.to_lowercase()
                    );
                    if !query.is_empty() && !text.contains(&query) {
                        continue;
                    }

                    entries.push(QuickOpenEntry {
                        project_id: project.id.clone(),
                        project_name: project.name.clone(),
                        worktree_name: worktree.name.clone(),
                        terminal_id: terminal.id.clone(),
                        terminal_name: terminal.name.clone(),
                    });
                }
            }
        }

        entries
    }

    pub(crate) fn filtered_project_indices(&self) -> Vec<usize> {
        let query = self.filter_query.trim().to_lowercase();
        if query.is_empty() {
            return (0..self.persisted.projects.len()).collect();
        }

        self.persisted
            .projects
            .iter()
            .enumerate()
            .filter_map(|(index, project)| {
                let project_match = project.name.to_lowercase().contains(&query);
                if project_match {
                    return Some(index);
                }

                let found = project.worktrees.iter().any(|worktree| {
                    if format!(
                        "{} {}",
                        worktree.name.to_lowercase(),
                        worktree.path.to_lowercase()
                    )
                    .contains(&query)
                    {
                        return true;
                    }

                    worktree
                        .terminals
                        .iter()
                        .any(|terminal| terminal.name.to_lowercase().contains(&query))
                });

                if found { Some(index) } else { None }
            })
            .collect()
    }

    pub(crate) fn project_collapsed(project: &ProjectRecord) -> bool {
        project.tree_state.collapsed_projects.contains(&project.id)
    }

    pub(crate) fn worktree_collapsed(project: &ProjectRecord, worktree_id: &str) -> bool {
        project
            .tree_state
            .collapsed_worktrees
            .iter()
            .any(|id| id == worktree_id)
    }

    pub(crate) fn ensure_runtime_for_terminal(&mut self, terminal_id: &str) -> Result<(), String> {
        if self.runtimes.contains_key(terminal_id) {
            return Ok(());
        }

        let Some(parent_ns_view) = self.host_ns_view else {
            return Ok(());
        };

        let (_, _, width_px, height_px) = self.terminal_frame_px();
        let scale = self.window_scale_factor.max(0.1) as f64;

        let Some(host_view) = host_view_new(parent_ns_view) else {
            return Err(String::from("failed to create terminal host view"));
        };

        let ghostty = match GhosttyEmbed::new(host_view, width_px, height_px, scale) {
            Ok(value) => value,
            Err(error) => {
                return Err(format!("failed to initialize terminal runtime: {error}"));
            }
        };

        self.runtimes.insert(
            terminal_id.to_string(),
            RuntimeSession::new(host_view, ghostty),
        );

        Ok(())
    }

    pub(crate) fn ensure_active_runtime(&mut self) {
        if let Some(active_terminal_id) = self.active_terminal_id()
            && let Err(error) = self.ensure_runtime_for_terminal(&active_terminal_id)
        {
            self.status = error;
        }
    }

    pub(crate) fn active_ghostty_mut(&mut self) -> Option<&mut GhosttyEmbed> {
        let active_terminal_id = self.active_terminal_id()?;
        self.runtimes
            .get_mut(&active_terminal_id)
            .map(|runtime| &mut runtime.ghostty)
    }

    pub(crate) fn sync_runtime_views(&mut self) {
        let (x_logical, y_logical, width_logical, height_logical) = self.terminal_frame_logical();
        let (_, _, width_px, height_px) = self.terminal_frame_px();
        let scale = self.window_scale_factor.max(0.1) as f64;
        let active_terminal_id = self.active_terminal_id();
        let modal_open = self.modal_open();

        for (terminal_id, runtime) in &mut self.runtimes {
            let active = !modal_open
                && active_terminal_id
                    .as_ref()
                    .is_some_and(|id| id == terminal_id);

            host_view_set_frame(
                runtime.host_view,
                x_logical as f64,
                y_logical as f64,
                width_logical as f64,
                height_logical as f64,
            );
            host_view_set_hidden(runtime.host_view, !active);

            runtime.ghostty.set_scale_factor(scale);
            runtime.ghostty.set_size(width_px, height_px);
            runtime.ghostty.set_focus(active);
            if active {
                runtime.ghostty.refresh();
            }
        }
    }

    pub(crate) fn remove_runtime(&mut self, terminal_id: &str) {
        self.runtimes.remove(terminal_id);
        self.branch_by_terminal.remove(terminal_id);
    }

    pub(crate) fn modal_open(&self) -> bool {
        self.quick_open_open || self.preferences_open || self.rename_dialog.is_some()
    }

    pub(crate) fn select_project(&mut self, project_id: &str) {
        if !self
            .persisted
            .projects
            .iter()
            .any(|project| project.id == project_id)
        {
            return;
        }

        self.persisted.active_project_id = Some(project_id.to_string());
        self.normalize_selection();
    }

    pub(crate) fn select_terminal(&mut self, project_id: &str, terminal_id: &str) {
        self.persisted.active_project_id = Some(project_id.to_string());

        if let Some(project) = self
            .persisted
            .projects
            .iter_mut()
            .find(|project| project.id == project_id)
            && project_terminal_exists(project, terminal_id)
        {
            project.selected_terminal_id = Some(terminal_id.to_string());
        }

        self.normalize_selection();
    }

    pub(crate) fn switch_terminal_by_offset(&mut self, offset: i32) {
        let sequence = self.global_terminal_sequence();
        if sequence.is_empty() {
            return;
        }

        let active_terminal_id = self.active_terminal_id();
        let current_index = active_terminal_id
            .as_ref()
            .and_then(|id| sequence.iter().position(|value| value == id))
            .unwrap_or(0);

        let len = sequence.len() as i32;
        let mut next_index = current_index as i32 + offset;
        while next_index < 0 {
            next_index += len;
        }
        let next_index = (next_index % len) as usize;

        if let Some(next_terminal_id) = sequence.get(next_index)
            && let Some(locator) = self.find_terminal_locator(next_terminal_id)
        {
            let project_id = self.persisted.projects[locator.project_idx].id.clone();
            self.select_terminal(&project_id, next_terminal_id);
        }
    }

    pub(crate) fn add_project_from_git_folder(&mut self, git_folder: &str) -> Result<(), String> {
        let scanned = scan_worktrees(git_folder)?;

        let project_id = create_id("project");
        let mut project = ProjectRecord {
            id: project_id.clone(),
            name: infer_project_name(git_folder),
            git_folder_path: Some(git_folder.to_string()),
            worktrees: scanned
                .into_iter()
                .map(|worktree| WorktreeRecord {
                    id: worktree.id,
                    name: worktree.name,
                    path: worktree.path,
                    missing: worktree.missing,
                    terminals: Vec::new(),
                })
                .collect(),
            tree_state: TreeStateRecord::default(),
            selected_terminal_id: None,
        };

        if project.name.trim().is_empty() {
            project.name = next_project_name(&self.persisted.projects);
        }

        self.persisted.projects.push(project);
        self.persisted.active_project_id = Some(project_id);
        self.normalize_selection();
        Ok(())
    }

    pub(crate) fn rescan_project(&mut self, project_id: &str) -> Result<(), String> {
        let Some(project_idx) = self
            .persisted
            .projects
            .iter()
            .position(|project| project.id == project_id)
        else {
            return Ok(());
        };

        let git_folder = self.persisted.projects[project_idx]
            .git_folder_path
            .clone()
            .ok_or_else(|| String::from("Project has no git folder configured"))?;

        let scanned = scan_worktrees(&git_folder)?;

        let project = &mut self.persisted.projects[project_idx];

        let mut existing_terminals = HashMap::<String, Vec<TerminalRecord>>::new();
        for worktree in &project.worktrees {
            existing_terminals.insert(worktree.path.clone(), worktree.terminals.clone());
        }

        let active_before = project.selected_terminal_id.clone();
        let mut next_worktrees = Vec::new();
        let mut seen = HashSet::new();

        for info in scanned {
            seen.insert(info.path.clone());
            let terminals = existing_terminals.remove(&info.path).unwrap_or_default();
            next_worktrees.push(WorktreeRecord {
                id: info.id,
                name: info.name,
                path: info.path,
                missing: info.missing,
                terminals,
            });
        }

        let mut removed_terminal_ids = Vec::new();
        for worktree in &project.worktrees {
            if seen.contains(&worktree.path) {
                continue;
            }
            removed_terminal_ids.extend(
                worktree
                    .terminals
                    .iter()
                    .map(|terminal| terminal.id.clone()),
            );
        }

        project.worktrees = next_worktrees;

        if let Some(selected) = active_before
            && project_terminal_exists(project, &selected)
        {
            project.selected_terminal_id = Some(selected);
        } else {
            project.selected_terminal_id = project.worktrees.iter().find_map(|worktree| {
                worktree
                    .terminals
                    .first()
                    .map(|terminal| terminal.id.clone())
            });
        }

        for terminal_id in removed_terminal_ids {
            self.remove_runtime(&terminal_id);
        }

        self.normalize_selection();
        Ok(())
    }

    pub(crate) fn toggle_project_collapsed(&mut self, project_id: &str) {
        if let Some(project) = self
            .persisted
            .projects
            .iter_mut()
            .find(|project| project.id == project_id)
        {
            toggle_in_list(&mut project.tree_state.collapsed_projects, project_id);
        }
    }

    pub(crate) fn toggle_worktree_collapsed(&mut self, project_id: &str, worktree_id: &str) {
        if let Some(project) = self
            .persisted
            .projects
            .iter_mut()
            .find(|project| project.id == project_id)
        {
            toggle_in_list(&mut project.tree_state.collapsed_worktrees, worktree_id);
        }
    }

    pub(crate) fn add_terminal(&mut self, project_id: &str, worktree_id: &str) -> Option<String> {
        let mut created = None;

        for project in &mut self.persisted.projects {
            if project.id != project_id {
                continue;
            }

            for worktree in &mut project.worktrees {
                if worktree.id != worktree_id {
                    continue;
                }

                let terminal_id = create_id("terminal");
                let terminal_name = next_terminal_name(&worktree.terminals);
                worktree.terminals.push(TerminalRecord {
                    id: terminal_id.clone(),
                    name: terminal_name,
                    manual_name: false,
                });
                project.selected_terminal_id = Some(terminal_id.clone());
                created = Some(terminal_id);
                break;
            }
        }

        self.normalize_selection();
        created
    }

    pub(crate) fn remove_terminal(
        &mut self,
        project_id: &str,
        worktree_id: &str,
        terminal_id: &str,
    ) {
        for project in &mut self.persisted.projects {
            if project.id != project_id {
                continue;
            }

            for worktree in &mut project.worktrees {
                if worktree.id != worktree_id {
                    continue;
                }

                worktree
                    .terminals
                    .retain(|terminal| terminal.id != terminal_id);
            }

            if project
                .selected_terminal_id
                .as_ref()
                .is_some_and(|selected| selected == terminal_id)
            {
                project.selected_terminal_id = project.worktrees.iter().find_map(|worktree| {
                    worktree
                        .terminals
                        .first()
                        .map(|terminal| terminal.id.clone())
                });
            }
        }

        self.remove_runtime(terminal_id);
        self.normalize_selection();
    }

    pub(crate) fn start_rename_focused(&mut self) {
        if let Some(active_terminal_id) = self.active_terminal_id()
            && let Some(locator) = self.find_terminal_locator(&active_terminal_id)
        {
            let project_id = self.persisted.projects[locator.project_idx].id.clone();
            let worktree_id = self.persisted.projects[locator.project_idx].worktrees
                [locator.worktree_idx]
                .id
                .clone();
            let terminal_name = self.persisted.projects[locator.project_idx].worktrees
                [locator.worktree_idx]
                .terminals[locator.terminal_idx]
                .name
                .clone();

            self.rename_dialog = Some(RenameDialog {
                target: RenameTarget::Terminal {
                    project_id,
                    worktree_id,
                    terminal_id: active_terminal_id,
                },
                value: terminal_name,
            });
            return;
        }

        if let Some(project_idx) = self.active_project_index() {
            let project_id = self.persisted.projects[project_idx].id.clone();
            let project_name = self.persisted.projects[project_idx].name.clone();
            self.rename_dialog = Some(RenameDialog {
                target: RenameTarget::Project { project_id },
                value: project_name,
            });
        }
    }

    pub(crate) fn start_rename_active_terminal(&mut self) {
        if let Some(active_terminal_id) = self.active_terminal_id()
            && let Some(locator) = self.find_terminal_locator(&active_terminal_id)
        {
            let project_id = self.persisted.projects[locator.project_idx].id.clone();
            let worktree_id = self.persisted.projects[locator.project_idx].worktrees
                [locator.worktree_idx]
                .id
                .clone();
            let terminal_name = self.persisted.projects[locator.project_idx].worktrees
                [locator.worktree_idx]
                .terminals[locator.terminal_idx]
                .name
                .clone();

            self.rename_dialog = Some(RenameDialog {
                target: RenameTarget::Terminal {
                    project_id,
                    worktree_id,
                    terminal_id: active_terminal_id,
                },
                value: terminal_name,
            });
        }
    }

    pub(crate) fn commit_rename(&mut self) -> bool {
        let Some(dialog) = self.rename_dialog.clone() else {
            return false;
        };

        let value = dialog.value.trim();
        if value.is_empty() {
            return false;
        }

        match dialog.target {
            RenameTarget::Project { project_id } => {
                if let Some(project) = self
                    .persisted
                    .projects
                    .iter_mut()
                    .find(|project| project.id == project_id)
                {
                    project.name = value.to_string();
                }
            }
            RenameTarget::Terminal {
                project_id,
                worktree_id,
                terminal_id,
            } => {
                if let Some(project) = self
                    .persisted
                    .projects
                    .iter_mut()
                    .find(|project| project.id == project_id)
                    && let Some(worktree) = project
                        .worktrees
                        .iter_mut()
                        .find(|worktree| worktree.id == worktree_id)
                    && let Some(terminal) = worktree
                        .terminals
                        .iter_mut()
                        .find(|terminal| terminal.id == terminal_id)
                {
                    terminal.name = value.to_string();
                    terminal.manual_name = true;
                }
            }
        }

        self.rename_dialog = None;
        true
    }

    pub(crate) fn select_terminal_by_id(&mut self, terminal_id: &str) {
        let Some(locator) = self.find_terminal_locator(terminal_id) else {
            return;
        };

        let project_id = self.persisted.projects[locator.project_idx].id.clone();
        self.select_terminal(&project_id, terminal_id);
    }
}

fn project_terminal_exists(project: &ProjectRecord, terminal_id: &str) -> bool {
    project.worktrees.iter().any(|worktree| {
        worktree
            .terminals
            .iter()
            .any(|terminal| terminal.id == terminal_id)
    })
}

fn toggle_in_list(values: &mut Vec<String>, target: &str) {
    if let Some(index) = values.iter().position(|value| value == target) {
        values.remove(index);
    } else {
        values.push(target.to_string());
    }
}
