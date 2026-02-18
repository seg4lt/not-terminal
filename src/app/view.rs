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

    let content: Element<'_, Message> = if app.sidebar_collapsed {
        container(terminal_area)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        let sidebar = sidebar_view(app);
        row![sidebar, terminal_area]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };

    let base: Element<'_, Message> = container(
        iced::widget::column![top_bar_view(app), content]
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| root_style())
    .into();

    if let Some(overlay) = modal_overlay(app) {
        stack([base, opaque(overlay)]).into()
    } else {
        base
    }
}

fn top_bar_view(app: &App) -> Element<'_, Message> {
    let mut bar = row![].width(Length::Fill).height(Length::Fill);

    if !app.sidebar_collapsed {
        let controls = row![
            button(text("◁").size(13))
                .padding([0, 6])
                .style(|_, status| toolbar_button_style(status))
                .on_press(Message::ToggleSidebar),
            text_input("Filter projects...", &app.filter_query)
                .on_input(Message::FilterChanged)
                .padding(3)
                .size(13)
                .style(|_, status| input_style(status))
                .width(Length::Fill),
            button(text("+").size(15))
                .padding([0, 6])
                .style(|_, status| toolbar_button_style(status))
                .on_press(Message::AddProject),
            button(text("☼").size(12))
                .padding([0, 6])
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

    let (context_label, branch_label) = if let Some(context) = app.active_terminal_context() {
        let breadcrumb = format!(
            "{} / {} / {}",
            context.project_name, context.worktree_name, context.terminal_name
        );
        let branch = app.active_branch().unwrap_or_else(|| String::from("..."));
        (breadcrumb, branch)
    } else {
        (String::from("No active terminal"), String::from("..."))
    };

    bar = bar.push(
        container(
            row![
                text(context_label)
                    .size(15)
                    .color(rgb(230, 232, 236))
                    .width(Length::Fill),
                container(text(branch_label).size(12).color(rgb(198, 220, 198)))
                    .padding([0, 6])
                    .style(|_| branch_chip_style()),
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

fn sidebar_view(app: &App) -> Element<'_, Message> {
    let project_indices = app.filtered_project_indices();
    let mut list = iced::widget::column![].spacing(6).width(Length::Fill);

    if project_indices.is_empty() {
        list = list.push(
            container(text("No projects yet").size(12).color(rgb(132, 138, 149))).padding([10, 8]),
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
                        .padding([0, 4])
                        .style(|_, status| tree_icon_button_style(status))
                        .on_press(Message::ToggleProjectCollapsed(project_id.clone())),
                    monogram_chip(&project.name),
                    button(text(project.name.clone()).size(15))
                        .padding([1, 1])
                        .style(move |_, status| tree_main_button_style(status, active_project))
                        .width(Length::Fill)
                        .on_press(Message::SelectProject(project_id.clone())),
                    text(format!(
                        "{}/{}",
                        project.worktrees.len(),
                        project_terminal_count
                    ))
                    .size(12)
                    .color(rgb(150, 156, 167)),
                    button(text("↻").size(11))
                        .padding([0, 5])
                        .style(|_, status| row_action_style(status))
                        .on_press(Message::ProjectRescan(project_id.clone())),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([5, 5])
            .style(move |_| project_row_style(active_project))
        ]
        .spacing(2);

        if !project_collapsed {
            for (worktree_index, worktree) in project.worktrees.iter().enumerate() {
                let worktree_id = worktree.id.clone();
                let worktree_collapsed = App::worktree_collapsed(project, &worktree_id);
                let terminal_count = worktree.terminals.len();
                let worktree_last = worktree_index + 1 == project.worktrees.len();
                let worktree_selected =
                    project
                        .selected_terminal_id
                        .as_ref()
                        .is_some_and(|selected| {
                            worktree
                                .terminals
                                .iter()
                                .any(|terminal| &terminal.id == selected)
                        });

                project_column = project_column.push(
                    container(
                        row![
                            text(if worktree_last { "└" } else { "├" })
                                .size(13)
                                .color(rgb(80, 86, 98)),
                            button(text(if worktree_collapsed { "▸" } else { "▾" }).size(11))
                                .padding([0, 3])
                                .style(|_, status| tree_icon_button_style(status))
                                .on_press(Message::ToggleWorktreeCollapsed {
                                    project_id: project_id.clone(),
                                    worktree_id: worktree_id.clone(),
                                }),
                            text(worktree_badge(&worktree.path))
                                .size(11)
                                .color(rgb(145, 150, 162)),
                            button(text(worktree.name.clone()).size(13))
                                .padding([1, 1])
                                .style(move |_, status| tree_main_button_style(
                                    status,
                                    worktree_selected
                                ))
                                .width(Length::Fill)
                                .on_press(Message::SelectProject(project_id.clone())),
                            text(format!("{} term", terminal_count))
                                .size(12)
                                .color(rgb(136, 142, 153)),
                            button(text("+").size(11))
                                .padding([0, 5])
                                .style(|_, status| row_action_style(status))
                                .on_press(Message::AddTerminal {
                                    project_id: project_id.clone(),
                                    worktree_id: worktree_id.clone(),
                                }),
                        ]
                        .spacing(5)
                        .align_y(Alignment::Center),
                    )
                    .padding([3, 4])
                    .style(move |_| worktree_row_style(worktree_selected)),
                );

                if !worktree_collapsed {
                    for (terminal_index, terminal) in worktree.terminals.iter().enumerate() {
                        let terminal_id = terminal.id.clone();
                        let terminal_id_for_action = terminal_id.clone();
                        let terminal_active = active_project
                            && project
                                .selected_terminal_id
                                .as_ref()
                                .is_some_and(|selected| selected == &terminal_id);
                        let terminal_last = terminal_index + 1 == worktree.terminals.len();
                        let parent_branch = if worktree_last { " " } else { "│" };
                        let leaf_branch = if terminal_last { "└" } else { "├" };
                        let terminal_status = if terminal_active { "active" } else { "idle" };

                        project_column = project_column.push(
                            container(
                                row![
                                    text(parent_branch).size(13).color(rgb(74, 80, 92)),
                                    text(leaf_branch).size(13).color(rgb(80, 86, 98)),
                                    text("●").size(11).color(if terminal_active {
                                        rgb(94, 193, 112)
                                    } else {
                                        rgb(126, 131, 140)
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
                                    text(terminal_status).size(12).color(if terminal_active {
                                        rgb(94, 193, 112)
                                    } else {
                                        rgb(128, 133, 142)
                                    }),
                                    button(text("x").size(11))
                                        .padding([0, 5])
                                        .style(|_, status| row_action_style(status))
                                        .on_press(Message::RemoveTerminal {
                                            project_id: project_id.clone(),
                                            worktree_id: worktree_id.clone(),
                                            terminal_id: terminal_id_for_action,
                                        }),
                                ]
                                .spacing(5)
                                .align_y(Alignment::Center),
                            )
                            .padding([2, 4])
                            .style(move |_| terminal_row_style(terminal_active)),
                        );
                    }
                }
            }
        }

        list = list.push(container(project_column).style(|_| project_group_style()));
    }

    list = list.push(
        container(text(app.status.clone()).size(11).color(rgb(142, 147, 156))).padding([8, 6]),
    );

    container(scrollable(list))
        .padding([8, 8])
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
                .id("quick-open-input")
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
                    .id("rename-input")
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
    ContainerStyle::default().background(rgb(13, 15, 20))
}

fn surface_style() -> ContainerStyle {
    ContainerStyle::default().background(rgb(10, 12, 16))
}

fn top_bar_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(16, 18, 24))),
        border: Border {
            width: 1.0,
            color: rgb(34, 38, 48),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn top_bar_sidebar_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(15, 17, 22))),
        border: Border {
            width: 1.0,
            color: rgb(34, 38, 48),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn top_bar_context_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(12, 14, 20))),
        ..Default::default()
    }
}

fn branch_chip_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(24, 35, 30))),
        border: Border {
            width: 1.0,
            color: rgb(56, 86, 62),
            radius: 2.0.into(),
        },
        ..Default::default()
    }
}

fn sidebar_style() -> ContainerStyle {
    ContainerStyle {
        text_color: Some(rgb(225, 228, 234)),
        background: Some(Background::Color(rgb(18, 20, 26))),
        border: Border {
            width: 1.0,
            color: rgb(36, 40, 50),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn chip_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(34, 38, 46))),
        border: Border {
            width: 1.0,
            color: rgb(52, 58, 70),
            radius: 2.0.into(),
        },
        ..Default::default()
    }
}

fn project_group_style() -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(rgb(20, 23, 30))),
        border: Border {
            width: 1.0,
            color: rgb(44, 49, 60),
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn project_row_style(active: bool) -> ContainerStyle {
    let bg = if active {
        rgb(30, 35, 46)
    } else {
        rgb(24, 27, 34)
    };

    ContainerStyle {
        background: Some(Background::Color(bg)),
        border: Border {
            width: 1.0,
            color: if active {
                rgb(58, 67, 88)
            } else {
                rgb(40, 45, 56)
            },
            radius: 3.0.into(),
        },
        ..Default::default()
    }
}

fn worktree_row_style(active: bool) -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(if active {
            rgb(24, 29, 38)
        } else {
            rgb(21, 24, 31)
        })),
        border: Border {
            width: 1.0,
            color: if active {
                rgb(50, 58, 75)
            } else {
                rgb(35, 39, 48)
            },
            radius: 3.0.into(),
        },
        ..Default::default()
    }
}

fn terminal_row_style(active: bool) -> ContainerStyle {
    ContainerStyle {
        background: Some(Background::Color(if active {
            rgb(20, 27, 35)
        } else {
            rgb(18, 21, 28)
        })),
        border: Border {
            width: 1.0,
            color: if active {
                rgb(45, 63, 74)
            } else {
                rgb(31, 35, 43)
            },
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
        background: Some(Background::Color(rgb(28, 33, 43))),
        text_color: rgb(219, 223, 232),
        border: Border {
            width: 1.0,
            color: rgb(58, 66, 82),
            radius: 2.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(rgb(36, 42, 55)));
            style.border.color = rgb(74, 83, 102);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(rgb(30, 35, 46)));
        }
        button::Status::Disabled => {
            style.background = Some(Background::Color(rgb(24, 27, 34)));
            style.text_color = rgb(118, 123, 132);
        }
        button::Status::Active => {}
    }

    style
}

fn tree_icon_button_style(status: button::Status) -> button::Style {
    let mut style = button::Style {
        background: Some(Background::Color(rgb(26, 30, 38))),
        text_color: rgb(189, 195, 209),
        border: Border {
            width: 1.0,
            color: rgb(50, 56, 68),
            radius: 2.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(rgb(33, 39, 50)));
            style.text_color = rgb(219, 223, 232);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(rgb(29, 34, 44)));
        }
        button::Status::Disabled => {
            style.text_color = rgb(115, 120, 128);
            style.background = Some(Background::Color(rgb(23, 26, 32)));
        }
        button::Status::Active => {}
    }

    style
}

fn tree_main_button_style(status: button::Status, active: bool) -> button::Style {
    let base_bg = if active {
        rgb(53, 63, 92)
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
                style.background = Some(Background::Color(rgb(33, 38, 48)));
            }
        }
        button::Status::Pressed => {
            if !active {
                style.background = Some(Background::Color(rgb(29, 33, 41)));
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
        background: Some(Background::Color(rgb(27, 31, 39))),
        text_color: rgb(197, 203, 216),
        border: Border {
            width: 1.0,
            color: rgb(48, 54, 66),
            radius: 1.0.into(),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(rgb(35, 40, 51)));
            style.border.color = rgb(66, 73, 90);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(rgb(30, 35, 45)));
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
