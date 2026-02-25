use super::state::{App, Message};
use crate::app::state::{QuickOpenEntry, QuickOpenEntryKind};
use crate::ghostty_embed::{disable_system_hide_shortcuts, register_focus_toggle_hotkey};
use iced::{Task, widget::operation, window};
use std::time::Instant;

mod browser;
mod input;

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
            let mut had_any_work = false;

            for runtime in app.runtimes.values_mut() {
                let tick = runtime.tick_all();
                had_any_work |= tick.had_pending_work;
                layout_changed |= tick.layout_changed;
            }

            if app.process_runtime_actions() || layout_changed {
                app.sync_runtime_views();
            }

            // Update activity timestamp if there was actual work to do
            if had_any_work {
                app.last_ghostty_activity = Instant::now();
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
                app.worktree_context_menu = None;
            }
            app.sync_runtime_views();
            Task::none()
        }
        Message::OpenQuickOpen(open) => {
            app.quick_open_open = open;
            if open {
                app.quick_open_query.clear();
                app.quick_open_selected_index = 0;
                app.quick_open_ignore_next_query_change = false;
                app.preferences_open = false;
                app.rename_dialog = None;
                app.add_worktree_dialog = None;
                app.worktree_context_menu = None;
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
            if app.quick_open_open && app.keyboard_modifiers.logo() {
                return Task::none();
            }
            if app.quick_open_ignore_next_query_change {
                app.quick_open_ignore_next_query_change = false;
                return Task::none();
            }
            app.quick_open_query = value;
            app.quick_open_selected_index = 0; // Reset selection when query changes
            Task::none()
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
            app.quick_open_open = false;
            app.preferences_open = false;
            app.add_worktree_dialog = None;
            app.worktree_context_menu = None;
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
            app.worktree_context_menu = None;
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
            app.worktree_context_menu = None;
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
            app.worktree_context_menu = None;
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
        Message::RemoveProject(project_id) => match app.remove_project(&project_id) {
            Ok(()) => {
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
            show_project_actions,
        } => {
            app.worktree_context_menu = Some(crate::app::state::WorktreeContextMenu {
                project_id,
                worktree_id,
                show_project_actions,
            });
            app.quick_open_open = false;
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
        Message::WorktreeContextMenuProjectRescan => {
            let Some(menu) = app.worktree_context_menu.clone() else {
                return Task::none();
            };
            app.worktree_context_menu = None;
            update(app, Message::ProjectRescan(menu.project_id))
        }
        Message::WorktreeContextMenuRemoveProject => {
            let Some(menu) = app.worktree_context_menu.clone() else {
                return Task::none();
            };
            app.worktree_context_menu = None;
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
