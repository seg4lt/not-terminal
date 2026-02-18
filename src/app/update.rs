use super::shortcuts::{ShortcutAction, detect_shortcut};
use super::state::{App, Message};
use iced::{Task, keyboard, mouse, window};

pub(crate) fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::WindowLocated(window_id) => {
            let Some(window_id) = window_id else {
                app.status = String::from("No window available for Ghostty embedding");
                return Task::none();
            };

            app.window_id = Some(window_id);
            App::app_ns_view(window_id)
        }
        Message::HostViewResolved(ns_view) => {
            app.host_ns_view = ns_view;
            if ns_view.is_none() {
                app.status = String::from("Failed to resolve AppKit NSView");
            }
            app.ensure_active_runtime();
            app.sync_runtime_views();
            Task::none()
        }
        Message::WindowSizeResolved(size) => {
            app.window_size = size;
            app.sync_runtime_views();
            Task::none()
        }
        Message::WindowScaleResolved(scale) => {
            app.window_scale_factor = scale;
            app.sync_runtime_views();
            Task::none()
        }
        Message::WindowEvent(window_id, event) => {
            if app.window_id.is_none_or(|current| current == window_id) {
                match event {
                    window::Event::Resized(size) => {
                        app.window_size = size;
                        app.sync_runtime_views();
                    }
                    window::Event::Rescaled(scale) => {
                        app.window_scale_factor = scale;
                        app.sync_runtime_views();
                    }
                    _ => {}
                }
            }
            Task::none()
        }
        Message::StateLoaded(result) => {
            match result {
                Ok(state) => {
                    app.apply_loaded_state(state);
                    app.status = String::from("State loaded");
                }
                Err(error) => {
                    app.status = format!("Failed to load state: {error}");
                }
            }

            app.ensure_active_runtime();
            app.sync_runtime_views();
            Task::none()
        }
        Message::StateSaved(result) => {
            if let Err(error) = result {
                app.status = format!("Failed to save state: {error}");
            }
            Task::none()
        }
        Message::GhosttyTick => {
            for runtime in app.runtimes.values_mut() {
                runtime.ghostty.tick_if_needed();
            }
            Task::none()
        }
        Message::Keyboard(event) => {
            if let keyboard::Event::ModifiersChanged(modifiers) = &event {
                app.keyboard_modifiers = *modifiers;
            }

            let allow_plain_rename = app.active_terminal_id().is_none();
            let modal_open =
                app.rename_dialog.is_some() || app.quick_open_open || app.preferences_open;
            if let Some(action) = detect_shortcut(&event, allow_plain_rename, modal_open) {
                return apply_shortcut(app, action);
            }

            if modal_open {
                return Task::none();
            }

            if let Some(ghostty) = app.active_ghostty_mut()
                && ghostty.handle_keyboard_event(&event)
            {
                ghostty.refresh();
                ghostty.force_tick();
            }
            Task::none()
        }
        Message::Mouse(event) => {
            let modifiers = app.keyboard_modifiers;
            match event {
                mouse::Event::CursorMoved { position } => {
                    app.cursor_position_logical = Some(position);
                    let local = app.terminal_local_from_position(position);
                    if let Some(ghostty) = app.active_ghostty_mut() {
                        match local {
                            Some((x, y)) => ghostty.handle_mouse_move(x, y, modifiers),
                            None => ghostty.handle_mouse_move(-1.0, -1.0, modifiers),
                        }
                    }
                }
                mouse::Event::CursorLeft => {
                    app.cursor_position_logical = None;
                    if let Some(ghostty) = app.active_ghostty_mut() {
                        ghostty.handle_mouse_move(-1.0, -1.0, modifiers);
                    }
                }
                mouse::Event::ButtonPressed(button) | mouse::Event::ButtonReleased(button) => {
                    let pressed = matches!(event, mouse::Event::ButtonPressed(_));
                    let local = app
                        .cursor_position_logical
                        .and_then(|position| app.terminal_local_from_position(position));

                    if let Some((x, y)) = local
                        && let Some(ghostty) = app.active_ghostty_mut()
                    {
                        ghostty.handle_mouse_move(x, y, modifiers);
                        if ghostty.handle_mouse_button(button, pressed, modifiers) {
                            ghostty.refresh();
                            ghostty.force_tick();
                        }
                    }
                }
                mouse::Event::WheelScrolled { delta } => {
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
                        }
                    }
                }
                mouse::Event::CursorEntered => {}
            }
            Task::none()
        }
        Message::ToggleSidebar => {
            app.sidebar_collapsed = !app.sidebar_collapsed;
            app.sync_runtime_views();
            app.save_task()
        }
        Message::FilterChanged(value) => {
            app.filter_query = value;
            Task::none()
        }
        Message::AddProject => {
            let selected = rfd::FileDialog::new()
                .set_title("Select Git Folder")
                .pick_folder();

            let Some(path) = selected else {
                return Task::none();
            };

            let path_str = path.to_string_lossy().to_string();
            match app.add_project_from_git_folder(&path_str) {
                Ok(()) => {
                    app.status = format!("Added project {}", path_str);
                    app.ensure_active_runtime();
                    app.sync_runtime_views();
                    app.save_task()
                }
                Err(error) => {
                    app.status = format!("Failed to add project: {error}");
                    Task::none()
                }
            }
        }
        Message::ProjectRescan(project_id) => match app.rescan_project(&project_id) {
            Ok(()) => {
                app.status = String::from("Rescanned worktrees");
                app.ensure_active_runtime();
                app.sync_runtime_views();
                app.save_task()
            }
            Err(error) => {
                app.status = format!("Failed to rescan worktrees: {error}");
                Task::none()
            }
        },
        Message::SelectProject(project_id) => {
            app.select_project(&project_id);
            app.ensure_active_runtime();
            app.sync_runtime_views();
            app.save_task()
        }
        Message::ToggleProjectCollapsed(project_id) => {
            app.toggle_project_collapsed(&project_id);
            app.save_task()
        }
        Message::ToggleWorktreeCollapsed {
            project_id,
            worktree_id,
        } => {
            app.toggle_worktree_collapsed(&project_id, &worktree_id);
            app.save_task()
        }
        Message::AddTerminal {
            project_id,
            worktree_id,
        } => {
            if let Some(terminal_id) = app.add_terminal(&project_id, &worktree_id) {
                if let Err(error) = app.ensure_runtime_for_terminal(&terminal_id) {
                    app.status = error;
                }
                app.select_terminal(&project_id, &terminal_id);
                app.sync_runtime_views();
                app.status = String::from("Terminal added");
                app.save_task()
            } else {
                Task::none()
            }
        }
        Message::SelectTerminal {
            project_id,
            terminal_id,
        } => {
            app.select_terminal(&project_id, &terminal_id);
            if let Err(error) = app.ensure_runtime_for_terminal(&terminal_id) {
                app.status = error;
            }
            app.sync_runtime_views();
            app.save_task()
        }
        Message::RemoveTerminal {
            project_id,
            worktree_id,
            terminal_id,
        } => {
            app.remove_terminal(&project_id, &worktree_id, &terminal_id);
            app.ensure_active_runtime();
            app.sync_runtime_views();
            app.save_task()
        }
        Message::OpenPreferences(open) => {
            app.preferences_open = open;
            if open {
                app.quick_open_open = false;
                app.rename_dialog = None;
            }
            Task::none()
        }
        Message::OpenQuickOpen(open) => {
            app.quick_open_open = open;
            if open {
                app.quick_open_query.clear();
                app.preferences_open = false;
                app.rename_dialog = None;
            }
            Task::none()
        }
        Message::QuickOpenQueryChanged(value) => {
            app.quick_open_query = value;
            Task::none()
        }
        Message::QuickOpenSubmit => {
            if let Some(entry) = app.quick_open_entries().first().cloned() {
                app.select_terminal(&entry.project_id, &entry.terminal_id);
                if let Err(error) = app.ensure_runtime_for_terminal(&entry.terminal_id) {
                    app.status = error;
                }
                app.quick_open_open = false;
                app.quick_open_query.clear();
                app.sync_runtime_views();
                return app.save_task();
            }
            Task::none()
        }
        Message::QuickOpenSelect(terminal_id) => {
            app.select_terminal_by_id(&terminal_id);
            if let Err(error) = app.ensure_runtime_for_terminal(&terminal_id) {
                app.status = error;
            }
            app.quick_open_open = false;
            app.quick_open_query.clear();
            app.sync_runtime_views();
            app.save_task()
        }
        Message::StartRenameFocused => {
            app.start_rename_focused();
            app.quick_open_open = false;
            app.preferences_open = false;
            Task::none()
        }
        Message::StartRenameTerminal => {
            app.start_rename_active_terminal();
            app.quick_open_open = false;
            app.preferences_open = false;
            Task::none()
        }
        Message::RenameValueChanged(value) => {
            if let Some(dialog) = app.rename_dialog.as_mut() {
                dialog.value = value;
            }
            Task::none()
        }
        Message::RenameCommit => {
            if app.commit_rename() {
                app.status = String::from("Renamed");
                app.save_task()
            } else {
                Task::none()
            }
        }
        Message::RenameCancel => {
            app.rename_dialog = None;
            Task::none()
        }
        Message::SwitchTerminalByOffset(offset) => {
            app.switch_terminal_by_offset(offset);
            app.ensure_active_runtime();
            app.sync_runtime_views();
            app.save_task()
        }
    }
}

fn apply_shortcut(app: &mut App, action: ShortcutAction) -> Task<Message> {
    match action {
        ShortcutAction::ToggleSidebar => update(app, Message::ToggleSidebar),
        ShortcutAction::OpenQuickOpen => update(app, Message::OpenQuickOpen(true)),
        ShortcutAction::OpenPreferences => update(app, Message::OpenPreferences(true)),
        ShortcutAction::RenameTerminal => update(app, Message::StartRenameTerminal),
        ShortcutAction::RenameFocused => update(app, Message::StartRenameFocused),
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
        ShortcutAction::NextTerminal => update(app, Message::SwitchTerminalByOffset(1)),
        ShortcutAction::PreviousTerminal => update(app, Message::SwitchTerminalByOffset(-1)),
        ShortcutAction::ModalCancel => {
            if app.rename_dialog.is_some() {
                return update(app, Message::RenameCancel);
            }
            if app.quick_open_open {
                return update(app, Message::OpenQuickOpen(false));
            }
            if app.preferences_open {
                return update(app, Message::OpenPreferences(false));
            }
            Task::none()
        }
        ShortcutAction::ModalSubmit => {
            if app.rename_dialog.is_some() {
                return update(app, Message::RenameCommit);
            }
            if app.quick_open_open {
                return update(app, Message::QuickOpenSubmit);
            }
            Task::none()
        }
    }
}
