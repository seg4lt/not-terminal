#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhosttySplitDirection {
    Right,
    Down,
    Left,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhosttyGotoSplitDirection {
    Previous,
    Next,
    Up,
    Left,
    Down,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhosttyResizeSplitDirection {
    Up,
    Left,
    Down,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GhosttyRuntimeAction {
    NewSplit {
        surface_ptr: usize,
        direction: GhosttySplitDirection,
    },
    GotoSplit {
        surface_ptr: usize,
        direction: GhosttyGotoSplitDirection,
    },
    ResizeSplit {
        surface_ptr: usize,
        direction: GhosttyResizeSplitDirection,
        amount: u16,
    },
    EqualizeSplits {
        surface_ptr: usize,
    },
    ToggleSplitZoom {
        surface_ptr: usize,
    },
    NewTab {
        surface_ptr: usize,
    },
    GotoTab {
        surface_ptr: usize,
        direction: i32,
    },
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{
        GhosttyGotoSplitDirection, GhosttyResizeSplitDirection, GhosttyRuntimeAction,
        GhosttySplitDirection,
    };
    use iced::keyboard::key::{Code, Named, NativeCode, Physical};
    use iced::keyboard::{Event as KeyboardEvent, Key, Location, Modifiers};
    use iced::mouse::Button as MouseButton;
    use iced::window::Window;
    use iced::window::raw_window_handle::RawWindowHandle;
    use std::ffi::{CString, c_char, c_int, c_void};
    use std::path::{Path, PathBuf};
    use std::ptr;

    type GhosttyInitFn = unsafe extern "C" fn(usize, *mut *mut c_char) -> c_int;

    #[repr(C)]
    struct RuntimeBundle {
        _private: [u8; 0],
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct GhosttyInputKey {
        action: c_int,
        mods: c_int,
        consumed_mods: c_int,
        keycode: u32,
        text: *const c_char,
        unshifted_codepoint: u32,
        composing: bool,
    }

    #[repr(C)]
    #[derive(Copy, Clone, Default)]
    struct RuntimeQueuedAction {
        tag: u32,
        surface: usize,
        arg0: i32,
        amount: u16,
        reserved: u16,
    }

    const GHOSTTY_ACTION_RELEASE: c_int = 0;
    const GHOSTTY_ACTION_PRESS: c_int = 1;
    const GHOSTTY_ACTION_REPEAT: c_int = 2;

    const RUST_GHOSTTY_ACTION_NEW_SPLIT: u32 = 1;
    const RUST_GHOSTTY_ACTION_GOTO_SPLIT: u32 = 2;
    const RUST_GHOSTTY_ACTION_RESIZE_SPLIT: u32 = 3;
    const RUST_GHOSTTY_ACTION_EQUALIZE_SPLITS: u32 = 4;
    const RUST_GHOSTTY_ACTION_TOGGLE_SPLIT_ZOOM: u32 = 5;
    const RUST_GHOSTTY_ACTION_NEW_TAB: u32 = 6;
    const RUST_GHOSTTY_ACTION_GOTO_TAB: u32 = 7;

    const GHOSTTY_SPLIT_DIRECTION_RIGHT: i32 = 0;
    const GHOSTTY_SPLIT_DIRECTION_DOWN: i32 = 1;
    const GHOSTTY_SPLIT_DIRECTION_LEFT: i32 = 2;
    const GHOSTTY_SPLIT_DIRECTION_UP: i32 = 3;

    const GHOSTTY_GOTO_SPLIT_PREVIOUS: i32 = 0;
    const GHOSTTY_GOTO_SPLIT_NEXT: i32 = 1;
    const GHOSTTY_GOTO_SPLIT_UP: i32 = 2;
    const GHOSTTY_GOTO_SPLIT_LEFT: i32 = 3;
    const GHOSTTY_GOTO_SPLIT_DOWN: i32 = 4;
    const GHOSTTY_GOTO_SPLIT_RIGHT: i32 = 5;

    const GHOSTTY_RESIZE_SPLIT_UP: i32 = 0;
    const GHOSTTY_RESIZE_SPLIT_DOWN: i32 = 1;
    const GHOSTTY_RESIZE_SPLIT_LEFT: i32 = 2;
    const GHOSTTY_RESIZE_SPLIT_RIGHT: i32 = 3;

    const GHOSTTY_MODS_NONE: c_int = 0;
    const GHOSTTY_MODS_SHIFT: c_int = 1 << 0;
    const GHOSTTY_MODS_CTRL: c_int = 1 << 1;
    const GHOSTTY_MODS_ALT: c_int = 1 << 2;
    const GHOSTTY_MODS_SUPER: c_int = 1 << 3;
    const GHOSTTY_MODS_SHIFT_RIGHT: c_int = 1 << 6;
    const GHOSTTY_MODS_CTRL_RIGHT: c_int = 1 << 7;
    const GHOSTTY_MODS_ALT_RIGHT: c_int = 1 << 8;
    const GHOSTTY_MODS_SUPER_RIGHT: c_int = 1 << 9;
    const GHOSTTY_SCROLL_MOD_PRECISION: c_int = 1;

    const GHOSTTY_MOUSE_RELEASE: c_int = 0;
    const GHOSTTY_MOUSE_PRESS: c_int = 1;
    const GHOSTTY_MOUSE_UNKNOWN: c_int = 0;
    const GHOSTTY_MOUSE_LEFT: c_int = 1;
    const GHOSTTY_MOUSE_RIGHT: c_int = 2;
    const GHOSTTY_MOUSE_MIDDLE: c_int = 3;
    const GHOSTTY_MOUSE_FOUR: c_int = 4;
    const GHOSTTY_MOUSE_FIVE: c_int = 5;
    const GHOSTTY_MOUSE_SIX: c_int = 6;
    const GHOSTTY_MOUSE_SEVEN: c_int = 7;
    const GHOSTTY_MOUSE_EIGHT: c_int = 8;
    const GHOSTTY_MOUSE_NINE: c_int = 9;
    const GHOSTTY_MOUSE_TEN: c_int = 10;
    const GHOSTTY_MOUSE_ELEVEN: c_int = 11;
    const DEFAULT_THEME_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/ghostty-theme.ghostty");

    unsafe extern "C" {
        fn ghostty_init(argc: usize, argv: *mut *mut c_char) -> c_int;
        fn ghostty_config_new() -> *mut c_void;
        fn ghostty_config_free(config: *mut c_void);
        fn ghostty_config_load_default_files(config: *mut c_void);
        fn ghostty_config_load_file(config: *mut c_void, path: *const c_char);
        fn ghostty_config_finalize(config: *mut c_void);
        fn ghostty_app_new(runtime_config: *const c_void, config: *mut c_void) -> *mut c_void;
        fn ghostty_app_free(app: *mut c_void);
        fn ghostty_app_tick(app: *mut c_void);
        fn ghostty_app_set_focus(app: *mut c_void, focused: bool);
        fn ghostty_surface_new(app: *mut c_void, config: *const c_void) -> *mut c_void;
        fn ghostty_surface_free(surface: *mut c_void);
        fn ghostty_surface_process_exited(surface: *mut c_void) -> bool;
        fn ghostty_surface_set_size(surface: *mut c_void, width: u32, height: u32);
        fn ghostty_surface_set_content_scale(surface: *mut c_void, x: f64, y: f64);
        fn ghostty_surface_set_focus(surface: *mut c_void, focused: bool);
        fn ghostty_surface_refresh(surface: *mut c_void);
        fn ghostty_surface_key(surface: *mut c_void, event: GhosttyInputKey) -> bool;
        fn ghostty_surface_key_is_binding(
            surface: *mut c_void,
            event: GhosttyInputKey,
            flags: *mut c_int,
        ) -> bool;
        fn ghostty_surface_mouse_button(
            surface: *mut c_void,
            state: c_int,
            button: c_int,
            mods: c_int,
        ) -> bool;
        fn ghostty_surface_mouse_pos(surface: *mut c_void, x: f64, y: f64, mods: c_int);
        fn ghostty_surface_mouse_scroll(surface: *mut c_void, x: f64, y: f64, scroll_mods: c_int);
        fn ghostty_surface_binding_action(
            surface: *mut c_void,
            action_ptr: *const u8,
            action_len: usize,
        ) -> bool;

        fn rust_ghostty_runtime_bundle_new() -> *mut RuntimeBundle;
        fn rust_ghostty_runtime_bundle_free(bundle: *mut RuntimeBundle);
        fn rust_ghostty_runtime_bundle_set_surface(bundle: *mut RuntimeBundle, surface: *mut c_void);
        fn rust_ghostty_runtime_config_ptr(bundle: *const RuntimeBundle) -> *const c_void;
        fn rust_ghostty_runtime_take_pending_tick(bundle: *const RuntimeBundle) -> bool;
        fn rust_ghostty_runtime_take_pending_action(
            bundle: *const RuntimeBundle,
            out_action: *mut RuntimeQueuedAction,
        ) -> bool;
        fn rust_ghostty_runtime_action_queue_len(bundle: *const RuntimeBundle) -> u32;
        fn rust_ghostty_surface_new_macos(
            surface_new_fn_raw: *mut c_void,
            app: *mut c_void,
            ns_view: *mut c_void,
            scale_factor: f64,
            font_size_points: f32,
            working_directory: *const c_char,
        ) -> *mut c_void;
        fn rust_ghostty_host_view_new(parent_ns_view: *mut c_void) -> *mut c_void;
        fn rust_ghostty_host_view_set_frame(
            host_ns_view: *mut c_void,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
        );
        fn rust_ghostty_host_view_set_hidden(host_ns_view: *mut c_void, hidden: bool);
        fn rust_ghostty_host_view_free(host_ns_view: *mut c_void);
        fn rust_ghostty_disable_system_hide_shortcuts();
    }

    pub struct GhosttyEmbed {
        runtime_bundle: *mut RuntimeBundle,
        config: *mut c_void,
        app: *mut c_void,
        surface: *mut c_void,
        modifiers: Modifiers,
    }

    impl GhosttyEmbed {
        pub fn new(
            ns_view: usize,
            width_px: u32,
            height_px: u32,
            scale_factor: f64,
            working_directory: Option<&str>,
        ) -> Result<Self, String> {
            if ns_view == 0 {
                return Err(String::from("received null NSView pointer"));
            }

            let ghostty_init_fn: GhosttyInitFn = ghostty_init;
            let mut runtime_bundle: *mut RuntimeBundle = ptr::null_mut();
            let mut config: *mut c_void = ptr::null_mut();
            let mut app: *mut c_void = ptr::null_mut();
            let mut surface: *mut c_void = ptr::null_mut();
            let working_directory_cstr = to_c_string_optional(working_directory);

            let create_result = (|| -> Result<(), String> {
                unsafe {
                    let init_result = ghostty_init_fn(0, ptr::null_mut());
                    if init_result != 0 {
                        return Err(format!("ghostty_init failed with code {init_result}"));
                    }

                    runtime_bundle = rust_ghostty_runtime_bundle_new();
                    if runtime_bundle.is_null() {
                        return Err(String::from("failed to allocate Ghostty runtime bundle"));
                    }

                    config = ghostty_config_new();
                    if config.is_null() {
                        return Err(String::from("ghostty_config_new returned null"));
                    }

                    ghostty_config_load_default_files(config);
                    if !has_user_ghostty_config() {
                        load_default_theme(config);
                    }
                    ghostty_config_finalize(config);

                    let runtime_config = rust_ghostty_runtime_config_ptr(runtime_bundle);
                    if runtime_config.is_null() {
                        return Err(String::from("failed to create Ghostty runtime config"));
                    }

                    app = ghostty_app_new(runtime_config, config);
                    if app.is_null() {
                        return Err(String::from("ghostty_app_new returned null"));
                    }

                    surface = rust_ghostty_surface_new_macos(
                        ghostty_surface_new as *const () as *mut c_void,
                        app,
                        ns_view as *mut c_void,
                        scale_factor,
                        0.0,
                        working_directory_cstr
                            .as_ref()
                            .map_or(ptr::null(), |value| value.as_ptr()),
                    );
                    if surface.is_null() {
                        return Err(String::from("ghostty_surface_new returned null"));
                    }

                    // Store the surface pointer in the runtime bundle for clipboard callbacks
                    rust_ghostty_runtime_bundle_set_surface(runtime_bundle, surface);

                    ghostty_surface_set_content_scale(surface, scale_factor, scale_factor);
                    ghostty_surface_set_size(surface, width_px.max(1), height_px.max(1));
                    ghostty_surface_set_focus(surface, true);
                    ghostty_app_set_focus(app, true);
                    ghostty_app_tick(app);
                }

                Ok(())
            })();

            if let Err(err) = create_result {
                unsafe {
                    if !surface.is_null() {
                        ghostty_surface_free(surface);
                    }
                    if !app.is_null() {
                        ghostty_app_free(app);
                    }
                    if !config.is_null() {
                        ghostty_config_free(config);
                    }
                    if !runtime_bundle.is_null() {
                        rust_ghostty_runtime_bundle_free(runtime_bundle);
                    }
                }
                return Err(err);
            }

            Ok(Self {
                runtime_bundle,
                config,
                app,
                surface,
                modifiers: Modifiers::default(),
            })
        }

        pub fn set_size(&mut self, width_px: u32, height_px: u32) {
            unsafe {
                ghostty_surface_set_size(self.surface, width_px.max(1), height_px.max(1));
            }
        }

        pub fn set_scale_factor(&mut self, scale_factor: f64) {
            unsafe {
                ghostty_surface_set_content_scale(self.surface, scale_factor, scale_factor);
            }
        }

        pub fn set_focus(&mut self, focused: bool) {
            unsafe {
                ghostty_surface_set_focus(self.surface, focused);
                ghostty_app_set_focus(self.app, focused);
            }
        }

        pub fn refresh(&mut self) {
            unsafe {
                ghostty_surface_refresh(self.surface);
            }
        }

        pub fn process_exited(&self) -> bool {
            unsafe { ghostty_surface_process_exited(self.surface) }
        }

        pub fn tick_if_needed(&mut self) {
            unsafe {
                if rust_ghostty_runtime_take_pending_tick(self.runtime_bundle) {
                    ghostty_app_tick(self.app);
                }
            }
        }

        pub fn force_tick(&mut self) {
            unsafe {
                ghostty_app_tick(self.app);
            }
        }

        pub fn surface_ptr(&self) -> usize {
            self.surface as usize
        }

        pub fn update_modifiers(&mut self, modifiers: Modifiers) {
            self.modifiers = modifiers;
        }

        pub fn modifiers(&self) -> Modifiers {
            self.modifiers
        }

        pub fn key_event_is_binding(&self, event: &KeyboardEvent) -> bool {
            let KeyboardEvent::KeyPressed {
                key,
                physical_key,
                modifiers,
                location,
                text,
                repeat,
                ..
            } = event
            else {
                return false;
            };

            let mut effective_modifiers = self.modifiers | *modifiers;
            apply_modifier_key_state(&mut effective_modifiers, key, true);
            let action = if *repeat {
                GHOSTTY_ACTION_REPEAT
            } else {
                GHOSTTY_ACTION_PRESS
            };
            let keycode = keycode_from_physical(physical_key);
            let modifiers = normalize_text_modifiers(effective_modifiers, text.as_deref());
            let mods = ghostty_mods(modifiers, key, location);
            let unshifted_codepoint = unshifted_codepoint(key, physical_key);
            let text_cstr = text.as_deref().and_then(to_c_string);
            let input = GhosttyInputKey {
                action,
                mods,
                consumed_mods: GHOSTTY_MODS_NONE,
                keycode,
                text: text_cstr
                    .as_ref()
                    .map(|s| s.as_ptr())
                    .unwrap_or(ptr::null()),
                unshifted_codepoint,
                composing: false,
            };

            unsafe { ghostty_surface_key_is_binding(self.surface, input, ptr::null_mut()) }
        }

        pub fn drain_actions(&mut self) -> Vec<GhosttyRuntimeAction> {
            let mut actions = Vec::new();

            unsafe {
                if rust_ghostty_runtime_action_queue_len(self.runtime_bundle) == 0 {
                    return actions;
                }
            }

            loop {
                let mut raw = RuntimeQueuedAction::default();
                let has_action = unsafe {
                    rust_ghostty_runtime_take_pending_action(self.runtime_bundle, &mut raw)
                };
                if !has_action {
                    break;
                }

                if let Some(action) = runtime_action_from_raw(raw) {
                    actions.push(action);
                }
            }

            actions
        }

        pub fn handle_keyboard_event(&mut self, event: &KeyboardEvent) -> bool {
            match event {
                KeyboardEvent::ModifiersChanged(modifiers) => {
                    self.modifiers = *modifiers;
                    false
                }
                KeyboardEvent::KeyPressed {
                    key,
                    physical_key,
                    modifiers,
                    location,
                    text,
                    repeat,
                    ..
                } => {
                    let mut effective_modifiers = self.modifiers | *modifiers;
                    apply_modifier_key_state(&mut effective_modifiers, key, true);
                    self.modifiers = effective_modifiers;
                    let action = if *repeat {
                        GHOSTTY_ACTION_REPEAT
                    } else {
                        GHOSTTY_ACTION_PRESS
                    };
                    self.send_key_event(
                        action,
                        key,
                        physical_key,
                        effective_modifiers,
                        location,
                        text.as_deref(),
                    )
                }
                KeyboardEvent::KeyReleased {
                    key,
                    physical_key,
                    modifiers,
                    location,
                    ..
                } => {
                    let mut effective_modifiers = self.modifiers | *modifiers;
                    apply_modifier_key_state(&mut effective_modifiers, key, false);
                    self.modifiers = effective_modifiers;
                    self.send_key_event(
                        GHOSTTY_ACTION_RELEASE,
                        key,
                        physical_key,
                        effective_modifiers,
                        location,
                        None,
                    )
                }
            }
        }

        pub fn handle_mouse_move(&mut self, x: f64, y: f64, modifiers: Modifiers) {
            unsafe {
                ghostty_surface_mouse_pos(self.surface, x, y, ghostty_mods_basic(modifiers));
            }
        }

        pub fn handle_mouse_button(
            &mut self,
            button: MouseButton,
            pressed: bool,
            modifiers: Modifiers,
        ) -> bool {
            let button = ghostty_mouse_button(button);
            let state = if pressed {
                GHOSTTY_MOUSE_PRESS
            } else {
                GHOSTTY_MOUSE_RELEASE
            };

            unsafe {
                ghostty_surface_mouse_button(
                    self.surface,
                    state,
                    button,
                    ghostty_mods_basic(modifiers),
                )
            }
        }

        pub fn handle_mouse_scroll(&mut self, x: f64, y: f64, precision: bool) {
            let mut scroll_mods: c_int = 0;
            if precision {
                scroll_mods |= GHOSTTY_SCROLL_MOD_PRECISION;
            }

            unsafe {
                ghostty_surface_mouse_scroll(self.surface, x, y, scroll_mods);
            }
        }

        pub fn binding_action(&mut self, action: &str) -> bool {
            if action.is_empty() {
                return false;
            }

            unsafe { ghostty_surface_binding_action(self.surface, action.as_ptr(), action.len()) }
        }

        fn send_key_event(
            &self,
            action: c_int,
            key: &Key,
            physical_key: &Physical,
            modifiers: Modifiers,
            location: &Location,
            text: Option<&str>,
        ) -> bool {
            let keycode = keycode_from_physical(physical_key);
            let modifiers = normalize_text_modifiers(modifiers, text);
            let mods = ghostty_mods(modifiers, key, location);
            let unshifted_codepoint = unshifted_codepoint(key, physical_key);
            let text_cstr = text.and_then(to_c_string);
            let input = GhosttyInputKey {
                action,
                mods,
                consumed_mods: GHOSTTY_MODS_NONE,
                keycode,
                text: text_cstr
                    .as_ref()
                    .map(|s| s.as_ptr())
                    .unwrap_or(ptr::null()),
                unshifted_codepoint,
                composing: false,
            };

            unsafe { ghostty_surface_key(self.surface, input) }
        }
    }

    impl Drop for GhosttyEmbed {
        fn drop(&mut self) {
            unsafe {
                if !self.surface.is_null() {
                    ghostty_surface_free(self.surface);
                }
                if !self.app.is_null() {
                    ghostty_app_free(self.app);
                }
                if !self.config.is_null() {
                    ghostty_config_free(self.config);
                }
                if !self.runtime_bundle.is_null() {
                    rust_ghostty_runtime_bundle_free(self.runtime_bundle);
                }
            }
        }
    }

    pub fn ns_view_ptr(window: &dyn Window) -> Option<usize> {
        let handle = window.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::AppKit(appkit) => Some(appkit.ns_view.as_ptr() as usize),
            _ => None,
        }
    }

    pub fn host_view_new(parent_ns_view: usize) -> Option<usize> {
        if parent_ns_view == 0 {
            return None;
        }

        let raw = unsafe { rust_ghostty_host_view_new(parent_ns_view as *mut c_void) };
        if raw.is_null() {
            None
        } else {
            Some(raw as usize)
        }
    }

    pub fn host_view_set_frame(host_ns_view: usize, x: f64, y: f64, width: f64, height: f64) {
        if host_ns_view == 0 {
            return;
        }

        unsafe {
            rust_ghostty_host_view_set_frame(host_ns_view as *mut c_void, x, y, width, height);
        }
    }

    pub fn host_view_set_hidden(host_ns_view: usize, hidden: bool) {
        if host_ns_view == 0 {
            return;
        }

        unsafe {
            rust_ghostty_host_view_set_hidden(host_ns_view as *mut c_void, hidden);
        }
    }

    pub fn host_view_free(host_ns_view: usize) {
        if host_ns_view == 0 {
            return;
        }

        unsafe {
            rust_ghostty_host_view_free(host_ns_view as *mut c_void);
        }
    }

    pub fn disable_system_hide_shortcuts() {
        unsafe {
            rust_ghostty_disable_system_hide_shortcuts();
        }
    }

    fn to_c_string(value: &str) -> Option<CString> {
        if value.is_empty() {
            return None;
        }

        if value.as_bytes().contains(&0) {
            let filtered: Vec<u8> = value
                .as_bytes()
                .iter()
                .copied()
                .filter(|byte| *byte != 0)
                .collect();
            if filtered.is_empty() {
                None
            } else {
                CString::new(filtered).ok()
            }
        } else {
            CString::new(value).ok()
        }
    }

    fn to_c_string_optional(value: Option<&str>) -> Option<CString> {
        value.and_then(to_c_string)
    }

    fn load_default_theme(config: *mut c_void) {
        if let Ok(path) = CString::new(DEFAULT_THEME_PATH) {
            unsafe {
                ghostty_config_load_file(config, path.as_ptr());
            }
        }
    }

    fn has_user_ghostty_config() -> bool {
        user_ghostty_config_candidates()
            .iter()
            .any(|path| file_exists_and_non_empty(path))
    }

    fn file_exists_and_non_empty(path: &Path) -> bool {
        path.metadata()
            .map(|metadata| metadata.is_file() && metadata.len() > 0)
            .unwrap_or(false)
    }

    fn user_ghostty_config_candidates() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Some(config_dir) = dirs::config_dir() {
            paths.push(config_dir.join("ghostty/config.ghostty"));
            paths.push(config_dir.join("ghostty/config"));
        }

        if let Some(home_dir) = dirs::home_dir() {
            paths.push(home_dir.join(".config/ghostty/config.ghostty"));
            paths.push(home_dir.join(".config/ghostty/config"));

            #[cfg(target_os = "macos")]
            {
                let app_support =
                    home_dir.join("Library/Application Support/com.mitchellh.ghostty");
                paths.push(app_support.join("config.ghostty"));
                paths.push(app_support.join("config"));
            }
        }

        paths
    }

    fn runtime_action_from_raw(raw: RuntimeQueuedAction) -> Option<GhosttyRuntimeAction> {
        match raw.tag {
            RUST_GHOSTTY_ACTION_NEW_SPLIT => {
                let direction = match raw.arg0 {
                    GHOSTTY_SPLIT_DIRECTION_RIGHT => GhosttySplitDirection::Right,
                    GHOSTTY_SPLIT_DIRECTION_DOWN => GhosttySplitDirection::Down,
                    GHOSTTY_SPLIT_DIRECTION_LEFT => GhosttySplitDirection::Left,
                    GHOSTTY_SPLIT_DIRECTION_UP => GhosttySplitDirection::Up,
                    _ => return None,
                };

                Some(GhosttyRuntimeAction::NewSplit {
                    surface_ptr: raw.surface,
                    direction,
                })
            }
            RUST_GHOSTTY_ACTION_GOTO_SPLIT => {
                let direction = match raw.arg0 {
                    GHOSTTY_GOTO_SPLIT_PREVIOUS => GhosttyGotoSplitDirection::Previous,
                    GHOSTTY_GOTO_SPLIT_NEXT => GhosttyGotoSplitDirection::Next,
                    GHOSTTY_GOTO_SPLIT_UP => GhosttyGotoSplitDirection::Up,
                    GHOSTTY_GOTO_SPLIT_LEFT => GhosttyGotoSplitDirection::Left,
                    GHOSTTY_GOTO_SPLIT_DOWN => GhosttyGotoSplitDirection::Down,
                    GHOSTTY_GOTO_SPLIT_RIGHT => GhosttyGotoSplitDirection::Right,
                    _ => return None,
                };

                Some(GhosttyRuntimeAction::GotoSplit {
                    surface_ptr: raw.surface,
                    direction,
                })
            }
            RUST_GHOSTTY_ACTION_RESIZE_SPLIT => {
                let direction = match raw.arg0 {
                    GHOSTTY_RESIZE_SPLIT_UP => GhosttyResizeSplitDirection::Up,
                    GHOSTTY_RESIZE_SPLIT_LEFT => GhosttyResizeSplitDirection::Left,
                    GHOSTTY_RESIZE_SPLIT_DOWN => GhosttyResizeSplitDirection::Down,
                    GHOSTTY_RESIZE_SPLIT_RIGHT => GhosttyResizeSplitDirection::Right,
                    _ => return None,
                };

                Some(GhosttyRuntimeAction::ResizeSplit {
                    surface_ptr: raw.surface,
                    direction,
                    amount: raw.amount,
                })
            }
            RUST_GHOSTTY_ACTION_EQUALIZE_SPLITS => Some(GhosttyRuntimeAction::EqualizeSplits {
                surface_ptr: raw.surface,
            }),
            RUST_GHOSTTY_ACTION_TOGGLE_SPLIT_ZOOM => Some(GhosttyRuntimeAction::ToggleSplitZoom {
                surface_ptr: raw.surface,
            }),
            RUST_GHOSTTY_ACTION_NEW_TAB => Some(GhosttyRuntimeAction::NewTab {
                surface_ptr: raw.surface,
            }),
            RUST_GHOSTTY_ACTION_GOTO_TAB => Some(GhosttyRuntimeAction::GotoTab {
                surface_ptr: raw.surface,
                direction: raw.arg0,
            }),
            _ => None,
        }
    }

    fn normalize_text_modifiers(modifiers: Modifiers, text: Option<&str>) -> Modifiers {
        let Some(text) = text else {
            return modifiers;
        };

        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return modifiers;
        };
        if chars.next().is_some() || first.is_control() {
            return modifiers;
        }

        if modifiers.shift() && !modifiers.control() && !modifiers.alt() && !modifiers.logo() {
            let mut normalized = modifiers;
            normalized.remove(Modifiers::SHIFT);
            normalized
        } else {
            modifiers
        }
    }

    fn apply_modifier_key_state(modifiers: &mut Modifiers, key: &Key, pressed: bool) {
        let mut set = |flag: Modifiers| {
            if pressed {
                modifiers.insert(flag);
            } else {
                modifiers.remove(flag);
            }
        };

        match key.as_ref() {
            Key::Named(Named::Shift) => set(Modifiers::SHIFT),
            Key::Named(Named::Control) => set(Modifiers::CTRL),
            Key::Named(Named::Alt) | Key::Named(Named::AltGraph) => set(Modifiers::ALT),
            Key::Named(Named::Super) | Key::Named(Named::Meta) | Key::Named(Named::Hyper) => {
                set(Modifiers::LOGO)
            }
            _ => {}
        }
    }

    fn ghostty_mods(modifiers: Modifiers, key: &Key, location: &Location) -> c_int {
        let mut bits = ghostty_mods_basic(modifiers);

        if *location == Location::Right {
            if matches!(key.as_ref(), Key::Named(iced::keyboard::key::Named::Shift)) {
                bits |= GHOSTTY_MODS_SHIFT_RIGHT;
            }
            if matches!(
                key.as_ref(),
                Key::Named(iced::keyboard::key::Named::Control)
            ) {
                bits |= GHOSTTY_MODS_CTRL_RIGHT;
            }
            if matches!(key.as_ref(), Key::Named(iced::keyboard::key::Named::Alt)) {
                bits |= GHOSTTY_MODS_ALT_RIGHT;
            }
            if matches!(
                key.as_ref(),
                Key::Named(iced::keyboard::key::Named::Super)
                    | Key::Named(iced::keyboard::key::Named::Meta)
            ) {
                bits |= GHOSTTY_MODS_SUPER_RIGHT;
            }
        }

        bits
    }

    fn ghostty_mods_basic(modifiers: Modifiers) -> c_int {
        let mut bits = GHOSTTY_MODS_NONE;
        if modifiers.shift() {
            bits |= GHOSTTY_MODS_SHIFT;
        }
        if modifiers.control() {
            bits |= GHOSTTY_MODS_CTRL;
        }
        if modifiers.alt() {
            bits |= GHOSTTY_MODS_ALT;
        }
        if modifiers.logo() {
            bits |= GHOSTTY_MODS_SUPER;
        }
        bits
    }

    fn ghostty_mouse_button(button: MouseButton) -> c_int {
        match button {
            MouseButton::Left => GHOSTTY_MOUSE_LEFT,
            MouseButton::Right => GHOSTTY_MOUSE_RIGHT,
            MouseButton::Middle => GHOSTTY_MOUSE_MIDDLE,
            MouseButton::Back => GHOSTTY_MOUSE_FOUR,
            MouseButton::Forward => GHOSTTY_MOUSE_FIVE,
            MouseButton::Other(value) => match value {
                0 => GHOSTTY_MOUSE_LEFT,
                1 => GHOSTTY_MOUSE_RIGHT,
                2 => GHOSTTY_MOUSE_MIDDLE,
                3 => GHOSTTY_MOUSE_FOUR,
                4 => GHOSTTY_MOUSE_FIVE,
                5 => GHOSTTY_MOUSE_SIX,
                6 => GHOSTTY_MOUSE_SEVEN,
                7 => GHOSTTY_MOUSE_EIGHT,
                8 => GHOSTTY_MOUSE_NINE,
                9 => GHOSTTY_MOUSE_TEN,
                10 => GHOSTTY_MOUSE_ELEVEN,
                _ => GHOSTTY_MOUSE_UNKNOWN,
            },
        }
    }

    fn unshifted_codepoint(key: &Key, physical_key: &Physical) -> u32 {
        if let Some(latin) = key.to_latin(*physical_key) {
            if latin.is_ascii_alphabetic() {
                return latin.to_ascii_lowercase() as u32;
            }
        }

        match physical_key {
            Physical::Code(code) => unshifted_char_from_code(*code)
                .map(|character| character as u32)
                .unwrap_or(0),
            Physical::Unidentified(_) => 0,
        }
    }

    fn unshifted_char_from_code(code: Code) -> Option<char> {
        Some(match code {
            Code::KeyA => 'a',
            Code::KeyB => 'b',
            Code::KeyC => 'c',
            Code::KeyD => 'd',
            Code::KeyE => 'e',
            Code::KeyF => 'f',
            Code::KeyG => 'g',
            Code::KeyH => 'h',
            Code::KeyI => 'i',
            Code::KeyJ => 'j',
            Code::KeyK => 'k',
            Code::KeyL => 'l',
            Code::KeyM => 'm',
            Code::KeyN => 'n',
            Code::KeyO => 'o',
            Code::KeyP => 'p',
            Code::KeyQ => 'q',
            Code::KeyR => 'r',
            Code::KeyS => 's',
            Code::KeyT => 't',
            Code::KeyU => 'u',
            Code::KeyV => 'v',
            Code::KeyW => 'w',
            Code::KeyX => 'x',
            Code::KeyY => 'y',
            Code::KeyZ => 'z',
            Code::Digit0 => '0',
            Code::Digit1 => '1',
            Code::Digit2 => '2',
            Code::Digit3 => '3',
            Code::Digit4 => '4',
            Code::Digit5 => '5',
            Code::Digit6 => '6',
            Code::Digit7 => '7',
            Code::Digit8 => '8',
            Code::Digit9 => '9',
            Code::Space => ' ',
            Code::Minus => '-',
            Code::Equal => '=',
            Code::BracketLeft => '[',
            Code::BracketRight => ']',
            Code::Backslash => '\\',
            Code::Semicolon => ';',
            Code::Quote => '\'',
            Code::Backquote => '`',
            Code::Comma => ',',
            Code::Period => '.',
            Code::Slash => '/',
            _ => return None,
        })
    }

    fn keycode_from_physical(physical_key: &Physical) -> u32 {
        match physical_key {
            Physical::Unidentified(NativeCode::MacOS(scan_code)) => u32::from(*scan_code),
            Physical::Code(code) => mac_keycode_from_code(*code).unwrap_or(0),
            _ => 0,
        }
    }

    fn mac_keycode_from_code(code: Code) -> Option<u32> {
        Some(match code {
            Code::AltLeft => 0x3a,
            Code::AltRight => 0x3d,
            Code::ArrowDown => 0x7d,
            Code::ArrowLeft => 0x7b,
            Code::ArrowRight => 0x7c,
            Code::ArrowUp => 0x7e,
            Code::AudioVolumeDown => 0x49,
            Code::AudioVolumeMute => 0x4a,
            Code::AudioVolumeUp => 0x48,
            Code::Backquote => 0x32,
            Code::Backslash => 0x2a,
            Code::Backspace => 0x33,
            Code::BracketLeft => 0x21,
            Code::BracketRight => 0x1e,
            Code::CapsLock => 0x39,
            Code::Comma => 0x2b,
            Code::ContextMenu => 0x6e,
            Code::ControlLeft => 0x3b,
            Code::ControlRight => 0x3e,
            Code::Delete => 0x75,
            Code::Digit0 => 0x1d,
            Code::Digit1 => 0x12,
            Code::Digit2 => 0x13,
            Code::Digit3 => 0x14,
            Code::Digit4 => 0x15,
            Code::Digit5 => 0x17,
            Code::Digit6 => 0x16,
            Code::Digit7 => 0x1a,
            Code::Digit8 => 0x1c,
            Code::Digit9 => 0x19,
            Code::End => 0x77,
            Code::Enter => 0x24,
            Code::Equal => 0x18,
            Code::Escape => 0x35,
            Code::F1 => 0x7a,
            Code::F2 => 0x78,
            Code::F3 => 0x63,
            Code::F4 => 0x76,
            Code::F5 => 0x60,
            Code::F6 => 0x61,
            Code::F7 => 0x62,
            Code::F8 => 0x64,
            Code::F9 => 0x65,
            Code::F10 => 0x6d,
            Code::F11 => 0x67,
            Code::F12 => 0x6f,
            Code::F13 => 0x69,
            Code::F14 => 0x6b,
            Code::F15 => 0x71,
            Code::F16 => 0x6a,
            Code::F17 => 0x40,
            Code::F18 => 0x4f,
            Code::F19 => 0x50,
            Code::F20 => 0x5a,
            Code::Home => 0x73,
            Code::Insert => 0x72,
            Code::IntlBackslash => 0x0a,
            Code::IntlRo => 0x5e,
            Code::IntlYen => 0x5d,
            Code::KeyA => 0x00,
            Code::KeyB => 0x0b,
            Code::KeyC => 0x08,
            Code::KeyD => 0x02,
            Code::KeyE => 0x0e,
            Code::KeyF => 0x03,
            Code::KeyG => 0x05,
            Code::KeyH => 0x04,
            Code::KeyI => 0x22,
            Code::KeyJ => 0x26,
            Code::KeyK => 0x28,
            Code::KeyL => 0x25,
            Code::KeyM => 0x2e,
            Code::KeyN => 0x2d,
            Code::KeyO => 0x1f,
            Code::KeyP => 0x23,
            Code::KeyQ => 0x0c,
            Code::KeyR => 0x0f,
            Code::KeyS => 0x01,
            Code::KeyT => 0x11,
            Code::KeyU => 0x20,
            Code::KeyV => 0x09,
            Code::KeyW => 0x0d,
            Code::KeyX => 0x07,
            Code::KeyY => 0x10,
            Code::KeyZ => 0x06,
            Code::Lang1 => 0x68,
            Code::Lang2 => 0x66,
            Code::SuperLeft => 0x37,
            Code::SuperRight => 0x36,
            Code::Minus => 0x1b,
            Code::NumLock => 0x47,
            Code::Numpad0 => 0x52,
            Code::Numpad1 => 0x53,
            Code::Numpad2 => 0x54,
            Code::Numpad3 => 0x55,
            Code::Numpad4 => 0x56,
            Code::Numpad5 => 0x57,
            Code::Numpad6 => 0x58,
            Code::Numpad7 => 0x59,
            Code::Numpad8 => 0x5b,
            Code::Numpad9 => 0x5c,
            Code::NumpadAdd => 0x45,
            Code::NumpadComma => 0x5f,
            Code::NumpadDecimal => 0x41,
            Code::NumpadDivide => 0x4b,
            Code::NumpadEnter => 0x4c,
            Code::NumpadEqual => 0x51,
            Code::NumpadMultiply => 0x43,
            Code::NumpadSubtract => 0x4e,
            Code::PageDown => 0x79,
            Code::PageUp => 0x74,
            Code::Period => 0x2f,
            Code::Quote => 0x27,
            Code::Semicolon => 0x29,
            Code::ShiftLeft => 0x38,
            Code::ShiftRight => 0x3c,
            Code::Slash => 0x2c,
            Code::Space => 0x31,
            Code::Tab => 0x30,
            _ => return None,
        })
    }
}

#[cfg(target_os = "macos")]
pub use macos::{
    GhosttyEmbed, disable_system_hide_shortcuts, host_view_free, host_view_new,
    host_view_set_frame, host_view_set_hidden, ns_view_ptr,
};

#[cfg(not(target_os = "macos"))]
pub struct GhosttyEmbed;

#[cfg(not(target_os = "macos"))]
impl GhosttyEmbed {
    pub fn new(
        _ns_view: usize,
        _width_px: u32,
        _height_px: u32,
        _scale_factor: f64,
        _working_directory: Option<&str>,
    ) -> Result<Self, String> {
        Err(String::from("Ghostty embedding spike is macOS-only"))
    }

    pub fn handle_keyboard_event(&mut self, _event: &iced::keyboard::Event) -> bool {
        false
    }

    pub fn refresh(&mut self) {}

    pub fn handle_mouse_move(&mut self, _x: f64, _y: f64, _modifiers: iced::keyboard::Modifiers) {}

    pub fn handle_mouse_button(
        &mut self,
        _button: iced::mouse::Button,
        _pressed: bool,
        _modifiers: iced::keyboard::Modifiers,
    ) -> bool {
        false
    }

    pub fn handle_mouse_scroll(&mut self, _x: f64, _y: f64, _precision: bool) {}

    pub fn binding_action(&mut self, _action: &str) -> bool {
        false
    }

    pub fn update_modifiers(&mut self, _modifiers: iced::keyboard::Modifiers) {}

    pub fn modifiers(&self) -> iced::keyboard::Modifiers {
        iced::keyboard::Modifiers::default()
    }
}

#[cfg(not(target_os = "macos"))]
pub fn ns_view_ptr(_window: &dyn iced::window::Window) -> Option<usize> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn host_view_new(_parent_ns_view: usize) -> Option<usize> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn host_view_set_frame(_host_ns_view: usize, _x: f64, _y: f64, _width: f64, _height: f64) {}

#[cfg(not(target_os = "macos"))]
pub fn host_view_set_hidden(_host_ns_view: usize, _hidden: bool) {}

#[cfg(not(target_os = "macos"))]
pub fn host_view_free(_host_ns_view: usize) {}

#[cfg(not(target_os = "macos"))]
pub fn disable_system_hide_shortcuts() {}
