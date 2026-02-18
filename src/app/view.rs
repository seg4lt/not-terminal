use super::state::{App, Message};
use iced::widget::{
    button, container, container::Style as ContainerStyle, opaque, row, scrollable, stack, text,
    text_input, text_input::Style as TextInputStyle,
};
use iced::{Alignment, Background, Border, Color, Element, Length};

pub(crate) fn view(app: &App) -> Element<'_, Message> {
    let terminal_area = container(text(""))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| surface_style());

    let base: Element<'_, Message> = if app.sidebar_collapsed {
        container(terminal_area)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| root_style())
            .into()
    } else {
        let sidebar = sidebar_view(app);
        container(
            row![sidebar, terminal_area]
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

fn sidebar_view(app: &App) -> Element<'_, Message> {
    let toolbar = row![
        button(text("◁").size(13))
            .padding([3, 7])
            .style(|_, status| toolbar_button_style(status))
            .on_press(Message::ToggleSidebar),
        text_input("Filter projects...", &app.filter_query)
            .on_input(Message::FilterChanged)
            .padding(5)
            .size(13)
            .style(|_, status| input_style(status))
            .width(Length::Fill),
        button(text("+").size(15))
            .padding([2, 7])
            .style(|_, status| toolbar_button_style(status))
            .on_press(Message::AddProject),
        button(text("☼").size(12))
            .padding([3, 7])
            .style(|_, status| toolbar_button_style(status))
            .on_press(Message::OpenPreferences(true)),
    ]
    .spacing(6)
    .width(Length::Fill)
    .align_y(Alignment::Center);

    let project_indices = app.filtered_project_indices();
    let mut list = iced::widget::column![toolbar]
        .spacing(2)
        .width(Length::Fill);

    if project_indices.is_empty() {
        list = list.push(
            container(text("No projects").size(12).color(rgb(130, 136, 146))).padding([8, 4]),
        );
    }

    for project_idx in project_indices {
        let project = &app.persisted.projects[project_idx];
        let project_id = project.id.clone();
        let active_project = app
            .persisted
            .active_project_id
            .as_ref()
            .is_some_and(|value| value == &project_id);
        let project_collapsed = App::project_collapsed(project);
        let project_terminal_count = project
            .worktrees
            .iter()
            .map(|worktree| worktree.terminals.len())
            .sum::<usize>();

        let mut project_column = iced::widget::column![
            container(
                row![
                    button(text(if project_collapsed { "▸" } else { "▾" }).size(12))
                        .padding([1, 4])
                        .style(|_, status| tree_icon_button_style(status))
                        .on_press(Message::ToggleProjectCollapsed(project_id.clone())),
                    monogram_chip(&project.name),
                    button(text(project.name.clone()).size(17))
                        .padding([1, 2])
                        .style(move |_, status| tree_main_button_style(status, active_project))
                        .width(Length::Fill)
                        .on_press(Message::SelectProject(project_id.clone())),
                    text(format!(
                        "{}/{}",
                        project.worktrees.len(),
                        project_terminal_count
                    ))
                    .size(13)
                    .color(rgb(148, 150, 160)),
                    button(text("r").size(11))
                        .padding([1, 5])
                        .style(|_, status| row_action_style(status))
                        .on_press(Message::ProjectRescan(project_id.clone())),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .padding([2, 2])
            .style(move |_| tree_row_style(active_project))
        ]
        .spacing(1);

        if !project_collapsed {
            for worktree in &project.worktrees {
                let worktree_id = worktree.id.clone();
                let worktree_collapsed = App::worktree_collapsed(project, &worktree_id);
                let terminal_count = worktree.terminals.len();

                project_column = project_column.push(
                    container(
                        row![
                            container(text(""))
                                .width(Length::Fixed(18.0))
                                .height(Length::Shrink),
                            button(text(if worktree_collapsed { "▸" } else { "▾" }).size(11))
                                .padding([0, 3])
                                .style(|_, status| tree_icon_button_style(status))
                                .on_press(Message::ToggleWorktreeCollapsed {
                                    project_id: project_id.clone(),
                                    worktree_id: worktree_id.clone(),
                                }),
                            text(format!(
                                "{} {}",
                                worktree_badge(&worktree.path),
                                worktree.name
                            ))
                            .size(12)
                            .color(if worktree.missing {
                                rgb(210, 150, 120)
                            } else {
                                rgb(205, 208, 214)
                            })
                            .width(Length::Fill),
                            text(format!("{} term", terminal_count))
                                .size(12)
                                .color(rgb(138, 143, 152)),
                            button(text("+").size(11))
                                .padding([1, 5])
                                .style(|_, status| row_action_style(status))
                                .on_press(Message::AddTerminal {
                                    project_id: project_id.clone(),
                                    worktree_id: worktree_id.clone(),
                                }),
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center),
                    )
                    .padding([1, 2])
                    .style(|_| subtree_row_style()),
                );

                if !worktree_collapsed {
                    for terminal in &worktree.terminals {
                        let terminal_id = terminal.id.clone();
                        let terminal_active = active_project
                            && project
                                .selected_terminal_id
                                .as_ref()
                                .is_some_and(|selected| selected == &terminal_id);

                        project_column = project_column.push(
                            container(
                                row![
                                    container(text(""))
                                        .width(Length::Fixed(40.0))
                                        .height(Length::Shrink),
                                    text("•").size(13).color(if terminal_active {
                                        rgb(120, 205, 130)
                                    } else {
                                        rgb(122, 126, 134)
                                    }),
                                    button(text(terminal.name.clone()).size(14))
                                        .padding([1, 2])
                                        .style(move |_, status| {
                                            tree_main_button_style(status, terminal_active)
                                        })
                                        .width(Length::Fill)
                                        .on_press(Message::SelectTerminal {
                                            project_id: project_id.clone(),
                                            terminal_id: terminal_id.clone(),
                                        }),
                                    text("idle").size(12).color(rgb(128, 133, 142)),
                                    button(text("x").size(11))
                                        .padding([1, 5])
                                        .style(|_, status| row_action_style(status))
                                        .on_press(Message::RemoveTerminal {
                                            project_id: project_id.clone(),
                                            worktree_id: worktree_id.clone(),
                                            terminal_id,
                                        }),
                                ]
                                .spacing(6)
                                .align_y(Alignment::Center),
                            )
                            .padding([1, 2])
                            .style(move |_| tree_row_style(terminal_active)),
                        );
                    }
                }
            }
        }

        list = list.push(project_column);
    }

    list = list.push(
        container(text(app.status.clone()).size(11).color(rgb(145, 149, 158))).padding([8, 4]),
    );

    container(scrollable(list))
        .padding([6, 6])
        .width(Length::Fixed(app.sidebar_width_logical()))
        .height(Length::Fill)
        .style(|_| sidebar_style())
        .into()
}

fn modal_overlay(app: &App) -> Option<Element<'_, Message>> {
    if app.quick_open_open {
        let entries = app.quick_open_entries();
        let mut list = iced::widget::column![
            text_input("Search terminal", &app.quick_open_query)
                .on_input(Message::QuickOpenQueryChanged)
                .on_submit(Message::QuickOpenSubmit)
                .padding(6)
                .size(14)
                .style(|_, status| input_style(status))
                .width(Length::Fill)
        ]
        .spacing(6)
        .width(Length::Fill);

        for entry in entries.iter().take(24) {
            list = list.push(
                button(
                    text(format!(
                        "{} / {} / {}",
                        entry.project_name, entry.worktree_name, entry.terminal_name
                    ))
                    .size(13),
                )
                .width(Length::Fill)
                .padding([4, 6])
                .style(|_, status| tree_icon_button_style(status))
                .on_press(Message::QuickOpenSelect(entry.terminal_id.clone())),
            );
        }

        if entries.is_empty() {
            list = list.push(container(text("No matching terminals").size(12)).padding([4, 2]));
        }

        let panel = container(
            iced::widget::column![
                row![
                    text("Quick Open").size(16),
                    button(text("Close").size(12))
                        .style(|_, status| toolbar_button_style(status))
                        .on_press(Message::OpenQuickOpen(false)),
                ]
                .spacing(8),
                scrollable(list).height(Length::Fill),
            ]
            .spacing(8),
        )
        .padding(12)
        .width(Length::Fixed(560.0))
        .height(Length::Fixed(420.0))
        .style(|_| modal_panel_style());

        return Some(
            container(panel)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .style(|_| modal_backdrop_style())
                .into(),
        );
    }

    if let Some(dialog) = &app.rename_dialog {
        let title = match dialog.target {
            super::state::RenameTarget::Project { .. } => "Rename Project",
            super::state::RenameTarget::Terminal { .. } => "Rename Terminal",
        };

        let panel = container(
            iced::widget::column![
                text(title).size(16),
                text_input("Name", &dialog.value)
                    .on_input(Message::RenameValueChanged)
                    .on_submit(Message::RenameCommit)
                    .padding(6)
                    .size(14)
                    .style(|_, status| input_style(status))
                    .width(Length::Fill),
                row![
                    button(text("Cancel").size(12))
                        .style(|_, status| toolbar_button_style(status))
                        .on_press(Message::RenameCancel),
                    button(text("Save").size(12))
                        .style(|_, status| toolbar_button_style(status))
                        .on_press(Message::RenameCommit),
                ]
                .spacing(8),
            ]
            .spacing(8),
        )
        .padding(12)
        .width(Length::Fixed(420.0))
        .style(|_| modal_panel_style());

        return Some(
            container(panel)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .style(|_| modal_backdrop_style())
                .into(),
        );
    }

    if app.preferences_open {
        let panel = container(
            iced::widget::column![
                row![
                    text("Preferences").size(16),
                    button(text("Close").size(12))
                        .style(|_, status| toolbar_button_style(status))
                        .on_press(Message::OpenPreferences(false)),
                ]
                .spacing(8),
                text("Shortcuts").size(14),
                text("Cmd/Ctrl+1: Toggle sidebar").size(12),
                text("Cmd/Ctrl+P: Quick open").size(12),
                text("Cmd/Ctrl+, : Preferences").size(12),
                text("Cmd/Ctrl+=/-/0: Font size").size(12),
                text("Cmd/Ctrl+Shift+[ or ]: Previous/Next terminal").size(12),
                text("Cmd/Ctrl+R: Rename active terminal").size(12),
                text("F2: Rename focused item").size(12),
            ]
            .spacing(6),
        )
        .padding(12)
        .width(Length::Fixed(460.0))
        .style(|_| modal_panel_style());

        return Some(
            container(panel)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .style(|_| modal_backdrop_style())
                .into(),
        );
    }

    None
}

fn monogram_chip(name: &str) -> Element<'static, Message> {
    let monogram = monogram(name);
    container(text(monogram).size(11).color(rgb(196, 201, 210)))
        .padding([1, 5])
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

fn worktree_badge(path: &str) -> &'static str {
    if path.contains("/.git/worktrees/") || path.contains("\\.git\\worktrees\\") {
        "[W]"
    } else {
        "[M]"
    }
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

fn root_style() -> ContainerStyle {
    ContainerStyle::default().background(rgb(17, 19, 24))
}

fn surface_style() -> ContainerStyle {
    ContainerStyle::default().background(rgb(13, 15, 19))
}

fn sidebar_style() -> ContainerStyle {
    ContainerStyle {
        text_color: Some(rgb(225, 228, 234)),
        background: Some(Background::Color(rgb(19, 21, 27))),
        border: Border {
            width: 1.0,
            color: rgb(44, 48, 58),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn chip_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(34, 38, 47))),
        border: Border {
            width: 1.0,
            color: rgb(56, 62, 74),
            radius: 2.0.into(),
        },
        ..Default::default()
    }
}

fn tree_row_style(active: bool) -> ContainerStyle {
    let bg = if active {
        rgb(49, 58, 95)
    } else {
        rgb(27, 29, 35)
    };

    ContainerStyle {
        background: Some(Background::Color(bg)),
        border: Border {
            width: 1.0,
            color: rgb(46, 50, 58),
            radius: 2.0.into(),
        },
        ..Default::default()
    }
}

fn subtree_row_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(22, 24, 30))),
        border: Border {
            width: 1.0,
            color: rgb(38, 42, 50),
            radius: 2.0.into(),
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
        background: Some(Background::Color(rgb(23, 26, 33))),
        border: Border {
            width: 1.0,
            color: rgb(66, 72, 84),
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn toolbar_button_style(status: button::Status) -> button::Style {
    let mut style = button::Style {
        background: Some(Background::Color(rgb(39, 44, 56))),
        text_color: rgb(225, 228, 236),
        border: Border {
            width: 1.0,
            color: rgb(78, 84, 98),
            radius: 2.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(rgb(58, 66, 84)));
            style.border.color = rgb(106, 112, 128);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(rgb(45, 51, 66)));
        }
        button::Status::Disabled => {
            style.background = Some(Background::Color(rgb(29, 31, 37)));
            style.text_color = rgb(118, 123, 132);
        }
        button::Status::Active => {}
    }

    style
}

fn tree_icon_button_style(status: button::Status) -> button::Style {
    let mut style = button::Style {
        background: Some(Background::Color(rgb(34, 38, 46))),
        text_color: rgb(195, 200, 212),
        border: Border {
            width: 1.0,
            color: rgb(64, 70, 82),
            radius: 2.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(rgb(48, 53, 66)));
            style.text_color = rgb(228, 232, 240);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(rgb(40, 45, 56)));
        }
        button::Status::Disabled => {
            style.text_color = rgb(115, 120, 128);
            style.background = Some(Background::Color(rgb(30, 33, 39)));
        }
        button::Status::Active => {}
    }

    style
}

fn tree_main_button_style(status: button::Status, active: bool) -> button::Style {
    let base_bg = if active {
        rgb(74, 89, 159)
    } else {
        Color::TRANSPARENT
    };
    let base_fg = if active {
        rgb(240, 242, 248)
    } else {
        rgb(218, 222, 228)
    };

    let mut style = button::Style {
        background: Some(Background::Color(base_bg)),
        text_color: base_fg,
        border: Border {
            width: 0.0,
            color: Color::TRANSPARENT,
            radius: 2.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            if !active {
                style.background = Some(Background::Color(rgb(43, 47, 58)));
            }
        }
        button::Status::Pressed => {
            if !active {
                style.background = Some(Background::Color(rgb(36, 39, 48)));
            }
        }
        button::Status::Disabled => {
            style.text_color = rgb(120, 124, 132);
        }
        button::Status::Active => {}
    }

    style
}

fn row_action_style(status: button::Status) -> button::Style {
    let mut style = button::Style {
        background: Some(Background::Color(rgb(30, 34, 42))),
        text_color: rgb(206, 210, 220),
        border: Border {
            width: 1.0,
            color: rgb(58, 64, 76),
            radius: 1.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(rgb(45, 51, 63)));
            style.border.color = rgb(87, 94, 110);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(rgb(39, 43, 53)));
        }
        button::Status::Disabled => {
            style.text_color = rgb(118, 123, 132);
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
