mod ghostty_embed;

use ghostty_embed::{GhosttyEmbed, ns_view_ptr};
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Length, Subscription, Task, window};
use iced_term::settings::{BackendSettings, Settings as TerminalSettings};
use iced_term::{Command as TerminalCommand, Event as TerminalEvent, Terminal, TerminalView};

struct App {
    title: String,
    counter: i32,
    terminal: Terminal,
    window_id: Option<window::Id>,
    window_size: iced::Size,
    window_scale_factor: f32,
    host_ns_view: Option<usize>,
    ghostty: Option<GhosttyEmbed>,
    ghostty_attempted: bool,
    ghostty_status: String,
}

#[derive(Debug, Clone)]
enum Message {
    Increment,
    Decrement,
    Terminal(TerminalEvent),
    WindowLocated(Option<window::Id>),
    HostViewResolved(Option<usize>),
    WindowSizeResolved(iced::Size),
    WindowScaleResolved(f32),
    WindowEvent(window::Id, window::Event),
    GhosttyTick,
    Keyboard(iced::keyboard::Event),
    RetryGhosttyInit,
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| String::from("/bin/zsh"));

        let terminal_settings = TerminalSettings {
            backend: BackendSettings {
                program: shell,
                ..Default::default()
            },
            ..Default::default()
        };

        let terminal =
            Terminal::new(0, terminal_settings).expect("failed to initialize terminal backend");
        let terminal_id = terminal.widget_id().clone();

        let app = Self {
            title: String::from("Iced + Embedded Terminal"),
            counter: 0,
            terminal,
            window_id: None,
            window_size: iced::Size::new(1200.0, 780.0),
            window_scale_factor: 1.0,
            host_ns_view: None,
            ghostty: None,
            ghostty_attempted: false,
            ghostty_status: String::from("Detecting host NSView for Ghostty embedding..."),
        };

        (
            app,
            Task::batch([
                TerminalView::focus::<Message>(terminal_id),
                window::latest().map(Message::WindowLocated),
            ]),
        )
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![self.terminal.subscription().map(Message::Terminal)];
        subscriptions.push(window::events().map(|(id, event)| Message::WindowEvent(id, event)));

        if self.ghostty.is_some() {
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

    fn try_init_ghostty(&mut self) {
        if self.ghostty.is_some() || self.ghostty_attempted {
            return;
        }

        let Some(ns_view) = self.host_ns_view else {
            return;
        };

        self.ghostty_attempted = true;

        let (width_px, height_px) = self.window_size_px();
        let scale = self.window_scale_factor.max(0.1) as f64;

        match GhosttyEmbed::new(ns_view, width_px, height_px, scale) {
            Ok(mut embed) => {
                embed.set_focus(true);
                embed.force_tick();
                self.ghostty_status = String::from("Ghostty embedded (static link)");
                self.title = String::from("Iced + Ghostty (Embedded Spike)");
                self.ghostty = Some(embed);
            }
            Err(err) => {
                self.ghostty_status = format!("Ghostty embed failed: {err}");
            }
        }
    }

    fn window_size_px(&self) -> (u32, u32) {
        let scale = self.window_scale_factor.max(0.1);
        let width_px = (self.window_size.width * scale).max(1.0).round() as u32;
        let height_px = (self.window_size.height * scale).max(1.0).round() as u32;
        (width_px, height_px)
    }
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Increment => {
            app.counter += 1;
            Task::none()
        }
        Message::Decrement => {
            app.counter -= 1;
            Task::none()
        }
        Message::Terminal(TerminalEvent::BackendCall(_, cmd)) => {
            match app.terminal.handle(TerminalCommand::ProxyToBackend(cmd)) {
                iced_term::actions::Action::Shutdown => {
                    return iced::exit();
                }
                iced_term::actions::Action::ChangeTitle(new_title) => {
                    app.title = new_title;
                }
                iced_term::actions::Action::Ignore => {}
            }

            Task::none()
        }
        Message::WindowLocated(window_id) => {
            let Some(window_id) = window_id else {
                app.ghostty_status =
                    String::from("No window handle available for Ghostty embedding");
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
                app.ghostty_status =
                    String::from("Failed to resolve AppKit NSView from iced window");
            }
            app.try_init_ghostty();
            Task::none()
        }
        Message::WindowSizeResolved(size) => {
            app.window_size = size;
            let (width_px, height_px) = app.window_size_px();
            if let Some(ghostty) = app.ghostty.as_mut() {
                ghostty.set_size(width_px, height_px);
                ghostty.refresh();
                ghostty.force_tick();
            } else {
                app.try_init_ghostty();
            }
            Task::none()
        }
        Message::WindowScaleResolved(scale) => {
            app.window_scale_factor = scale;
            let (width_px, height_px) = app.window_size_px();
            if let Some(ghostty) = app.ghostty.as_mut() {
                ghostty.set_scale_factor(scale.max(0.1) as f64);
                ghostty.set_size(width_px, height_px);
                ghostty.refresh();
                ghostty.force_tick();
            } else {
                app.try_init_ghostty();
            }
            Task::none()
        }
        Message::WindowEvent(window_id, event) => {
            if app.window_id.is_none_or(|current| current == window_id) {
                match event {
                    window::Event::Resized(size) => {
                        app.window_size = size;
                        let (width_px, height_px) = app.window_size_px();
                        if let Some(ghostty) = app.ghostty.as_mut() {
                            ghostty.set_size(width_px, height_px);
                            ghostty.refresh();
                            ghostty.force_tick();
                        }
                    }
                    window::Event::Rescaled(scale) => {
                        app.window_scale_factor = scale;
                        let (width_px, height_px) = app.window_size_px();
                        if let Some(ghostty) = app.ghostty.as_mut() {
                            ghostty.set_scale_factor(scale.max(0.1) as f64);
                            ghostty.set_size(width_px, height_px);
                            ghostty.refresh();
                            ghostty.force_tick();
                        }
                    }
                    _ => {}
                }
            }
            Task::none()
        }
        Message::GhosttyTick => {
            let (width_px, height_px) = app.window_size_px();
            if let Some(ghostty) = app.ghostty.as_mut() {
                if width_px > 0 && height_px > 0 {
                    ghostty.set_size(width_px, height_px);
                    ghostty.refresh();
                }
                ghostty.tick_if_needed();
            }
            Task::none()
        }
        Message::Keyboard(event) => {
            if let Some(ghostty) = app.ghostty.as_mut() {
                if ghostty.handle_keyboard_event(&event) {
                    ghostty.refresh();
                    ghostty.force_tick();
                }
            }
            Task::none()
        }
        Message::RetryGhosttyInit => {
            app.ghostty_attempted = false;
            app.ghostty_status = String::from("Retrying Ghostty embed...");
            app.try_init_ghostty();
            Task::none()
        }
    }
}

fn view(app: &App) -> iced::Element<'_, Message> {
    if app.ghostty.is_some() {
        return container(text(""))
            .height(Length::Fill)
            .width(Length::Fill)
            .into();
    }

    let header = row![
        text("Hello, world!"),
        button("-").on_press(Message::Decrement),
        text(app.counter.to_string()),
        button("+").on_press(Message::Increment),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let status_bar = row![
        text(&app.ghostty_status),
        button("Retry Ghostty").on_press(Message::RetryGhosttyInit),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let content = container(TerminalView::show(&app.terminal).map(Message::Terminal))
        .height(Length::Fill)
        .width(Length::Fill);

    container(
        column![header, status_bar, content]
            .padding(12)
            .spacing(12)
            .height(Length::Fill)
            .width(Length::Fill),
    )
    .height(Length::Fill)
    .width(Length::Fill)
    .into()
}

fn main() -> iced::Result {
    iced::application(App::boot, update, view)
        .title(App::title)
        .subscription(App::subscription)
        .window_size((1200.0, 780.0))
        .run()
}
