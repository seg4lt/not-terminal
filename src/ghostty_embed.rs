#[cfg(target_os = "macos")]
mod macos {
    use iced::keyboard::key::{Code, NativeCode, Physical};
    use iced::keyboard::{Event as KeyboardEvent, Key, Location, Modifiers};
    use iced::window::Window;
    use iced::window::raw_window_handle::RawWindowHandle;
    use std::ffi::{CString, c_char, c_int, c_void};
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

    const GHOSTTY_ACTION_RELEASE: c_int = 0;
    const GHOSTTY_ACTION_PRESS: c_int = 1;
    const GHOSTTY_ACTION_REPEAT: c_int = 2;

    const GHOSTTY_MODS_NONE: c_int = 0;
    const GHOSTTY_MODS_SHIFT: c_int = 1 << 0;
    const GHOSTTY_MODS_CTRL: c_int = 1 << 1;
    const GHOSTTY_MODS_ALT: c_int = 1 << 2;
    const GHOSTTY_MODS_SUPER: c_int = 1 << 3;
    const GHOSTTY_MODS_SHIFT_RIGHT: c_int = 1 << 6;
    const GHOSTTY_MODS_CTRL_RIGHT: c_int = 1 << 7;
    const GHOSTTY_MODS_ALT_RIGHT: c_int = 1 << 8;
    const GHOSTTY_MODS_SUPER_RIGHT: c_int = 1 << 9;
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
        fn ghostty_surface_set_size(surface: *mut c_void, width: u32, height: u32);
        fn ghostty_surface_set_content_scale(surface: *mut c_void, x: f64, y: f64);
        fn ghostty_surface_set_focus(surface: *mut c_void, focused: bool);
        fn ghostty_surface_refresh(surface: *mut c_void);
        fn ghostty_surface_key(surface: *mut c_void, event: GhosttyInputKey) -> bool;

        fn rust_ghostty_runtime_bundle_new() -> *mut RuntimeBundle;
        fn rust_ghostty_runtime_bundle_free(bundle: *mut RuntimeBundle);
        fn rust_ghostty_runtime_config_ptr(bundle: *const RuntimeBundle) -> *const c_void;
        fn rust_ghostty_runtime_take_pending_tick(bundle: *const RuntimeBundle) -> bool;
        fn rust_ghostty_surface_new_macos(
            surface_new_fn_raw: *mut c_void,
            app: *mut c_void,
            ns_view: *mut c_void,
            scale_factor: f64,
            font_size_points: f32,
        ) -> *mut c_void;
    }

    pub struct GhosttyEmbed {
        runtime_bundle: *mut RuntimeBundle,
        config: *mut c_void,
        app: *mut c_void,
        surface: *mut c_void,
    }

    impl GhosttyEmbed {
        pub fn new(
            ns_view: usize,
            width_px: u32,
            height_px: u32,
            scale_factor: f64,
        ) -> Result<Self, String> {
            if ns_view == 0 {
                return Err(String::from("received null NSView pointer"));
            }

            let ghostty_init_fn: GhosttyInitFn = ghostty_init;
            let mut runtime_bundle: *mut RuntimeBundle = ptr::null_mut();
            let mut config: *mut c_void = ptr::null_mut();
            let mut app: *mut c_void = ptr::null_mut();
            let mut surface: *mut c_void = ptr::null_mut();

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
                    load_default_theme(config);
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
                    );
                    if surface.is_null() {
                        return Err(String::from("ghostty_surface_new returned null"));
                    }

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

        pub fn handle_keyboard_event(&mut self, event: &KeyboardEvent) -> bool {
            match event {
                KeyboardEvent::KeyPressed {
                    key,
                    physical_key,
                    modifiers,
                    location,
                    text,
                    repeat,
                    ..
                } => {
                    let action = if *repeat {
                        GHOSTTY_ACTION_REPEAT
                    } else {
                        GHOSTTY_ACTION_PRESS
                    };
                    self.send_key_event(
                        action,
                        key,
                        physical_key,
                        modifiers,
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
                } => self.send_key_event(
                    GHOSTTY_ACTION_RELEASE,
                    key,
                    physical_key,
                    modifiers,
                    location,
                    None,
                ),
                KeyboardEvent::ModifiersChanged(_) => false,
            }
        }

        fn send_key_event(
            &mut self,
            action: c_int,
            key: &Key,
            physical_key: &Physical,
            modifiers: &Modifiers,
            location: &Location,
            text: Option<&str>,
        ) -> bool {
            let keycode = keycode_from_physical(physical_key);
            let mods = ghostty_mods(*modifiers, key, location);
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

    fn load_default_theme(config: *mut c_void) {
        if let Ok(path) = CString::new(DEFAULT_THEME_PATH) {
            unsafe {
                ghostty_config_load_file(config, path.as_ptr());
            }
        }
    }

    fn ghostty_mods(modifiers: Modifiers, key: &Key, location: &Location) -> c_int {
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
pub use macos::{GhosttyEmbed, ns_view_ptr};

#[cfg(not(target_os = "macos"))]
pub struct GhosttyEmbed;

#[cfg(not(target_os = "macos"))]
impl GhosttyEmbed {
    pub fn new(
        _ns_view: usize,
        _width_px: u32,
        _height_px: u32,
        _scale_factor: f64,
    ) -> Result<Self, String> {
        Err(String::from("Ghostty embedding spike is macOS-only"))
    }

    pub fn handle_keyboard_event(&mut self, _event: &iced::keyboard::Event) -> bool {
        false
    }

    pub fn refresh(&mut self) {}
}

#[cfg(not(target_os = "macos"))]
pub fn ns_view_ptr(_window: &dyn iced::window::Window) -> Option<usize> {
    None
}
