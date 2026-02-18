use crate::ghostty_embed::{
    GhosttyEmbed, host_view_free, host_view_new, host_view_set_frame, host_view_set_hidden,
    ns_view_ptr,
};
use iced::{Point, Subscription, Task, keyboard, window};

pub(crate) const SESSION_COUNT: usize = 2;
pub(crate) const SIDEBAR_WIDTH_LOGICAL: f32 = 220.0;

pub(crate) struct Session {
    pub(crate) label: String,
    pub(crate) host_view: usize,
    pub(crate) ghostty: Option<GhosttyEmbed>,
}

impl Session {
    fn new(label: String, host_view: usize, ghostty: GhosttyEmbed) -> Self {
        Self {
            label,
            host_view,
            ghostty: Some(ghostty),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.ghostty.take();
        host_view_free(self.host_view);
    }
}

pub(crate) struct App {
    pub(crate) title: String,
    pub(crate) window_id: Option<window::Id>,
    pub(crate) window_size: iced::Size,
    pub(crate) window_scale_factor: f32,
    pub(crate) cursor_position_logical: Option<Point>,
    pub(crate) keyboard_modifiers: keyboard::Modifiers,
    pub(crate) host_ns_view: Option<usize>,
    pub(crate) sessions: Vec<Session>,
    pub(crate) active_session: usize,
    pub(crate) ghostty_attempted: bool,
    pub(crate) ghostty_status: String,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    WindowLocated(Option<window::Id>),
    HostViewResolved(Option<usize>),
    WindowSizeResolved(iced::Size),
    WindowScaleResolved(f32),
    WindowEvent(window::Id, window::Event),
    GhosttyTick,
    Keyboard(iced::keyboard::Event),
    Mouse(iced::mouse::Event),
    SelectSession(usize),
}

impl App {
    pub(crate) fn boot() -> (Self, Task<Message>) {
        let app = Self {
            title: String::from("Iced + Ghostty Tabs"),
            window_id: None,
            window_size: iced::Size::new(1280.0, 820.0),
            window_scale_factor: 1.0,
            cursor_position_logical: None,
            keyboard_modifiers: keyboard::Modifiers::default(),
            host_ns_view: None,
            sessions: Vec::new(),
            active_session: 0,
            ghostty_attempted: false,
            ghostty_status: String::from("Starting Ghostty sessions..."),
        };

        (app, window::latest().map(Message::WindowLocated))
    }

    pub(crate) fn title(&self) -> String {
        self.title.clone()
    }

    pub(crate) fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions =
            vec![window::events().map(|(id, event)| Message::WindowEvent(id, event))];

        if !self.sessions.is_empty() {
            subscriptions.push(window::frames().map(|_| Message::GhosttyTick));
            subscriptions.push(iced::event::listen_with(
                |event, _status, _window| match event {
                    iced::Event::Keyboard(event) => Some(Message::Keyboard(event)),
                    iced::Event::Mouse(event) => Some(Message::Mouse(event)),
                    _ => None,
                },
            ));
        }

        Subscription::batch(subscriptions)
    }

    pub(crate) fn window_size_px(&self) -> (u32, u32) {
        let scale = self.window_scale_factor.max(0.1);
        let width_px = (self.window_size.width * scale).max(1.0).round() as u32;
        let height_px = (self.window_size.height * scale).max(1.0).round() as u32;
        (width_px, height_px)
    }

    pub(crate) fn terminal_frame_px(&self) -> (u32, u32, u32, u32) {
        let (window_width_px, window_height_px) = self.window_size_px();
        let scale = self.window_scale_factor.max(0.1);
        let mut sidebar_width_px = (SIDEBAR_WIDTH_LOGICAL * scale).max(0.0).round() as u32;
        sidebar_width_px = sidebar_width_px.min(window_width_px.saturating_sub(1));

        let terminal_width_px = window_width_px.saturating_sub(sidebar_width_px).max(1);
        let terminal_height_px = window_height_px.max(1);

        (sidebar_width_px, 0, terminal_width_px, terminal_height_px)
    }

    pub(crate) fn terminal_frame_logical(&self) -> (f32, f32, f32, f32) {
        let mut sidebar_width = SIDEBAR_WIDTH_LOGICAL.max(0.0);
        sidebar_width = sidebar_width.min(self.window_size.width.max(1.0) - 1.0);

        let terminal_width = (self.window_size.width - sidebar_width).max(1.0);
        let terminal_height = self.window_size.height.max(1.0);

        (sidebar_width, 0.0, terminal_width, terminal_height)
    }

    pub(crate) fn terminal_local_px_from_position(&self, position: Point) -> Option<(f64, f64)> {
        let (x, y, width, height) = self.terminal_frame_logical();
        let within_x = position.x >= x && position.x < x + width;
        let within_y = position.y >= y && position.y < y + height;

        if !(within_x && within_y) {
            return None;
        }

        // Ghostty embedded runtime converts host coordinates to pixels internally
        // using the content scale, so we must provide unscaled logical coordinates.
        let local_x = (position.x - x) as f64;
        let local_y = (position.y - y) as f64;

        Some((local_x, local_y))
    }

    pub(crate) fn active_ghostty_mut(&mut self) -> Option<&mut GhosttyEmbed> {
        self.sessions
            .get_mut(self.active_session)
            .and_then(|session| session.ghostty.as_mut())
    }

    pub(crate) fn try_init_sessions(&mut self) {
        if !self.sessions.is_empty() || self.ghostty_attempted {
            return;
        }

        let Some(parent_ns_view) = self.host_ns_view else {
            return;
        };

        self.ghostty_attempted = true;
        let (_, _, width_px, height_px) = self.terminal_frame_px();
        let scale = self.window_scale_factor.max(0.1) as f64;

        let mut created_sessions = Vec::with_capacity(SESSION_COUNT);
        for session_idx in 0..SESSION_COUNT {
            let Some(host_view) = host_view_new(parent_ns_view) else {
                self.ghostty_status = String::from("Failed to create terminal host view");
                return;
            };

            let ghostty = match GhosttyEmbed::new(host_view, width_px, height_px, scale) {
                Ok(ghostty) => ghostty,
                Err(err) => {
                    host_view_free(host_view);
                    self.ghostty_status =
                        format!("Failed to initialize terminal {}: {err}", session_idx + 1);
                    return;
                }
            };

            created_sessions.push(Session::new(
                format!("Terminal {}", session_idx + 1),
                host_view,
                ghostty,
            ));
        }

        self.sessions = created_sessions;
        self.ghostty_status = String::from("Ready");
        self.sync_session_views();
        if let Some(active) = self.active_ghostty_mut() {
            active.refresh();
            active.force_tick();
        }
    }

    pub(crate) fn sync_session_views(&mut self) {
        if self.sessions.is_empty() {
            return;
        }

        let (x_logical, y_logical, width_logical, height_logical) = self.terminal_frame_logical();
        let (_, _, width_px, height_px) = self.terminal_frame_px();
        let scale = self.window_scale_factor.max(0.1) as f64;

        for (index, session) in self.sessions.iter_mut().enumerate() {
            let active = index == self.active_session;
            host_view_set_frame(
                session.host_view,
                x_logical as f64,
                y_logical as f64,
                width_logical as f64,
                height_logical as f64,
            );
            host_view_set_hidden(session.host_view, !active);

            if let Some(ghostty) = session.ghostty.as_mut() {
                ghostty.set_scale_factor(scale);
                ghostty.set_size(width_px, height_px);
                ghostty.set_focus(active);
                if active {
                    ghostty.refresh();
                }
            }
        }
    }

    pub(crate) fn app_ns_view(window_id: window::Id) -> Task<Message> {
        Task::batch([
            window::run(window_id, ns_view_ptr).map(Message::HostViewResolved),
            window::size(window_id).map(Message::WindowSizeResolved),
            window::scale_factor(window_id).map(Message::WindowScaleResolved),
        ])
    }
}
