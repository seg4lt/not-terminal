use super::state::{App, Message};
use iced::widget::{
    button, checkbox, container, container::Style as ContainerStyle, opaque, row, scrollable,
    stack, text, text_input, text_input::Style as TextInputStyle,
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

    let base: Element<'_, Message> = if app.sidebar_collapsed {
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

    if !app.sidebar_collapsed {
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
                .on_press(Message::AddProject),
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

    let context_label = if let Some(context) = app.active_terminal_context() {
        let breadcrumb = format!(
            "{} / {} / {}",
            truncate_label(&context.project_name),
            truncate_label(&context.worktree_name),
            truncate_label(&context.terminal_name)
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
                    .width(Length::Fill),
                button(text("+").size(13))
                    .padding([0, 6])
                    .style(|_, status| toolbar_button_style(status))
                    .on_press(Message::AddProject),
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
    let mut list = iced::widget::column![].spacing(10).width(Length::Fill);
    let active_terminal_id = app.active_terminal_id();
    let detached_active = app.persisted.selected_detached_terminal_id.is_some();

    let mut detached_column = iced::widget::column![
        container(
            row![
                button(detached_icon_chip())
                    .padding([0, 0])
                    .style(|_, status| tree_icon_button_style(status)),
                text("Detached")
                    .size(13)
                    .color(rgb(226, 229, 235))
                    .width(Length::Fill),
                container(
                    text(format!("{}", app.persisted.detached_terminals.len()))
                        .size(11)
                        .color(rgb(180, 185, 195))
                )
                .padding([2, 6])
                .style(|_| count_badge_style()),
                button(text("+").size(12))
                    .padding([0, 6])
                    .style(|_, status| action_button_style(status))
                    .on_press(Message::AddDetachedTerminal),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([6, 8])
        .style(move |_| project_header_style(detached_active))
    ]
    .spacing(2);

    if app.persisted.detached_terminals.is_empty() {
        detached_column = detached_column.push(
            container(
                text("No detached terminals")
                    .size(11)
                    .color(rgb(130, 135, 145)),
            )
            .padding([8, 12]),
        );
    } else {
        for terminal in &app.persisted.detached_terminals {
            let terminal_id = terminal.id.clone();
            let terminal_id_for_action = terminal_id.clone();
            let terminal_active = active_terminal_id
                .as_ref()
                .is_some_and(|active| active == &terminal_id);
            let terminal_status = if terminal_active { "active" } else { "idle" };

            detached_column = detached_column.push(
                container(
                    row![
                        container(text("●").size(8).color(if terminal_active {
                            rgb(88, 186, 108)
                        } else {
                            rgb(118, 123, 132)
                        }))
                        .padding([2, 0])
                        .width(Length::Fixed(12.0)),
                        button(text(truncate_label(&terminal.name)).size(13))
                            .padding([3, 4])
                            .style(move |_, status| tree_main_button_style(status, terminal_active))
                            .width(Length::Fill)
                            .on_press(Message::SelectDetachedTerminal(terminal_id.clone())),
                        text(terminal_status).size(10).color(if terminal_active {
                            rgb(88, 186, 108)
                        } else {
                            rgb(120, 125, 135)
                        }),
                        button(text("×").size(14))
                            .padding([0, 6])
                            .style(|_, status| delete_button_style(status))
                            .on_press(Message::RemoveDetachedTerminal(terminal_id_for_action)),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .padding([3, 8])
                .style(move |_| terminal_row_style(terminal_active)),
            );
        }
    }

    list = list.push(container(detached_column).style(|_| project_group_style()));

    if project_indices.is_empty() {
        list = list.push(
            container(
                row![text("No projects yet").size(11).color(rgb(130, 135, 145)),]
                    .align_y(Alignment::Center),
            )
            .padding([12, 12])
            .style(|_| empty_state_style()),
        );
    }

    for project_idx in project_indices {
        let project = &app.persisted.projects[project_idx];
        let project_id = project.id.clone();
        let active_project = if let Some(active_terminal_id) = active_terminal_id.as_ref() {
            project.worktrees.iter().any(|worktree| {
                worktree
                    .terminals
                    .iter()
                    .any(|terminal| &terminal.id == active_terminal_id)
            })
        } else {
            app.persisted
                .active_project_id
                .as_ref()
                .is_some_and(|value| value == &project_id)
        };
        let project_collapsed = App::project_collapsed(project);
        let project_terminal_count = project
            .worktrees
            .iter()
            .map(|worktree| worktree.terminals.len())
            .sum::<usize>();

        let mut project_column = iced::widget::column![
            container(
                row![
                    button(text(if project_collapsed { "▸" } else { "▾" }).size(11))
                        .padding([0, 4])
                        .style(|_, status| chevron_button_style(status))
                        .on_press(Message::ToggleProjectCollapsed(project_id.clone())),
                    button(monogram_chip(&project.name))
                        .padding([0, 0])
                        .style(|_, status| tree_icon_button_style(status))
                        .on_press(Message::SelectProject(project_id.clone())),
                    button(text(truncate_label(&project.name)).size(14))
                        .padding([2, 2])
                        .style(move |_, status| tree_main_button_style(status, active_project))
                        .width(Length::Fill)
                        .on_press(Message::ToggleProjectCollapsed(project_id.clone())),
                    container(
                        text(format!(
                            "{}·{}",
                            project.worktrees.len(),
                            project_terminal_count
                        ))
                        .size(11)
                        .color(rgb(165, 170, 180))
                    )
                    .padding([2, 6])
                    .style(|_| count_badge_style()),
                    button(text("⊕").size(12))
                        .padding([0, 6])
                        .style(|_, status| action_button_style(status))
                        .on_press(Message::StartAddWorktree(project_id.clone())),
                    button(text("✎").size(11))
                        .padding([0, 6])
                        .style(|_, status| action_button_style(status))
                        .on_press(Message::StartRenameProject(project_id.clone())),
                    button(text("↻").size(11))
                        .padding([0, 6])
                        .style(|_, status| action_button_style(status))
                        .on_press(Message::ProjectRescan(project_id.clone())),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([6, 8])
            .style(move |_| project_header_style(active_project))
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
                                .size(12)
                                .color(rgb(72, 78, 90)),
                            text(worktree_badge(&worktree.path))
                                .size(10)
                                .color(rgb(135, 140, 152)),
                            button(text(truncate_label(&worktree.name)).size(13))
                                .padding([3, 4])
                                .style(move |_, status| tree_main_button_style(
                                    status,
                                    worktree_selected
                                ))
                                .width(Length::Fill)
                                .on_press(Message::ToggleWorktreeCollapsed {
                                    project_id: project_id.clone(),
                                    worktree_id: worktree_id.clone(),
                                }),
                            text(if worktree_collapsed { "▸" } else { "▾" })
                                .size(11)
                                .color(rgb(125, 132, 145)),
                            text(format!("{}t", terminal_count))
                                .size(10)
                                .color(rgb(135, 142, 153)),
                            button(text("+").size(12))
                                .padding([0, 6])
                                .style(|_, status| action_button_style(status))
                                .on_press(Message::AddTerminal {
                                    project_id: project_id.clone(),
                                    worktree_id: worktree_id.clone(),
                                }),
                            button(text("✎").size(11))
                                .padding([0, 6])
                                .style(|_, status| action_button_style(status))
                                .on_press(Message::StartRenameWorktree {
                                    project_id: project_id.clone(),
                                    worktree_id: worktree_id.clone(),
                                }),
                            button(text("×").size(14))
                                .padding([0, 6])
                                .style(|_, status| delete_button_style(status))
                                .on_press(Message::RemoveWorktree {
                                    project_id: project_id.clone(),
                                    worktree_id: worktree_id.clone(),
                                }),
                        ]
                        .spacing(5)
                        .align_y(Alignment::Center),
                    )
                    .padding([3, 8])
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
                                    text(parent_branch).size(12).color(rgb(68, 74, 86)),
                                    text(leaf_branch).size(12).color(rgb(74, 80, 92)),
                                    container(text("●").size(8).color(if terminal_active {
                                        rgb(88, 186, 108)
                                    } else {
                                        rgb(118, 123, 132)
                                    }))
                                    .padding([2, 0])
                                    .width(Length::Fixed(12.0)),
                                    button(text(truncate_label(&terminal.name)).size(13))
                                        .padding([3, 4])
                                        .style(move |_, status| {
                                            tree_main_button_style(status, terminal_active)
                                        })
                                        .width(Length::Fill)
                                        .on_press(Message::SelectTerminal {
                                            project_id: project_id.clone(),
                                            terminal_id: terminal_id.clone(),
                                        }),
                                    text(terminal_status).size(10).color(if terminal_active {
                                        rgb(88, 186, 108)
                                    } else {
                                        rgb(120, 125, 135)
                                    }),
                                    button(text("×").size(14))
                                        .padding([0, 6])
                                        .style(|_, status| delete_button_style(status))
                                        .on_press(Message::RemoveTerminal {
                                            project_id: project_id.clone(),
                                            worktree_id: worktree_id.clone(),
                                            terminal_id: terminal_id_for_action,
                                        }),
                                ]
                                .spacing(4)
                                .align_y(Alignment::Center),
                            )
                            .padding([3, 8])
                            .style(move |_| terminal_row_style(terminal_active)),
                        );
                    }
                }
            }
        }

        list = list.push(container(project_column).style(|_| project_group_style()));
    }

    list = list.push(
        container(
            row![text(app.status.clone()).size(10).color(rgb(135, 140, 150)),]
                .align_y(Alignment::Center),
        )
        .padding([10, 10])
        .style(|_| status_bar_style()),
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
            super::state::RenameTarget::Worktree { .. } => "Rename Worktree",
            super::state::RenameTarget::Terminal { .. } => "Rename Terminal",
            super::state::RenameTarget::DetachedTerminal { .. } => "Rename Terminal",
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

    if let Some(dialog) = &app.add_worktree_dialog {
        let panel = container(
            iced::widget::column![
                text("Add Worktree").size(16),
                text_input("Branch name", &dialog.branch_name)
                    .id("add-worktree-branch-input")
                    .on_input(Message::AddWorktreeBranchChanged)
                    .on_submit(Message::FocusAddWorktreePath)
                    .padding(6)
                    .size(14)
                    .style(|_, status| input_style(status))
                    .width(Length::Fill),
                text_input("Destination path", &dialog.destination_path)
                    .id("add-worktree-path-input")
                    .on_input(Message::AddWorktreePathChanged)
                    .on_submit(Message::AddWorktreeCommit)
                    .padding(6)
                    .size(14)
                    .style(|_, status| input_style(status))
                    .width(Length::Fill),
                row![
                    button(text("Cancel").size(12))
                        .style(|_, status| toolbar_button_style(status))
                        .on_press(Message::AddWorktreeCancel),
                    button(text("Create").size(12))
                        .style(|_, status| toolbar_button_style(status))
                        .on_press(Message::AddWorktreeCommit),
                ]
                .spacing(8),
            ]
            .spacing(8),
        )
        .padding(12)
        .width(Length::Fixed(520.0))
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
                checkbox(app.show_native_title_bar)
                    .label("Show native title bar")
                    .on_toggle(Message::SetShowNativeTitleBar)
                    .text_size(13),
                text("Cmd+1: Toggle sidebar").size(12),
                text("Cmd+T: New terminal in active worktree").size(12),
                text("Cmd+Shift+T: New detached terminal").size(12),
                text("Cmd+W: Close active terminal").size(12),
                text("Cmd+P: Quick open").size(12),
                text("Cmd+, : Preferences").size(12),
                text("Cmd+=/-/0: Font size").size(12),
                text("Cmd+Shift+[ or ]: Previous/Next terminal").size(12),
                text("Cmd+R: Rename active terminal").size(12),
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

fn truncate_label(value: &str) -> String {
    value.chars().take(15).collect()
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
