use super::shortcuts::{ShortcutAction, detect_shortcut};
use super::state::{App, Message};
use crate::ghostty_embed::disable_system_hide_shortcuts;
use iced::{Task, keyboard, mouse, widget::operation, window};

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
            if ns_view.is_some() {
                disable_system_hide_shortcuts();
            }
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
            let mut layout_changed = false;
            for runtime in app.runtimes.values_mut() {
                layout_changed |= runtime.tick_all();
            }
            if app.process_runtime_actions() || layout_changed {
                app.sync_runtime_views();
            }
            Task::none()
        }
        Message::Keyboard(event) => {
            if let keyboard::Event::ModifiersChanged(modifiers) = &event {
                app.keyboard_modifiers = *modifiers;
                // Forward modifier changes to ALL panes to keep their state in sync.
                // This is important because when we switch panes, the new pane needs
                // to know the current modifier state for key combinations like cmd+c/v.
                for runtime in app.runtimes.values_mut() {
                    for pane in runtime.panes_mut() {
                        pane.update_modifiers(*modifiers);
                    }
                }
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
                        | ShortcutAction::NextTerminal
                        | ShortcutAction::PreviousTerminal
                )
            ) {
                if let Some(action) = shortcut_action {
                    return apply_shortcut(app, action);
                }
            }

            let ghostty_claims_binding = app
                .active_ghostty_mut()
                .is_some_and(|ghostty| ghostty.key_event_is_binding(&event));
            if ghostty_claims_binding {
                let mut should_refresh_branch = false;
                if let Some(ghostty) = app.active_ghostty_mut()
                    && ghostty.handle_keyboard_event(&event)
                {
                    ghostty.refresh();
                    ghostty.force_tick();
                    should_refresh_branch = true;
                }
                // Sync modifier state from active pane to all other panes
                app.sync_modifiers_from_active_pane();
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
            if let Some(ghostty) = app.active_ghostty_mut()
                && ghostty.handle_keyboard_event(&event)
            {
                ghostty.refresh();
                ghostty.force_tick();
                should_refresh_branch = true;
            }
            // Sync modifier state from active pane to all other panes
            app.sync_modifiers_from_active_pane();
            if app.process_runtime_actions() {
                app.sync_runtime_views();
            }

            if should_refresh_branch {
                return app.refresh_active_branch_task();
            }
            Task::none()
        }
        Message::Mouse(event) => {
            if app.modal_open() {
                return Task::none();
            }

            let modifiers = app.keyboard_modifiers;
            let mut should_refresh_branch = false;
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
                    let mut focus_changed = false;
                    let local = if pressed {
                        app.cursor_position_logical.and_then(|position| {
                            app.focus_terminal_pane_from_position(position).map(
                                |(x, y, changed)| {
                                    focus_changed = changed;
                                    (x, y)
                                },
                            )
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
        Message::ToggleSidebar => {
            app.sidebar_collapsed = !app.sidebar_collapsed;
            app.sync_runtime_views();
            app.save_task()
        }
        Message::SetShowNativeTitleBar(value) => {
            let changed = app.show_native_title_bar != value;
            app.show_native_title_bar = value;

            if changed && let Some(window_id) = app.window_id {
                Task::batch([window::toggle_decorations(window_id), app.save_task()])
            } else {
                app.save_task()
            }
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
        Message::AddDetachedTerminal => {
            let terminal_id = app.add_detached_terminal();
            if let Err(error) = app.ensure_runtime_for_terminal(&terminal_id) {
                app.status = error;
            }
            app.select_detached_terminal(&terminal_id);
            app.sync_runtime_views();
            app.status = String::from("Detached terminal added");
            app.save_task()
        }
        Message::CloseActiveTerminal => {
            if app.close_active_terminal() {
                app.ensure_active_runtime();
                app.sync_runtime_views();
                app.status = String::from("Terminal closed");
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
        Message::SelectDetachedTerminal(terminal_id) => {
            app.select_detached_terminal(&terminal_id);
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
        Message::RemoveDetachedTerminal(terminal_id) => {
            app.remove_detached_terminal(&terminal_id);
            app.ensure_active_runtime();
            app.sync_runtime_views();
            app.save_task()
        }
        Message::OpenPreferences(open) => {
            app.preferences_open = open;
            if open {
                app.quick_open_open = false;
                app.rename_dialog = None;
                app.add_worktree_dialog = None;
            }
            app.sync_runtime_views();
            Task::none()
        }
        Message::OpenQuickOpen(open) => {
            app.quick_open_open = open;
            if open {
                app.quick_open_query.clear();
                app.quick_open_selected_index = 0;
                app.preferences_open = false;
                app.rename_dialog = None;
                app.add_worktree_dialog = None;
            }
            app.sync_runtime_views();
            if open {
                Task::batch([
                    operation::focus("quick-open-input"),
                    operation::move_cursor_to_end("quick-open-input"),
                ])
            } else {
                Task::none()
            }
        }
        Message::QuickOpenQueryChanged(value) => {
            app.quick_open_query = value;
            app.quick_open_selected_index = 0;  // Reset selection when query changes
            Task::none()
        }
        Message::QuickOpenSubmit => {
            let entries = app.quick_open_entries();
            if let Some(entry) = entries.get(app.quick_open_selected_index) {
                app.select_terminal_by_id(&entry.terminal_id);
                if let Err(error) = app.ensure_runtime_for_terminal(&entry.terminal_id) {
                    app.status = error;
                }
                app.quick_open_open = false;
                app.quick_open_query.clear();
                app.quick_open_selected_index = 0;
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
            app.quick_open_selected_index = 0;
            app.sync_runtime_views();
            app.save_task()
        }
        Message::StartRenameProject(project_id) => {
            app.start_rename_project(&project_id);
            app.quick_open_open = false;
            app.preferences_open = false;
            app.add_worktree_dialog = None;
            app.sync_runtime_views();
            if app.rename_dialog.is_some() {
                Task::batch([
                    operation::focus("rename-input"),
                    operation::move_cursor_to_end("rename-input"),
                ])
            } else {
                Task::none()
            }
        }
        Message::StartRenameWorktree {
            project_id,
            worktree_id,
        } => {
            app.start_rename_worktree(&project_id, &worktree_id);
            app.quick_open_open = false;
            app.preferences_open = false;
            app.add_worktree_dialog = None;
            app.sync_runtime_views();
            if app.rename_dialog.is_some() {
                Task::batch([
                    operation::focus("rename-input"),
                    operation::move_cursor_to_end("rename-input"),
                ])
            } else {
                Task::none()
            }
        }
        Message::StartRenameFocused => {
            app.start_rename_focused();
            app.quick_open_open = false;
            app.preferences_open = false;
            app.add_worktree_dialog = None;
            app.sync_runtime_views();
            if app.rename_dialog.is_some() {
                Task::batch([
                    operation::focus("rename-input"),
                    operation::move_cursor_to_end("rename-input"),
                ])
            } else {
                Task::none()
            }
        }
        Message::StartRenameTerminal => {
            app.start_rename_active_terminal();
            app.quick_open_open = false;
            app.preferences_open = false;
            app.add_worktree_dialog = None;
            app.sync_runtime_views();
            if app.rename_dialog.is_some() {
                Task::batch([
                    operation::focus("rename-input"),
                    operation::move_cursor_to_end("rename-input"),
                ])
            } else {
                Task::none()
            }
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
                app.sync_runtime_views();
                app.save_task()
            } else {
                Task::none()
            }
        }
        Message::RenameCancel => {
            app.rename_dialog = None;
            app.sync_runtime_views();
            Task::none()
        }
        Message::StartAddWorktree(project_id) => {
            app.start_add_worktree(&project_id);
            app.quick_open_open = false;
            app.preferences_open = false;
            app.rename_dialog = None;
            app.sync_runtime_views();
            if app.add_worktree_dialog.is_some() {
                Task::batch([
                    operation::focus("add-worktree-branch-input"),
                    operation::move_cursor_to_end("add-worktree-branch-input"),
                ])
            } else {
                Task::none()
            }
        }
        Message::AddWorktreeBranchChanged(value) => {
            let project_id = app
                .add_worktree_dialog
                .as_ref()
                .map(|dialog| dialog.project_id.clone());
            let suggested = project_id
                .as_deref()
                .and_then(|project_id| app.suggested_worktree_destination(project_id, &value));

            if let Some(dialog) = app.add_worktree_dialog.as_mut() {
                dialog.branch_name = value;
                if let Some(path) = suggested {
                    dialog.destination_path = path;
                }
            }
            Task::none()
        }
        Message::AddWorktreePathChanged(value) => {
            if let Some(dialog) = app.add_worktree_dialog.as_mut() {
                dialog.destination_path = value;
            }
            Task::none()
        }
        Message::FocusAddWorktreePath => Task::batch([
            operation::focus("add-worktree-path-input"),
            operation::move_cursor_to_end("add-worktree-path-input"),
        ]),
        Message::AddWorktreeCommit => match app.commit_add_worktree() {
            Ok(()) => {
                app.status = String::from("Worktree added");
                app.ensure_active_runtime();
                app.sync_runtime_views();
                app.save_task()
            }
            Err(error) => {
                app.status = format!("Failed to add worktree: {error}");
                Task::none()
            }
        },
        Message::AddWorktreeCancel => {
            app.add_worktree_dialog = None;
            app.sync_runtime_views();
            Task::none()
        }
        Message::RemoveWorktree {
            project_id,
            worktree_id,
        } => match app.remove_worktree(&project_id, &worktree_id) {
            Ok(()) => {
                app.status = String::from("Worktree removed");
                app.ensure_active_runtime();
                app.sync_runtime_views();
                app.save_task()
            }
            Err(error) => {
                app.status = format!("Failed to remove worktree: {error}");
                Task::none()
            }
        },
        Message::SwitchTerminalByOffset(offset) => {
            app.switch_terminal_by_offset(offset);
            app.ensure_active_runtime();
            app.sync_runtime_views();
            app.save_task()
        }
        Message::ActiveBranchResolved {
            terminal_id,
            branch,
        } => {
            if !app.terminal_exists(&terminal_id) {
                return Task::none();
            }

            if let Some(branch) = branch {
                app.branch_by_terminal.insert(terminal_id, branch);
            } else {
                app.branch_by_terminal.remove(&terminal_id);
            }
            Task::none()
        }
    }
}

fn apply_shortcut(app: &mut App, action: ShortcutAction) -> Task<Message> {
    match action {
        ShortcutAction::ToggleSidebar => update(app, Message::ToggleSidebar),
        ShortcutAction::NewTerminal => {
            if let Some((project_id, worktree_id)) = app.active_worktree_ids() {
                update(
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
        ShortcutAction::NewDetachedTerminal => update(app, Message::AddDetachedTerminal),
        ShortcutAction::CloseActiveTerminal => update(app, Message::CloseActiveTerminal),
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
            app.suppress_next_key_release = true;
            if app.rename_dialog.is_some() {
                return update(app, Message::RenameCancel);
            }
            if app.add_worktree_dialog.is_some() {
                return update(app, Message::AddWorktreeCancel);
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
            app.suppress_next_key_release = true;
            if app.rename_dialog.is_some() {
                return update(app, Message::RenameCommit);
            }
            if app.add_worktree_dialog.is_some() {
                return update(app, Message::AddWorktreeCommit);
            }
            if app.quick_open_open {
                return update(app, Message::QuickOpenSubmit);
            }
            Task::none()
        }
        ShortcutAction::ModalFocusNext => {
            app.suppress_next_key_release = true;
            if app.quick_open_open {
                let entries = app.quick_open_entries();
                if !entries.is_empty() {
                    app.quick_open_selected_index = (app.quick_open_selected_index + 1) % entries.len().min(24);
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
