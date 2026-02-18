use super::state::{App, Message};
use iced::widget::{button, column, container, opaque, row, scrollable, stack, text, text_input};
use iced::{Element, Length};

pub(crate) fn view(app: &App) -> Element<'_, Message> {
    let terminal_area = container(text("")).width(Length::Fill).height(Length::Fill);

    let base: Element<'_, Message> = if app.sidebar_collapsed {
        container(terminal_area)
            .width(Length::Fill)
            .height(Length::Fill)
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
        button(text("<").size(14))
            .padding([2, 6])
            .on_press(Message::ToggleSidebar),
        text_input("Filter", &app.filter_query)
            .on_input(Message::FilterChanged)
            .padding(4)
            .size(13)
            .width(Length::Fill),
        button(text("+").size(14))
            .padding([2, 6])
            .on_press(Message::AddProject),
        button(text(",").size(14))
            .padding([2, 6])
            .on_press(Message::OpenPreferences(true)),
    ]
    .spacing(4)
    .width(Length::Fill);

    let project_indices = app.filtered_project_indices();
    let mut list = column![].spacing(2).width(Length::Fill).push(toolbar);

    if project_indices.is_empty() {
        list = list.push(container(text("No projects").size(12)).padding([8, 4]));
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

        let mut project_column = column![
            row![
                button(text(if project_collapsed { "▸" } else { "▾" }).size(12))
                    .padding([2, 4])
                    .on_press(Message::ToggleProjectCollapsed(project_id.clone())),
                button(text(project.name.clone()).size(13))
                    .padding([2, 4])
                    .width(Length::Fill)
                    .on_press(Message::SelectProject(project_id.clone())),
                button(text("r").size(11))
                    .padding([1, 4])
                    .on_press(Message::ProjectRescan(project_id.clone())),
            ]
            .spacing(2)
            .width(Length::Fill)
        ]
        .spacing(2);

        if !project_collapsed {
            for worktree in &project.worktrees {
                let worktree_id = worktree.id.clone();
                let worktree_collapsed = App::worktree_collapsed(project, &worktree_id);
                let missing_suffix = if worktree.missing { " (missing)" } else { "" };
                project_column = project_column.push(
                    row![
                        text("  ").size(12),
                        button(text(if worktree_collapsed { "▸" } else { "▾" }).size(11))
                            .padding([1, 3])
                            .on_press(Message::ToggleWorktreeCollapsed {
                                project_id: project_id.clone(),
                                worktree_id: worktree_id.clone(),
                            }),
                        text(format!("{}{}", worktree.name, missing_suffix)).size(12),
                        button(text("+").size(11))
                            .padding([1, 4])
                            .on_press(Message::AddTerminal {
                                project_id: project_id.clone(),
                                worktree_id: worktree_id.clone(),
                            }),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                );

                if !worktree_collapsed {
                    for terminal in &worktree.terminals {
                        let terminal_id = terminal.id.clone();
                        let terminal_active = active_project
                            && project
                                .selected_terminal_id
                                .as_ref()
                                .is_some_and(|selected| selected == &terminal_id);
                        let mut label = terminal.name.clone();
                        if terminal_active {
                            label = format!("> {label}");
                        }

                        project_column = project_column.push(
                            row![
                                text("    ").size(11),
                                button(text(label).size(12))
                                    .padding([1, 4])
                                    .width(Length::Fill)
                                    .on_press(Message::SelectTerminal {
                                        project_id: project_id.clone(),
                                        terminal_id: terminal_id.clone(),
                                    }),
                                button(text("x").size(11)).padding([1, 4]).on_press(
                                    Message::RemoveTerminal {
                                        project_id: project_id.clone(),
                                        worktree_id: worktree_id.clone(),
                                        terminal_id,
                                    }
                                ),
                            ]
                            .spacing(2)
                            .width(Length::Fill),
                        );
                    }
                }
            }
        }

        list = list.push(container(project_column).padding([2, 2]));
    }

    list = list.push(container(text(app.status.clone()).size(11)).padding([6, 4]));

    container(scrollable(list))
        .padding([4, 4])
        .width(Length::Fixed(app.sidebar_width_logical()))
        .height(Length::Fill)
        .into()
}

fn modal_overlay(app: &App) -> Option<Element<'_, Message>> {
    if app.quick_open_open {
        let entries = app.quick_open_entries();
        let mut list = column![].spacing(4).width(Length::Fill).push(
            text_input("Search terminal", &app.quick_open_query)
                .on_input(Message::QuickOpenQueryChanged)
                .on_submit(Message::QuickOpenSubmit)
                .padding(6)
                .size(14)
                .width(Length::Fill),
        );

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
                .on_press(Message::QuickOpenSelect(entry.terminal_id.clone())),
            );
        }

        if entries.is_empty() {
            list = list.push(container(text("No matching terminals").size(12)).padding([4, 2]));
        }

        let panel = container(
            column![
                row![
                    text("Quick Open").size(16),
                    button(text("Close").size(12)).on_press(Message::OpenQuickOpen(false)),
                ]
                .spacing(8),
                scrollable(list).height(Length::Fill),
            ]
            .spacing(8),
        )
        .padding(12)
        .width(Length::Fixed(560.0))
        .height(Length::Fixed(420.0));

        return Some(
            container(panel)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into(),
        );
    }

    if let Some(dialog) = &app.rename_dialog {
        let title = match dialog.target {
            super::state::RenameTarget::Project { .. } => "Rename Project",
            super::state::RenameTarget::Terminal { .. } => "Rename Terminal",
        };

        let panel = container(
            column![
                text(title).size(16),
                text_input("Name", &dialog.value)
                    .on_input(Message::RenameValueChanged)
                    .on_submit(Message::RenameCommit)
                    .padding(6)
                    .size(14)
                    .width(Length::Fill),
                row![
                    button(text("Cancel").size(12)).on_press(Message::RenameCancel),
                    button(text("Save").size(12)).on_press(Message::RenameCommit),
                ]
                .spacing(8),
            ]
            .spacing(8),
        )
        .padding(12)
        .width(Length::Fixed(420.0));

        return Some(
            container(panel)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into(),
        );
    }

    if app.preferences_open {
        let panel = container(
            column![
                row![
                    text("Preferences").size(16),
                    button(text("Close").size(12)).on_press(Message::OpenPreferences(false)),
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
        .width(Length::Fixed(460.0));

        return Some(
            container(panel)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into(),
        );
    }

    None
}
