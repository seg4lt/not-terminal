use super::*;
use crate::app::state::{
    ADD_WORKTREE_PROJECT_SCROLL_ID, App, COMMAND_PALETTE_SCROLL_ID,
    DELETE_WORKTREE_PROJECT_SCROLL_ID, DELETE_WORKTREE_SCROLL_ID, Message, QUICK_OPEN_SCROLL_ID,
    QuickOpenEntryKind,
};
use iced::widget::{button, checkbox, container, row, scrollable, text, text_input};
use iced::{Element, Length};

pub(super) fn modal_overlay(app: &App) -> Option<Element<'_, Message>> {
    if app.command_palette_open {
        let entries = app.command_palette_entries();
        let input = text_input("Run a command", &app.command_palette_query)
            .id("command-palette-input")
            .on_input(Message::CommandPaletteQueryChanged)
            .on_submit(Message::CommandPaletteSubmit)
            .padding(6)
            .size(14)
            .style(|_, status| input_style(status))
            .width(Length::Fill);
        let mut list = iced::widget::column![].spacing(6).width(Length::Fill);

        for (idx, entry) in entries.iter().enumerate() {
            let is_selected = idx == app.command_palette_selected_index;
            let title = entry.title.clone();
            let detail = entry.detail.clone();
            let badge_text = if is_selected {
                rgb(255, 255, 255)
            } else {
                rgb(145, 152, 165)
            };
            let badge_bg = if is_selected {
                Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.18,
                }
            } else {
                rgb(35, 40, 50)
            };

            list = list.push(
                button(
                    row![
                        container(text("CMD").size(10).color(badge_text))
                            .padding([1, 5])
                            .style(move |_| ContainerStyle {
                                background: Some(Background::Color(badge_bg)),
                                border: Border {
                                    width: 0.0,
                                    color: Color::TRANSPARENT,
                                    radius: 3.0.into(),
                                },
                                ..Default::default()
                            }),
                        iced::widget::column![
                            text(title).size(13),
                            text(detail).size(11).color(rgb(138, 144, 156)),
                        ]
                        .spacing(2)
                    ]
                    .spacing(8),
                )
                .width(Length::Fill)
                .padding([6, 6])
                .style(move |_, status| {
                    if is_selected {
                        selected_entry_style(status)
                    } else {
                        tree_icon_button_style(status)
                    }
                })
                .on_press(Message::CommandPaletteSelect(idx)),
            );
        }

        if entries.is_empty() {
            list = list.push(container(text("No matching commands").size(12)).padding([4, 2]));
        }

        let panel = container(
            iced::widget::column![
                row![
                    text("Command Palette").size(16),
                    button(text("Close").size(12))
                        .style(|_, status| toolbar_button_style(status))
                        .on_press(Message::OpenCommandPalette(false)),
                ]
                .spacing(8),
                text("Enter: run command")
                    .size(11)
                    .color(rgb(138, 144, 156)),
                input,
                scrollable(list)
                    .id(COMMAND_PALETTE_SCROLL_ID)
                    .height(Length::Fill),
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

    if app.quick_open_open {
        let entries = app.quick_open_entries();
        let input = text_input("Search terminal", &app.quick_open_query)
            .id("quick-open-input")
            .on_input(Message::QuickOpenQueryChanged)
            .on_submit(Message::QuickOpenSubmit)
            .padding(6)
            .size(14)
            .style(|_, status| input_style(status))
            .width(Length::Fill);
        let mut list = iced::widget::column![].spacing(6).width(Length::Fill);

        let mut create_header_shown = false;
        for (idx, entry) in entries.iter().enumerate() {
            let is_create_entry = matches!(entry.kind, QuickOpenEntryKind::CreateTerminal { .. });
            if is_create_entry && !create_header_shown {
                list = list.push(
                    container(
                        text("Create Terminal Targets")
                            .size(11)
                            .color(rgb(132, 156, 182)),
                    )
                    .padding([8, 6])
                    .style(|_| quick_open_section_label_style()),
                );
                create_header_shown = true;
            }

            let is_selected = idx == app.quick_open_selected_index;
            let style = if is_selected && is_create_entry {
                quick_open_create_selected_entry_style
            } else if is_selected {
                selected_entry_style
            } else if is_create_entry {
                quick_open_create_entry_style
            } else {
                tree_icon_button_style
            };
            let row_text = if is_create_entry {
                format!(
                    "+ New terminal in {} / {}",
                    entry.project_name, entry.worktree_name
                )
            } else {
                format!(
                    "{} / {} / {}",
                    entry.project_name, entry.worktree_name, entry.terminal_name
                )
            };
            let badge = if is_create_entry { "NEW" } else { "TERM" };
            let badge_text = if is_selected {
                rgb(255, 255, 255)
            } else if is_create_entry {
                rgb(177, 225, 194)
            } else {
                rgb(145, 152, 165)
            };
            let badge_bg = if is_selected {
                Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: if is_create_entry { 0.22 } else { 0.18 },
                }
            } else if is_create_entry {
                rgb(33, 59, 43)
            } else {
                rgb(35, 40, 50)
            };

            list = list.push(
                button(
                    row![
                        container(text(badge).size(10).color(badge_text))
                            .padding([1, 5])
                            .style(move |_| ContainerStyle {
                                background: Some(Background::Color(badge_bg)),
                                border: Border {
                                    width: 0.0,
                                    color: Color::TRANSPARENT,
                                    radius: 3.0.into(),
                                },
                                ..Default::default()
                            }),
                        text(row_text).size(13),
                    ]
                    .spacing(8),
                )
                .width(Length::Fill)
                .padding([4, 6])
                .style(move |_, status| style(status))
                .on_press(Message::QuickOpenSelect(idx)),
            );
        }

        if entries.is_empty() {
            list = list.push(
                container(text("No terminals or worktrees available").size(12)).padding([4, 2]),
            );
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
                text("Enter: open terminal or create in selected worktree")
                    .size(11)
                    .color(rgb(138, 144, 156)),
                input,
                scrollable(list)
                    .id(QUICK_OPEN_SCROLL_ID)
                    .height(Length::Fill),
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

    if app.delete_worktree_project_picker_open {
        let entries = app.delete_worktree_project_entries();
        let mut list = iced::widget::column![].spacing(6).width(Length::Fill);

        for (idx, entry) in entries.iter().enumerate() {
            let is_selected = idx == app.delete_worktree_project_selected_index;
            let badge_text = if is_selected {
                rgb(255, 255, 255)
            } else {
                rgb(145, 152, 165)
            };
            let badge_bg = if is_selected {
                Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.18,
                }
            } else {
                rgb(35, 40, 50)
            };
            let detail = if entry.worktree_count == 1 {
                String::from("1 removable worktree")
            } else {
                format!("{} removable worktrees", entry.worktree_count)
            };

            list = list.push(
                button(
                    row![
                        container(text("PROJ").size(10).color(badge_text))
                            .padding([1, 5])
                            .style(move |_| ContainerStyle {
                                background: Some(Background::Color(badge_bg)),
                                border: Border {
                                    width: 0.0,
                                    color: Color::TRANSPARENT,
                                    radius: 3.0.into(),
                                },
                                ..Default::default()
                            }),
                        iced::widget::column![
                            text(entry.project_name.clone()).size(13),
                            text(detail).size(11).color(rgb(138, 144, 156)),
                        ]
                        .spacing(2)
                    ]
                    .spacing(8),
                )
                .width(Length::Fill)
                .padding([6, 6])
                .style(move |_, status| {
                    if is_selected {
                        selected_entry_style(status)
                    } else {
                        tree_icon_button_style(status)
                    }
                })
                .on_press(Message::DeleteWorktreeProjectSelect(idx)),
            );
        }

        if entries.is_empty() {
            list = list.push(container(text("No projects available").size(12)).padding([4, 2]));
        }

        let panel = container(
            iced::widget::column![
                row![
                    text("Choose Project").size(16),
                    button(text("Close").size(12))
                        .style(|_, status| toolbar_button_style(status))
                        .on_press(Message::DeleteWorktreeProjectCancel),
                ]
                .spacing(8),
                text("Enter: choose project and then pick a worktree to remove")
                    .size(11)
                    .color(rgb(138, 144, 156)),
                scrollable(list)
                    .id(DELETE_WORKTREE_PROJECT_SCROLL_ID)
                    .height(Length::Fill),
            ]
            .spacing(8),
        )
        .padding(12)
        .width(Length::Fixed(520.0))
        .height(Length::Fixed(360.0))
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

    if let Some(picker) = &app.delete_worktree_picker {
        let entries = app.delete_worktree_entries(&picker.project_id);
        let mut list = iced::widget::column![].spacing(6).width(Length::Fill);

        for (idx, entry) in entries.iter().enumerate() {
            let is_selected = idx == picker.selected_index;
            let badge_text = if is_selected {
                rgb(255, 255, 255)
            } else {
                rgb(145, 152, 165)
            };
            let badge_bg = if is_selected {
                Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.18,
                }
            } else {
                rgb(35, 40, 50)
            };

            list = list.push(
                button(
                    row![
                        container(text("W").size(10).color(badge_text))
                            .padding([1, 5])
                            .style(move |_| ContainerStyle {
                                background: Some(Background::Color(badge_bg)),
                                border: Border {
                                    width: 0.0,
                                    color: Color::TRANSPARENT,
                                    radius: 3.0.into(),
                                },
                                ..Default::default()
                            }),
                        iced::widget::column![
                            text(entry.worktree_name.clone()).size(13),
                            text(entry.project_name.clone())
                                .size(11)
                                .color(rgb(138, 144, 156)),
                        ]
                        .spacing(2)
                    ]
                    .spacing(8),
                )
                .width(Length::Fill)
                .padding([6, 6])
                .style(move |_, status| {
                    if is_selected {
                        selected_entry_style(status)
                    } else {
                        tree_icon_button_style(status)
                    }
                })
                .on_press(Message::DeleteWorktreeSelect(idx)),
            );
        }

        if entries.is_empty() {
            list = list
                .push(container(text("No removable worktrees available").size(12)).padding([4, 2]));
        }

        let panel = container(
            iced::widget::column![
                row![
                    text("Choose Worktree").size(16),
                    button(text("Close").size(12))
                        .style(|_, status| toolbar_button_style(status))
                        .on_press(Message::DeleteWorktreeCancel),
                ]
                .spacing(8),
                text("Enter: remove the selected worktree immediately")
                    .size(11)
                    .color(rgb(138, 144, 156)),
                scrollable(list)
                    .id(DELETE_WORKTREE_SCROLL_ID)
                    .height(Length::Fill),
            ]
            .spacing(8),
        )
        .padding(12)
        .width(Length::Fixed(520.0))
        .height(Length::Fixed(360.0))
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

    if app.add_worktree_project_picker_open {
        let entries = app.add_worktree_project_entries();
        let mut list = iced::widget::column![].spacing(6).width(Length::Fill);

        for (idx, entry) in entries.iter().enumerate() {
            let is_selected = idx == app.add_worktree_project_selected_index;
            let badge_text = if is_selected {
                rgb(255, 255, 255)
            } else {
                rgb(145, 152, 165)
            };
            let badge_bg = if is_selected {
                Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.18,
                }
            } else {
                rgb(35, 40, 50)
            };
            let detail = if entry.worktree_count == 1 {
                String::from("1 worktree")
            } else {
                format!("{} worktrees", entry.worktree_count)
            };

            list = list.push(
                button(
                    row![
                        container(text("PROJ").size(10).color(badge_text))
                            .padding([1, 5])
                            .style(move |_| ContainerStyle {
                                background: Some(Background::Color(badge_bg)),
                                border: Border {
                                    width: 0.0,
                                    color: Color::TRANSPARENT,
                                    radius: 3.0.into(),
                                },
                                ..Default::default()
                            }),
                        iced::widget::column![
                            text(entry.project_name.clone()).size(13),
                            text(detail).size(11).color(rgb(138, 144, 156)),
                        ]
                        .spacing(2)
                    ]
                    .spacing(8),
                )
                .width(Length::Fill)
                .padding([6, 6])
                .style(move |_, status| {
                    if is_selected {
                        selected_entry_style(status)
                    } else {
                        tree_icon_button_style(status)
                    }
                })
                .on_press(Message::AddWorktreeProjectSelect(idx)),
            );
        }

        if entries.is_empty() {
            list = list.push(container(text("No projects available").size(12)).padding([4, 2]));
        }

        let panel = container(
            iced::widget::column![
                row![
                    text("Choose Project").size(16),
                    button(text("Close").size(12))
                        .style(|_, status| toolbar_button_style(status))
                        .on_press(Message::AddWorktreeProjectCancel),
                ]
                .spacing(8),
                text("Enter: choose project and open the add-worktree form")
                    .size(11)
                    .color(rgb(138, 144, 156)),
                scrollable(list)
                    .id(ADD_WORKTREE_PROJECT_SCROLL_ID)
                    .height(Length::Fill),
            ]
            .spacing(8),
        )
        .padding(12)
        .width(Length::Fixed(520.0))
        .height(Length::Fixed(360.0))
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

    if let Some(_menu) = &app.project_context_menu {
        let actions = iced::widget::column![
            text("Project Actions").size(16),
            button(text("Rescan project").size(12))
                .style(|_, status| toolbar_button_style(status))
                .on_press(Message::ProjectContextMenuProjectRescan),
            button(text("Remove project").size(12))
                .style(|_, status| subtle_delete_button_style(status))
                .on_press(Message::ProjectContextMenuRemoveProject),
            button(text("Close").size(12))
                .style(|_, status| toolbar_button_style(status))
                .on_press(Message::CloseProjectContextMenu),
        ]
        .spacing(8);

        let panel = container(actions)
            .padding(12)
            .width(Length::Fixed(260.0))
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

    if let Some(menu) = &app.worktree_context_menu {
        let project_id = menu.project_id.clone();
        let worktree_id = menu.worktree_id.clone();
        let mut actions = iced::widget::column![
            text("Worktree Actions").size(16),
            button(text("New terminal").size(12))
                .style(|_, status| toolbar_button_style(status))
                .on_press(Message::WorktreeContextMenuNewTerminal),
            button(text("Rename worktree").size(12))
                .style(|_, status| toolbar_button_style(status))
                .on_press(Message::WorktreeContextMenuRenameWorktree),
            button(text("Remove worktree").size(12))
                .style(|_, status| subtle_delete_button_style(status))
                .on_press(Message::RemoveWorktree {
                    project_id,
                    worktree_id,
                }),
        ]
        .spacing(8);

        actions = actions.push(
            button(text("Close").size(12))
                .style(|_, status| toolbar_button_style(status))
                .on_press(Message::CloseWorktreeContextMenu),
        );

        let panel = container(actions)
            .padding(12)
            .width(Length::Fixed(260.0))
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
        let (title, placeholder, helper_text) = match dialog.target {
            crate::app::state::RenameTarget::Worktree { .. } => (
                "Rename Worktree Label",
                "Worktree label",
                Some("Shown on the worktree row. The project name stays separate."),
            ),
            crate::app::state::RenameTarget::Terminal { .. } => {
                ("Rename Terminal", "Terminal name", None)
            }
            crate::app::state::RenameTarget::DetachedTerminal { .. } => {
                ("Rename Terminal", "Terminal name", None)
            }
        };

        let mut content = iced::widget::column![
            text(title).size(16),
            text_input(placeholder, &dialog.value)
                .id("rename-input")
                .on_input(Message::RenameValueChanged)
                .on_submit(Message::RenameCommit)
                .padding(6)
                .size(14)
                .style(|_, status| input_style(status))
                .width(Length::Fill),
        ]
        .spacing(8);

        if let Some(helper_text) = helper_text {
            content = content.push(
                text(helper_text)
                    .size(11)
                    .color(rgb(138, 144, 156))
                    .width(Length::Fill),
            );
        }

        content = content.push(
            row![
                button(text("Cancel").size(12))
                    .style(|_, status| toolbar_button_style(status))
                    .on_press(Message::RenameCancel),
                button(text("Save").size(12))
                    .style(|_, status| toolbar_button_style(status))
                    .on_press(Message::RenameCommit),
            ]
            .spacing(8),
        );

        let panel = container(content)
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
        let enable_browsers = app.persisted.ui.enable_browsers;
        let browser_shortcut_color = if enable_browsers {
            rgb(145, 150, 160)
        } else {
            rgb(100, 105, 115)
        };

        let panel = container(
            iced::widget::column![
                row![
                    text("Preferences").size(16),
                    button(text("Close").size(12))
                        .style(|_, status| toolbar_button_style(status))
                        .on_press(Message::OpenPreferences(false)),
                ]
                .spacing(8),
                text("General").size(14),
                checkbox(app.show_native_title_bar)
                    .label("Show native title bar")
                    .on_toggle(Message::SetShowNativeTitleBar)
                    .text_size(13),
                checkbox(enable_browsers)
                    .label("Enable browsers feature")
                    .on_toggle(Message::SetEnableBrowsers)
                    .text_size(13),
                text("Preferred editor command").size(13),
                text_input("zed", &app.persisted.ui.preferred_editor_command)
                    .on_input(Message::SetPreferredEditorCommand)
                    .padding(6)
                    .size(13)
                    .style(|_, status| input_style(status))
                    .width(Length::Fill),
                text("Examples: zed, code, idea")
                    .size(11)
                    .color(rgb(138, 144, 156)),
                text("Shortcuts").size(14),
                text("Cmd+1: Toggle sidebar").size(12),
                text("Cmd+T: New terminal in active worktree").size(12),
                text("Cmd+Shift+T: New detached terminal").size(12),
                text("Cmd+W: Close active terminal").size(12),
                text("Cmd+O: Open active worktree in preferred editor").size(12),
                text("Cmd+P: Quick open").size(12),
                text("Cmd+Shift+P: Command palette").size(12),
                text("Cmd+Option+Shift+O: Toggle app focus").size(12),
                text("Quick Open: Cmd+Backspace closes selected terminal").size(12),
                container(text("Cmd+B: New browser").size(12)).style(move |_| ContainerStyle {
                    text_color: Some(browser_shortcut_color),
                    ..Default::default()
                }),
                container(text("Cmd+Option+I: Browser DevTools").size(12)).style(move |_| {
                    ContainerStyle {
                        text_color: Some(browser_shortcut_color),
                        ..Default::default()
                    }
                }),
                text("Cmd+, : Preferences").size(12),
                text("Cmd+=/-/0: Font size").size(12),
                text("Cmd+Shift+[ or ]: Previous/Next terminal").size(12),
                text("Cmd+R: Rename active terminal").size(12),
                text("F2: Rename focused item").size(12),
                text("Cmd+H/J/K/L: Move between split panes").size(12),
            ]
            .spacing(6),
        )
        .padding(12)
        .width(Length::Fixed(500.0))
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
