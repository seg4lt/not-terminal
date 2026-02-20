use super::state::{App, Message};

mod modal;
mod panel;
mod sidebar;

use iced::widget::text::Wrapping;
use iced::widget::{
    button, container, container::Style as ContainerStyle, opaque, row, stack, text, text_input,
    text_input::Style as TextInputStyle,
};
use iced::{Alignment, Background, Border, Color, Element, Length};
use modal::modal_overlay;
use panel::browser_panel_view;
use sidebar::sidebar_view;

pub(crate) fn view(app: &App) -> Element<'_, Message> {
    // Show browser panel if there's an active browser, otherwise terminal area
    let main_area = if app.active_browser().is_some() {
        browser_panel_view(app)
    } else {
        container(text(""))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| surface_style())
            .into()
    };

    let content: Element<'_, Message> = if app.sidebar_state.is_hidden() {
        container(main_area)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        let sidebar = sidebar_view(app);
        row![sidebar, main_area]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };

    let base: Element<'_, Message> = if app.sidebar_state.is_hidden() {
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| root_style())
            .into()
    } else {
        container(
            iced::widget::column![top_bar_view(app), content]
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| root_style())
        .into()
    };

    if let Some(overlay) = modal_overlay(app) {
        stack([base, opaque(overlay)]).into()
    } else {
        base
    }
}

fn top_bar_view(app: &App) -> Element<'_, Message> {
    let mut bar = row![].width(Length::Fill).height(Length::Fill);

    if !app.sidebar_state.is_hidden() {
        let controls = row![
            button(text("◁").size(13))
                .padding([0, 7])
                .style(|_, status| toolbar_button_style(status))
                .on_press(Message::ToggleSidebar),
            text_input("Filter projects...", &app.filter_query)
                .on_input(Message::FilterChanged)
                .padding(3)
                .size(13)
                .style(|_, status| input_style(status))
                .width(Length::Fill),
            button(text("+").size(14))
                .padding([0, 7])
                .style(|_, status| toolbar_button_style(status))
                .on_press(Message::AddDetachedTerminal),
            button(text("⚙").size(12))
                .padding([0, 7])
                .style(|_, status| toolbar_button_style(status))
                .on_press(Message::OpenPreferences(true)),
        ]
        .spacing(4)
        .width(Length::Fill)
        .align_y(Alignment::Center);

        bar = bar.push(
            container(controls)
                .padding([1, 4])
                .width(Length::Fixed(app.sidebar_width_logical()))
                .height(Length::Fill)
                .style(|_| top_bar_sidebar_style()),
        );
    }

    let context_label = if let Some(browser) = app.active_browser() {
        format!("🌐 {}", &browser.name)
    } else if let Some(context) = app.active_terminal_context() {
        let breadcrumb = format!(
            "{} / {} / {}",
            &context.project_name, &context.worktree_name, &context.terminal_name
        );
        breadcrumb
    } else {
        String::from("No active terminal")
    };

    bar = bar.push(
        container(
            row![
                text(context_label)
                    .size(15)
                    .color(rgb(230, 232, 236))
                    .width(Length::Fill)
                    .wrapping(Wrapping::None),
                button(text("+").size(13))
                    .padding([0, 6])
                    .style(|_, status| toolbar_button_style(status))
                    .on_press(Message::AddDetachedTerminal),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([0, 8])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| top_bar_context_style()),
    );

    container(bar)
        .width(Length::Fill)
        .height(Length::Fixed(app.header_height_logical()))
        .style(|_| top_bar_style())
        .into()
}

fn monogram_chip(name: &str) -> Element<'static, Message> {
    let monogram = monogram(name);
    container(text(monogram).size(10).color(rgb(200, 205, 215)))
        .padding([2, 6])
        .style(|_| chip_style())
        .into()
}

fn detached_icon_chip() -> Element<'static, Message> {
    container(text("⬚").size(12).color(rgb(185, 190, 200)))
        .padding([2, 6])
        .style(|_| chip_style())
        .into()
}

fn browser_icon_chip() -> Element<'static, Message> {
    container(text("🌐").size(11).color(rgb(100, 180, 255)))
        .padding([2, 6])
        .style(|_| chip_style())
        .into()
}

fn project_icon_chip() -> Element<'static, Message> {
    container(text("P").size(11).color(rgb(100, 165, 140)))
        .padding([2, 6])
        .style(|_| chip_style())
        .into()
}

fn monogram(name: &str) -> String {
    let mut chars = name.chars().filter(|ch| ch.is_alphanumeric());
    let first = chars.next().unwrap_or('P');
    let second = chars.next().unwrap_or(' ');
    if second == ' ' {
        first.to_uppercase().collect()
    } else {
        format!(
            "{}{}",
            first.to_uppercase().next().unwrap_or('P'),
            second.to_uppercase().next().unwrap_or('R')
        )
    }
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

fn root_style() -> ContainerStyle {
    ContainerStyle::default().background(rgb(14, 16, 22))
}

fn surface_style() -> ContainerStyle {
    ContainerStyle::default().background(rgb(11, 13, 19))
}

fn top_bar_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(17, 19, 26))),
        border: Border {
            width: 1.0,
            color: rgb(30, 34, 44),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn top_bar_sidebar_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(16, 18, 25))),
        border: Border {
            width: 0.0,
            color: rgb(30, 34, 44),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn top_bar_context_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(13, 15, 22))),
        ..Default::default()
    }
}

fn sidebar_style() -> ContainerStyle {
    ContainerStyle {
        text_color: Some(rgb(222, 226, 234)),
        background: Some(Background::Color(rgb(16, 18, 25))),
        border: Border {
            width: 1.0,
            color: rgb(26, 30, 40),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn resize_handle_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(22, 25, 33))),
        border: Border {
            width: 0.0,
            color: Color::TRANSPARENT,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn chip_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(32, 36, 44))),
        border: Border {
            width: 1.0,
            color: rgb(48, 54, 66),
            radius: 3.0.into(),
        },
        ..Default::default()
    }
}

#[allow(dead_code)]
fn count_badge_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(28, 32, 40))),
        border: Border {
            width: 1.0,
            color: rgb(44, 50, 62),
            radius: 3.0.into(),
        },
        ..Default::default()
    }
}

fn empty_state_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(17, 20, 27))),
        border: Border {
            width: 1.0,
            color: rgb(30, 34, 44),
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn project_group_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(18, 20, 27))),
        border: Border {
            width: 1.0,
            color: rgb(28, 32, 42),
            radius: 5.0.into(),
        },
        ..Default::default()
    }
}

fn project_header_style(active: bool) -> ContainerStyle {
    let bg = if active {
        rgb(35, 42, 58)
    } else {
        rgb(22, 25, 33)
    };

    ContainerStyle {
        background: Some(Background::Color(bg)),
        border: Border {
            width: 0.0,
            color: if active {
                rgb(58, 70, 98)
            } else {
                Color::TRANSPARENT
            },
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn worktree_row_style(active: bool) -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(if active {
            rgb(30, 36, 50)
        } else {
            rgb(19, 22, 30)
        })),
        border: Border {
            width: 0.0,
            color: if active {
                rgb(52, 62, 86)
            } else {
                Color::TRANSPARENT
            },
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn terminal_row_style(active: bool) -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(if active {
            rgb(34, 42, 58)
        } else {
            rgb(17, 20, 28)
        })),
        border: Border {
            width: 0.0,
            color: if active {
                rgb(56, 68, 96)
            } else {
                Color::TRANSPARENT
            },
            radius: 3.0.into(),
        },
        ..Default::default()
    }
}

fn modal_backdrop_style() -> ContainerStyle {
    ContainerStyle::default().background(Background::Color(Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.52,
    }))
}

fn modal_panel_style() -> ContainerStyle {
    ContainerStyle {
        text_color: Some(rgb(230, 232, 238)),
        background: Some(Background::Color(rgb(21, 24, 31))),
        border: Border {
            width: 1.0,
            color: rgb(58, 66, 80),
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn toolbar_button_style(status: button::Status) -> button::Style {
    let mut style = button::Style {
        background: Some(Background::Color(rgb(26, 30, 40))),
        text_color: rgb(215, 220, 230),
        border: Border {
            width: 1.0,
            color: rgb(54, 60, 74),
            radius: 3.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(rgb(34, 40, 52)));
            style.border.color = rgb(70, 78, 96);
            style.text_color = rgb(230, 235, 243);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(rgb(30, 34, 46)));
        }
        button::Status::Disabled => {
            style.background = Some(Background::Color(rgb(22, 26, 34)));
            style.text_color = rgb(115, 120, 132);
        }
        button::Status::Active => {}
    }

    style
}

fn status_bar_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(17, 20, 27))),
        border: Border {
            width: 1.0,
            color: rgb(28, 32, 42),
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn tree_icon_button_style(status: button::Status) -> button::Style {
    let mut style = button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: rgb(168, 175, 188),
        border: Border {
            width: 0.0,
            color: Color::TRANSPARENT,
            radius: 3.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(rgb(28, 32, 41)));
            style.text_color = rgb(218, 222, 231);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(rgb(24, 28, 37)));
        }
        button::Status::Disabled => {
            style.text_color = rgb(110, 116, 128);
        }
        button::Status::Active => {}
    }

    style
}

fn selected_entry_style(status: button::Status) -> button::Style {
    let mut style = button::Style {
        background: Some(Background::Color(rgb(59, 130, 246))),
        text_color: rgb(255, 255, 255),
        border: Border {
            width: 0.0,
            color: Color::TRANSPARENT,
            radius: 3.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(rgb(80, 150, 250)));
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(rgb(40, 110, 230)));
        }
        button::Status::Disabled => {
            style.background = Some(Background::Color(rgb(60, 70, 80)));
            style.text_color = rgb(150, 160, 170);
        }
        button::Status::Active => {}
    }

    style
}

fn chevron_button_style(status: button::Status) -> button::Style {
    let mut style = button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: rgb(145, 152, 165),
        border: Border {
            width: 0.0,
            color: Color::TRANSPARENT,
            radius: 3.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(rgb(30, 34, 43)));
            style.text_color = rgb(195, 200, 210);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(rgb(26, 30, 39)));
        }
        button::Status::Disabled => {
            style.text_color = rgb(100, 106, 118);
        }
        button::Status::Active => {}
    }

    style
}

fn tree_main_button_style(status: button::Status, active: bool) -> button::Style {
    let base_bg = if active {
        rgb(58, 72, 102)
    } else {
        Color::TRANSPARENT
    };
    let base_fg = if active {
        rgb(242, 244, 250)
    } else {
        rgb(215, 220, 228)
    };

    let mut style = button::Style {
        background: Some(Background::Color(base_bg)),
        text_color: base_fg,
        border: Border {
            width: 0.0,
            color: Color::TRANSPARENT,
            radius: 3.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            if !active {
                style.background = Some(Background::Color(rgb(28, 33, 44)));
                style.text_color = rgb(225, 230, 238);
            } else {
                style.background = Some(Background::Color(rgb(68, 82, 115)));
            }
        }
        button::Status::Pressed => {
            if !active {
                style.background = Some(Background::Color(rgb(24, 28, 38)));
            } else {
                style.background = Some(Background::Color(rgb(52, 64, 92)));
            }
        }
        button::Status::Disabled => {
            style.text_color = rgb(115, 120, 130);
        }
        button::Status::Active => {}
    }

    style
}

#[allow(dead_code)]
fn action_button_style(status: button::Status) -> button::Style {
    let mut style = button::Style {
        background: Some(Background::Color(rgb(24, 28, 36))),
        text_color: rgb(185, 192, 205),
        border: Border {
            width: 1.0,
            color: rgb(48, 54, 66),
            radius: 3.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(rgb(32, 38, 50)));
            style.border.color = rgb(68, 76, 92);
            style.text_color = rgb(200, 206, 218);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(rgb(28, 32, 42)));
        }
        button::Status::Disabled => {
            style.text_color = rgb(110, 116, 128);
        }
        button::Status::Active => {}
    }

    style
}

#[allow(dead_code)]
fn delete_button_style(status: button::Status) -> button::Style {
    let mut style = button::Style {
        background: Some(Background::Color(rgb(22, 25, 33))),
        text_color: rgb(185, 130, 130),
        border: Border {
            width: 1.0,
            color: rgb(52, 58, 70),
            radius: 3.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(rgb(45, 30, 32)));
            style.border.color = rgb(85, 50, 54);
            style.text_color = rgb(215, 150, 150);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(rgb(35, 25, 28)));
        }
        button::Status::Disabled => {
            style.text_color = rgb(110, 100, 100);
        }
        button::Status::Active => {}
    }

    style
}

fn input_style(status: text_input::Status) -> TextInputStyle {
    let mut style = TextInputStyle {
        background: Background::Color(rgb(26, 29, 36)),
        border: Border {
            width: 1.0,
            color: rgb(61, 67, 79),
            radius: 1.0.into(),
        },
        icon: rgb(154, 160, 170),
        placeholder: rgb(126, 132, 144),
        value: rgb(224, 228, 236),
        selection: rgb(74, 89, 159),
    };

    match status {
        text_input::Status::Hovered => {
            style.border.color = rgb(86, 93, 108);
        }
        text_input::Status::Focused { .. } => {
            style.border.color = rgb(110, 117, 136);
        }
        text_input::Status::Disabled => {
            style.value = rgb(120, 126, 136);
        }
        text_input::Status::Active => {}
    }

    style
}

// New styles for cleaner tree design
fn subtle_badge_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(32, 36, 44))),
        border: Border {
            width: 1.0,
            color: rgb(44, 50, 62),
            radius: 10.0.into(),
        },
        ..Default::default()
    }
}

fn subtle_action_button_style(status: button::Status) -> button::Style {
    let mut style = button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: rgb(140, 148, 162),
        border: Border {
            width: 0.0,
            color: Color::TRANSPARENT,
            radius: 3.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(rgb(40, 46, 58)));
            style.text_color = rgb(200, 208, 220);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(rgb(34, 40, 52)));
        }
        button::Status::Disabled => {
            style.text_color = rgb(100, 108, 122);
        }
        button::Status::Active => {}
    }

    style
}

fn subtle_delete_button_style(status: button::Status) -> button::Style {
    let mut style = button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: rgb(160, 120, 120),
        border: Border {
            width: 0.0,
            color: Color::TRANSPARENT,
            radius: 3.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(rgb(55, 35, 38)));
            style.text_color = rgb(220, 160, 160);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(rgb(45, 30, 32)));
        }
        button::Status::Disabled => {
            style.text_color = rgb(110, 100, 100);
        }
        button::Status::Active => {}
    }

    style
}

fn worktree_left_border_style(active: bool) -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(if active {
            rgb(88, 120, 168)
        } else {
            rgb(45, 52, 68)
        })),
        border: Border {
            width: 0.0,
            color: Color::TRANSPARENT,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn terminal_left_border_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(35, 40, 52))),
        border: Border {
            width: 0.0,
            color: Color::TRANSPARENT,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[allow(dead_code)]
fn terminal_left_border_style_active(active: bool) -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(if active {
            rgb(88, 186, 108)
        } else {
            rgb(55, 62, 78)
        })),
        border: Border {
            width: 0.0,
            color: Color::TRANSPARENT,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn terminal_button_style(status: button::Status, active: bool) -> button::Style {
    let base_bg = if active {
        rgb(45, 55, 75)
    } else {
        Color::TRANSPARENT
    };
    let base_fg = if active {
        rgb(235, 238, 245)
    } else {
        rgb(195, 200, 210)
    };

    let mut style = button::Style {
        background: Some(Background::Color(base_bg)),
        text_color: base_fg,
        border: Border {
            width: 0.0,
            color: Color::TRANSPARENT,
            radius: 3.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            if !active {
                style.background = Some(Background::Color(rgb(35, 42, 55)));
                style.text_color = rgb(215, 220, 230);
            } else {
                style.background = Some(Background::Color(rgb(55, 65, 88)));
            }
        }
        button::Status::Pressed => {
            if !active {
                style.background = Some(Background::Color(rgb(30, 36, 48)));
            } else {
                style.background = Some(Background::Color(rgb(50, 60, 82)));
            }
        }
        button::Status::Disabled => {
            style.text_color = rgb(115, 120, 130);
        }
        button::Status::Active => {}
    }

    style
}
