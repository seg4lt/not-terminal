mod app;
mod ghostty_embed;

use app::{App, update, view};

fn main() -> iced::Result {
    iced::application(App::boot, update, view)
        .title(App::title)
        .subscription(App::subscription)
        .window_size((1280.0, 820.0))
        .run()
}
