use crate::app::shortcuts::{ShortcutAction, detect_shortcut};
use crate::app::state::{App, Message, SIDEBAR_WIDTH_MAX, SIDEBAR_WIDTH_MIN};
use iced::{Task, keyboard, mouse, widget::operation};
use std::time::Instant;

pub(super) fn handle_keyboard(app: &mut App, event: keyboard::Event) -> Task<Message> {
    // Mark activity on keyboard input
    app.last_ghostty_activity = Instant::now();

    if let keyboard::Event::ModifiersChanged(modifiers) = &event {
        app.keyboard_modifiers = *modifiers;
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

    let allow_plain_rename = app.active_terminal_id().is_none();
    let modal_open = app.modal_open();
    let shortcut_action = detect_shortcut(&event, allow_plain_rename, modal_open);
    if modal_open {
        if let Some(
            action @ (ShortcutAction::ModalCancel
            | ShortcutAction::ModalSubmit
            | ShortcutAction::ModalFocusNext
            | ShortcutAction::ModalFocusPrevious),
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
                | ShortcutAction::OpenQuickOpen
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
        return Task::none();
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
    Task::none()
}

pub(super) fn handle_mouse(app: &mut App, event: mouse::Event) -> Task<Message> {
    // Mark activity on mouse input (but not cursor movement)
    if !matches!(event, mouse::Event::CursorMoved { .. }) {
        app.last_ghostty_activity = Instant::now();
    }

    if app.modal_open() {
        return Task::none();
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

fn apply_shortcut(app: &mut App, action: ShortcutAction) -> Task<Message> {
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
        ShortcutAction::OpenQuickOpen => super::update(app, Message::OpenQuickOpen(true)),
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
        ShortcutAction::ModalCancel => {
            app.suppress_next_key_release = true;
            if app.rename_dialog.is_some() {
                return super::update(app, Message::RenameCancel);
            }
            if app.add_worktree_dialog.is_some() {
                return super::update(app, Message::AddWorktreeCancel);
            }
            if app.quick_open_open {
                return super::update(app, Message::OpenQuickOpen(false));
            }
            if app.preferences_open {
                return super::update(app, Message::OpenPreferences(false));
            }
            Task::none()
        }
        ShortcutAction::ModalSubmit => {
            app.suppress_next_key_release = true;
            if app.rename_dialog.is_some() {
                return super::update(app, Message::RenameCommit);
            }
            if app.add_worktree_dialog.is_some() {
                return super::update(app, Message::AddWorktreeCommit);
            }
            if app.quick_open_open {
                return super::update(app, Message::QuickOpenSubmit);
            }
            Task::none()
        }
        ShortcutAction::ModalFocusNext => {
            app.suppress_next_key_release = true;
            if app.quick_open_open {
                let entries = app.quick_open_entries();
                if !entries.is_empty() {
                    app.quick_open_selected_index =
                        (app.quick_open_selected_index + 1) % entries.len().min(24);
                }
                Task::none()
            } else {
                operation::focus_next()
            }
        }
        ShortcutAction::ModalFocusPrevious => {
            app.suppress_next_key_release = true;
            if app.quick_open_open {
                let entries = app.quick_open_entries();
                if !entries.is_empty() {
                    let count = entries.len().min(24);
                    app.quick_open_selected_index = if app.quick_open_selected_index == 0 {
                        count - 1
                    } else {
                        app.quick_open_selected_index - 1
                    };
                }
                Task::none()
            } else {
                operation::focus_previous()
            }
        }
    }
}
