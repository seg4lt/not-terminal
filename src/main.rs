mod app;
mod ghostty_embed;
mod webview;

use app::{App, update, view};

fn main() -> iced::Result {
    let show_native_title_bar = app::initial_show_native_title_bar();

    iced::application(App::boot, update, view)
        .title(App::title)
        .subscription(App::subscription)
        .window(window_settings(show_native_title_bar))
        .run()
}

#[cfg(target_os = "macos")]
fn window_settings(show_native_title_bar: bool) -> iced::window::Settings {
    iced::window::Settings {
        size: iced::Size::new(1280.0, 820.0),
        decorations: show_native_title_bar,
        ..iced::window::Settings::default()
    }
}

#[cfg(not(target_os = "macos"))]
fn window_settings(show_native_title_bar: bool) -> iced::window::Settings {
    iced::window::Settings {
        size: iced::Size::new(1280.0, 820.0),
        decorations: show_native_title_bar,
        ..iced::window::Settings::default()
    }
}
