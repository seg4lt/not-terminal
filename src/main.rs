use iced::Alignment;
use iced::widget::{button, column, row, text};

#[derive(Default)]
struct Counter {
    value: i32,
}

#[derive(Debug, Clone, Copy)]
enum Message {
    Increment,
    Decrement,
}

fn update(counter: &mut Counter, message: Message) {
    match message {
        Message::Increment => counter.value += 1,
        Message::Decrement => counter.value -= 1,
    }
}

fn view(counter: &Counter) -> iced::widget::Column<'_, Message> {
    column![
        text("Hello, world!"),
        row![
            button("-").on_press(Message::Decrement),
            text(counter.value.to_string()),
            button("+").on_press(Message::Increment),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    ]
    .padding(20)
    .spacing(16)
    .align_x(Alignment::Center)
}

fn main() -> iced::Result {
    iced::application(Counter::default, update, view)
        .title("Iced Counter")
        .run()
}
