use super::state::{App, Message};
use crate::app::shortcuts::ShortcutAction;
use crate::app::state::{
    COMMAND_PALETTE_SCROLL_ID, CommandPaletteAction, ProjectRescanSummary, QUICK_OPEN_SCROLL_ID,
    QuickOpenEntry, QuickOpenEntryKind,
};
use crate::ghostty_embed::{
    disable_system_hide_shortcuts, register_focus_toggle_hotkey, take_pending_attention_badge_click,
};
use iced::{Task, widget::operation, window};
use std::process::{Command, Stdio};
use std::time::Instant;

mod browser;
mod input;

fn rescan_status(summary: &ProjectRescanSummary, startup: bool) -> String {
    if summary.total_projects == 0 {
        return if startup {
            String::from("State loaded")
        } else {
            String::from("No projects to rescan")
        };
    }

    let project_label = if summary.successful_projects == 1 {
        "project"
    } else {
        "projects"
    };

    if summary.failed_projects.is_empty() {
        return if startup {
            format!(
                "State loaded and rescanned {} {}",
                summary.successful_projects, project_label
            )
        } else {
            format!(
                "Rescanned {} {}",
                summary.successful_projects, project_label
            )
        };
    }

    let failure_suffix = if summary.failed_projects.len() == 1 {
        format!(" Failed: {}", summary.failed_projects[0])
    } else {
        format!(
            " First failure: {} (+{} more)",
            summary.failed_projects[0],
            summary.failed_projects.len() - 1
        )
    };

    if startup {
        format!(
            "State loaded and rescanned {} of {} projects.{}",
            summary.successful_projects, summary.total_projects, failure_suffix
        )
    } else {
        format!(
            "Rescanned {} of {} projects.{}",
            summary.successful_projects, summary.total_projects, failure_suffix
        )
    }
}

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
                register_focus_toggle_hotkey();
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
                    let summary = app.rescan_all_projects();
                    app.status = rescan_status(&summary, true);
                    app.ensure_active_runtime();
                    app.sync_runtime_views();
                    return if summary.changed_projects > 0 {
                        app.save_task()
                    } else {
                        Task::none()
                    };
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
            if take_pending_attention_badge_click() && app.sidebar_state.is_hidden() {
                app.sidebar_state = app.sidebar_state.toggle();
                app.sync_runtime_views();
                return app.save_task();
            }

            let mut layout_changed = false;
            let mut had_any_work = false;

            for runtime in app.runtimes.values_mut() {
                let tick = runtime.tick_all();
                if tick.had_pending_work {
                    had_any_work = true;
                }
                layout_changed |= tick.layout_changed;
            }

            if app.process_runtime_actions() || layout_changed {
                app.sync_runtime_views();
            }

            // Update activity timestamp if there was actual work to do
            if had_any_work {
                app.last_ghostty_activity = Instant::now();
            }
            if !app.terminal_progress_active.is_empty() {
                app.advance_terminal_activity_frame();
            }

            Task::none()
        }
        Message::Keyboard(event) => input::handle_keyboard(app, event),
        Message::Mouse(event) => input::handle_mouse(app, event),
        Message::ToggleSidebar => {
            app.sidebar_state = app.sidebar_state.toggle();
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
        Message::SetEnableBrowsers(value) => {
            app.persisted.ui.enable_browsers = value;
            app.save_task()
        }
        Message::SetPreferredEditorCommand(value) => {
            app.persisted.ui.preferred_editor_command = value;
            app.save_task()
        }
        Message::SetSecondaryEditorCommand(value) => {
            app.persisted.ui.secondary_editor_command = value;
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
                app.project_context_menu = None;
                app.worktree_context_menu = None;
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
        Message::RescanAllProjects => {
            let summary = app.rescan_all_projects();
            app.project_context_menu = None;
            app.worktree_context_menu = None;
            app.status = rescan_status(&summary, false);
            app.ensure_active_runtime();
            app.sync_runtime_views();
            if summary.changed_projects > 0 {
                app.save_task()
            } else {
                Task::none()
            }
        }
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
        Message::ToggleAllProjectTreesCollapsed => {
            app.toggle_all_project_trees_collapsed();
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
        Message::OpenInPreferredEditor => {
            let editor_command = app.persisted.ui.preferred_editor_command.clone();
            open_active_worktree_in_editor(app, editor_command.trim(), "preferred")
        }
        Message::OpenInSecondaryEditor => {
            let editor_command = app.persisted.ui.secondary_editor_command.clone();
            open_active_worktree_in_editor(app, editor_command.trim(), "secondary")
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
        Message::TogglePinnedTerminal(terminal_id) => {
            if app.is_terminal_pinned(&terminal_id) {
                if app.unpin_terminal(&terminal_id) {
                    app.status = String::from("Terminal unpinned");
                    app.save_task()
                } else {
                    Task::none()
                }
            } else {
                match app.pin_terminal(&terminal_id) {
                    crate::app::state::PinTerminalOutcome::Pinned(slot) => {
                        app.status =
                            format!("Pinned terminal to Cmd+Option+{}", slot.saturating_add(1));
                        app.save_task()
                    }
                    crate::app::state::PinTerminalOutcome::AlreadyPinned(slot) => {
                        app.status = format!(
                            "Terminal is already pinned on Cmd+Option+{}",
                            slot.saturating_add(1)
                        );
                        Task::none()
                    }
                    crate::app::state::PinTerminalOutcome::LimitReached => {
                        app.status = String::from(
                            "Pinned slots are full (Cmd+Option+1 through Cmd+Option+9)",
                        );
                        Task::none()
                    }
                    crate::app::state::PinTerminalOutcome::Missing => {
                        app.status = String::from("Terminal is no longer available");
                        Task::none()
                    }
                }
            }
        }
        Message::SelectPinnedTerminal(terminal_id) => {
            app.select_terminal_by_id(&terminal_id);
            if let Err(error) = app.ensure_runtime_for_terminal(&terminal_id) {
                app.status = error;
            }
            app.sync_runtime_views();
            app.save_task()
        }
        Message::SelectPinnedTerminalSlot(slot) => {
            let Some(terminal_id) = app.select_pinned_terminal_slot(slot) else {
                app.status = format!(
                    "No pinned terminal on Cmd+Option+{}",
                    slot.saturating_add(1)
                );
                return Task::none();
            };
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
                app.command_palette_open = false;
                app.quick_open_open = false;
                app.add_worktree_project_picker_open = false;
                app.add_worktree_project_selected_index = 0;
                app.delete_worktree_project_picker_open = false;
                app.delete_worktree_project_selected_index = 0;
                app.delete_worktree_picker = None;
                app.rename_dialog = None;
                app.add_worktree_dialog = None;
                app.worktree_context_menu = None;
                app.project_context_menu = None;
            }
            app.sync_runtime_views();
            Task::none()
        }
        Message::OpenCommandPalette(open) => {
            app.command_palette_open = open;
            if open {
                app.command_palette_query.clear();
                app.command_palette_selected_index = 0;
                app.quick_open_open = false;
                app.add_worktree_project_picker_open = false;
                app.add_worktree_project_selected_index = 0;
                app.delete_worktree_project_picker_open = false;
                app.delete_worktree_project_selected_index = 0;
                app.delete_worktree_picker = None;
                app.preferences_open = false;
                app.rename_dialog = None;
                app.add_worktree_dialog = None;
                app.worktree_context_menu = None;
                app.project_context_menu = None;
            }
            app.sync_runtime_views();
            if open {
                Task::batch([
                    operation::focus("command-palette-input"),
                    operation::move_cursor_to_end("command-palette-input"),
                    operation::snap_to(COMMAND_PALETTE_SCROLL_ID, operation::RelativeOffset::START),
                ])
            } else {
                Task::none()
            }
        }
        Message::CommandPaletteQueryChanged(value) => {
            if app.command_palette_open && app.keyboard_modifiers.logo() {
                return Task::none();
            }
            app.command_palette_query = value;
            app.command_palette_selected_index = 0;
            operation::snap_to(COMMAND_PALETTE_SCROLL_ID, operation::RelativeOffset::START)
        }
        Message::CommandPaletteSubmit => {
            let entries = app.command_palette_entries();
            let Some(entry) = entries.get(app.command_palette_selected_index) else {
                return Task::none();
            };

            let action = entry.action.clone();
            app.command_palette_open = false;
            app.command_palette_query.clear();
            app.command_palette_selected_index = 0;
            app.sync_runtime_views();
            Task::done(Message::RunCommandPaletteAction(action))
        }
        Message::CommandPaletteSelect(index) => {
            let entries = app.command_palette_entries();
            let Some(entry) = entries.get(index) else {
                return Task::none();
            };

            let action = entry.action.clone();
            app.command_palette_open = false;
            app.command_palette_query.clear();
            app.command_palette_selected_index = 0;
            app.sync_runtime_views();
            Task::done(Message::RunCommandPaletteAction(action))
        }
        Message::RunCommandPaletteAction(action) => activate_command_palette_action(app, action),
        Message::OpenQuickOpen(open) => {
            app.quick_open_open = open;
            if open {
                app.quick_open_query.clear();
                app.quick_open_selected_index = 0;
                app.quick_open_ignore_next_query_change = false;
                app.command_palette_open = false;
                app.add_worktree_project_picker_open = false;
                app.add_worktree_project_selected_index = 0;
                app.delete_worktree_project_picker_open = false;
                app.delete_worktree_project_selected_index = 0;
                app.delete_worktree_picker = None;
                app.preferences_open = false;
                app.rename_dialog = None;
                app.add_worktree_dialog = None;
                app.worktree_context_menu = None;
                app.project_context_menu = None;
            }
            app.sync_runtime_views();
            if open {
                Task::batch([
                    operation::focus("quick-open-input"),
                    operation::move_cursor_to_end("quick-open-input"),
                    operation::snap_to(QUICK_OPEN_SCROLL_ID, operation::RelativeOffset::START),
                ])
            } else {
                Task::none()
            }
        }
        Message::QuickOpenQueryChanged(value) => {
            if app.quick_open_open && app.keyboard_modifiers.logo() {
                return Task::none();
            }
            if app.quick_open_ignore_next_query_change {
                app.quick_open_ignore_next_query_change = false;
                return Task::none();
            }
            app.quick_open_query = value;
            app.quick_open_selected_index = 0; // Reset selection when query changes
            operation::snap_to(QUICK_OPEN_SCROLL_ID, operation::RelativeOffset::START)
        }
        Message::QuickOpenSubmit => {
            let entries = app.quick_open_entries();
            if let Some(entry) = entries.get(app.quick_open_selected_index) {
                if !activate_quick_open_entry(app, entry) {
                    return Task::none();
                }
                app.quick_open_open = false;
                app.quick_open_query.clear();
                app.quick_open_selected_index = 0;
                app.sync_runtime_views();
                return app.save_task();
            }
            Task::none()
        }
        Message::QuickOpenSelect(index) => {
            let entries = app.quick_open_entries();
            if let Some(entry) = entries.get(index) {
                if !activate_quick_open_entry(app, entry) {
                    return Task::none();
                }
            } else {
                return Task::none();
            }
            app.quick_open_open = false;
            app.quick_open_query.clear();
            app.quick_open_selected_index = 0;
            app.sync_runtime_views();
            app.save_task()
        }
        Message::QuickOpenCloseTerminal(terminal_id) => {
            if app.close_terminal_by_id(&terminal_id) {
                app.ensure_active_runtime();

                let remaining_count = app.quick_open_entries().len();
                app.quick_open_selected_index = if remaining_count == 0 {
                    0
                } else {
                    app.quick_open_selected_index.min(remaining_count - 1)
                };

                app.sync_runtime_views();
                app.status = String::from("Terminal closed");
                app.save_task()
            } else {
                Task::none()
            }
        }
        Message::StartRenameWorktree {
            project_id,
            worktree_id,
        } => {
            app.start_rename_worktree(&project_id, &worktree_id);
            app.command_palette_open = false;
            app.quick_open_open = false;
            app.add_worktree_project_picker_open = false;
            app.add_worktree_project_selected_index = 0;
            app.delete_worktree_project_picker_open = false;
            app.delete_worktree_project_selected_index = 0;
            app.delete_worktree_picker = None;
            app.preferences_open = false;
            app.add_worktree_dialog = None;
            app.worktree_context_menu = None;
            app.project_context_menu = None;
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
            app.command_palette_open = false;
            app.quick_open_open = false;
            app.add_worktree_project_picker_open = false;
            app.add_worktree_project_selected_index = 0;
            app.delete_worktree_project_picker_open = false;
            app.delete_worktree_project_selected_index = 0;
            app.delete_worktree_picker = None;
            app.preferences_open = false;
            app.add_worktree_dialog = None;
            app.worktree_context_menu = None;
            app.project_context_menu = None;
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
            app.command_palette_open = false;
            app.quick_open_open = false;
            app.add_worktree_project_picker_open = false;
            app.add_worktree_project_selected_index = 0;
            app.delete_worktree_project_picker_open = false;
            app.delete_worktree_project_selected_index = 0;
            app.delete_worktree_picker = None;
            app.preferences_open = false;
            app.add_worktree_dialog = None;
            app.worktree_context_menu = None;
            app.project_context_menu = None;
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
        Message::StartRenamePinnedTerminal(terminal_id) => {
            app.start_rename_pinned_terminal(&terminal_id);
            app.command_palette_open = false;
            app.quick_open_open = false;
            app.add_worktree_project_picker_open = false;
            app.add_worktree_project_selected_index = 0;
            app.delete_worktree_project_picker_open = false;
            app.delete_worktree_project_selected_index = 0;
            app.delete_worktree_picker = None;
            app.preferences_open = false;
            app.add_worktree_dialog = None;
            app.worktree_context_menu = None;
            app.project_context_menu = None;
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
        Message::OpenAddWorktreeProjectPicker => {
            let entries = app.add_worktree_project_entries();
            if entries.is_empty() {
                app.status = String::from("No projects available to add a worktree");
                return Task::none();
            }

            let selected_index = app
                .persisted
                .active_project_id
                .as_ref()
                .and_then(|active_id| {
                    entries
                        .iter()
                        .position(|entry| &entry.project_id == active_id)
                })
                .unwrap_or(0);

            app.command_palette_open = false;
            app.quick_open_open = false;
            app.preferences_open = false;
            app.rename_dialog = None;
            app.add_worktree_dialog = None;
            app.worktree_context_menu = None;
            app.project_context_menu = None;
            app.add_worktree_project_picker_open = true;
            app.add_worktree_project_selected_index = selected_index;
            app.delete_worktree_project_picker_open = false;
            app.delete_worktree_project_selected_index = 0;
            app.delete_worktree_picker = None;
            app.sync_runtime_views();

            operation::snap_to(
                crate::app::state::ADD_WORKTREE_PROJECT_SCROLL_ID,
                operation::RelativeOffset {
                    x: 0.0,
                    y: if entries.len() <= 1 {
                        0.0
                    } else {
                        selected_index as f32 / (entries.len() - 1) as f32
                    },
                },
            )
        }
        Message::AddWorktreeProjectSubmit => {
            let entries = app.add_worktree_project_entries();
            let Some(entry) = entries.get(app.add_worktree_project_selected_index) else {
                return Task::none();
            };

            update(app, Message::StartAddWorktree(entry.project_id.clone()))
        }
        Message::AddWorktreeProjectSelect(index) => {
            let entries = app.add_worktree_project_entries();
            let Some(entry) = entries.get(index) else {
                return Task::none();
            };

            app.add_worktree_project_selected_index = index;
            update(app, Message::StartAddWorktree(entry.project_id.clone()))
        }
        Message::AddWorktreeProjectCancel => {
            app.add_worktree_project_picker_open = false;
            app.add_worktree_project_selected_index = 0;
            app.sync_runtime_views();
            Task::none()
        }
        Message::OpenDeleteWorktreeProjectPicker => {
            let entries = app.delete_worktree_project_entries();
            if entries.is_empty() {
                app.status = String::from("No removable worktrees available");
                return Task::none();
            }

            let selected_index = app
                .persisted
                .active_project_id
                .as_ref()
                .and_then(|active_id| {
                    entries
                        .iter()
                        .position(|entry| &entry.project_id == active_id)
                })
                .unwrap_or(0);

            app.command_palette_open = false;
            app.quick_open_open = false;
            app.preferences_open = false;
            app.rename_dialog = None;
            app.add_worktree_dialog = None;
            app.worktree_context_menu = None;
            app.project_context_menu = None;
            app.add_worktree_project_picker_open = false;
            app.add_worktree_project_selected_index = 0;
            app.delete_worktree_project_picker_open = true;
            app.delete_worktree_project_selected_index = selected_index;
            app.delete_worktree_picker = None;
            app.sync_runtime_views();

            operation::snap_to(
                crate::app::state::DELETE_WORKTREE_PROJECT_SCROLL_ID,
                operation::RelativeOffset {
                    x: 0.0,
                    y: if entries.len() <= 1 {
                        0.0
                    } else {
                        selected_index as f32 / (entries.len() - 1) as f32
                    },
                },
            )
        }
        Message::DeleteWorktreeProjectSubmit => {
            let entries = app.delete_worktree_project_entries();
            let Some(entry) = entries.get(app.delete_worktree_project_selected_index) else {
                return Task::none();
            };

            update(
                app,
                Message::OpenDeleteWorktreePicker(entry.project_id.clone()),
            )
        }
        Message::DeleteWorktreeProjectSelect(index) => {
            let entries = app.delete_worktree_project_entries();
            let Some(entry) = entries.get(index) else {
                return Task::none();
            };

            app.delete_worktree_project_selected_index = index;
            update(
                app,
                Message::OpenDeleteWorktreePicker(entry.project_id.clone()),
            )
        }
        Message::DeleteWorktreeProjectCancel => {
            app.delete_worktree_project_picker_open = false;
            app.delete_worktree_project_selected_index = 0;
            app.sync_runtime_views();
            Task::none()
        }
        Message::OpenDeleteWorktreePicker(project_id) => {
            let entries = app.delete_worktree_entries(&project_id);
            if entries.is_empty() {
                app.status = String::from("No removable worktrees available in that project");
                return Task::none();
            }

            let selected_index = app
                .active_worktree_ids()
                .and_then(|(active_project_id, active_worktree_id)| {
                    (active_project_id == project_id).then(|| {
                        entries
                            .iter()
                            .position(|entry| entry.worktree_id == active_worktree_id)
                    })?
                })
                .unwrap_or(0);

            app.command_palette_open = false;
            app.quick_open_open = false;
            app.preferences_open = false;
            app.rename_dialog = None;
            app.add_worktree_dialog = None;
            app.worktree_context_menu = None;
            app.project_context_menu = None;
            app.add_worktree_project_picker_open = false;
            app.add_worktree_project_selected_index = 0;
            app.delete_worktree_project_picker_open = false;
            app.delete_worktree_project_selected_index = 0;
            app.delete_worktree_picker = Some(crate::app::state::DeleteWorktreePicker {
                project_id,
                selected_index,
            });
            app.sync_runtime_views();

            operation::snap_to(
                crate::app::state::DELETE_WORKTREE_SCROLL_ID,
                operation::RelativeOffset {
                    x: 0.0,
                    y: if entries.len() <= 1 {
                        0.0
                    } else {
                        selected_index as f32 / (entries.len() - 1) as f32
                    },
                },
            )
        }
        Message::DeleteWorktreeSubmit => {
            let Some(picker) = app.delete_worktree_picker.clone() else {
                return Task::none();
            };
            let entries = app.delete_worktree_entries(&picker.project_id);
            let Some(entry) = entries.get(picker.selected_index) else {
                return Task::none();
            };

            update(
                app,
                Message::RemoveWorktree {
                    project_id: entry.project_id.clone(),
                    worktree_id: entry.worktree_id.clone(),
                },
            )
        }
        Message::DeleteWorktreeSelect(index) => {
            let Some(project_id) = app
                .delete_worktree_picker
                .as_ref()
                .map(|picker| picker.project_id.clone())
            else {
                return Task::none();
            };

            if let Some(picker) = app.delete_worktree_picker.as_mut() {
                picker.selected_index = index;
            }

            let entries = app.delete_worktree_entries(&project_id);
            let Some(entry) = entries.get(index) else {
                return Task::none();
            };

            update(
                app,
                Message::RemoveWorktree {
                    project_id: entry.project_id.clone(),
                    worktree_id: entry.worktree_id.clone(),
                },
            )
        }
        Message::DeleteWorktreeCancel => {
            app.delete_worktree_picker = None;
            app.sync_runtime_views();
            Task::none()
        }
        Message::StartAddWorktree(project_id) => {
            app.start_add_worktree(&project_id);
            app.command_palette_open = false;
            app.quick_open_open = false;
            app.add_worktree_project_picker_open = false;
            app.add_worktree_project_selected_index = 0;
            app.delete_worktree_project_picker_open = false;
            app.delete_worktree_project_selected_index = 0;
            app.delete_worktree_picker = None;
            app.preferences_open = false;
            app.rename_dialog = None;
            app.worktree_context_menu = None;
            app.project_context_menu = None;
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
                app.worktree_context_menu = None;
                app.project_context_menu = None;
                app.delete_worktree_project_picker_open = false;
                app.delete_worktree_project_selected_index = 0;
                app.delete_worktree_picker = None;
                app.status = String::from("Worktree removed");
                app.ensure_active_runtime();
                app.sync_runtime_views();
                app.save_task()
            }
            Err(error) => {
                app.delete_worktree_project_picker_open = false;
                app.delete_worktree_project_selected_index = 0;
                app.delete_worktree_picker = None;
                app.status = format!("Failed to remove worktree: {error}");
                Task::none()
            }
        },
        Message::RemoveProject(project_id) => match app.remove_project(&project_id) {
            Ok(()) => {
                app.project_context_menu = None;
                app.worktree_context_menu = None;
                app.status = String::from("Project removed");
                app.ensure_active_runtime();
                app.sync_runtime_views();
                app.save_task()
            }
            Err(error) => {
                app.status = format!("Failed to remove project: {error}");
                Task::none()
            }
        },
        Message::OpenWorktreeContextMenu {
            project_id,
            worktree_id,
        } => {
            app.worktree_context_menu = Some(crate::app::state::WorktreeContextMenu {
                project_id,
                worktree_id,
            });
            app.project_context_menu = None;
            app.quick_open_open = false;
            app.add_worktree_project_picker_open = false;
            app.add_worktree_project_selected_index = 0;
            app.delete_worktree_project_picker_open = false;
            app.delete_worktree_project_selected_index = 0;
            app.delete_worktree_picker = None;
            app.preferences_open = false;
            app.rename_dialog = None;
            app.add_worktree_dialog = None;
            app.sync_runtime_views();
            Task::none()
        }
        Message::OpenProjectContextMenu(project_id) => {
            app.project_context_menu = Some(crate::app::state::ProjectContextMenu { project_id });
            app.worktree_context_menu = None;
            app.quick_open_open = false;
            app.add_worktree_project_picker_open = false;
            app.add_worktree_project_selected_index = 0;
            app.delete_worktree_project_picker_open = false;
            app.delete_worktree_project_selected_index = 0;
            app.delete_worktree_picker = None;
            app.preferences_open = false;
            app.rename_dialog = None;
            app.add_worktree_dialog = None;
            app.sync_runtime_views();
            Task::none()
        }
        Message::CloseWorktreeContextMenu => {
            app.worktree_context_menu = None;
            app.sync_runtime_views();
            Task::none()
        }
        Message::CloseProjectContextMenu => {
            app.project_context_menu = None;
            app.sync_runtime_views();
            Task::none()
        }
        Message::WorktreeContextMenuNewTerminal => {
            let Some(menu) = app.worktree_context_menu.clone() else {
                return Task::none();
            };
            app.worktree_context_menu = None;
            update(
                app,
                Message::AddTerminal {
                    project_id: menu.project_id,
                    worktree_id: menu.worktree_id,
                },
            )
        }
        Message::WorktreeContextMenuRenameWorktree => {
            let Some(menu) = app.worktree_context_menu.clone() else {
                return Task::none();
            };
            app.worktree_context_menu = None;
            update(
                app,
                Message::StartRenameWorktree {
                    project_id: menu.project_id,
                    worktree_id: menu.worktree_id,
                },
            )
        }
        Message::ProjectContextMenuProjectRescan => {
            let Some(menu) = app.project_context_menu.clone() else {
                return Task::none();
            };
            app.project_context_menu = None;
            update(app, Message::ProjectRescan(menu.project_id))
        }
        Message::ProjectContextMenuRemoveProject => {
            let Some(menu) = app.project_context_menu.clone() else {
                return Task::none();
            };
            app.project_context_menu = None;
            update(app, Message::RemoveProject(menu.project_id))
        }
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
        Message::SidebarResizeHandlePressed => {
            app.sidebar_resizing = true;
            Task::none()
        }
        Message::SidebarResizeHandleReleased => {
            if app.sidebar_resizing {
                app.sidebar_resizing = false;
                return app.save_task();
            }
            Task::none()
        }
        Message::AddBrowser => browser::handle_add_browser(app),
        Message::RemoveBrowser(browser_id) => browser::handle_remove_browser(app, browser_id),
        Message::SelectBrowser(browser_id) => browser::handle_select_browser(app, browser_id),
        Message::BrowserUrlChanged(value) => browser::handle_browser_url_changed(app, value),
        Message::BrowserNavigate => browser::handle_browser_navigate(app),
        Message::BrowserBack => browser::handle_browser_back(app),
        Message::BrowserForward => browser::handle_browser_forward(app),
        Message::BrowserReload => browser::handle_browser_reload(app),
        Message::BrowserDevTools => browser::handle_browser_devtools(app),
    }
}

fn activate_quick_open_entry(app: &mut App, entry: &QuickOpenEntry) -> bool {
    match &entry.kind {
        QuickOpenEntryKind::ExistingTerminal { terminal_id } => {
            app.select_terminal_by_id(terminal_id);
            if let Err(error) = app.ensure_runtime_for_terminal(terminal_id) {
                app.status = error;
                return false;
            }
            true
        }
        QuickOpenEntryKind::CreateTerminal {
            project_id,
            worktree_id,
        } => {
            let Some(terminal_id) = app.add_terminal(project_id, worktree_id) else {
                return false;
            };
            if let Err(error) = app.ensure_runtime_for_terminal(&terminal_id) {
                app.status = error;
                return false;
            }
            app.select_terminal(project_id, &terminal_id);
            app.status = format!(
                "Terminal added in {} / {}",
                entry.project_name, entry.worktree_name
            );
            true
        }
    }
}

fn activate_command_palette_action(app: &mut App, action: CommandPaletteAction) -> Task<Message> {
    match action {
        CommandPaletteAction::OpenQuickOpen => update(app, Message::OpenQuickOpen(true)),
        CommandPaletteAction::ToggleSidebar => update(app, Message::ToggleSidebar),
        CommandPaletteAction::NewTerminal => {
            input::apply_shortcut(app, ShortcutAction::NewTerminal)
        }
        CommandPaletteAction::NewDetachedTerminal => update(app, Message::AddDetachedTerminal),
        CommandPaletteAction::CloseActiveTerminal => update(app, Message::CloseActiveTerminal),
        CommandPaletteAction::PinFocusedItem => {
            let Some(terminal_id) = app.active_terminal_id() else {
                app.status = String::from("No focused terminal to pin");
                return Task::none();
            };
            match app.pin_terminal(&terminal_id) {
                crate::app::state::PinTerminalOutcome::Pinned(slot) => {
                    app.status =
                        format!("Pinned terminal to Cmd+Option+{}", slot.saturating_add(1));
                    app.save_task()
                }
                crate::app::state::PinTerminalOutcome::AlreadyPinned(slot) => {
                    app.status = format!(
                        "Terminal is already pinned on Cmd+Option+{}",
                        slot.saturating_add(1)
                    );
                    Task::none()
                }
                crate::app::state::PinTerminalOutcome::LimitReached => {
                    app.status =
                        String::from("Pinned slots are full (Cmd+Option+1 through Cmd+Option+9)");
                    Task::none()
                }
                crate::app::state::PinTerminalOutcome::Missing => {
                    app.status = String::from("Terminal is no longer available");
                    Task::none()
                }
            }
        }
        CommandPaletteAction::UnpinFocusedItem => {
            let Some(terminal_id) = app.active_terminal_id() else {
                app.status = String::from("No focused terminal to unpin");
                return Task::none();
            };
            if !app.is_terminal_pinned(&terminal_id) {
                app.status = String::from("Focused terminal is not pinned");
                return Task::none();
            }
            if app.unpin_terminal(&terminal_id) {
                app.status = String::from("Terminal unpinned");
                app.save_task()
            } else {
                Task::none()
            }
        }
        CommandPaletteAction::RenameFocused => update(app, Message::StartRenameFocused),
        CommandPaletteAction::RenameTerminal => update(app, Message::StartRenameTerminal),
        CommandPaletteAction::RenameWorktree => {
            let Some((project_id, worktree_id)) = app.active_worktree_ids() else {
                app.status = String::from("No active worktree to rename");
                return Task::none();
            };
            update(
                app,
                Message::StartRenameWorktree {
                    project_id,
                    worktree_id,
                },
            )
        }
        CommandPaletteAction::OpenPreferences => update(app, Message::OpenPreferences(true)),
        CommandPaletteAction::AddProject => update(app, Message::AddProject),
        CommandPaletteAction::AddWorktreeToProject => {
            update(app, Message::OpenAddWorktreeProjectPicker)
        }
        CommandPaletteAction::AddWorktreeToActiveProject => {
            let Some(project_id) = app.persisted.active_project_id.clone() else {
                app.status = String::from("No active project to add a worktree to");
                return Task::none();
            };
            update(app, Message::StartAddWorktree(project_id))
        }
        CommandPaletteAction::DeleteWorktreeFromProject => {
            update(app, Message::OpenDeleteWorktreeProjectPicker)
        }
        CommandPaletteAction::RescanAllProjects => update(app, Message::RescanAllProjects),
        CommandPaletteAction::RescanActiveProject => {
            let Some(project_id) = app.persisted.active_project_id.clone() else {
                app.status = String::from("No active project to rescan");
                return Task::none();
            };
            update(app, Message::ProjectRescan(project_id))
        }
        CommandPaletteAction::ToggleBrowsers => update(
            app,
            Message::SetEnableBrowsers(!app.persisted.ui.enable_browsers),
        ),
        CommandPaletteAction::AddBrowser => update(app, Message::AddBrowser),
        CommandPaletteAction::BrowserDevTools => update(app, Message::BrowserDevTools),
        CommandPaletteAction::FontIncrease => {
            input::apply_shortcut(app, ShortcutAction::FontIncrease)
        }
        CommandPaletteAction::FontDecrease => {
            input::apply_shortcut(app, ShortcutAction::FontDecrease)
        }
        CommandPaletteAction::FontReset => input::apply_shortcut(app, ShortcutAction::FontReset),
        CommandPaletteAction::NextTerminal => update(app, Message::SwitchTerminalByOffset(1)),
        CommandPaletteAction::PreviousTerminal => update(app, Message::SwitchTerminalByOffset(-1)),
    }
}

fn open_active_worktree_in_editor(
    app: &mut App,
    editor_command: &str,
    editor_label: &str,
) -> Task<Message> {
    if editor_command.is_empty() {
        app.status = format!("Set a {editor_label} editor command in Preferences");
        return Task::none();
    }

    if editor_command.chars().any(char::is_whitespace) {
        app.status = format!(
            "{} editor must be a single command like zed or code",
            capitalize(editor_label)
        );
        return Task::none();
    }

    let Some(target_path) = app.active_editor_target_path() else {
        app.status = String::from("No active worktree folder to open");
        return Task::none();
    };

    match open_in_editor_command(editor_command, &target_path) {
        Ok(()) => {
            app.status = format!("Opened {} in {}", target_path, editor_command);
        }
        Err(error) => {
            app.status = format!("Failed to open editor: {error}");
        }
    }

    Task::none()
}

fn open_in_editor_command(editor_command: &str, target_path: &str) -> Result<(), String> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| String::from("/bin/zsh"));

    let output = Command::new(&shell)
        .arg("-lc")
        .arg("command -v -- \"$1\"")
        .arg("not-terminal")
        .arg(editor_command)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(|error| format!("failed to resolve editor command via shell: {error}"))?;

    if !output.status.success() {
        return Err(format!("editor command not found: {}", editor_command));
    }

    let resolved_output = String::from_utf8_lossy(&output.stdout);
    let resolved = resolved_output
        .lines()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("editor command not found: {}", editor_command))?;

    Command::new(resolved)
        .arg(".")
        .current_dir(target_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not launch {}: {}", editor_command, error))
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut output = first.to_uppercase().collect::<String>();
    output.push_str(chars.as_str());
    output
}
