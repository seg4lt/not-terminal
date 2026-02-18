use super::state::{App, Message, SESSION_COUNT, SIDEBAR_WIDTH_LOGICAL};
use iced::Length;
use iced::widget::{button, column, container, row, text};

pub(crate) fn view(app: &App) -> iced::Element<'_, Message> {
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
