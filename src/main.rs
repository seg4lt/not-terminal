mod ghostty_embed;

use ghostty_embed::{
    GhosttyEmbed, host_view_free, host_view_new, host_view_set_frame, host_view_set_hidden,
    ns_view_ptr,
};
use iced::widget::{button, column, container, row, text};
use iced::{Length, Subscription, Task, window};

const SESSION_COUNT: usize = 2;
const SIDEBAR_WIDTH_LOGICAL: f32 = 220.0;

struct Session {
    label: String,
    host_view: usize,
    ghostty: Option<GhosttyEmbed>,
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

struct App {
    title: String,
    window_id: Option<window::Id>,
    window_size: iced::Size,
    window_scale_factor: f32,
    host_ns_view: Option<usize>,
    sessions: Vec<Session>,
    active_session: usize,
    ghostty_attempted: bool,
    ghostty_status: String,
}

#[derive(Debug, Clone)]
enum Message {
    WindowLocated(Option<window::Id>),
    HostViewResolved(Option<usize>),
    WindowSizeResolved(iced::Size),
    WindowScaleResolved(f32),
    WindowEvent(window::Id, window::Event),
    GhosttyTick,
    Keyboard(iced::keyboard::Event),
    SelectSession(usize),
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        let app = Self {
            title: String::from("Iced + Ghostty Tabs"),
            window_id: None,
            window_size: iced::Size::new(1280.0, 820.0),
            window_scale_factor: 1.0,
            host_ns_view: None,
            sessions: Vec::new(),
            active_session: 0,
            ghostty_attempted: false,
            ghostty_status: String::from("Starting Ghostty sessions..."),
        };

        (app, window::latest().map(Message::WindowLocated))
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions =
            vec![window::events().map(|(id, event)| Message::WindowEvent(id, event))];

        if !self.sessions.is_empty() {
            subscriptions.push(window::frames().map(|_| Message::GhosttyTick));
            subscriptions.push(iced::event::listen_with(
                |event, _status, _window| match event {
                    iced::Event::Keyboard(event) => Some(Message::Keyboard(event)),
                    _ => None,
                },
            ));
        }

        Subscription::batch(subscriptions)
    }

    fn window_size_px(&self) -> (u32, u32) {
        let scale = self.window_scale_factor.max(0.1);
        let width_px = (self.window_size.width * scale).max(1.0).round() as u32;
        let height_px = (self.window_size.height * scale).max(1.0).round() as u32;
        (width_px, height_px)
    }

    fn terminal_frame_px(&self) -> (u32, u32, u32, u32) {
        let (window_width_px, window_height_px) = self.window_size_px();
        let scale = self.window_scale_factor.max(0.1);
        let mut sidebar_width_px = (SIDEBAR_WIDTH_LOGICAL * scale).max(0.0).round() as u32;
        sidebar_width_px = sidebar_width_px.min(window_width_px.saturating_sub(1));

        let terminal_width_px = window_width_px.saturating_sub(sidebar_width_px).max(1);
        let terminal_height_px = window_height_px.max(1);

        (sidebar_width_px, 0, terminal_width_px, terminal_height_px)
    }

    fn active_ghostty_mut(&mut self) -> Option<&mut GhosttyEmbed> {
        self.sessions
            .get_mut(self.active_session)
            .and_then(|session| session.ghostty.as_mut())
    }

    fn try_init_sessions(&mut self) {
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

    fn sync_session_views(&mut self) {
        if self.sessions.is_empty() {
            return;
        }

        let (x_px, y_px, width_px, height_px) = self.terminal_frame_px();
        let scale = self.window_scale_factor.max(0.1) as f64;

        for (index, session) in self.sessions.iter_mut().enumerate() {
            let active = index == self.active_session;
            host_view_set_frame(
                session.host_view,
                x_px as f64,
                y_px as f64,
                width_px as f64,
                height_px as f64,
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
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::WindowLocated(window_id) => {
            let Some(window_id) = window_id else {
                app.ghostty_status = String::from("No window available for Ghostty embedding");
                return Task::none();
            };

            app.window_id = Some(window_id);

            Task::batch([
                window::run(window_id, ns_view_ptr).map(Message::HostViewResolved),
                window::size(window_id).map(Message::WindowSizeResolved),
                window::scale_factor(window_id).map(Message::WindowScaleResolved),
            ])
        }
        Message::HostViewResolved(ns_view) => {
            app.host_ns_view = ns_view;
            if ns_view.is_none() {
                app.ghostty_status = String::from("Failed to resolve AppKit NSView");
            }
            app.try_init_sessions();
            Task::none()
        }
        Message::WindowSizeResolved(size) => {
            app.window_size = size;
            if app.sessions.is_empty() {
                app.try_init_sessions();
            } else {
                app.sync_session_views();
            }
            Task::none()
        }
        Message::WindowScaleResolved(scale) => {
            app.window_scale_factor = scale;
            if app.sessions.is_empty() {
                app.try_init_sessions();
            } else {
                app.sync_session_views();
            }
            Task::none()
        }
        Message::WindowEvent(window_id, event) => {
            if app.window_id.is_none_or(|current| current == window_id) {
                match event {
                    window::Event::Resized(size) => {
                        app.window_size = size;
                        app.sync_session_views();
                    }
                    window::Event::Rescaled(scale) => {
                        app.window_scale_factor = scale;
                        app.sync_session_views();
                    }
                    _ => {}
                }
            }
            Task::none()
        }
        Message::GhosttyTick => {
            for session in &mut app.sessions {
                if let Some(ghostty) = session.ghostty.as_mut() {
                    ghostty.tick_if_needed();
                }
            }
            Task::none()
        }
        Message::Keyboard(event) => {
            if let Some(ghostty) = app.active_ghostty_mut() {
                if ghostty.handle_keyboard_event(&event) {
                    ghostty.refresh();
                    ghostty.force_tick();
                }
            }
            Task::none()
        }
        Message::SelectSession(index) => {
            if index < SESSION_COUNT {
                app.active_session = index;
                app.sync_session_views();
                if let Some(ghostty) = app.active_ghostty_mut() {
                    ghostty.refresh();
                    ghostty.force_tick();
                }
            }
            Task::none()
        }
    }
}

fn view(app: &App) -> iced::Element<'_, Message> {
    let mut tabs = column![text("Terminals").size(18)].spacing(8);

    for idx in 0..SESSION_COUNT {
        let base = app
            .sessions
            .get(idx)
            .map(|session| session.label.as_str())
            .unwrap_or_else(|| if idx == 0 { "Terminal 1" } else { "Terminal 2" });
        let mut label = String::from(base);
        if idx == app.active_session {
            label = format!("> {label}");
        }
        tabs = tabs.push(
            button(text(label))
                .width(Length::Fill)
                .on_press(Message::SelectSession(idx)),
        );
    }

    tabs = tabs.push(text(&app.ghostty_status).size(12));

    let sidebar = container(tabs)
        .padding(12)
        .width(Length::Fixed(SIDEBAR_WIDTH_LOGICAL))
        .height(Length::Fill);

    let terminal_area = container(text("")).width(Length::Fill).height(Length::Fill);

    container(
        row![sidebar, terminal_area]
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn main() -> iced::Result {
    iced::application(App::boot, update, view)
        .title(App::title)
        .subscription(App::subscription)
        .window_size((1280.0, 820.0))
        .run()
}
