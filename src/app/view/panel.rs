use super::*;
use crate::app::state::{App, Message};
use iced::widget::{button, container, row, text, text_input};
use iced::{Alignment, Element, Length};

pub(super) fn browser_panel_view(app: &App) -> Element<'_, Message> {
    let active_browser = match app.active_browser() {
        Some(b) => b,
        None => {
            return container(text("No browser selected"))
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }
    };

    let browser_id = active_browser.id.clone();
    let current_url = active_browser.url.clone();

    let can_go_back = app
        .browser_webviews
        .get(&browser_id)
        .is_some_and(|w| w.can_go_back());
    let can_go_forward = app
        .browser_webviews
        .get(&browser_id)
        .is_some_and(|w| w.can_go_forward());

    let toolbar = row![
        // Back button
        button(text("◁").size(13))
            .padding([0, 7])
            .style(|_, status| toolbar_button_style(status))
            .on_press_maybe(if can_go_back {
                Some(Message::BrowserBack)
            } else {
                None
            }),
        // Forward button
        button(text("▷").size(13))
            .padding([0, 7])
            .style(|_, status| toolbar_button_style(status))
            .on_press_maybe(if can_go_forward {
                Some(Message::BrowserForward)
            } else {
                None
            }),
        // Reload button
        button(text("↻").size(13))
            .padding([0, 7])
            .style(|_, status| toolbar_button_style(status))
            .on_press(Message::BrowserReload),
        // DevTools button
        button(text("⟲").size(13))
            .padding([0, 7])
            .style(|_, status| toolbar_button_style(status))
            .on_press(Message::BrowserDevTools),
        // URL input
        text_input("Enter URL...", &current_url)
            .id("browser-url-input")
            .on_input(Message::BrowserUrlChanged)
            .on_submit(Message::BrowserNavigate)
            .padding(4)
            .size(13)
            .style(|_, status| input_style(status))
            .width(Length::Fill),
        // Go button
        button(text("Go").size(12))
            .padding([0, 8])
            .style(|_, status| toolbar_button_style(status))
            .on_press(Message::BrowserNavigate),
        // Close button to return to terminal
        button(text("×").size(14))
            .padding([0, 7])
            .style(|_, status| toolbar_button_style(status))
            .on_press(Message::RemoveBrowser(browser_id.clone())),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let content = iced::widget::column![
        // Toolbar at the top
        container(toolbar)
            .padding([4, 6])
            .width(Length::Fill)
            .height(Length::Fixed(32.0))
            .style(|_| top_bar_context_style()),
        // The webview area (actual webview is rendered natively)
        container(text(""))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| surface_style()),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
