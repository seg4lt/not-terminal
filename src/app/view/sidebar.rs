use super::*;
use crate::app::state::{App, Message, TerminalStatus};
use iced::mouse::Interaction;
use iced::widget::text::Wrapping;
use iced::widget::{button, container, mouse_area, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

fn terminal_working_frame(app: &App) -> &'static str {
    const FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];
    FRAMES[app.terminal_activity_frame % FRAMES.len()]
}

fn terminal_working_badge(app: &App, terminal_id: &str) -> Option<Element<'static, Message>> {
    if !app.is_terminal_progress_active(terminal_id) {
        return None;
    }

    Some(
        container(
            text(terminal_working_frame(app))
                .size(10)
                .color(rgb(108, 214, 128)),
        )
        .padding([1, 4])
        .style(|_| ContainerStyle {
            background: Some(Background::Color(rgb(25, 43, 31))),
            border: Border {
                width: 1.0,
                color: rgb(46, 82, 56),
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into(),
    )
}

/// Get the status indicator symbol and color for a terminal
fn terminal_status_indicator(
    app: &App,
    terminal_id: &str,
    is_active: bool,
) -> (&'static str, Color) {
    let status = app.get_terminal_status(terminal_id);

    // Check awaiting state first (takes precedence)
    if app.terminal_needs_attention(terminal_id) {
        return ("🔔", rgb(220, 180, 50));
    }

    match status {
        TerminalStatus::Running => {
            // Active = green filled circle, idle = gray hollow circle
            if is_active {
                ("●", rgb(88, 186, 108))
            } else {
                ("○", rgb(118, 123, 132))
            }
        }
        TerminalStatus::Success => {
            // Green checkmark
            ("✓", rgb(88, 186, 108))
        }
        TerminalStatus::Error(_code) => {
            // Red X
            ("✗", rgb(220, 80, 80))
        }
        TerminalStatus::AwaitingResponse => {
            // Should be handled above, but include for completeness
            ("🔔", rgb(255, 140, 0)) // Orange
        }
    }
}

/// Get the border color for a terminal based on its status
fn terminal_status_border_color(app: &App, terminal_id: &str, is_active: bool) -> Color {
    let status = app.get_terminal_status(terminal_id);

    // Check awaiting state first (takes precedence)
    if app.terminal_needs_attention(terminal_id) {
        return rgb(255, 140, 0); // Orange
    }

    match status {
        TerminalStatus::Running => {
            if is_active {
                rgb(88, 186, 108) // Green
            } else {
                rgb(55, 62, 78) // Dark gray
            }
        }
        TerminalStatus::Success => {
            rgb(88, 186, 108) // Green
        }
        TerminalStatus::Error(_) => {
            rgb(220, 80, 80) // Red
        }
        TerminalStatus::AwaitingResponse => {
            // Should be handled above, but include for completeness
            rgb(220, 180, 50) // Amber
        }
    }
}

fn normalize_path_key(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
}

fn is_main_project_worktree(
    project: &crate::app::model::ProjectRecord,
    worktree: &crate::app::model::WorktreeRecord,
) -> bool {
    let Some(git_folder) = project.git_folder_path.as_deref() else {
        return false;
    };

    normalize_path_key(git_folder) == normalize_path_key(&worktree.path)
}

fn tree_letter_chip(label: &'static str, text_color: Color) -> Element<'static, Message> {
    container(text(label).size(10).color(text_color))
        .padding([2, 5])
        .style(|_| chip_style())
        .into()
}

fn missing_chip() -> Element<'static, Message> {
    container(text("!").size(10).color(rgb(220, 150, 150)))
        .padding([2, 5])
        .style(|_| ContainerStyle {
            background: Some(Background::Color(rgb(42, 28, 31))),
            border: Border {
                width: 1.0,
                color: rgb(74, 48, 52),
                radius: 3.0.into(),
            },
            ..Default::default()
        })
        .into()
}

pub(super) fn sidebar_view(app: &App) -> Element<'_, Message> {
    let project_indices = app.filtered_project_indices();
    let mut list = iced::widget::column![].spacing(8).width(Length::Fill);
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
                        .size(10)
                        .color(rgb(145, 150, 160))
                )
                .padding([3, 6])
                .style(|_| subtle_badge_style()),
                button(text("+").size(12))
                    .padding([0, 5])
                    .style(|_, status| subtle_action_button_style(status))
                    .on_press(Message::AddDetachedTerminal),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([8, 10])
        .style(move |_| project_header_style(detached_active))
    ]
    .spacing(0);

    if app.persisted.detached_terminals.is_empty() {
        detached_column = detached_column.push(
            container(
                text("No detached terminals")
                    .size(11)
                    .color(rgb(130, 135, 145)),
            )
            .padding([12, 16]),
        );
    } else {
        for terminal in &app.persisted.detached_terminals {
            let terminal_id = terminal.id.clone();
            let terminal_id_for_action = terminal_id.clone();
            let terminal_active = active_terminal_id
                .as_ref()
                .is_some_and(|active| active == &terminal_id);
            let terminal_has_splits = app.terminal_has_splits(&terminal_id);

            // Get status-based indicator
            let (status_symbol, status_color) =
                terminal_status_indicator(app, &terminal_id, terminal_active);
            let border_color = terminal_status_border_color(app, &terminal_id, terminal_active);

            detached_column = detached_column.push(
                container(
                    row![
                        // Left border with status color
                        container("")
                            .width(Length::Fixed(2.0))
                            .height(Length::Fill)
                            .style(move |_| ContainerStyle {
                                background: Some(Background::Color(border_color)),
                                ..Default::default()
                            }),
                        // Status indicator
                        container(text(status_symbol).size(7).color(status_color))
                            .padding([0, 4])
                            .width(Length::Fixed(16.0)),
                        // Terminal name (with exit code badge if error)
                        {
                            let name_element =
                                text(&terminal.name).size(12).wrapping(Wrapping::None);
                            let mut name_with_badge =
                                row![container(name_element).width(Length::Fill).clip(true)]
                                    .spacing(4)
                                    .width(Length::Fill);

                            if terminal_has_splits {
                                name_with_badge = name_with_badge.push(
                                    container(text("◫").size(9).color(rgb(176, 188, 212)))
                                        .padding([1, 4])
                                        .style(|_| subtle_badge_style()),
                                );
                            }

                            if let Some(working_badge) = terminal_working_badge(app, &terminal_id) {
                                name_with_badge = name_with_badge.push(working_badge);
                            }

                            if let TerminalStatus::Error(code) =
                                app.get_terminal_status(&terminal_id)
                            {
                                name_with_badge = name_with_badge.push(
                                    container(
                                        text(format!("{}", code)).size(9).color(rgb(220, 80, 80)),
                                    )
                                    .padding([1, 4])
                                    .style(|_| {
                                        ContainerStyle {
                                            background: Some(Background::Color(rgb(40, 30, 32))),
                                            border: Border {
                                                width: 1.0,
                                                color: rgb(70, 50, 54),
                                                radius: 8.0.into(),
                                            },
                                            ..Default::default()
                                        }
                                    }),
                                );
                            }

                            button(container(name_with_badge).width(Length::Fill).clip(true))
                                .padding([2, 4])
                                .style(move |_, status| {
                                    terminal_button_style(status, terminal_active)
                                })
                                .width(Length::Fill)
                                .on_press(Message::SelectDetachedTerminal(terminal_id.clone()))
                        },
                        // Delete button
                        button(text("×").size(12))
                            .padding([0, 5])
                            .style(|_, status| subtle_delete_button_style(status))
                            .on_press(Message::RemoveDetachedTerminal(terminal_id_for_action)),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .padding([4, 8])
                .style(move |_| terminal_row_style(terminal_active)),
            );
        }
    }

    list = list.push(container(detached_column).style(|_| project_group_style()));

    // Browsers section (only if enabled)
    if app.persisted.ui.enable_browsers {
        let browser_active = app.active_browser_id().is_some();
        let mut browser_column = iced::widget::column![
            container(
                row![
                    button(browser_icon_chip())
                        .padding([0, 0])
                        .style(|_, status| tree_icon_button_style(status)),
                    text("Browsers")
                        .size(13)
                        .color(rgb(226, 229, 235))
                        .width(Length::Fill),
                    container(
                        text(format!("{}", app.persisted.browsers.len()))
                            .size(10)
                            .color(rgb(145, 150, 160))
                    )
                    .padding([3, 6])
                    .style(|_| subtle_badge_style()),
                    button(text("+").size(12))
                        .padding([0, 5])
                        .style(|_, status| subtle_action_button_style(status))
                        .on_press(Message::AddBrowser),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([8, 10])
            .style(move |_| project_header_style(browser_active))
        ]
        .spacing(0);

        if app.persisted.browsers.is_empty() {
            browser_column = browser_column.push(
                container(text("No browsers").size(11).color(rgb(130, 135, 145))).padding([12, 16]),
            );
        } else {
            for browser in &app.persisted.browsers {
                let browser_id = browser.id.clone();
                let browser_id_for_action = browser_id.clone();
                let is_active = app
                    .active_browser_id()
                    .as_ref()
                    .is_some_and(|id| id == &browser_id);

                browser_column = browser_column.push(
                    container(
                        row![
                            // Left border with blue color to indicate browser
                            container("")
                                .width(Length::Fixed(2.0))
                                .height(Length::Fill)
                                .style(move |_| ContainerStyle {
                                    background: Some(Background::Color(if is_active {
                                        rgb(66, 165, 245) // Blue for active browser
                                    } else {
                                        rgb(45, 55, 72) // Dark for inactive
                                    })),
                                    ..Default::default()
                                }),
                            // Globe icon for browser
                            container(text("🌐").size(10).color(rgb(100, 180, 255)))
                                .padding([0, 4])
                                .width(Length::Fixed(16.0)),
                            // Browser name
                            button(
                                container(text(&browser.name).size(12).wrapping(Wrapping::None))
                                    .width(Length::Fill)
                                    .clip(true)
                            )
                            .padding([2, 4])
                            .style(move |_, status| terminal_button_style(status, is_active))
                            .width(Length::Fill)
                            .on_press(Message::SelectBrowser(browser_id.clone())),
                            // Delete button
                            button(text("×").size(12))
                                .padding([0, 5])
                                .style(|_, status| subtle_delete_button_style(status))
                                .on_press(Message::RemoveBrowser(browser_id_for_action)),
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center),
                    )
                    .padding([4, 8])
                    .style(move |_| terminal_row_style(is_active)),
                );
            }
        }

        list = list.push(container(browser_column).style(|_| project_group_style()));
    }

    // Projects section
    let projects_active = app.persisted.active_project_id.is_some();
    let all_project_trees_expanded = app.all_project_trees_expanded();
    let filter_query = app.normalized_filter_query();
    let mut projects_column = iced::widget::column![
        container(
            row![
                button(project_icon_chip())
                    .padding([0, 0])
                    .style(|_, status| tree_icon_button_style(status)),
                text("Projects")
                    .size(13)
                    .color(rgb(226, 229, 235))
                    .width(Length::Fill),
                container(
                    text(format!("{}", app.persisted.projects.len()))
                        .size(10)
                        .color(rgb(145, 150, 160))
                )
                .padding([3, 6])
                .style(|_| subtle_badge_style()),
                button(
                    text(if all_project_trees_expanded {
                        "Collapse all"
                    } else {
                        "Expand all"
                    })
                    .size(10)
                )
                .padding([0, 5])
                .style(|_, status| subtle_action_button_style(status))
                .on_press(Message::ToggleAllProjectTreesCollapsed),
                button(text("Rescan").size(10))
                    .padding([0, 5])
                    .style(|_, status| subtle_action_button_style(status))
                    .on_press(Message::RescanAllProjects),
                button(text("+").size(12))
                    .padding([0, 5])
                    .style(|_, status| subtle_action_button_style(status))
                    .on_press(Message::AddProject),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([8, 10])
        .style(move |_| project_header_style(projects_active))
    ]
    .spacing(6);

    if project_indices.is_empty() {
        projects_column = projects_column.push(
            container(text("No projects").size(11).color(rgb(130, 135, 145))).padding([12, 16]),
        );
    }

    for project_idx in project_indices {
        let project = &app.persisted.projects[project_idx];
        let project_id = project.id.clone();
        let project_active = app
            .persisted
            .active_project_id
            .as_ref()
            .is_some_and(|active_id| active_id == &project_id);
        let project_collapsed = filter_query
            .as_deref()
            .is_none_or(|query| !App::project_matches_filter(project, query))
            && App::project_collapsed(project);
        let worktree_total = project.worktrees.len();

        let project_header = row![
            button(text(if project_collapsed { "›" } else { "⌄" }).size(16))
                .padding([0, 2])
                .style(|_, status| chevron_button_style(status))
                .on_press(Message::ToggleProjectCollapsed(project_id.clone())),
            button(
                container(text(&project.name).size(13).wrapping(Wrapping::None))
                    .width(Length::Fill)
                    .clip(true)
            )
            .padding([4, 4])
            .style(move |_, status| tree_main_button_style(status, project_active))
            .width(Length::Fill)
            .on_press(Message::SelectProject(project_id.clone())),
            container(
                text(format!("{}", worktree_total))
                    .size(10)
                    .color(rgb(135, 142, 153))
            )
            .padding([2, 5])
            .style(|_| subtle_badge_style()),
            button(text("+").size(12))
                .padding([0, 5])
                .style(|_, status| subtle_action_button_style(status))
                .on_press(Message::StartAddWorktree(project_id.clone())),
            button(text("⋯").size(13))
                .padding([0, 5])
                .style(|_, status| subtle_action_button_style(status))
                .on_press(Message::OpenProjectContextMenu(project_id.clone())),
        ]
        .spacing(5)
        .align_y(Alignment::Center);

        let mut project_card = iced::widget::column![
            container(project_header)
                .padding([4, 8])
                .style(move |_| project_header_style(project_active))
        ]
        .spacing(0);

        if !project_collapsed {
            let mut worktree_list = iced::widget::column![].spacing(3);

            if project.worktrees.is_empty() {
                worktree_list = worktree_list.push(
                    container(text("No worktrees").size(11).color(rgb(130, 135, 145)))
                        .padding([10, 14])
                        .style(|_| empty_state_style()),
                );
            } else {
                for worktree in &project.worktrees {
                    let worktree_id = worktree.id.clone();
                    let worktree_collapsed = filter_query
                        .as_deref()
                        .is_none_or(|query| !App::worktree_matches_filter(worktree, query))
                        && App::worktree_collapsed(project, &worktree_id);
                    let terminal_count = worktree.terminals.len();
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
                    let main_worktree = is_main_project_worktree(project, worktree);

                    let mut worktree_label_row = row![if main_worktree {
                        tree_letter_chip("M", rgb(219, 225, 235))
                    } else {
                        tree_letter_chip("W", rgb(184, 214, 240))
                    }]
                    .spacing(6)
                    .align_y(Alignment::Center);

                    if worktree.missing {
                        worktree_label_row = worktree_label_row.push(missing_chip());
                    }

                    worktree_label_row = worktree_label_row
                        .push(text(&worktree.name).size(13).wrapping(Wrapping::None));

                    let worktree_row = row![
                        container("").width(Length::Fixed(8.0)),
                        button(text(if worktree_collapsed { "›" } else { "⌄" }).size(16))
                            .padding([0, 2])
                            .style(|_, status| chevron_button_style(status))
                            .on_press(Message::ToggleWorktreeCollapsed {
                                project_id: project_id.clone(),
                                worktree_id: worktree_id.clone(),
                            }),
                        button(container(worktree_label_row).width(Length::Fill).clip(true))
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
                        container(
                            text(format!("{}", terminal_count))
                                .size(10)
                                .color(rgb(135, 142, 153))
                        )
                        .padding([2, 5])
                        .style(|_| subtle_badge_style()),
                        button(text("⋯").size(13))
                            .padding([0, 5])
                            .style(|_, status| subtle_action_button_style(status))
                            .on_press(Message::OpenWorktreeContextMenu {
                                project_id: project_id.clone(),
                                worktree_id: worktree_id.clone(),
                            }),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center);

                    worktree_list = worktree_list.push(
                        container(worktree_row)
                            .padding([2, 6])
                            .style(move |_| worktree_row_style(worktree_selected)),
                    );

                    if !worktree_collapsed {
                        let mut terminals_column = iced::widget::column![].spacing(3);

                        for terminal in &worktree.terminals {
                            let terminal_id = terminal.id.clone();
                            let terminal_id_for_action = terminal_id.clone();
                            let terminal_active = active_terminal_id
                                .as_ref()
                                .is_some_and(|active| active == &terminal_id);
                            let terminal_has_splits = app.terminal_has_splits(&terminal_id);

                            let (status_symbol, status_color) =
                                terminal_status_indicator(app, &terminal_id, terminal_active);
                            let border_color =
                                terminal_status_border_color(app, &terminal_id, terminal_active);

                            terminals_column = terminals_column.push(
                                container(
                                    row![
                                        container("").width(Length::Fixed(26.0)),
                                        container("")
                                            .width(Length::Fixed(2.0))
                                            .height(Length::Fill)
                                            .style(move |_| ContainerStyle {
                                                background: Some(Background::Color(border_color)),
                                                ..Default::default()
                                            }),
                                        container(text(status_symbol).size(7).color(status_color))
                                            .padding([0, 4])
                                            .width(Length::Fixed(16.0)),
                                        {
                                            let name_element = text(&terminal.name)
                                                .size(12)
                                                .wrapping(Wrapping::None);
                                            let mut name_with_badge = row![
                                                container(name_element)
                                                    .width(Length::Fill)
                                                    .clip(true)
                                            ]
                                            .spacing(4)
                                            .width(Length::Fill);

                                            if terminal_has_splits {
                                                name_with_badge = name_with_badge.push(
                                                    container(
                                                        text("◫").size(9).color(rgb(176, 188, 212)),
                                                    )
                                                    .padding([1, 4])
                                                    .style(|_| subtle_badge_style()),
                                                );
                                            }

                                            if let Some(working_badge) =
                                                terminal_working_badge(app, &terminal_id)
                                            {
                                                name_with_badge =
                                                    name_with_badge.push(working_badge);
                                            }

                                            if let TerminalStatus::Error(code) =
                                                app.get_terminal_status(&terminal_id)
                                            {
                                                name_with_badge = name_with_badge.push(
                                                    container(
                                                        text(format!("{}", code))
                                                            .size(9)
                                                            .color(rgb(220, 80, 80)),
                                                    )
                                                    .padding([1, 4])
                                                    .style(|_| ContainerStyle {
                                                        background: Some(Background::Color(rgb(
                                                            40, 30, 32,
                                                        ))),
                                                        border: Border {
                                                            width: 1.0,
                                                            color: rgb(70, 50, 54),
                                                            radius: 8.0.into(),
                                                        },
                                                        ..Default::default()
                                                    }),
                                                );
                                            }

                                            button(
                                                container(name_with_badge)
                                                    .width(Length::Fill)
                                                    .clip(true),
                                            )
                                            .padding([2, 4])
                                            .style(move |_, status| {
                                                terminal_button_style(status, terminal_active)
                                            })
                                            .width(Length::Fill)
                                            .on_press(
                                                Message::SelectTerminal {
                                                    project_id: project_id.clone(),
                                                    terminal_id: terminal_id.clone(),
                                                },
                                            )
                                        },
                                        button(text("×").size(12))
                                            .padding([0, 5])
                                            .style(|_, status| subtle_delete_button_style(status))
                                            .on_press(Message::RemoveTerminal {
                                                project_id: project_id.clone(),
                                                worktree_id: worktree_id.clone(),
                                                terminal_id: terminal_id_for_action,
                                            }),
                                    ]
                                    .spacing(4)
                                    .align_y(Alignment::Center),
                                )
                                .padding([2, 6])
                                .style(move |_| terminal_row_style(terminal_active)),
                            );
                        }

                        worktree_list = worktree_list.push(
                            container(terminals_column)
                                .padding([0, 0])
                                .width(Length::Fill),
                        );
                    }
                }
            }

            project_card =
                project_card.push(container(worktree_list).padding([4, 6]).width(Length::Fill));
        }

        projects_column =
            projects_column.push(container(project_card).style(|_| project_group_style()));
    }

    list = list.push(container(projects_column).style(|_| project_group_style()));

    list = list.push(
        container(
            row![text(app.status.clone()).size(10).color(rgb(135, 140, 150)),]
                .align_y(Alignment::Center),
        )
        .padding([10, 10])
        .style(|_| status_bar_style()),
    );

    let sidebar_content = container(scrollable(list))
        .padding([6, 6])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| sidebar_style());

    // Resize handle at the right edge
    let resize_handle = mouse_area(
        container("")
            .width(Length::Fixed(6.0))
            .height(Length::Fill)
            .style(|_| resize_handle_style()),
    )
    .on_press(Message::SidebarResizeHandlePressed)
    .on_release(Message::SidebarResizeHandleReleased)
    .interaction(Interaction::ResizingHorizontally);

    row![sidebar_content, resize_handle]
        .width(Length::Fixed(app.sidebar_width_logical()))
        .height(Length::Fill)
        .into()
}
