use crate::app::shortcuts::{ShortcutAction, detect_shortcut};
use crate::app::state::{
    ADD_WORKTREE_PROJECT_SCROLL_ID, App, COMMAND_PALETTE_SCROLL_ID,
    DELETE_WORKTREE_PROJECT_SCROLL_ID, DELETE_WORKTREE_SCROLL_ID, Message, QUICK_OPEN_SCROLL_ID,
    QuickOpenEntryKind, REMOVE_PROJECT_SCROLL_ID, SIDEBAR_WIDTH_MAX, SIDEBAR_WIDTH_MIN,
};
use iced::keyboard::key::{Code, Key, Named, Physical};
use iced::{Task, keyboard, mouse, widget::operation};
use std::time::Instant;

pub(super) fn handle_keyboard(app: &mut App, event: keyboard::Event) -> Task<Message> {
    // Mark activity on keyboard input
    app.last_ghostty_activity = Instant::now();

    if let keyboard::Event::ModifiersChanged(modifiers) = &event {
        app.keyboard_modifiers = *modifiers;
    }
    if matches!(event, keyboard::Event::KeyPressed { .. }) {
        app.quick_open_ignore_next_query_change = false;
    }

    if app.suppress_next_key_release {
        match event {
            keyboard::Event::KeyReleased { .. } => {
                app.suppress_next_key_release = false;
                return Task::none();
            }
            keyboard::Event::KeyPressed { .. } => {
                app.suppress_next_key_release = false;
            }
            keyboard::Event::ModifiersChanged(_) => {}
        }
    }

    if app.terminal_search_is_open() {
        if let Some(task) = handle_terminal_search_keyboard(app, &event) {
            return task;
        }

        let allow_plain_rename = app.active_terminal_id().is_none();
        let shortcut_action =
            detect_shortcut(&event, app.keyboard_modifiers, allow_plain_rename, false);
        if let Some(action) = shortcut_action {
            return apply_shortcut(app, action);
        }

        return Task::none();
    }

    let allow_plain_rename = app.active_terminal_id().is_none();
    let modal_open = app.modal_open();
    let shortcut_action = detect_shortcut(
        &event,
        app.keyboard_modifiers,
        allow_plain_rename,
        modal_open,
    );
    if modal_open {
        if let Some(
            action @ (ShortcutAction::ModalCancel
            | ShortcutAction::ModalSubmit
            | ShortcutAction::ModalFocusNext
            | ShortcutAction::ModalFocusPrevious
            | ShortcutAction::ModalCloseQuickOpenTerminal
            | ShortcutAction::OpenQuickOpen
            | ShortcutAction::OpenCommandPalette),
        ) = shortcut_action
        {
            return apply_shortcut(app, action);
        }
        return Task::none();
    }

    if matches!(
        shortcut_action,
        Some(
            ShortcutAction::ToggleSidebar
                | ShortcutAction::NewTerminal
                | ShortcutAction::NewDetachedTerminal
                | ShortcutAction::CloseActiveTerminal
                | ShortcutAction::OpenInPreferredEditor
                | ShortcutAction::OpenInSecondaryEditor
                | ShortcutAction::OpenQuickOpen
                | ShortcutAction::OpenCommandPalette
                | ShortcutAction::OpenPreferences
                | ShortcutAction::AddBrowser
                | ShortcutAction::BrowserDevTools
                | ShortcutAction::RenameTerminal
                | ShortcutAction::RenameFocused
                | ShortcutAction::FontIncrease
                | ShortcutAction::FontDecrease
                | ShortcutAction::FontReset
                | ShortcutAction::NextTerminal
                | ShortcutAction::PreviousTerminal
                | ShortcutAction::SelectPinnedTerminal(_)
        )
    ) && let Some(action) = shortcut_action
    {
        return apply_shortcut(app, action);
    }

    let ghostty_claims_binding = app
        .active_ghostty_mut()
        .is_some_and(|ghostty| ghostty.key_event_is_binding(&event));
    if ghostty_claims_binding {
        let active_terminal_id = app.active_terminal_id();
        let terminal_id_clone = active_terminal_id.as_ref().cloned();
        if let Some(ref terminal_id) = terminal_id_clone {
            app.clear_awaiting_on_activity(terminal_id);
        }
        let mut should_refresh_branch = false;
        if let Some(ghostty) = app.active_ghostty_mut()
            && ghostty.handle_keyboard_event(&event)
        {
            ghostty.refresh();
            ghostty.force_tick();
            should_refresh_branch = true;
        }
        if app.process_runtime_actions() {
            app.sync_runtime_views();
        }
        if should_refresh_branch {
            return app.refresh_active_branch_task();
        }
        return super::terminal_search_focus_task(app);
    }

    if let Some(action) = shortcut_action {
        return apply_shortcut(app, action);
    }

    let mut should_refresh_branch = false;
    let active_terminal_id = app.active_terminal_id();
    let terminal_id_clone = active_terminal_id.as_ref().cloned();
    if let Some(ref terminal_id) = terminal_id_clone {
        app.clear_awaiting_on_activity(terminal_id);
    }
    if let Some(ghostty) = app.active_ghostty_mut()
        && ghostty.handle_keyboard_event(&event)
    {
        ghostty.refresh();
        ghostty.force_tick();
        should_refresh_branch = true;
    }
    if app.process_runtime_actions() {
        app.sync_runtime_views();
    }

    if should_refresh_branch {
        return app.refresh_active_branch_task();
    }
    super::terminal_search_focus_task(app)
}

pub(super) fn handle_mouse(app: &mut App, event: mouse::Event) -> Task<Message> {
    // Mark activity on mouse input (but not cursor movement)
    if !matches!(event, mouse::Event::CursorMoved { .. }) {
        app.last_ghostty_activity = Instant::now();
    }

    if app.modal_open() {
        return Task::none();
    }

    if app.sidebar_drag.is_some() {
        match event {
            mouse::Event::CursorMoved { position } => {
                app.cursor_position_logical = Some(position);
                return Task::none();
            }
            mouse::Event::CursorLeft => {
                app.cursor_position_logical = None;
                app.cancel_sidebar_drag();
                return Task::none();
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                if let Some(status) = app.finish_sidebar_drag() {
                    app.status = String::from(status);
                    return app.save_task();
                }

                return Task::none();
            }
            mouse::Event::ButtonPressed(_)
            | mouse::Event::ButtonReleased(_)
            | mouse::Event::WheelScrolled { .. }
            | mouse::Event::CursorEntered => {
                return Task::none();
            }
        }
    }

    let modifiers = app.keyboard_modifiers;
    let mut should_refresh_branch = false;
    match event {
        mouse::Event::CursorMoved { position } => {
            app.cursor_position_logical = Some(position);

            // Handle sidebar resizing
            if app.sidebar_resizing {
                let new_width = position.x.clamp(
                    SIDEBAR_WIDTH_MIN,
                    SIDEBAR_WIDTH_MAX.min(app.window_size.width - 50.0),
                );
                if (new_width - app.sidebar_width).abs() > 0.5 {
                    app.sidebar_width = new_width;
                    app.sync_runtime_views();
                }
            } else {
                let local = app.terminal_local_from_position(position);
                if let Some(ghostty) = app.active_ghostty_mut() {
                    match local {
                        Some((x, y)) => ghostty.handle_mouse_move(x, y, modifiers),
                        None => ghostty.handle_mouse_move(-1.0, -1.0, modifiers),
                    }
                }
            }
        }
        mouse::Event::CursorLeft => {
            app.cursor_position_logical = None;
            // Stop resizing if cursor leaves window
            if app.sidebar_resizing {
                app.sidebar_resizing = false;
                return app.save_task();
            }
            if let Some(ghostty) = app.active_ghostty_mut() {
                ghostty.handle_mouse_move(-1.0, -1.0, modifiers);
            }
        }
        mouse::Event::ButtonPressed(button) | mouse::Event::ButtonReleased(button) => {
            let pressed = matches!(event, mouse::Event::ButtonPressed(_));

            // Don't process terminal mouse events if we're resizing
            if app.sidebar_resizing {
                if !pressed {
                    app.sidebar_resizing = false;
                    return app.save_task();
                }
                return Task::none();
            }

            let mut focus_changed = false;
            let local = if pressed {
                app.cursor_position_logical.and_then(|position| {
                    app.focus_terminal_pane_from_position(position)
                        .map(|(x, y, changed)| {
                            focus_changed = changed;
                            (x, y)
                        })
                })
            } else {
                app.cursor_position_logical
                    .and_then(|position| app.terminal_local_from_position(position))
            };

            if focus_changed {
                app.sync_runtime_views();
            }

            if let Some((x, y)) = local
                && let Some(ghostty) = app.active_ghostty_mut()
            {
                ghostty.handle_mouse_move(x, y, modifiers);
                if pressed {
                    should_refresh_branch = true;
                }
                if ghostty.handle_mouse_button(button, pressed, modifiers) {
                    ghostty.refresh();
                    ghostty.force_tick();
                }
            }
        }
        mouse::Event::WheelScrolled { delta } => {
            // Don't scroll terminal if we're resizing
            if app.sidebar_resizing {
                return Task::none();
            }

            let local = app
                .cursor_position_logical
                .and_then(|position| app.terminal_local_from_position(position));

            if let Some((x, y)) = local {
                let (scroll_x, scroll_y, precision) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (x as f64, y as f64, false),
                    mouse::ScrollDelta::Pixels { x, y } => (x as f64, y as f64, true),
                };

                if let Some(ghostty) = app.active_ghostty_mut() {
                    ghostty.handle_mouse_move(x, y, modifiers);
                    ghostty.handle_mouse_scroll(scroll_x, scroll_y, precision);
                    ghostty.refresh();
                    ghostty.force_tick();
                    should_refresh_branch = true;
                }
            }
        }
        mouse::Event::CursorEntered => {}
    }
    if should_refresh_branch {
        app.refresh_active_branch_task()
    } else {
        Task::none()
    }
}

pub(super) fn apply_shortcut(app: &mut App, action: ShortcutAction) -> Task<Message> {
    match action {
        ShortcutAction::ToggleSidebar => super::update(app, Message::ToggleSidebar),
        ShortcutAction::NewTerminal => {
            if let Some((project_id, worktree_id)) = app.active_worktree_ids() {
                super::update(
                    app,
                    Message::AddTerminal {
                        project_id,
                        worktree_id,
                    },
                )
            } else {
                app.status = String::from("No active worktree to create a terminal in");
                Task::none()
            }
        }
        ShortcutAction::NewDetachedTerminal => super::update(app, Message::AddDetachedTerminal),
        ShortcutAction::CloseActiveTerminal => super::update(app, Message::CloseActiveTerminal),
        ShortcutAction::OpenInPreferredEditor => super::update(app, Message::OpenInPreferredEditor),
        ShortcutAction::OpenInSecondaryEditor => super::update(app, Message::OpenInSecondaryEditor),
        ShortcutAction::OpenQuickOpen => {
            super::update(app, Message::OpenQuickOpen(!app.quick_open_open))
        }
        ShortcutAction::OpenCommandPalette => {
            super::update(app, Message::OpenCommandPalette(!app.command_palette_open))
        }
        ShortcutAction::OpenPreferences => super::update(app, Message::OpenPreferences(true)),
        ShortcutAction::AddBrowser => super::update(app, Message::AddBrowser),
        ShortcutAction::BrowserDevTools => super::update(app, Message::BrowserDevTools),
        ShortcutAction::RenameTerminal => super::update(app, Message::StartRenameTerminal),
        ShortcutAction::RenameFocused => super::update(app, Message::StartRenameFocused),
        ShortcutAction::FontIncrease => {
            if let Some(ghostty) = app.active_ghostty_mut() {
                let _ = ghostty.binding_action("increase_font_size:1");
                ghostty.refresh();
                ghostty.force_tick();
            }
            Task::none()
        }
        ShortcutAction::FontDecrease => {
            if let Some(ghostty) = app.active_ghostty_mut() {
                let _ = ghostty.binding_action("decrease_font_size:1");
                ghostty.refresh();
                ghostty.force_tick();
            }
            Task::none()
        }
        ShortcutAction::FontReset => {
            if let Some(ghostty) = app.active_ghostty_mut() {
                let _ = ghostty.binding_action("reset_font_size");
                ghostty.refresh();
                ghostty.force_tick();
            }
            Task::none()
        }
        ShortcutAction::NextTerminal => super::update(app, Message::SwitchTerminalByOffset(1)),
        ShortcutAction::PreviousTerminal => super::update(app, Message::SwitchTerminalByOffset(-1)),
        ShortcutAction::SelectPinnedTerminal(slot) => {
            super::update(app, Message::SelectPinnedTerminalSlot(slot))
        }
        ShortcutAction::ModalCancel => {
            app.suppress_next_key_release = true;
            if app.delete_worktree_picker.is_some() {
                return super::update(app, Message::DeleteWorktreeCancel);
            }
            if app.delete_worktree_project_picker_open {
                return super::update(app, Message::DeleteWorktreeProjectCancel);
            }
            if app.remove_project_picker_open {
                return super::update(app, Message::RemoveProjectCancel);
            }
            if app.add_worktree_project_picker_open {
                return super::update(app, Message::AddWorktreeProjectCancel);
            }
            if app.project_context_menu.is_some() {
                return super::update(app, Message::CloseProjectContextMenu);
            }
            if app.worktree_context_menu.is_some() {
                return super::update(app, Message::CloseWorktreeContextMenu);
            }
            if app.rename_dialog.is_some() {
                return super::update(app, Message::RenameCancel);
            }
            if app.add_worktree_dialog.is_some() {
                return super::update(app, Message::AddWorktreeCancel);
            }
            if app.quick_open_open {
                return super::update(app, Message::OpenQuickOpen(false));
            }
            if app.command_palette_open {
                return super::update(app, Message::OpenCommandPalette(false));
            }
            if app.preferences_open {
                return super::update(app, Message::OpenPreferences(false));
            }
            Task::none()
        }
        ShortcutAction::ModalSubmit => {
            app.suppress_next_key_release = true;
            if app.delete_worktree_picker.is_some() {
                return super::update(app, Message::DeleteWorktreeSubmit);
            }
            if app.delete_worktree_project_picker_open {
                return super::update(app, Message::DeleteWorktreeProjectSubmit);
            }
            if app.remove_project_picker_open {
                return super::update(app, Message::RemoveProjectSubmit);
            }
            if app.add_worktree_project_picker_open {
                return super::update(app, Message::AddWorktreeProjectSubmit);
            }
            if app.rename_dialog.is_some() {
                return super::update(app, Message::RenameCommit);
            }
            if app.add_worktree_dialog.is_some() {
                return super::update(app, Message::AddWorktreeCommit);
            }
            if app.command_palette_open {
                return super::update(app, Message::CommandPaletteSubmit);
            }
            if app.quick_open_open {
                return super::update(app, Message::QuickOpenSubmit);
            }
            Task::none()
        }
        ShortcutAction::ModalFocusNext => {
            app.suppress_next_key_release = true;
            if let Some(project_id) = app
                .delete_worktree_picker
                .as_ref()
                .map(|picker| picker.project_id.clone())
            {
                let entries = app.delete_worktree_entries(&project_id);
                let selected_index = if let Some(picker) = app.delete_worktree_picker.as_mut() {
                    if !entries.is_empty() {
                        picker.selected_index = (picker.selected_index + 1) % entries.len();
                    }
                    picker.selected_index
                } else {
                    0
                };
                modal_selection_scroll_task(
                    DELETE_WORKTREE_SCROLL_ID,
                    selected_index,
                    entries.len(),
                )
            } else if app.delete_worktree_project_picker_open {
                let entries = app.delete_worktree_project_entries();
                if !entries.is_empty() {
                    app.delete_worktree_project_selected_index =
                        (app.delete_worktree_project_selected_index + 1) % entries.len();
                }
                modal_selection_scroll_task(
                    DELETE_WORKTREE_PROJECT_SCROLL_ID,
                    app.delete_worktree_project_selected_index,
                    entries.len(),
                )
            } else if app.remove_project_picker_open {
                let entries = app.remove_project_entries();
                if !entries.is_empty() {
                    app.remove_project_selected_index =
                        (app.remove_project_selected_index + 1) % entries.len();
                }
                modal_selection_scroll_task(
                    REMOVE_PROJECT_SCROLL_ID,
                    app.remove_project_selected_index,
                    entries.len(),
                )
            } else if app.add_worktree_project_picker_open {
                let entries = app.add_worktree_project_entries();
                if !entries.is_empty() {
                    app.add_worktree_project_selected_index =
                        (app.add_worktree_project_selected_index + 1) % entries.len();
                }
                modal_selection_scroll_task(
                    ADD_WORKTREE_PROJECT_SCROLL_ID,
                    app.add_worktree_project_selected_index,
                    entries.len(),
                )
            } else if app.command_palette_open {
                let entries = app.command_palette_entries();
                if !entries.is_empty() {
                    app.command_palette_selected_index =
                        (app.command_palette_selected_index + 1) % entries.len();
                }
                modal_selection_scroll_task(
                    COMMAND_PALETTE_SCROLL_ID,
                    app.command_palette_selected_index,
                    entries.len(),
                )
            } else if app.quick_open_open {
                let entries = app.quick_open_entries();
                if !entries.is_empty() {
                    app.quick_open_selected_index =
                        (app.quick_open_selected_index + 1) % entries.len();
                }
                modal_selection_scroll_task(
                    QUICK_OPEN_SCROLL_ID,
                    app.quick_open_selected_index,
                    entries.len(),
                )
            } else {
                operation::focus_next()
            }
        }
        ShortcutAction::ModalFocusPrevious => {
            app.suppress_next_key_release = true;
            if let Some(project_id) = app
                .delete_worktree_picker
                .as_ref()
                .map(|picker| picker.project_id.clone())
            {
                let entries = app.delete_worktree_entries(&project_id);
                let selected_index = if let Some(picker) = app.delete_worktree_picker.as_mut() {
                    if !entries.is_empty() {
                        let count = entries.len();
                        picker.selected_index = if picker.selected_index == 0 {
                            count - 1
                        } else {
                            picker.selected_index - 1
                        };
                    }
                    picker.selected_index
                } else {
                    0
                };
                modal_selection_scroll_task(
                    DELETE_WORKTREE_SCROLL_ID,
                    selected_index,
                    entries.len(),
                )
            } else if app.delete_worktree_project_picker_open {
                let entries = app.delete_worktree_project_entries();
                if !entries.is_empty() {
                    let count = entries.len();
                    app.delete_worktree_project_selected_index =
                        if app.delete_worktree_project_selected_index == 0 {
                            count - 1
                        } else {
                            app.delete_worktree_project_selected_index - 1
                        };
                }
                modal_selection_scroll_task(
                    DELETE_WORKTREE_PROJECT_SCROLL_ID,
                    app.delete_worktree_project_selected_index,
                    entries.len(),
                )
            } else if app.remove_project_picker_open {
                let entries = app.remove_project_entries();
                if !entries.is_empty() {
                    let count = entries.len();
                    app.remove_project_selected_index = if app.remove_project_selected_index == 0 {
                        count - 1
                    } else {
                        app.remove_project_selected_index - 1
                    };
                }
                modal_selection_scroll_task(
                    REMOVE_PROJECT_SCROLL_ID,
                    app.remove_project_selected_index,
                    entries.len(),
                )
            } else if app.add_worktree_project_picker_open {
                let entries = app.add_worktree_project_entries();
                if !entries.is_empty() {
                    let count = entries.len();
                    app.add_worktree_project_selected_index =
                        if app.add_worktree_project_selected_index == 0 {
                            count - 1
                        } else {
                            app.add_worktree_project_selected_index - 1
                        };
                }
                modal_selection_scroll_task(
                    ADD_WORKTREE_PROJECT_SCROLL_ID,
                    app.add_worktree_project_selected_index,
                    entries.len(),
                )
            } else if app.command_palette_open {
                let entries = app.command_palette_entries();
                if !entries.is_empty() {
                    let count = entries.len();
                    app.command_palette_selected_index = if app.command_palette_selected_index == 0
                    {
                        count - 1
                    } else {
                        app.command_palette_selected_index - 1
                    };
                }
                modal_selection_scroll_task(
                    COMMAND_PALETTE_SCROLL_ID,
                    app.command_palette_selected_index,
                    entries.len(),
                )
            } else if app.quick_open_open {
                let entries = app.quick_open_entries();
                if !entries.is_empty() {
                    let count = entries.len();
                    app.quick_open_selected_index = if app.quick_open_selected_index == 0 {
                        count - 1
                    } else {
                        app.quick_open_selected_index - 1
                    };
                }
                modal_selection_scroll_task(
                    QUICK_OPEN_SCROLL_ID,
                    app.quick_open_selected_index,
                    entries.len(),
                )
            } else {
                operation::focus_previous()
            }
        }
        ShortcutAction::ModalCloseQuickOpenTerminal => {
            if app.quick_open_open {
                let entries = app.quick_open_entries();
                let Some(entry) = entries.get(app.quick_open_selected_index) else {
                    return Task::none();
                };
                let QuickOpenEntryKind::ExistingTerminal { terminal_id } = &entry.kind else {
                    return Task::none();
                };

                app.suppress_next_key_release = true;
                app.quick_open_ignore_next_query_change = true;
                super::update(app, Message::QuickOpenCloseTerminal(terminal_id.clone()))
            } else {
                Task::none()
            }
        }
    }
}

fn modal_selection_scroll_task(
    scroll_id: &'static str,
    selected_index: usize,
    entry_count: usize,
) -> Task<Message> {
    let y = if entry_count <= 1 {
        0.0
    } else {
        selected_index as f32 / (entry_count - 1) as f32
    };

    operation::snap_to(scroll_id, operation::RelativeOffset { x: 0.0, y })
}

fn handle_terminal_search_keyboard(
    app: &mut App,
    event: &keyboard::Event,
) -> Option<Task<Message>> {
    let keyboard::Event::KeyPressed {
        key,
        physical_key,
        modifiers,
        ..
    } = event
    else {
        return None;
    };

    let modifiers = *modifiers | app.keyboard_modifiers;
    if is_named_key(key, Named::Escape)
        && !modifiers.logo()
        && !modifiers.control()
        && !modifiers.alt()
    {
        return Some(super::update(app, Message::TerminalSearchClose));
    }

    if is_named_key(key, Named::Enter)
        && !modifiers.logo()
        && !modifiers.control()
        && !modifiers.alt()
    {
        let message = if modifiers.shift() {
            Message::TerminalSearchPrevious
        } else {
            Message::TerminalSearchNext
        };
        return Some(super::update(app, message));
    }

    if modifiers.logo() && !modifiers.control() && !modifiers.alt() {
        if is_key_char(key, physical_key, "f", Code::KeyF) && !modifiers.shift() {
            return Some(super::terminal_search_focus_task(app));
        }

        if is_key_char(key, physical_key, "g", Code::KeyG) {
            let message = if modifiers.shift() {
                Message::TerminalSearchPrevious
            } else {
                Message::TerminalSearchNext
            };
            return Some(super::update(app, message));
        }

        if is_key_char(key, physical_key, "e", Code::KeyE) && !modifiers.shift() {
            let Some(search) = app.terminal_search.as_ref() else {
                return Some(Task::none());
            };
            let terminal_id = search.terminal_id.clone();
            let surface_ptr = search.surface_ptr;
            let _ =
                app.perform_surface_binding_action(&terminal_id, surface_ptr, "search_selection");
            if app.process_runtime_actions() {
                app.sync_runtime_views();
            }
            return Some(super::terminal_search_focus_task(app));
        }
    }

    None
}

fn is_named_key(key: &keyboard::Key, expected: Named) -> bool {
    matches!(key.as_ref(), Key::Named(named) if named == expected)
}

fn is_key_char(key: &keyboard::Key, physical_key: &Physical, expected: &str, code: Code) -> bool {
    matches!(key.as_ref(), Key::Character(value) if value.eq_ignore_ascii_case(expected))
        || matches!(physical_key, Physical::Code(value) if *value == code)
}
