use crate::app::diff_runtime::DiffPaneRuntime;
use crate::app::git_diff::DiffSnapshot;
use crate::app::git_worktrees::{add_worktree, remove_worktree, scan_worktrees};
use crate::app::model::{
    BrowserRecord, PersistedState, PinnedTerminalRecord, ProjectRecord, TerminalRecord,
    TreeStateRecord, UiState, WorktreeRecord, create_id, infer_project_name, next_browser_name,
    next_project_name, next_terminal_name,
};
use crate::app::persistence;
use crate::app::runtime::{
    PaneRuntime, RuntimeDiffAction, RuntimeSession, SplitAxis, SplitDivider,
};
use crate::ghostty_embed::{
    GhosttyEmbed, GhosttyProgressReportState, GhosttyRuntimeAction, host_view_focus_terminal,
    host_view_free, host_view_new, ns_view_ptr, parent_view_reclaim_focus,
    parent_view_set_attention_badge,
};
use crate::webview::WebView;
use iced::{
    Point, Size, Subscription, Task, keyboard,
    mouse::Interaction,
    time,
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
pub(crate) const ADD_WORKTREE_PROJECT_SCROLL_ID: &str = "add-worktree-project-scroll";
pub(crate) const REMOVE_PROJECT_SCROLL_ID: &str = "remove-project-scroll";
pub(crate) const DELETE_WORKTREE_PROJECT_SCROLL_ID: &str = "delete-worktree-project-scroll";
pub(crate) const DELETE_WORKTREE_SCROLL_ID: &str = "delete-worktree-scroll";
pub(crate) const MAX_PINNED_TERMINALS: usize = 9;
const BRANCH_REFRESH_INTERVAL: Duration = Duration::from_millis(350);
pub(crate) const TERMINAL_SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);
pub(crate) const DIFF_REFRESH_DEBOUNCE: Duration = Duration::from_millis(350);

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
    PinnedTerminal {
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
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectContextMenu {
    pub(crate) project_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AddWorktreeProjectEntry {
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    pub(crate) worktree_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct DeleteWorktreeProjectEntry {
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    pub(crate) worktree_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct RemoveProjectEntry {
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    pub(crate) worktree_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct DeleteWorktreeEntry {
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    pub(crate) worktree_id: String,
    pub(crate) worktree_name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DeleteWorktreePicker {
    pub(crate) project_id: String,
    pub(crate) selected_index: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct PinnedTerminalEntry {
    pub(crate) slot: usize,
    pub(crate) terminal_id: String,
    pub(crate) alias: String,
    pub(crate) location_label: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ProjectRescanSummary {
    pub(crate) total_projects: usize,
    pub(crate) successful_projects: usize,
    pub(crate) changed_projects: usize,
    pub(crate) failed_projects: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum AddProjectOutcome {
    Added { path: String },
    AlreadyExists { project_name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PinTerminalOutcome {
    Pinned(usize),
    AlreadyPinned(usize),
    LimitReached,
    Missing,
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
pub(crate) struct TerminalSearchState {
    pub(crate) terminal_id: String,
    pub(crate) surface_ptr: usize,
    pub(crate) query: String,
    pub(crate) total: Option<usize>,
    pub(crate) selected: Option<usize>,
    pub(crate) pending_apply: bool,
    pub(crate) pending_deadline: Option<Instant>,
    pub(crate) focus_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SidebarDragItem {
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
}

#[derive(Debug, Clone)]
pub(crate) struct SidebarDragState {
    pub(crate) item: SidebarDragItem,
    pub(crate) hover: Option<SidebarDragItem>,
}

#[derive(Debug, Clone)]
pub(crate) struct SplitResizeDragState {
    pub(crate) terminal_id: String,
    pub(crate) branch_path: Vec<bool>,
    pub(crate) axis: SplitAxis,
    pub(crate) grab_offset: f32,
}

#[derive(Debug, Clone)]
pub(crate) enum CommandPaletteAction {
    OpenQuickOpen,
    ToggleSidebar,
    NewTerminal,
    NewDetachedTerminal,
    CloseActiveTerminal,
    PinFocusedItem,
    UnpinFocusedItem,
    RenameFocused,
    RenameTerminal,
    RenameWorktree,
    OpenPreferences,
    AddProject,
    AddWorktreeToProject,
    AddWorktreeToActiveProject,
    RemoveProject,
    ExpandAllProjects,
    CollapseAllProjects,
    DeleteWorktreeFromProject,
    RescanAllProjects,
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
    pub(crate) sidebar_drag: Option<SidebarDragState>,
    pub(crate) split_resize_drag: Option<SplitResizeDragState>,
    pub(crate) add_worktree_project_picker_open: bool,
    pub(crate) add_worktree_project_selected_index: usize,
    pub(crate) remove_project_picker_open: bool,
    pub(crate) remove_project_selected_index: usize,
    pub(crate) delete_worktree_project_picker_open: bool,
    pub(crate) delete_worktree_project_selected_index: usize,
    pub(crate) delete_worktree_picker: Option<DeleteWorktreePicker>,
    pub(crate) rename_dialog: Option<RenameDialog>,
    pub(crate) add_worktree_dialog: Option<AddWorktreeDialog>,
    pub(crate) worktree_context_menu: Option<WorktreeContextMenu>,
    pub(crate) project_context_menu: Option<ProjectContextMenu>,
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
    /// Debounced diff refresh deadlines by terminal session.
    pub(crate) diff_refresh_deadlines: HashMap<String, Instant>,
    /// Terminal sessions with an in-flight diff request.
    pub(crate) diff_refresh_in_flight: HashSet<String>,
    /// Ephemeral terminal search state for the active Ghostty surface.
    pub(crate) terminal_search: Option<TerminalSearchState>,
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
    SetPreferredEditorCommand(String),
    SetSecondaryEditorCommand(String),
    FilterChanged(String),
    AddProject,
    ProjectRescan(String),
    RescanAllProjects,
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
    OpenInPreferredEditor,
    OpenInSecondaryEditor,
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
    TerminalSearchNext,
    TerminalSearchPrevious,
    TerminalSearchClose,
    StartSidebarDrag(SidebarDragItem),
    SidebarDragHover(SidebarDragItem),
    SidebarDragHoverExit(SidebarDragItem),
    StartRenameWorktree {
        project_id: String,
        worktree_id: String,
    },
    StartRenameFocused,
    StartRenameTerminal,
    RenameValueChanged(String),
    RenameCommit,
    RenameCancel,
    OpenAddWorktreeProjectPicker,
    AddWorktreeProjectSubmit,
    AddWorktreeProjectSelect(usize),
    AddWorktreeProjectCancel,
    OpenRemoveProjectPicker,
    RemoveProjectSubmit,
    RemoveProjectSelect(usize),
    RemoveProjectCancel,
    OpenDeleteWorktreeProjectPicker,
    DeleteWorktreeProjectSubmit,
    DeleteWorktreeProjectSelect(usize),
    DeleteWorktreeProjectCancel,
    OpenDeleteWorktreePicker(String),
    DeleteWorktreeSubmit,
    DeleteWorktreeSelect(usize),
    DeleteWorktreeCancel,
    TogglePinnedTerminal(String),
    SelectPinnedTerminal(String),
    SelectPinnedTerminalSlot(usize),
    StartRenamePinnedTerminal(String),
    StartAddWorktree(String),
    OpenWorktreeContextMenu {
        project_id: String,
        worktree_id: String,
    },
    OpenProjectContextMenu(String),
    CloseWorktreeContextMenu,
    CloseProjectContextMenu,
    WorktreeContextMenuNewTerminal,
    WorktreeContextMenuRenameWorktree,
    ProjectContextMenuProjectRescan,
    ProjectContextMenuRemoveProject,
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
    ToggleDiffView,
    DiffDataLoaded {
        terminal_id: String,
        worktree_path: String,
        result: Result<DiffSnapshot, String>,
    },
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
            sidebar_drag: None,
            split_resize_drag: None,
            add_worktree_project_picker_open: false,
            add_worktree_project_selected_index: 0,
            remove_project_picker_open: false,
            remove_project_selected_index: 0,
            delete_worktree_project_picker_open: false,
            delete_worktree_project_selected_index: 0,
            delete_worktree_picker: None,
            rename_dialog: None,
            add_worktree_dialog: None,
            worktree_context_menu: None,
            project_context_menu: None,
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
            diff_refresh_deadlines: HashMap::new(),
            diff_refresh_in_flight: HashSet::new(),
            terminal_search: None,
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
            preferred_editor_command: self.persisted.ui.preferred_editor_command.clone(),
            secondary_editor_command: self.persisted.ui.secondary_editor_command.clone(),
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

    pub(crate) fn split_divider_at_position(&self, position: Point) -> Option<SplitDivider> {
        if self.active_browser_id().is_some() || self.modal_open() {
            return None;
        }

        let (x, y, width, height) = self.terminal_frame_logical();
        let within_x = position.x >= x && position.x < x + width;
        let within_y = position.y >= y && position.y < y + height;

        if !(within_x && within_y) {
            return None;
        }

        let local_x = position.x - x;
        let local_y = position.y - y;
        let active_terminal_id = self.active_terminal_id()?;
        self.runtimes
            .get(&active_terminal_id)
            .and_then(|runtime| runtime.split_divider_at(local_x, local_y, width, height))
    }

    pub(crate) fn start_split_resize_drag(&mut self, position: Point) -> bool {
        let Some(terminal_id) = self.active_terminal_id() else {
            return false;
        };
        let Some(divider) = self.split_divider_at_position(position) else {
            return false;
        };

        let grab_offset = match divider.axis {
            SplitAxis::Vertical => {
                position.x
                    - self.terminal_frame_logical().0
                    - divider.rect.x
                    - divider.rect.width / 2.0
            }
            SplitAxis::Horizontal => {
                position.y
                    - self.terminal_frame_logical().1
                    - divider.rect.y
                    - divider.rect.height / 2.0
            }
        };

        self.split_resize_drag = Some(SplitResizeDragState {
            terminal_id,
            branch_path: divider.branch_path,
            axis: divider.axis,
            grab_offset,
        });
        true
    }

    pub(crate) fn update_split_resize_drag(&mut self, position: Point) -> bool {
        let Some(drag) = self.split_resize_drag.clone() else {
            return false;
        };
        if self.active_browser_id().is_some() || self.modal_open() {
            self.split_resize_drag = None;
            return false;
        }

        let (frame_x, frame_y, width, height) = self.terminal_frame_logical();
        let local_x = position.x - frame_x;
        let local_y = position.y - frame_y;

        let pointer_x = match drag.axis {
            SplitAxis::Vertical => local_x - drag.grab_offset,
            SplitAxis::Horizontal => local_x,
        };
        let pointer_y = match drag.axis {
            SplitAxis::Vertical => local_y,
            SplitAxis::Horizontal => local_y - drag.grab_offset,
        };

        self.runtimes
            .get_mut(&drag.terminal_id)
            .is_some_and(|runtime| {
                runtime.set_split_ratio_from_position(
                    &drag.branch_path,
                    pointer_x,
                    pointer_y,
                    width,
                    height,
                )
            })
    }

    pub(crate) fn finish_split_resize_drag(&mut self) -> bool {
        self.split_resize_drag.take().is_some()
    }

    pub(crate) fn terminal_split_resize_interaction(&self) -> Interaction {
        if let Some(drag) = &self.split_resize_drag {
            return match drag.axis {
                SplitAxis::Vertical => Interaction::ResizingHorizontally,
                SplitAxis::Horizontal => Interaction::ResizingVertically,
            };
        }

        let Some(position) = self.cursor_position_logical else {
            return Interaction::None;
        };
        let Some(divider) = self.split_divider_at_position(position) else {
            return Interaction::None;
        };

        match divider.axis {
            SplitAxis::Vertical => Interaction::ResizingHorizontally,
            SplitAxis::Horizontal => Interaction::ResizingVertically,
        }
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

        let valid_terminal_ids: HashSet<String> =
            self.global_terminal_sequence().into_iter().collect();
        let mut seen_pinned_terminal_ids = HashSet::new();
        self.persisted.pinned_terminals.retain(|pin| {
            valid_terminal_ids.contains(&pin.terminal_id)
                && seen_pinned_terminal_ids.insert(pin.terminal_id.clone())
        });
        if self.persisted.pinned_terminals.len() > MAX_PINNED_TERMINALS {
            self.persisted
                .pinned_terminals
                .truncate(MAX_PINNED_TERMINALS);
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

    pub(crate) fn terminal_name_by_id(&self, terminal_id: &str) -> Option<String> {
        if let Some(locator) = self.find_terminal_locator(terminal_id) {
            return self
                .persisted
                .projects
                .get(locator.project_idx)
                .and_then(|project| project.worktrees.get(locator.worktree_idx))
                .and_then(|worktree| worktree.terminals.get(locator.terminal_idx))
                .map(|terminal| terminal.name.clone());
        }

        self.persisted
            .detached_terminals
            .iter()
            .find(|terminal| terminal.id == terminal_id)
            .map(|terminal| terminal.name.clone())
    }

    pub(crate) fn terminal_location_label(&self, terminal_id: &str) -> Option<String> {
        if let Some(locator) = self.find_terminal_locator(terminal_id) {
            let project = self.persisted.projects.get(locator.project_idx)?;
            let worktree = project.worktrees.get(locator.worktree_idx)?;
            return Some(format!("{} / {}", project.name, worktree.name));
        }

        self.persisted
            .detached_terminals
            .iter()
            .find(|terminal| terminal.id == terminal_id)
            .map(|_| String::from("Detached"))
    }

    pub(crate) fn is_terminal_pinned(&self, terminal_id: &str) -> bool {
        self.persisted
            .pinned_terminals
            .iter()
            .any(|pin| pin.terminal_id == terminal_id)
    }

    pub(crate) fn pin_slot_for_terminal(&self, terminal_id: &str) -> Option<usize> {
        self.pinned_terminal_entries()
            .iter()
            .position(|entry| entry.terminal_id == terminal_id)
    }

    pub(crate) fn pinned_terminal_entries(&self) -> Vec<PinnedTerminalEntry> {
        let mut entries = Vec::new();

        for pin in &self.persisted.pinned_terminals {
            let Some(location_label) = self.terminal_location_label(&pin.terminal_id) else {
                continue;
            };
            let alias = if !pin.manual_alias || pin.alias.trim().is_empty() {
                let Some(name) = self.terminal_name_by_id(&pin.terminal_id) else {
                    continue;
                };
                name
            } else {
                pin.alias.clone()
            };

            entries.push(PinnedTerminalEntry {
                slot: entries.len(),
                terminal_id: pin.terminal_id.clone(),
                alias,
                location_label,
            });
        }

        entries
    }

    pub(crate) fn pin_terminal(&mut self, terminal_id: &str) -> PinTerminalOutcome {
        if let Some(slot) = self.pin_slot_for_terminal(terminal_id) {
            return PinTerminalOutcome::AlreadyPinned(slot);
        }

        let Some(alias) = self.terminal_name_by_id(terminal_id) else {
            return PinTerminalOutcome::Missing;
        };

        let next_slot = self.pinned_terminal_entries().len();
        if next_slot >= MAX_PINNED_TERMINALS {
            return PinTerminalOutcome::LimitReached;
        }

        self.persisted.pinned_terminals.push(PinnedTerminalRecord {
            terminal_id: terminal_id.to_string(),
            alias,
            manual_alias: false,
        });

        PinTerminalOutcome::Pinned(next_slot)
    }

    pub(crate) fn unpin_terminal(&mut self, terminal_id: &str) -> bool {
        let len_before = self.persisted.pinned_terminals.len();
        self.persisted
            .pinned_terminals
            .retain(|pin| pin.terminal_id != terminal_id);
        self.persisted.pinned_terminals.len() != len_before
    }

    pub(crate) fn select_pinned_terminal_slot(&mut self, slot: usize) -> Option<String> {
        let terminal_id = self
            .pinned_terminal_entries()
            .get(slot)?
            .terminal_id
            .clone();
        if !self.terminal_exists(terminal_id.as_str()) {
            return None;
        }
        self.select_terminal_by_id(&terminal_id);
        Some(terminal_id)
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

    pub(crate) fn active_editor_target_path(&self) -> Option<String> {
        if self.active_browser_id().is_some() {
            return None;
        }

        self.active_terminal_context()?.worktree_path
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

    pub(crate) fn add_worktree_project_entries(&self) -> Vec<AddWorktreeProjectEntry> {
        self.persisted
            .projects
            .iter()
            .map(|project| AddWorktreeProjectEntry {
                project_id: project.id.clone(),
                project_name: project.name.clone(),
                worktree_count: project.worktrees.len(),
            })
            .collect()
    }

    pub(crate) fn delete_worktree_project_entries(&self) -> Vec<DeleteWorktreeProjectEntry> {
        self.persisted
            .projects
            .iter()
            .filter_map(|project| {
                let worktree_count = project
                    .worktrees
                    .iter()
                    .filter(|worktree| !Self::is_main_worktree(project, worktree))
                    .count();

                (worktree_count > 0).then_some(DeleteWorktreeProjectEntry {
                    project_id: project.id.clone(),
                    project_name: project.name.clone(),
                    worktree_count,
                })
            })
            .collect()
    }

    pub(crate) fn remove_project_entries(&self) -> Vec<RemoveProjectEntry> {
        self.persisted
            .projects
            .iter()
            .map(|project| RemoveProjectEntry {
                project_id: project.id.clone(),
                project_name: project.name.clone(),
                worktree_count: project.worktrees.len(),
            })
            .collect()
    }

    pub(crate) fn delete_worktree_entries(&self, project_id: &str) -> Vec<DeleteWorktreeEntry> {
        let Some(project) = self
            .persisted
            .projects
            .iter()
            .find(|project| project.id == project_id)
        else {
            return Vec::new();
        };

        project
            .worktrees
            .iter()
            .filter(|worktree| !Self::is_main_worktree(project, worktree))
            .map(|worktree| DeleteWorktreeEntry {
                project_id: project.id.clone(),
                project_name: project.name.clone(),
                worktree_id: worktree.id.clone(),
                worktree_name: worktree.name.clone(),
            })
            .collect()
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
        let active_terminal_context = self.active_terminal_context();
        let active_terminal_label = active_terminal_context
            .as_ref()
            .map(|context| context.terminal_name.clone())
            .unwrap_or_else(|| String::from("No active terminal"));
        let active_terminal_id = active_terminal_context
            .as_ref()
            .map(|context| context.terminal_id.clone());
        let active_terminal_pinned = active_terminal_id
            .as_ref()
            .is_some_and(|terminal_id| self.is_terminal_pinned(terminal_id));

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
                CommandPaletteAction::PinFocusedItem,
                format!("Pin {active_terminal_label}"),
                "Pin the active terminal into the pinned section",
                "pin favorite focused active terminal shortcut",
            ),
            command_palette_entry(
                CommandPaletteAction::UnpinFocusedItem,
                if active_terminal_pinned {
                    format!("Unpin {active_terminal_label}")
                } else {
                    String::from("Unpin Focused Terminal")
                },
                "Remove the active terminal from the pinned section",
                "unpin remove favorite focused active terminal shortcut",
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
                CommandPaletteAction::AddWorktreeToProject,
                "Add Worktree",
                "Choose a project, then create a new worktree",
                "worktree branch create project add",
            ),
            command_palette_entry(
                CommandPaletteAction::RemoveProject,
                "Remove Project",
                "Choose a project, then remove it from the sidebar",
                "project remove sidebar delete repository repo",
            ),
            command_palette_entry(
                CommandPaletteAction::ExpandAllProjects,
                "Expand All Projects",
                "Expand every project and worktree in the sidebar",
                "expand all projects worktrees sidebar tree unfold open",
            ),
            command_palette_entry(
                CommandPaletteAction::CollapseAllProjects,
                "Collapse All Projects",
                "Collapse every project and worktree in the sidebar",
                "collapse all projects worktrees sidebar tree fold close",
            ),
            command_palette_entry(
                CommandPaletteAction::DeleteWorktreeFromProject,
                "Delete Worktree",
                "Choose a project, then remove one of its worktrees",
                "worktree delete remove project",
            ),
            command_palette_entry(
                CommandPaletteAction::RescanAllProjects,
                "Rescan All Projects",
                "Refresh every project's worktree list",
                "rescan refresh all projects worktree git",
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
                "Skip project picker and use the active project",
                "worktree branch create project active",
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
        let Some(query) = self.normalized_filter_query() else {
            return (0..self.persisted.projects.len()).collect();
        };

        self.persisted
            .projects
            .iter()
            .enumerate()
            .filter_map(|(index, project)| {
                Self::project_matches_filter(project, &query).then_some(index)
            })
            .collect()
    }

    pub(crate) fn normalized_filter_query(&self) -> Option<String> {
        let query = self.filter_query.trim().to_lowercase();
        if query.is_empty() { None } else { Some(query) }
    }

    pub(crate) fn project_matches_filter(project: &ProjectRecord, query: &str) -> bool {
        project.name.to_lowercase().contains(query)
            || project
                .worktrees
                .iter()
                .any(|worktree| Self::worktree_matches_filter(worktree, query))
    }

    pub(crate) fn worktree_matches_filter(worktree: &WorktreeRecord, query: &str) -> bool {
        format!(
            "{} {}",
            worktree.name.to_lowercase(),
            worktree.path.to_lowercase()
        )
        .contains(query)
            || worktree
                .terminals
                .iter()
                .any(|terminal| terminal.name.to_lowercase().contains(query))
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

    pub(crate) fn start_sidebar_drag(&mut self, item: SidebarDragItem) -> Result<(), String> {
        if self.normalized_filter_query().is_some() {
            return Err(String::from(
                "Clear the filter before reordering the sidebar",
            ));
        }

        self.worktree_context_menu = None;
        self.project_context_menu = None;
        self.sidebar_drag = Some(SidebarDragState { item, hover: None });
        Ok(())
    }

    pub(crate) fn set_sidebar_drag_hover(&mut self, target: SidebarDragItem) {
        let Some(drag) = self.sidebar_drag.as_mut() else {
            return;
        };

        if !sidebar_drag_target_allowed(&drag.item, &target) || drag.item == target {
            drag.hover = None;
            return;
        }

        drag.hover = Some(target);
    }

    pub(crate) fn clear_sidebar_drag_hover(&mut self, target: &SidebarDragItem) {
        let Some(drag) = self.sidebar_drag.as_mut() else {
            return;
        };

        if drag.hover.as_ref() == Some(target) {
            drag.hover = None;
        }
    }

    pub(crate) fn cancel_sidebar_drag(&mut self) {
        self.sidebar_drag = None;
    }

    pub(crate) fn finish_sidebar_drag(&mut self) -> Option<&'static str> {
        let drag = self.sidebar_drag.take()?;
        let target = drag.hover?;

        if drag.item == target {
            return None;
        }

        let changed = match (drag.item, target) {
            (
                SidebarDragItem::Project {
                    project_id: dragged_id,
                },
                SidebarDragItem::Project {
                    project_id: target_id,
                },
            ) => self.reorder_project(&dragged_id, &target_id),
            (
                SidebarDragItem::Worktree {
                    project_id,
                    worktree_id: dragged_id,
                },
                SidebarDragItem::Worktree {
                    project_id: target_project_id,
                    worktree_id: target_id,
                },
            ) if project_id == target_project_id => {
                self.reorder_worktree(&project_id, &dragged_id, &target_id)
            }
            (
                SidebarDragItem::Terminal {
                    project_id,
                    worktree_id,
                    terminal_id: dragged_id,
                },
                SidebarDragItem::Terminal {
                    project_id: target_project_id,
                    worktree_id: target_worktree_id,
                    terminal_id: target_id,
                },
            ) if project_id == target_project_id && worktree_id == target_worktree_id => {
                self.reorder_terminal(&project_id, &worktree_id, &dragged_id, &target_id)
            }
            _ => false,
        };

        changed.then_some("Sidebar order updated")
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

    pub(crate) fn active_terminal_surface_ptr(&self) -> Option<usize> {
        let active_terminal_id = self.active_terminal_id()?;
        self.runtimes
            .get(&active_terminal_id)
            .and_then(RuntimeSession::active_surface_ptr)
    }

    pub(crate) fn active_terminal_host_view(&self) -> Option<usize> {
        let active_terminal_id = self.active_terminal_id()?;
        self.runtimes
            .get(&active_terminal_id)
            .and_then(RuntimeSession::active_host_view)
    }

    pub(crate) fn terminal_search_is_open(&self) -> bool {
        self.terminal_search.is_some()
    }

    pub(crate) fn take_terminal_search_focus_request(&mut self) -> bool {
        let Some(search) = self.terminal_search.as_mut() else {
            return false;
        };

        let requested = search.focus_requested;
        search.focus_requested = false;
        requested
    }

    pub(crate) fn open_terminal_search(
        &mut self,
        terminal_id: String,
        surface_ptr: usize,
        needle: String,
    ) -> bool {
        let mut search = match self.terminal_search.take() {
            Some(existing)
                if existing.terminal_id == terminal_id && existing.surface_ptr == surface_ptr =>
            {
                existing
            }
            _ => TerminalSearchState {
                terminal_id,
                surface_ptr,
                query: String::new(),
                total: None,
                selected: None,
                pending_apply: false,
                pending_deadline: None,
                focus_requested: false,
            },
        };

        search.focus_requested = true;
        if !needle.is_empty() && search.query != needle {
            search.query = needle;
            search.total = None;
            search.selected = None;
            search.pending_apply = true;
            search.pending_deadline = terminal_search_deadline(&search.query);
        }

        let should_apply_now = search.pending_apply && search.pending_deadline.is_none();
        self.terminal_search = Some(search);

        if should_apply_now {
            let _ = self.apply_terminal_search_if_due(Instant::now());
        }

        true
    }

    pub(crate) fn apply_terminal_search_if_due(&mut self, now: Instant) -> bool {
        let (terminal_id, surface_ptr, query) = {
            let Some(search) = self.terminal_search.as_mut() else {
                return false;
            };
            if !search.pending_apply {
                return false;
            }
            if let Some(deadline) = search.pending_deadline
                && now < deadline
            {
                return false;
            }

            search.pending_apply = false;
            search.pending_deadline = None;
            (
                search.terminal_id.clone(),
                search.surface_ptr,
                search.query.clone(),
            )
        };

        let action = format!("search:{query}");
        self.perform_surface_binding_action(&terminal_id, surface_ptr, &action)
    }

    pub(crate) fn navigate_terminal_search(&mut self, previous: bool) -> bool {
        let Some(search) = self.terminal_search.as_ref() else {
            return false;
        };
        let terminal_id = search.terminal_id.clone();
        let surface_ptr = search.surface_ptr;

        let action = if previous {
            "navigate_search:previous"
        } else {
            "navigate_search:next"
        };
        self.perform_surface_binding_action(&terminal_id, surface_ptr, action)
    }

    pub(crate) fn close_terminal_search(&mut self, notify_ghostty: bool) -> bool {
        let Some(search) = self.terminal_search.take() else {
            return false;
        };

        if notify_ghostty {
            let _ = self.perform_surface_binding_action(
                &search.terminal_id,
                search.surface_ptr,
                "end_search",
            );
        }

        true
    }

    pub(crate) fn reconcile_terminal_search(&mut self) -> bool {
        let Some(search) = self.terminal_search.as_ref() else {
            return false;
        };

        let runtime_exists = self.runtimes.contains_key(&search.terminal_id);
        let active_surface_ptr = self
            .runtimes
            .get(&search.terminal_id)
            .and_then(RuntimeSession::active_surface_ptr);
        let should_close = self.active_browser().is_some()
            || self.modal_open()
            || self.active_terminal_id().as_deref() != Some(search.terminal_id.as_str())
            || active_surface_ptr != Some(search.surface_ptr);

        if should_close {
            return self.close_terminal_search(runtime_exists);
        }

        false
    }

    pub(crate) fn sync_terminal_search_total(
        &mut self,
        terminal_id: &str,
        surface_ptr: usize,
        total: Option<usize>,
    ) -> bool {
        let Some(search) = self.terminal_search.as_mut() else {
            return false;
        };
        if search.terminal_id != terminal_id || search.surface_ptr != surface_ptr {
            return false;
        }
        if search.total == total {
            return false;
        }

        search.total = total;
        true
    }

    pub(crate) fn sync_terminal_search_selected(
        &mut self,
        terminal_id: &str,
        surface_ptr: usize,
        selected: Option<usize>,
    ) -> bool {
        let Some(search) = self.terminal_search.as_mut() else {
            return false;
        };
        if search.terminal_id != terminal_id || search.surface_ptr != surface_ptr {
            return false;
        }
        if search.selected == selected {
            return false;
        }

        search.selected = selected;
        true
    }

    pub(crate) fn perform_surface_binding_action(
        &mut self,
        terminal_id: &str,
        surface_ptr: usize,
        action: &str,
    ) -> bool {
        let Some(runtime) = self.runtimes.get_mut(terminal_id) else {
            return false;
        };
        let Some(ghostty) = runtime.ghostty_for_surface_mut(surface_ptr) else {
            return false;
        };

        let performed = ghostty.binding_action(action);
        ghostty.refresh();
        ghostty.force_tick();
        performed
    }

    pub(crate) fn sync_runtime_views(&mut self) {
        let _ = self.reconcile_terminal_search();
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

            // When a modal is open, ensure the winit content view holds macOS
            // first-responder status.  Native child views (Ghostty terminals,
            // WKWebViews) may have claimed it; without this, keyDown: events
            // are dispatched to the native view instead of winit/Iced, making
            // the modal text input unresponsive.
            if modal_open {
                parent_view_reclaim_focus(parent_ns_view);
            }
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

    pub(crate) fn create_diff_pane_runtime(
        &self,
        worktree_path: &str,
    ) -> Result<DiffPaneRuntime, String> {
        let Some(parent_ns_view) = self.host_ns_view else {
            return Err(String::from("failed to resolve host NSView"));
        };

        let Some(webview) = WebView::new_hosted(parent_ns_view) else {
            return Err(String::from("failed to create diff webview"));
        };

        // Diff panes are read-only; prevent the WKWebView from stealing
        // keyboard focus away from the terminal.
        webview.set_keyboard_enabled(false);

        Ok(DiffPaneRuntime::new(
            create_id("diff"),
            webview,
            worktree_path.to_string(),
        ))
    }

    pub(crate) fn schedule_diff_refresh(&mut self, terminal_id: &str, now: Instant) -> bool {
        let should_refresh = self
            .runtimes
            .get(terminal_id)
            .is_some_and(RuntimeSession::has_diff_view);
        if !should_refresh {
            return false;
        }

        self.diff_refresh_deadlines
            .insert(terminal_id.to_string(), now + DIFF_REFRESH_DEBOUNCE);
        true
    }

    pub(crate) fn clear_diff_refresh_state(&mut self, terminal_id: &str) {
        self.diff_refresh_deadlines.remove(terminal_id);
        self.diff_refresh_in_flight.remove(terminal_id);
    }

    pub(crate) fn begin_diff_refresh(&mut self, terminal_id: &str) -> Option<String> {
        if !self
            .runtimes
            .get(terminal_id)
            .is_some_and(RuntimeSession::has_diff_view)
        {
            self.clear_diff_refresh_state(terminal_id);
            return None;
        }

        if self.diff_refresh_in_flight.contains(terminal_id) {
            return None;
        }

        let worktree_path = self.runtimes.get(terminal_id)?.diff_worktree_path()?;
        self.diff_refresh_deadlines.remove(terminal_id);
        self.diff_refresh_in_flight.insert(terminal_id.to_string());
        Some(worktree_path)
    }

    pub(crate) fn finish_diff_refresh(&mut self, terminal_id: &str) {
        self.diff_refresh_in_flight.remove(terminal_id);
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
                    GhosttyRuntimeAction::StartSearch {
                        surface_ptr,
                        needle,
                    } => {
                        if self.active_browser().is_some()
                            || self.active_terminal_id().as_deref() != Some(terminal_id.as_str())
                            || self.active_terminal_surface_ptr() != Some(surface_ptr)
                        {
                            false
                        } else {
                            self.open_terminal_search(terminal_id.clone(), surface_ptr, needle)
                        }
                    }
                    GhosttyRuntimeAction::EndSearch { surface_ptr } => {
                        let should_close = self.terminal_search.as_ref().is_some_and(|search| {
                            search.terminal_id == terminal_id && search.surface_ptr == surface_ptr
                        });
                        if should_close {
                            self.close_terminal_search(false)
                        } else {
                            false
                        }
                    }
                    GhosttyRuntimeAction::SearchTotal { surface_ptr, total } => {
                        self.sync_terminal_search_total(&terminal_id, surface_ptr, total)
                    }
                    GhosttyRuntimeAction::SearchSelected {
                        surface_ptr,
                        selected,
                    } => self.sync_terminal_search_selected(&terminal_id, surface_ptr, selected),
                };

                changed = changed || action_changed;
            }
        }

        changed
    }

    pub(crate) fn process_diff_pane_actions(&mut self) -> bool {
        let mut changed = false;
        let terminal_ids: Vec<String> = self.runtimes.keys().cloned().collect();

        for terminal_id in terminal_ids {
            let actions = if let Some(runtime) = self.runtimes.get_mut(&terminal_id) {
                runtime.drain_diff_actions()
            } else {
                continue;
            };

            for RuntimeDiffAction { pane_id, action } in actions {
                let action_changed = match action {
                    crate::app::diff_runtime::DiffPaneAction::ToggleSplitZoom => self
                        .runtimes
                        .get_mut(&terminal_id)
                        .is_some_and(|runtime| runtime.toggle_split_zoom_for_pane(&pane_id)),
                    crate::app::diff_runtime::DiffPaneAction::ToggleDiffView => {
                        let changed = self
                            .runtimes
                            .get_mut(&terminal_id)
                            .is_some_and(RuntimeSession::close_diff_view);
                        if changed {
                            self.clear_diff_refresh_state(&terminal_id);
                            // If another input surface opened while the diff-close action was
                            // waiting to be processed (for example Cmd+P opening the command
                            // palette), do not steal first responder back to the terminal.
                            if !self.modal_open()
                                && let Some(host_view) = self.active_terminal_host_view()
                            {
                                host_view_focus_terminal(host_view);
                            }
                        }
                        changed
                    }
                };
                changed = changed || action_changed;
            }
        }

        changed
    }

    pub(crate) fn remove_runtime(&mut self, terminal_id: &str) {
        if self
            .terminal_search
            .as_ref()
            .is_some_and(|search| search.terminal_id == terminal_id)
        {
            self.terminal_search = None;
        }
        self.runtimes.remove(terminal_id);
        self.branch_by_terminal.remove(terminal_id);
        self.clear_diff_refresh_state(terminal_id);
        self.remove_terminal_status(terminal_id);
    }

    pub(crate) fn modal_open(&self) -> bool {
        self.command_palette_open
            || self.quick_open_open
            || self.add_worktree_project_picker_open
            || self.remove_project_picker_open
            || self.delete_worktree_project_picker_open
            || self.delete_worktree_picker.is_some()
            || self.preferences_open
            || self.rename_dialog.is_some()
            || self.add_worktree_dialog.is_some()
            || self.worktree_context_menu.is_some()
            || self.project_context_menu.is_some()
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

    pub(crate) fn start_rename_pinned_terminal(&mut self, terminal_id: &str) {
        let Some(pin) = self
            .persisted
            .pinned_terminals
            .iter()
            .find(|pin| pin.terminal_id == terminal_id)
        else {
            return;
        };

        self.rename_dialog = Some(RenameDialog {
            target: RenameTarget::PinnedTerminal {
                terminal_id: terminal_id.to_string(),
            },
            value: if !pin.manual_alias || pin.alias.trim().is_empty() {
                self.terminal_name_by_id(terminal_id)
                    .unwrap_or_else(|| pin.alias.clone())
            } else {
                pin.alias.clone()
            },
        });
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
            RenameTarget::PinnedTerminal { terminal_id } => {
                if let Some(pin) = self
                    .persisted
                    .pinned_terminals
                    .iter_mut()
                    .find(|pin| pin.terminal_id == terminal_id)
                {
                    pin.alias = value.to_string();
                    pin.manual_alias = true;
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

impl App {
    fn normalize_path_key(path: &str) -> &str {
        path.trim_end_matches(['/', '\\'])
    }

    pub(crate) fn is_main_worktree(project: &ProjectRecord, worktree: &WorktreeRecord) -> bool {
        let Some(git_folder) = project.git_folder_path.as_deref() else {
            return false;
        };

        Self::normalize_path_key(git_folder) == Self::normalize_path_key(&worktree.path)
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

fn sidebar_drag_target_allowed(item: &SidebarDragItem, target: &SidebarDragItem) -> bool {
    match (item, target) {
        (SidebarDragItem::Project { .. }, SidebarDragItem::Project { .. }) => true,
        (
            SidebarDragItem::Worktree {
                project_id: item_project_id,
                ..
            },
            SidebarDragItem::Worktree {
                project_id: target_project_id,
                ..
            },
        ) => item_project_id == target_project_id,
        (
            SidebarDragItem::Terminal {
                project_id: item_project_id,
                worktree_id: item_worktree_id,
                ..
            },
            SidebarDragItem::Terminal {
                project_id: target_project_id,
                worktree_id: target_worktree_id,
                ..
            },
        ) => item_project_id == target_project_id && item_worktree_id == target_worktree_id,
        _ => false,
    }
}

fn move_vec_item_by<T>(
    values: &mut Vec<T>,
    is_dragged: impl Fn(&T) -> bool,
    is_target: impl Fn(&T) -> bool,
) -> bool {
    let Some(from_index) = values.iter().position(is_dragged) else {
        return false;
    };
    let Some(target_index) = values.iter().position(is_target) else {
        return false;
    };

    if from_index == target_index {
        return false;
    }

    let item = values.remove(from_index);
    values.insert(target_index, item);
    true
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

fn terminal_search_deadline(query: &str) -> Option<Instant> {
    if query.is_empty() || query.chars().count() >= 3 {
        None
    } else {
        Some(Instant::now() + TERMINAL_SEARCH_DEBOUNCE)
    }
}
