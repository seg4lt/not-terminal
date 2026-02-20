use crate::app::git_worktrees::{add_worktree, remove_worktree, scan_worktrees};
use crate::app::model::{
    BrowserRecord, PersistedState, ProjectRecord, TerminalRecord, TreeStateRecord, UiState, WorktreeRecord,
    create_id, infer_project_name, next_browser_name, next_project_name, next_terminal_name,
};
use crate::app::persistence;
use crate::app::runtime::{PaneRuntime, RuntimeSession};
use crate::ghostty_embed::{
    GhosttyEmbed, GhosttyRuntimeAction, host_view_free, host_view_new,
    ns_view_ptr,
};
use crate::webview::WebView;
use iced::{
    Point, Size, Subscription, Task, keyboard,
    window::{self},
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Represents the current status of a terminal
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalStatus {
    /// Process is still running
    Running,
    /// Process exited successfully (exit code 0)
    Success,
    /// Process exited with an error (non-zero exit code)
    Error(i16),
    /// AI or tool is waiting for user input (bell was rung)
    AwaitingResponse,
}

pub(crate) const SIDEBAR_WIDTH_MIN: f32 = 150.0;
pub(crate) const SIDEBAR_WIDTH_MAX: f32 = 500.0;
pub(crate) const SIDEBAR_WIDTH_DEFAULT: f32 = 248.0;
const BRANCH_REFRESH_INTERVAL: Duration = Duration::from_millis(350);

/// Represents the different states of the sidebar
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SidebarState {
    /// Sidebar is completely hidden
    Hidden,
    /// Sidebar shows full details (expanded mode)
    Expanded,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self::Expanded
    }
}

impl SidebarState {
    pub(crate) fn is_hidden(&self) -> bool {
        matches!(self, Self::Hidden)
    }

    #[allow(dead_code)]
    pub(crate) fn is_expanded(&self) -> bool {
        matches!(self, Self::Expanded)
    }

    pub(crate) fn toggle(&self) -> Self {
        match self {
            Self::Hidden => Self::Expanded,
            Self::Expanded => Self::Hidden,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum RenameTarget {
    Project {
        project_id: String,
    },
    Worktree {
        project_id: String,
        worktree_id: String,
    },
    Terminal {
        project_id: String,
        worktree_id: String,
        terminal_id: String,
    },
    DetachedTerminal {
        terminal_id: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct RenameDialog {
    pub(crate) target: RenameTarget,
    pub(crate) value: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AddWorktreeDialog {
    pub(crate) project_id: String,
    pub(crate) branch_name: String,
    pub(crate) destination_path: String,
}

#[derive(Debug, Clone)]
pub(crate) struct QuickOpenEntry {
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
    pub(crate) worktree_path: Option<String>,
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
    pub(crate) sidebar_state: SidebarState,
    pub(crate) sidebar_width: f32,
    pub(crate) sidebar_resizing: bool,
    pub(crate) show_native_title_bar: bool,
    pub(crate) preferences_open: bool,
    pub(crate) quick_open_open: bool,
    pub(crate) quick_open_query: String,
    pub(crate) quick_open_selected_index: usize,
    pub(crate) rename_dialog: Option<RenameDialog>,
    pub(crate) add_worktree_dialog: Option<AddWorktreeDialog>,
    pub(crate) suppress_next_key_release: bool,
    pub(crate) branch_by_terminal: HashMap<String, String>,
    pub(crate) last_branch_refresh_terminal_id: Option<String>,
    pub(crate) last_branch_refresh_at: Option<Instant>,
    pub(crate) last_ghostty_activity: Instant,
    pub(crate) frame_counter: u64,
    /// Tracks exit status of terminals by ID
    pub(crate) terminal_status: HashMap<String, TerminalStatus>,
    /// Terminals that have rung the bell (awaiting user input)
    pub(crate) terminal_awaiting_response: HashSet<String>,
    /// Stores title per terminal_id for bell detection via title emoji
    pub(crate) terminal_titles: HashMap<String, String>,
    /// Browser webviews by ID
    pub(crate) browser_webviews: HashMap<String, WebView>,
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
    SetShowNativeTitleBar(bool),
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
    AddDetachedTerminal,
    CloseActiveTerminal,
    SelectTerminal {
        project_id: String,
        terminal_id: String,
    },
    SelectDetachedTerminal(String),
    RemoveTerminal {
        project_id: String,
        worktree_id: String,
        terminal_id: String,
    },
    RemoveDetachedTerminal(String),
    OpenPreferences(bool),
    OpenQuickOpen(bool),
    QuickOpenQueryChanged(String),
    QuickOpenSubmit,
    QuickOpenSelect(String),
    StartRenameProject(String),
    StartRenameWorktree {
        project_id: String,
        worktree_id: String,
    },
    StartRenameFocused,
    StartRenameTerminal,
    RenameValueChanged(String),
    RenameCommit,
    RenameCancel,
    StartAddWorktree(String),
    AddWorktreeBranchChanged(String),
    AddWorktreePathChanged(String),
    FocusAddWorktreePath,
    AddWorktreeCommit,
    AddWorktreeCancel,
    RemoveWorktree {
        project_id: String,
        worktree_id: String,
    },
    SwitchTerminalByOffset(i32),
    ActiveBranchResolved {
        terminal_id: String,
        branch: Option<String>,
    },
    SidebarResizeHandlePressed,
    SidebarResizeHandleReleased,
    AddBrowser,
    RemoveBrowser(String),
    SelectBrowser(String),
    BrowserUrlChanged(String),
    BrowserNavigate,
    BrowserBack,
    BrowserForward,
    BrowserReload,
    BrowserDevTools,
}

impl App {
    pub(crate) fn boot() -> (Self, Task<Message>) {
        let app = Self {
            title: String::from("Not Terminal"),
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
            sidebar_state: SidebarState::Expanded,
            sidebar_width: SIDEBAR_WIDTH_DEFAULT,
            sidebar_resizing: false,
            show_native_title_bar: crate::app::initial_show_native_title_bar(),
            preferences_open: false,
            quick_open_open: false,
            quick_open_query: String::new(),
            quick_open_selected_index: 0,
            rename_dialog: None,
            add_worktree_dialog: None,
            suppress_next_key_release: false,
            branch_by_terminal: HashMap::new(),
            last_branch_refresh_terminal_id: None,
            last_branch_refresh_at: None,
            last_ghostty_activity: Instant::now(),
            frame_counter: 0,
            terminal_status: HashMap::new(),
            terminal_awaiting_response: HashSet::new(),
            terminal_titles: HashMap::new(),
            browser_webviews: HashMap::new(),
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
            // Still use window::frames for the tick, but we'll skip work adaptively
            // based on last_ghostty_activity in the tick handler
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
        // Convert old boolean state to new enum state
        self.sidebar_state = if self.persisted.ui.sidebar_collapsed {
            SidebarState::Hidden
        } else {
            SidebarState::Expanded
        };
        // Load persisted sidebar width, clamped to valid range
        self.sidebar_width = self
            .persisted
            .ui
            .sidebar_width
            .clamp(SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX);
        self.show_native_title_bar = self.persisted.ui.show_native_title_bar;
        self.filter_query.clear();
        self.normalize_selection();
    }

    pub(crate) fn save_task(&self) -> Task<Message> {
        let mut snapshot = self.persisted.clone();
        // Convert new enum state to old boolean for persistence
        snapshot.ui = UiState {
            sidebar_collapsed: self.sidebar_state.is_hidden(),
            show_native_title_bar: self.show_native_title_bar,
            sidebar_width: self.sidebar_width,
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
        if self.sidebar_state.is_hidden() {
            0.0
        } else {
            self.sidebar_width
                .clamp(SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX)
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
        0.0
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

        let local_x = position.x - x;
        let local_y = position.y - y;
        let active_terminal_id = self.active_terminal_id()?;
        if let Some(runtime) = self.runtimes.get(&active_terminal_id) {
            return runtime.active_pane_local(local_x, local_y, width, height);
        }

        Some((local_x as f64, local_y as f64))
    }

    pub(crate) fn focus_terminal_pane_from_position(
        &mut self,
        position: Point,
    ) -> Option<(f64, f64, bool)> {
        let (x, y, width, height) = self.terminal_frame_logical();
        let within_x = position.x >= x && position.x < x + width;
        let within_y = position.y >= y && position.y < y + height;

        if !(within_x && within_y) {
            return None;
        }

        let local_x = position.x - x;
        let local_y = position.y - y;
        let active_terminal_id = self.active_terminal_id()?;
        if let Some(runtime) = self.runtimes.get_mut(&active_terminal_id) {
            return runtime.focus_pane_at(local_x, local_y, width, height);
        }

        Some((local_x as f64, local_y as f64, false))
    }

    pub(crate) fn normalize_selection(&mut self) {
        if self
            .persisted
            .selected_detached_terminal_id
            .as_ref()
            .is_some_and(|selected| {
                !self
                    .persisted
                    .detached_terminals
                    .iter()
                    .any(|terminal| &terminal.id == selected)
            })
        {
            self.persisted.selected_detached_terminal_id = None;
        }

        if self
            .persisted
            .selected_browser_id
            .as_ref()
            .is_some_and(|selected| {
                !self
                    .persisted
                    .browsers
                    .iter()
                    .any(|browser| &browser.id == selected)
            })
        {
            self.persisted.selected_browser_id = None;
        }

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

    pub(crate) fn active_worktree_ids(&self) -> Option<(String, String)> {
        if let Some(terminal_id) = self.active_terminal_id()
            && let Some(locator) = self.find_terminal_locator(&terminal_id)
        {
            let project_id = self.persisted.projects.get(locator.project_idx)?.id.clone();
            let worktree_id = self
                .persisted
                .projects
                .get(locator.project_idx)?
                .worktrees
                .get(locator.worktree_idx)?
                .id
                .clone();
            return Some((project_id, worktree_id));
        }

        let project = self.active_project()?;
        let worktree = project.worktrees.first()?;
        Some((project.id.clone(), worktree.id.clone()))
    }

    pub(crate) fn active_terminal_id(&self) -> Option<String> {
        if let Some(selected_detached) = &self.persisted.selected_detached_terminal_id
            && self
                .persisted
                .detached_terminals
                .iter()
                .any(|terminal| &terminal.id == selected_detached)
        {
            return Some(selected_detached.clone());
        }

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
        if let Some(detached_terminal) = self
            .persisted
            .detached_terminals
            .iter()
            .find(|terminal| terminal.id == active_terminal_id)
        {
            return Some(ActiveTerminalContext {
                project_name: String::from("Detached"),
                worktree_name: String::from("-"),
                terminal_name: detached_terminal.name.clone(),
                terminal_id: detached_terminal.id.clone(),
                worktree_path: None,
            });
        }

        let locator = self.find_terminal_locator(&active_terminal_id)?;

        let project = self.persisted.projects.get(locator.project_idx)?;
        let worktree = project.worktrees.get(locator.worktree_idx)?;
        let terminal = worktree.terminals.get(locator.terminal_idx)?;

        Some(ActiveTerminalContext {
            project_name: project.name.clone(),
            worktree_name: worktree.name.clone(),
            terminal_name: terminal.name.clone(),
            terminal_id: terminal.id.clone(),
            worktree_path: Some(worktree.path.clone()),
        })
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
        let Some(worktree_path) = context.worktree_path.clone() else {
            self.branch_by_terminal.remove(&context.terminal_id);
            return Task::none();
        };

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
        for terminal in &self.persisted.detached_terminals {
            result.push(terminal.id.clone());
        }
        result
    }

    pub(crate) fn quick_open_entries(&self) -> Vec<QuickOpenEntry> {
        let query = self.quick_open_query.trim().to_lowercase();
        let mut entries = Vec::new();

        // Split query by spaces for fuzzy matching
        let search_terms: Vec<&str> = if query.is_empty() {
            Vec::new()
        } else {
            query.split_whitespace().collect()
        };

        for project in &self.persisted.projects {
            for worktree in &project.worktrees {
                for terminal in &worktree.terminals {
                    let text = format!(
                        "{} {} {}",
                        project.name.to_lowercase(),
                        worktree.name.to_lowercase(),
                        terminal.name.to_lowercase()
                    );

                    // Fuzzy match: all search terms must be found in the text
                    if !search_terms.is_empty() && !search_terms.iter().all(|term| text.contains(term)) {
                        continue;
                    }

                    entries.push(QuickOpenEntry {
                        project_name: project.name.clone(),
                        worktree_name: worktree.name.clone(),
                        terminal_id: terminal.id.clone(),
                        terminal_name: terminal.name.clone(),
                    });
                }
            }
        }

        for terminal in &self.persisted.detached_terminals {
            let text = format!("detached global {}", terminal.name.to_lowercase());

            // Fuzzy match: all search terms must be found in the text
            if !search_terms.is_empty() && !search_terms.iter().all(|term| text.contains(term)) {
                continue;
            }

            entries.push(QuickOpenEntry {
                project_name: String::from("Detached"),
                worktree_name: String::from("-"),
                terminal_id: terminal.id.clone(),
                terminal_name: terminal.name.clone(),
            });
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

        let working_directory = self.terminal_working_directory(terminal_id);
        let pane = self.create_pane_runtime(working_directory.as_deref())?;
        self.runtimes
            .insert(terminal_id.to_string(), RuntimeSession::new(pane));

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
            .and_then(|runtime| runtime.active_ghostty_mut())
    }

    pub(crate) fn sync_runtime_views(&mut self) {
        let (x_logical, y_logical, width_logical, height_logical) = self.terminal_frame_logical();
        let scale = self.window_scale_factor.max(0.1) as f64;
        let active_terminal_id = self.active_terminal_id();
        let active_browser_id = self.active_browser_id();
        let modal_open = self.modal_open();

        // Sync browser webviews - only the active one is visible
        let browser_toolbar_height = 32.0;
        for (browser_id, webview) in &mut self.browser_webviews {
            let is_active = active_browser_id.as_ref().is_some_and(|id| id == browser_id);
            if is_active {
                webview.set_frame(
                    x_logical as f64,
                    (y_logical + browser_toolbar_height) as f64,
                    width_logical as f64,
                    (height_logical - browser_toolbar_height) as f64,
                );
                webview.set_hidden(false);
            } else {
                webview.set_hidden(true);
            }
        }

        for (terminal_id, runtime) in &mut self.runtimes {
            let active = !modal_open
                && active_browser_id.is_none()
                && active_terminal_id
                    .as_ref()
                    .is_some_and(|id| id == terminal_id);
            runtime.apply_layout(
                x_logical,
                y_logical,
                width_logical,
                height_logical,
                active,
                scale,
            );
        }
    }

    fn terminal_working_directory(&self, terminal_id: &str) -> Option<String> {
        self.find_terminal_locator(terminal_id).and_then(|locator| {
            self.persisted
                .projects
                .get(locator.project_idx)
                .and_then(|project| project.worktrees.get(locator.worktree_idx))
                .map(|worktree| worktree.path.clone())
        })
    }

    fn create_pane_runtime(&self, working_directory: Option<&str>) -> Result<PaneRuntime, String> {
        let Some(parent_ns_view) = self.host_ns_view else {
            return Err(String::from("failed to resolve host NSView"));
        };

        let (_, _, width_px, height_px) = self.terminal_frame_px();
        let scale = self.window_scale_factor.max(0.1) as f64;
        let Some(host_view) = host_view_new(parent_ns_view) else {
            return Err(String::from("failed to create terminal host view"));
        };

        let ghostty =
            match GhosttyEmbed::new(host_view, width_px, height_px, scale, working_directory) {
                Ok(value) => value,
                Err(error) => {
                    host_view_free(host_view);
                    return Err(format!("failed to initialize terminal runtime: {error}"));
                }
            };

        Ok(PaneRuntime::new(create_id("pane"), host_view, ghostty))
    }

    pub(crate) fn process_runtime_actions(&mut self) -> bool {
        let mut changed = false;
        let terminal_ids: Vec<String> = self.runtimes.keys().cloned().collect();
        let (.., width, height) = self.terminal_frame_logical();

        for terminal_id in terminal_ids {
            let actions = if let Some(runtime) = self.runtimes.get_mut(&terminal_id) {
                runtime.drain_actions()
            } else {
                continue;
            };

            for action in actions {
                let action_changed = match action {
                    GhosttyRuntimeAction::NewSplit {
                        surface_ptr,
                        direction,
                    } => {
                        let working_directory = self.terminal_working_directory(&terminal_id);
                        let pane = match self.create_pane_runtime(working_directory.as_deref()) {
                            Ok(pane) => pane,
                            Err(error) => {
                                self.status = error;
                                continue;
                            }
                        };

                        self.runtimes.get_mut(&terminal_id).is_some_and(|runtime| {
                            runtime.split_from_surface(surface_ptr, direction, pane)
                        })
                    }
                    GhosttyRuntimeAction::GotoSplit {
                        surface_ptr,
                        direction,
                    } => self.runtimes.get_mut(&terminal_id).is_some_and(|runtime| {
                        runtime.goto_split_from_surface(surface_ptr, direction, width, height)
                    }),
                    GhosttyRuntimeAction::ResizeSplit {
                        surface_ptr,
                        direction,
                        amount,
                    } => self.runtimes.get_mut(&terminal_id).is_some_and(|runtime| {
                        runtime.resize_split_from_surface(surface_ptr, direction, amount)
                    }),
                    GhosttyRuntimeAction::EqualizeSplits { .. } => self
                        .runtimes
                        .get_mut(&terminal_id)
                        .is_some_and(|runtime| runtime.equalize_splits()),
                    GhosttyRuntimeAction::ToggleSplitZoom { surface_ptr } => self
                        .runtimes
                        .get_mut(&terminal_id)
                        .is_some_and(|runtime| runtime.toggle_split_zoom_from_surface(surface_ptr)),
                    GhosttyRuntimeAction::NewTab { .. } | GhosttyRuntimeAction::GotoTab { .. } => {
                        self.status =
                            String::from("Ghostty tab actions are not supported in this app yet");
                        false
                    }
                    GhosttyRuntimeAction::CommandFinished { exit_code, .. } => {
                        // Set the terminal status based on exit code
                        // BUT don't override AwaitingResponse state - bell takes precedence
                        let current_status = self.get_terminal_status(&terminal_id);
                        if !matches!(current_status, TerminalStatus::AwaitingResponse) {
                            let status = if exit_code == 0 {
                                TerminalStatus::Success
                            } else {
                                TerminalStatus::Error(exit_code)
                            };
                            self.set_terminal_status(terminal_id.clone(), status);
                        }
                        // Note: We DON'T clear awaiting state here - only keyboard input should clear it
                        true  // Trigger UI update
                    }
                    GhosttyRuntimeAction::RingBell { .. } => {
                        // Mark terminal as awaiting response
                        self.on_terminal_bell(terminal_id.clone());
                        true  // Trigger UI update
                    }
                    GhosttyRuntimeAction::SetTitle { surface_ptr, title } => {
                        // Handle title change - check for bell emoji
                        self.on_terminal_title(surface_ptr, title);
                        true  // Trigger UI update
                    }
                    GhosttyRuntimeAction::DesktopNotification { .. } => {
                        // Desktop notification means the terminal needs attention
                        // Mark as awaiting response
                        self.on_terminal_bell(terminal_id.clone());
                        true  // Trigger UI update
                    }
                };

                changed = changed || action_changed;
            }
        }

        changed
    }

    pub(crate) fn remove_runtime(&mut self, terminal_id: &str) {
        self.runtimes.remove(terminal_id);
        self.branch_by_terminal.remove(terminal_id);
        self.remove_terminal_status(terminal_id);
    }

    pub(crate) fn modal_open(&self) -> bool {
        self.quick_open_open
            || self.preferences_open
            || self.rename_dialog.is_some()
            || self.add_worktree_dialog.is_some()
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
        self.persisted.selected_detached_terminal_id = None;
        self.normalize_selection();
    }

    pub(crate) fn select_terminal(&mut self, project_id: &str, terminal_id: &str) {
        self.persisted.active_project_id = Some(project_id.to_string());
        self.persisted.selected_detached_terminal_id = None;

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

        if let Some(next_terminal_id) = sequence.get(next_index) {
            self.select_terminal_by_id(next_terminal_id);
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
                    manual_name: false,
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

        let mut existing_worktrees =
            HashMap::<String, (Vec<TerminalRecord>, String, bool, String)>::new();
        for worktree in &project.worktrees {
            existing_worktrees.insert(
                worktree.path.clone(),
                (
                    worktree.terminals.clone(),
                    worktree.name.clone(),
                    worktree.manual_name,
                    worktree.id.clone(),
                ),
            );
        }

        let active_before = project.selected_terminal_id.clone();
        let mut next_worktrees = Vec::new();
        let mut seen = HashSet::new();

        for info in scanned {
            seen.insert(info.path.clone());
            let (terminals, name, manual_name, id) = existing_worktrees
                .remove(&info.path)
                .unwrap_or((Vec::new(), info.name.clone(), false, info.id.clone()));
            next_worktrees.push(WorktreeRecord {
                id,
                name: if manual_name { name } else { info.name },
                manual_name,
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

    pub(crate) fn start_add_worktree(&mut self, project_id: &str) {
        let Some(project) = self
            .persisted
            .projects
            .iter()
            .find(|project| project.id == project_id)
        else {
            return;
        };

        let Some(git_folder) = project.git_folder_path.as_ref() else {
            return;
        };

        let branch_name = String::from("feature");
        let destination_path = suggest_worktree_destination(git_folder, &branch_name);
        self.add_worktree_dialog = Some(AddWorktreeDialog {
            project_id: project_id.to_string(),
            branch_name,
            destination_path,
        });
    }

    pub(crate) fn commit_add_worktree(&mut self) -> Result<(), String> {
        let Some(dialog) = self.add_worktree_dialog.clone() else {
            return Ok(());
        };

        let branch_name = dialog.branch_name.trim();
        let destination_path = dialog.destination_path.trim();
        if branch_name.is_empty() {
            return Err(String::from("Branch name cannot be empty"));
        }
        if destination_path.is_empty() {
            return Err(String::from("Destination path cannot be empty"));
        }

        let Some(project) = self
            .persisted
            .projects
            .iter()
            .find(|project| project.id == dialog.project_id)
        else {
            return Err(String::from("Project not found"));
        };

        let git_folder = project
            .git_folder_path
            .as_ref()
            .ok_or_else(|| String::from("Project has no git folder configured"))?;

        add_worktree(git_folder, destination_path, branch_name)?;
        self.add_worktree_dialog = None;
        self.rescan_project(&dialog.project_id)?;
        Ok(())
    }

    pub(crate) fn suggested_worktree_destination(
        &self,
        project_id: &str,
        branch_name: &str,
    ) -> Option<String> {
        let project = self
            .persisted
            .projects
            .iter()
            .find(|project| project.id == project_id)?;
        let git_folder = project.git_folder_path.as_ref()?;
        Some(suggest_worktree_destination(git_folder, branch_name))
    }

    pub(crate) fn remove_worktree(
        &mut self,
        project_id: &str,
        worktree_id: &str,
    ) -> Result<(), String> {
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

        let worktree = self.persisted.projects[project_idx]
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
            .ok_or_else(|| String::from("Worktree not found"))?;

        if worktree.path == git_folder {
            return Err(String::from("Main worktree cannot be removed"));
        }

        remove_worktree(&git_folder, &worktree.path)?;
        self.rescan_project(project_id)?;
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

        // Initialize terminal status after the loop to avoid borrow issues
        if let Some(ref terminal_id) = created {
            self.set_terminal_status(terminal_id.clone(), TerminalStatus::Running);
        }

        self.normalize_selection();
        created
    }

    pub(crate) fn add_detached_terminal(&mut self) -> String {
        let terminal_id = create_id("terminal");
        let terminal_name = next_terminal_name(&self.persisted.detached_terminals);
        self.persisted.detached_terminals.push(TerminalRecord {
            id: terminal_id.clone(),
            name: terminal_name,
            manual_name: false,
        });
        self.persisted.selected_detached_terminal_id = Some(terminal_id.clone());
        // Initialize terminal status as Running
        self.set_terminal_status(terminal_id.clone(), TerminalStatus::Running);
        self.normalize_selection();
        terminal_id
    }

    pub(crate) fn select_detached_terminal(&mut self, terminal_id: &str) {
        if self
            .persisted
            .detached_terminals
            .iter()
            .any(|terminal| terminal.id == terminal_id)
        {
            self.persisted.selected_detached_terminal_id = Some(terminal_id.to_string());
        }
        self.normalize_selection();
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

    pub(crate) fn remove_detached_terminal(&mut self, terminal_id: &str) {
        self.persisted
            .detached_terminals
            .retain(|terminal| terminal.id != terminal_id);

        if self
            .persisted
            .selected_detached_terminal_id
            .as_ref()
            .is_some_and(|selected| selected == terminal_id)
        {
            self.persisted.selected_detached_terminal_id = self
                .persisted
                .detached_terminals
                .first()
                .map(|terminal| terminal.id.clone());
        }

        self.remove_runtime(terminal_id);
        self.normalize_selection();
    }

    pub(crate) fn add_browser(&mut self) -> String {
        let browser_id = create_id("browser");
        let browser_name = next_browser_name(&self.persisted.browsers);
        self.persisted.browsers.push(BrowserRecord {
            id: browser_id.clone(),
            name: browser_name,
            url: String::from("https://"),
        });
        self.persisted.selected_browser_id = Some(browser_id.clone());
        self.normalize_selection();
        browser_id
    }

    pub(crate) fn select_browser(&mut self, browser_id: &str) {
        if self
            .persisted
            .browsers
            .iter()
            .any(|browser| browser.id == browser_id)
        {
            self.persisted.selected_browser_id = Some(browser_id.to_string());
        }
        self.normalize_selection();
    }

    pub(crate) fn remove_browser(&mut self, browser_id: &str) {
        self.persisted
            .browsers
            .retain(|browser| browser.id != browser_id);

        if self
            .persisted
            .selected_browser_id
            .as_ref()
            .is_some_and(|selected| selected == browser_id)
        {
            self.persisted.selected_browser_id = self
                .persisted
                .browsers
                .first()
                .map(|browser| browser.id.clone());
        }

        self.browser_webviews.remove(browser_id);
        self.normalize_selection();
    }

    pub(crate) fn active_browser_id(&self) -> Option<String> {
        self.persisted.selected_browser_id.clone()
    }

    pub(crate) fn active_browser(&self) -> Option<&BrowserRecord> {
        let browser_id = self.active_browser_id()?;
        self.persisted
            .browsers
            .iter()
            .find(|browser| browser.id == browser_id)
    }

    pub(crate) fn close_active_terminal(&mut self) -> bool {
        let Some(active_terminal_id) = self.active_terminal_id() else {
            return false;
        };

        if let Some(locator) = self.find_terminal_locator(&active_terminal_id) {
            let project_id = self.persisted.projects[locator.project_idx].id.clone();
            let worktree_id = self.persisted.projects[locator.project_idx].worktrees
                [locator.worktree_idx]
                .id
                .clone();
            self.remove_terminal(&project_id, &worktree_id, &active_terminal_id);
            return true;
        }

        if self
            .persisted
            .detached_terminals
            .iter()
            .any(|terminal| terminal.id == active_terminal_id)
        {
            self.remove_detached_terminal(&active_terminal_id);
            return true;
        }

        false
    }

    pub(crate) fn start_rename_focused(&mut self) {
        self.start_rename_active_terminal();
        if self.rename_dialog.is_some() {
            return;
        }

        if let Some(project_idx) = self.active_project_index() {
            let project_id = self.persisted.projects[project_idx].id.clone();
            self.start_rename_project(&project_id);
        }
    }

    pub(crate) fn start_rename_project(&mut self, project_id: &str) {
        let Some(project) = self
            .persisted
            .projects
            .iter()
            .find(|project| project.id == project_id)
        else {
            return;
        };

        self.rename_dialog = Some(RenameDialog {
            target: RenameTarget::Project {
                project_id: project_id.to_string(),
            },
            value: project.name.clone(),
        });
    }

    pub(crate) fn start_rename_worktree(&mut self, project_id: &str, worktree_id: &str) {
        let Some(project) = self
            .persisted
            .projects
            .iter()
            .find(|project| project.id == project_id)
        else {
            return;
        };

        let Some(worktree) = project
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
        else {
            return;
        };

        self.rename_dialog = Some(RenameDialog {
            target: RenameTarget::Worktree {
                project_id: project_id.to_string(),
                worktree_id: worktree_id.to_string(),
            },
            value: worktree.name.clone(),
        });
    }

    pub(crate) fn start_rename_active_terminal(&mut self) {
        let Some(active_terminal_id) = self.active_terminal_id() else {
            return;
        };

        if let Some(locator) = self.find_terminal_locator(&active_terminal_id) {
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

        if let Some(terminal) = self
            .persisted
            .detached_terminals
            .iter()
            .find(|terminal| terminal.id == active_terminal_id)
        {
            self.rename_dialog = Some(RenameDialog {
                target: RenameTarget::DetachedTerminal {
                    terminal_id: terminal.id.clone(),
                },
                value: terminal.name.clone(),
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
            RenameTarget::Worktree {
                project_id,
                worktree_id,
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
                {
                    worktree.name = value.to_string();
                    worktree.manual_name = true;
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
            RenameTarget::DetachedTerminal { terminal_id } => {
                if let Some(terminal) = self
                    .persisted
                    .detached_terminals
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
        if let Some(locator) = self.find_terminal_locator(terminal_id) {
            let project_id = self.persisted.projects[locator.project_idx].id.clone();
            self.select_terminal(&project_id, terminal_id);
            return;
        }

        self.select_detached_terminal(terminal_id);
    }

    pub(crate) fn terminal_exists(&self, terminal_id: &str) -> bool {
        self.find_terminal_locator(terminal_id).is_some()
            || self
                .persisted
                .detached_terminals
                .iter()
                .any(|terminal| terminal.id == terminal_id)
    }

    /// Get the current status of a terminal
    pub(crate) fn get_terminal_status(&self, terminal_id: &str) -> TerminalStatus {
        self.terminal_status
            .get(terminal_id)
            .cloned()
            .unwrap_or(TerminalStatus::Running)
    }

    /// Set the status of a terminal
    pub(crate) fn set_terminal_status(&mut self, terminal_id: String, status: TerminalStatus) {
        self.terminal_status.insert(terminal_id, status);
    }

    /// Mark a terminal as awaiting response (bell was rung)
    pub(crate) fn on_terminal_bell(&mut self, terminal_id: String) {
        self.terminal_awaiting_response.insert(terminal_id.clone());
        self.terminal_status.insert(terminal_id, TerminalStatus::AwaitingResponse);
    }

    /// Clear awaiting response state when user provides input
    pub(crate) fn clear_awaiting_on_activity(&mut self, terminal_id: &str) {
        self.terminal_awaiting_response.remove(terminal_id);
        // If the terminal was in AwaitingResponse state, change it back to Running
        if self.terminal_status.get(terminal_id) == Some(&TerminalStatus::AwaitingResponse) {
            self.terminal_status.insert(terminal_id.to_string(), TerminalStatus::Running);
        }
    }

    /// Check if a terminal is awaiting response
    pub(crate) fn is_awaiting_response(&self, terminal_id: &str) -> bool {
        self.terminal_awaiting_response.contains(terminal_id)
    }

    /// Clean up status when terminal is removed
    pub(crate) fn remove_terminal_status(&mut self, terminal_id: &str) {
        self.terminal_status.remove(terminal_id);
        self.terminal_awaiting_response.remove(terminal_id);
        self.terminal_titles.remove(terminal_id);
    }

    /// Handle title change from Ghostty, checking for bell emoji
    pub(crate) fn on_terminal_title(&mut self, surface_ptr: usize, title: String) {
        // Find which terminal owns this surface by searching all runtimes
        let terminal_id = self.runtimes
            .iter()
            .find(|(_, runtime)| runtime.pane_id_for_surface(surface_ptr).is_some())
            .map(|(tid, _)| tid.clone());

        let terminal_id = match terminal_id {
            Some(tid) => tid,
            None => return,
        };

        // Check if title contains bell emoji - set awaiting state if so
        // Note: We DON'T clear the state when emoji is removed - only keyboard input should clear it
        if title.contains('🔔') {
            self.on_terminal_bell(terminal_id.clone());
        }

        // Store title for future reference
        self.terminal_titles.insert(terminal_id, title);
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

fn suggest_worktree_destination(git_folder: &str, branch_name: &str) -> String {
    let root = PathBuf::from(git_folder);
    let repo_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("worktree");
    let suffix = sanitize_branch_component(branch_name);
    let name = if suffix.is_empty() {
        format!("{repo_name}-worktree")
    } else {
        format!("{repo_name}-{suffix}")
    };

    root.parent()
        .unwrap_or(root.as_path())
        .join(name)
        .to_string_lossy()
        .to_string()
}

fn sanitize_branch_component(value: &str) -> String {
    let filtered: String = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    filtered.trim_matches('-').to_string()
}
