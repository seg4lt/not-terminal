use crate::app::git_worktrees::{add_worktree, remove_worktree, scan_worktrees};
use crate::app::model::{
    BrowserRecord, PersistedState, ProjectRecord, TerminalRecord, TreeStateRecord, UiState,
    WorktreeRecord, create_id, infer_project_name, next_browser_name, next_project_name,
    next_terminal_name,
};
use crate::app::persistence;
use crate::app::runtime::{PaneRuntime, RuntimeSession};
use crate::ghostty_embed::{
    GhosttyEmbed, GhosttyProgressReportState, GhosttyRuntimeAction, host_view_free, host_view_new,
    ns_view_ptr, parent_view_set_attention_badge,
};
use crate::webview::WebView;
use iced::{
    Point, Size, Subscription, Task, keyboard, time,
    window::{self},
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

mod browser;
mod project;
mod terminal;
mod terminal_status;

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
pub(crate) const COMMAND_PALETTE_SCROLL_ID: &str = "command-palette-scroll";
pub(crate) const QUICK_OPEN_SCROLL_ID: &str = "quick-open-scroll";
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
pub(crate) struct WorktreeContextMenu {
    pub(crate) project_id: String,
    pub(crate) worktree_id: String,
    pub(crate) show_project_actions: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum QuickOpenEntryKind {
    ExistingTerminal {
        terminal_id: String,
    },
    CreateTerminal {
        project_id: String,
        worktree_id: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct QuickOpenEntry {
    pub(crate) project_name: String,
    pub(crate) worktree_name: String,
    pub(crate) terminal_name: String,
    pub(crate) kind: QuickOpenEntryKind,
}

#[derive(Debug, Clone)]
pub(crate) enum CommandPaletteAction {
    OpenQuickOpen,
    ToggleSidebar,
    NewTerminal,
    NewDetachedTerminal,
    CloseActiveTerminal,
    RenameFocused,
    RenameTerminal,
    RenameWorktree,
    OpenPreferences,
    AddProject,
    AddWorktreeToActiveProject,
    RescanActiveProject,
    ToggleBrowsers,
    AddBrowser,
    BrowserDevTools,
    FontIncrease,
    FontDecrease,
    FontReset,
    NextTerminal,
    PreviousTerminal,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandPaletteEntry {
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) search_text: String,
    pub(crate) action: CommandPaletteAction,
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
    pub(crate) command_palette_open: bool,
    pub(crate) command_palette_query: String,
    pub(crate) command_palette_selected_index: usize,
    pub(crate) quick_open_open: bool,
    pub(crate) quick_open_query: String,
    pub(crate) quick_open_selected_index: usize,
    pub(crate) quick_open_ignore_next_query_change: bool,
    pub(crate) rename_dialog: Option<RenameDialog>,
    pub(crate) add_worktree_dialog: Option<AddWorktreeDialog>,
    pub(crate) worktree_context_menu: Option<WorktreeContextMenu>,
    pub(crate) suppress_next_key_release: bool,
    pub(crate) branch_by_terminal: HashMap<String, String>,
    pub(crate) last_branch_refresh_terminal_id: Option<String>,
    pub(crate) last_branch_refresh_at: Option<Instant>,
    pub(crate) last_ghostty_activity: Instant,
    /// Tracks exit status of terminals by ID
    pub(crate) terminal_status: HashMap<String, TerminalStatus>,
    /// Terminals that have rung the bell (awaiting user input)
    pub(crate) terminal_awaiting_response: HashSet<String>,
    /// Terminals currently reporting explicit progress via Ghostty OSC progress.
    pub(crate) terminal_progress_active: HashSet<String>,
    /// Stores title per terminal_id for bell detection via title emoji
    pub(crate) terminal_titles: HashMap<String, String>,
    /// Animation frame for active terminal progress indicators.
    pub(crate) terminal_activity_frame: usize,
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
    SetEnableBrowsers(bool),
    FilterChanged(String),
    AddProject,
    ProjectRescan(String),
    #[allow(dead_code)]
    SelectProject(String),
    #[allow(dead_code)]
    ToggleProjectCollapsed(String),
    ToggleAllProjectTreesCollapsed,
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
    OpenCommandPalette(bool),
    CommandPaletteQueryChanged(String),
    CommandPaletteSubmit,
    CommandPaletteSelect(usize),
    RunCommandPaletteAction(CommandPaletteAction),
    OpenQuickOpen(bool),
    QuickOpenQueryChanged(String),
    QuickOpenSubmit,
    QuickOpenSelect(usize),
    QuickOpenCloseTerminal(String),
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
    OpenWorktreeContextMenu {
        project_id: String,
        worktree_id: String,
        show_project_actions: bool,
    },
    CloseWorktreeContextMenu,
    WorktreeContextMenuNewTerminal,
    WorktreeContextMenuRenameWorktree,
    WorktreeContextMenuProjectRescan,
    WorktreeContextMenuRemoveProject,
    AddWorktreeBranchChanged(String),
    AddWorktreePathChanged(String),
    FocusAddWorktreePath,
    AddWorktreeCommit,
    AddWorktreeCancel,
    RemoveWorktree {
        project_id: String,
        worktree_id: String,
    },
    RemoveProject(String),
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
            command_palette_open: false,
            command_palette_query: String::new(),
            command_palette_selected_index: 0,
            quick_open_open: false,
            quick_open_query: String::new(),
            quick_open_selected_index: 0,
            quick_open_ignore_next_query_change: false,
            rename_dialog: None,
            add_worktree_dialog: None,
            worktree_context_menu: None,
            suppress_next_key_release: false,
            branch_by_terminal: HashMap::new(),
            last_branch_refresh_terminal_id: None,
            last_branch_refresh_at: None,
            last_ghostty_activity: Instant::now(),
            terminal_status: HashMap::new(),
            terminal_awaiting_response: HashSet::new(),
            terminal_progress_active: HashSet::new(),
            terminal_titles: HashMap::new(),
            terminal_activity_frame: 0,
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
            let cadence = if !self.terminal_progress_active.is_empty() {
                Duration::from_millis(120)
            } else {
                let time_since_activity = self.last_ghostty_activity.elapsed();

                if time_since_activity > Duration::from_secs(30) {
                    Duration::from_secs(2)
                } else if time_since_activity > Duration::from_secs(10) {
                    Duration::from_millis(500)
                } else if time_since_activity > Duration::from_secs(3) {
                    Duration::from_millis(100)
                } else if time_since_activity > Duration::from_millis(500) {
                    Duration::from_millis(33)
                } else {
                    Duration::from_millis(16)
                }
            };

            subscriptions.push(time::every(cadence).map(|_| Message::GhosttyTick));
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
            enable_browsers: self.persisted.ui.enable_browsers,
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

    pub(crate) fn terminal_has_splits(&self, terminal_id: &str) -> bool {
        self.runtimes
            .get(terminal_id)
            .is_some_and(RuntimeSession::has_splits)
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

    pub(crate) fn terminal_needs_attention(&self, terminal_id: &str) -> bool {
        self.is_awaiting_response(terminal_id)
            || matches!(
                self.terminal_status.get(terminal_id),
                Some(TerminalStatus::AwaitingResponse)
            )
    }

    pub(crate) fn attention_terminal_count(&self) -> usize {
        self.global_terminal_sequence()
            .into_iter()
            .filter(|terminal_id| self.terminal_needs_attention(terminal_id))
            .count()
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
                    if !search_terms.is_empty()
                        && !search_terms.iter().all(|term| text.contains(term))
                    {
                        continue;
                    }

                    entries.push(QuickOpenEntry {
                        project_name: project.name.clone(),
                        worktree_name: worktree.name.clone(),
                        terminal_name: terminal.name.clone(),
                        kind: QuickOpenEntryKind::ExistingTerminal {
                            terminal_id: terminal.id.clone(),
                        },
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
                terminal_name: terminal.name.clone(),
                kind: QuickOpenEntryKind::ExistingTerminal {
                    terminal_id: terminal.id.clone(),
                },
            });
        }

        // "Create terminal" actions are always shown at the end for every worktree.
        for project in &self.persisted.projects {
            for worktree in &project.worktrees {
                if worktree.missing {
                    continue;
                }

                let text = format!(
                    "{} {} new terminal",
                    project.name.to_lowercase(),
                    worktree.name.to_lowercase()
                );

                if !search_terms.is_empty() && !search_terms.iter().all(|term| text.contains(term))
                {
                    continue;
                }

                entries.push(QuickOpenEntry {
                    project_name: project.name.clone(),
                    worktree_name: worktree.name.clone(),
                    terminal_name: String::from("New terminal"),
                    kind: QuickOpenEntryKind::CreateTerminal {
                        project_id: project.id.clone(),
                        worktree_id: worktree.id.clone(),
                    },
                });
            }
        }

        entries
    }

    pub(crate) fn command_palette_entries(&self) -> Vec<CommandPaletteEntry> {
        let query = self.command_palette_query.trim().to_lowercase();
        let search_terms: Vec<&str> = if query.is_empty() {
            Vec::new()
        } else {
            query.split_whitespace().collect()
        };

        let active_project = self.active_project();
        let active_project_label = active_project
            .map(|project| project.name.clone())
            .unwrap_or_else(|| String::from("No active project"));
        let active_worktree_label = self
            .active_worktree_ids()
            .and_then(|(_, worktree_id)| {
                self.persisted.projects.iter().find_map(|project| {
                    project
                        .worktrees
                        .iter()
                        .find(|worktree| worktree.id == worktree_id)
                        .map(|worktree| format!("{} / {}", project.name, worktree.name))
                })
            })
            .unwrap_or_else(|| String::from("the active worktree"));
        let active_browser_label = self
            .active_browser()
            .map(|browser| browser.name.clone())
            .unwrap_or_else(|| String::from("browser"));

        let mut entries = vec![
            command_palette_entry(
                CommandPaletteAction::OpenQuickOpen,
                "Quick Open",
                "Search terminals and worktrees • Cmd+P",
                "terminal worktree switch search open",
            ),
            command_palette_entry(
                CommandPaletteAction::ToggleSidebar,
                "Toggle Sidebar",
                "Show or hide the sidebar • Cmd+1",
                "sidebar layout panel",
            ),
            command_palette_entry(
                CommandPaletteAction::NewTerminal,
                format!("New Terminal in {active_worktree_label}"),
                "Create a terminal in the active worktree • Cmd+T",
                "terminal shell tab create worktree",
            ),
            command_palette_entry(
                CommandPaletteAction::NewDetachedTerminal,
                "New Detached Terminal",
                "Create a standalone terminal • Cmd+Shift+T",
                "terminal detached floating create",
            ),
            command_palette_entry(
                CommandPaletteAction::CloseActiveTerminal,
                "Close Active Terminal",
                "Close the currently selected terminal • Cmd+W",
                "terminal close kill remove",
            ),
            command_palette_entry(
                CommandPaletteAction::RenameFocused,
                "Rename Focused Item",
                "Rename the active terminal or worktree • F2",
                "rename terminal worktree focused edit",
            ),
            command_palette_entry(
                CommandPaletteAction::RenameTerminal,
                "Rename Active Terminal",
                "Rename just the active terminal • Cmd+R",
                "rename terminal title",
            ),
            command_palette_entry(
                CommandPaletteAction::RenameWorktree,
                format!("Rename Worktree in {active_worktree_label}"),
                "Rename the active worktree",
                "rename worktree branch project folder",
            ),
            command_palette_entry(
                CommandPaletteAction::OpenPreferences,
                "Open Preferences",
                "Show application settings • Cmd+,",
                "preferences settings configuration",
            ),
            command_palette_entry(
                CommandPaletteAction::AddProject,
                "Add Project",
                "Import a Git folder into the sidebar",
                "project repository repo add open folder",
            ),
            command_palette_entry(
                CommandPaletteAction::ToggleBrowsers,
                if self.persisted.ui.enable_browsers {
                    "Disable Browsers"
                } else {
                    "Enable Browsers"
                },
                "Toggle the embedded browser feature",
                "browser webview toggle preferences",
            ),
            command_palette_entry(
                CommandPaletteAction::FontIncrease,
                "Increase Terminal Font Size",
                "Grow the active terminal font • Cmd+=",
                "font zoom in increase terminal",
            ),
            command_palette_entry(
                CommandPaletteAction::FontDecrease,
                "Decrease Terminal Font Size",
                "Shrink the active terminal font • Cmd+-",
                "font zoom out decrease terminal",
            ),
            command_palette_entry(
                CommandPaletteAction::FontReset,
                "Reset Terminal Font Size",
                "Restore the default terminal font size • Cmd+0",
                "font zoom reset terminal",
            ),
            command_palette_entry(
                CommandPaletteAction::NextTerminal,
                "Next Terminal",
                "Move to the next terminal in the global sequence • Cmd+Shift+]",
                "terminal next cycle switch",
            ),
            command_palette_entry(
                CommandPaletteAction::PreviousTerminal,
                "Previous Terminal",
                "Move to the previous terminal in the global sequence • Cmd+Shift+[",
                "terminal previous cycle switch",
            ),
        ];

        if active_project.is_some() {
            entries.push(command_palette_entry(
                CommandPaletteAction::AddWorktreeToActiveProject,
                format!("Add Worktree to {active_project_label}"),
                "Create a new worktree for the active project",
                "worktree branch create project",
            ));
            entries.push(command_palette_entry(
                CommandPaletteAction::RescanActiveProject,
                format!("Rescan {active_project_label}"),
                "Refresh the active project's worktree list",
                "rescan refresh project worktree git",
            ));
        }

        if self.persisted.ui.enable_browsers {
            entries.push(command_palette_entry(
                CommandPaletteAction::AddBrowser,
                "New Browser",
                "Create an embedded browser • Cmd+B",
                "browser webview new create",
            ));
            entries.push(command_palette_entry(
                CommandPaletteAction::BrowserDevTools,
                format!("Open DevTools for {active_browser_label}"),
                "Open Web Inspector for the active browser • Cmd+Option+I",
                "browser devtools inspector webview",
            ));
        }

        if search_terms.is_empty() {
            return entries;
        }

        entries
            .into_iter()
            .filter(|entry| {
                let haystack = entry.search_text.to_lowercase();
                search_terms.iter().all(|term| haystack.contains(term))
            })
            .collect()
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

    #[allow(dead_code)]
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
        let attention_count = self.attention_terminal_count();

        if let Some(parent_ns_view) = self.host_ns_view {
            let visible = self.sidebar_state.is_hidden() && !modal_open && attention_count > 0;
            let count = attention_count.min(i32::MAX as usize) as i32;
            parent_view_set_attention_badge(parent_ns_view, visible, count);
        }

        // Sync browser webviews - only the active one is visible
        let browser_toolbar_height = 32.0;
        for (browser_id, webview) in &mut self.browser_webviews {
            let is_active = active_browser_id
                .as_ref()
                .is_some_and(|id| id == browser_id);
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
                        self.set_terminal_progress_active(&terminal_id, false);
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
                        true // Trigger UI update
                    }
                    GhosttyRuntimeAction::RingBell { .. } => {
                        // Mark terminal as awaiting response
                        self.on_terminal_bell(terminal_id.clone());
                        true // Trigger UI update
                    }
                    GhosttyRuntimeAction::SetTitle { surface_ptr, title } => {
                        // Handle title change - check for bell emoji
                        self.on_terminal_title(surface_ptr, title);
                        true // Trigger UI update
                    }
                    GhosttyRuntimeAction::DesktopNotification { .. } => {
                        // Desktop notification means the terminal needs attention
                        // Mark as awaiting response
                        self.on_terminal_bell(terminal_id.clone());
                        true // Trigger UI update
                    }
                    GhosttyRuntimeAction::ProgressReport { state, .. } => {
                        let active = matches!(
                            state,
                            GhosttyProgressReportState::Set
                                | GhosttyProgressReportState::Indeterminate
                        );
                        self.set_terminal_progress_active(&terminal_id, active);
                        true
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
        self.command_palette_open
            || self.quick_open_open
            || self.preferences_open
            || self.rename_dialog.is_some()
            || self.add_worktree_dialog.is_some()
            || self.worktree_context_menu.is_some()
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

    pub(crate) fn start_rename_focused(&mut self) {
        self.start_rename_active_terminal();
        if self.rename_dialog.is_some() {
            return;
        }

        if let Some((project_id, worktree_id)) = self.active_worktree_ids() {
            self.start_rename_worktree(&project_id, &worktree_id);
        }
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

fn command_palette_entry(
    action: CommandPaletteAction,
    title: impl Into<String>,
    detail: impl Into<String>,
    search_terms: &str,
) -> CommandPaletteEntry {
    let title = title.into();
    let detail = detail.into();
    let search_text = format!("{title} {detail} {search_terms}");
    CommandPaletteEntry {
        title,
        detail,
        search_text,
        action,
    }
}
