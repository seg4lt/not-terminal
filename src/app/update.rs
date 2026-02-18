use super::state::{App, Message, SESSION_COUNT};
use iced::{Task, keyboard, mouse, window};

pub(crate) fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::WindowLocated(window_id) => {
            let Some(window_id) = window_id else {
                app.ghostty_status = String::from("No window available for Ghostty embedding");
                return Task::none();
            };

            app.window_id = Some(window_id);
            App::app_ns_view(window_id)
        }
        Message::HostViewResolved(ns_view) => {
            app.host_ns_view = ns_view;
            if ns_view.is_none() {
                app.ghostty_status = String::from("Failed to resolve AppKit NSView");
            }
            app.try_init_sessions();
            Task::none()
        }
        Message::WindowSizeResolved(size) => {
            app.window_size = size;
            if app.sessions.is_empty() {
                app.try_init_sessions();
            } else {
                app.sync_session_views();
            }
            Task::none()
        }
        Message::WindowScaleResolved(scale) => {
            app.window_scale_factor = scale;
            if app.sessions.is_empty() {
                app.try_init_sessions();
            } else {
                app.sync_session_views();
            }
            Task::none()
        }
        Message::WindowEvent(window_id, event) => {
            if app.window_id.is_none_or(|current| current == window_id) {
                match event {
                    window::Event::Resized(size) => {
                        app.window_size = size;
                        app.sync_session_views();
                    }
                    window::Event::Rescaled(scale) => {
                        app.window_scale_factor = scale;
                        app.sync_session_views();
                    }
                    _ => {}
                }
            }
            Task::none()
        }
        Message::GhosttyTick => {
            for session in &mut app.sessions {
                if let Some(ghostty) = session.ghostty.as_mut() {
                    ghostty.tick_if_needed();
                }
            }
            Task::none()
        }
        Message::Keyboard(event) => {
            if let keyboard::Event::ModifiersChanged(modifiers) = &event {
                app.keyboard_modifiers = *modifiers;
            }

            if let Some(ghostty) = app.active_ghostty_mut() {
                if ghostty.handle_keyboard_event(&event) {
                    ghostty.refresh();
                    ghostty.force_tick();
                }
            }
            Task::none()
        }
        Message::Mouse(event) => {
            let modifiers = app.keyboard_modifiers;
            match event {
                mouse::Event::CursorMoved { position } => {
                    app.cursor_position_logical = Some(position);
                    let local = app.terminal_local_px_from_position(position);
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
                        .and_then(|position| app.terminal_local_px_from_position(position));

                    if let Some((x, y)) = local {
                        if let Some(ghostty) = app.active_ghostty_mut() {
                            ghostty.handle_mouse_move(x, y, modifiers);
                            if ghostty.handle_mouse_button(button, pressed, modifiers) {
                                ghostty.refresh();
                                ghostty.force_tick();
                            }
                        }
                    }
                }
                mouse::Event::WheelScrolled { delta } => {
                    let local = app
                        .cursor_position_logical
                        .and_then(|position| app.terminal_local_px_from_position(position));

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
        Message::SelectSession(index) => {
            if index < SESSION_COUNT {
                app.active_session = index;
                app.sync_session_views();
                if let Some(ghostty) = app.active_ghostty_mut() {
                    ghostty.refresh();
                    ghostty.force_tick();
                }
            }
            Task::none()
        }
    }
}
