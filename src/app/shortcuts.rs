use iced::keyboard;
use iced::keyboard::key::{Code, Key, Named, Physical};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShortcutAction {
    ToggleSidebar,
    OpenQuickOpen,
    OpenPreferences,
    RenameTerminal,
    RenameFocused,
    FontIncrease,
    FontDecrease,
    FontReset,
    NextTerminal,
    PreviousTerminal,
    ModalCancel,
    ModalSubmit,
}

pub(crate) fn detect_shortcut(
    event: &keyboard::Event,
    allow_plain_rename: bool,
    modal_open: bool,
) -> Option<ShortcutAction> {
    let keyboard::Event::KeyPressed {
        key,
        modifiers,
        physical_key,
        ..
    } = event
    else {
        return None;
    };

    if matches!(key.as_ref(), Key::Named(Named::Escape)) && modal_open {
        return Some(ShortcutAction::ModalCancel);
    }
    if matches!(key.as_ref(), Key::Named(Named::Enter)) && modal_open {
        return Some(ShortcutAction::ModalSubmit);
    }

    if matches!(key.as_ref(), Key::Named(Named::F2))
        && !modifiers.logo()
        && !modifiers.control()
        && !modifiers.shift()
        && !modifiers.alt()
    {
        return Some(ShortcutAction::RenameFocused);
    }

    let key_char = key_character(key);
    if allow_plain_rename
        && matches!(key_char.as_deref(), Some("n"))
        && !modifiers.logo()
        && !modifiers.control()
        && !modifiers.shift()
        && !modifiers.alt()
    {
        return Some(ShortcutAction::RenameFocused);
    }

    let primary = modifiers.logo() || modifiers.control();
    if !primary {
        return None;
    }

    if modifiers.shift() && !modifiers.alt() {
        if is_bracket_right(key_char.as_deref(), physical_key) {
            return Some(ShortcutAction::NextTerminal);
        }

        if is_bracket_left(key_char.as_deref(), physical_key) {
            return Some(ShortcutAction::PreviousTerminal);
        }
    }

    if modifiers.alt() {
        return None;
    }

    if is_digit_one(key_char.as_deref(), physical_key) && !modifiers.shift() {
        return Some(ShortcutAction::ToggleSidebar);
    }

    if is_letter(key_char.as_deref(), "p") && !modifiers.shift() {
        return Some(ShortcutAction::OpenQuickOpen);
    }

    if is_letter(key_char.as_deref(), "r") && !modifiers.shift() {
        return Some(ShortcutAction::RenameTerminal);
    }

    if is_comma(key_char.as_deref(), physical_key) && !modifiers.shift() {
        return Some(ShortcutAction::OpenPreferences);
    }

    if is_digit_zero(key_char.as_deref(), physical_key) {
        return Some(ShortcutAction::FontReset);
    }

    if is_minus(key_char.as_deref(), physical_key) {
        return Some(ShortcutAction::FontDecrease);
    }

    if is_plus_or_equal(key_char.as_deref(), physical_key) {
        return Some(ShortcutAction::FontIncrease);
    }

    None
}

fn key_character(key: &Key) -> Option<String> {
    match key.as_ref() {
        Key::Character(value) => Some(value.to_lowercase()),
        _ => None,
    }
}

fn is_letter(value: Option<&str>, target: &str) -> bool {
    matches!(value, Some(v) if v == target)
}

fn is_comma(value: Option<&str>, physical: &Physical) -> bool {
    matches!(value, Some(",")) || matches!(physical, Physical::Code(Code::Comma))
}

fn is_digit_one(value: Option<&str>, physical: &Physical) -> bool {
    matches!(value, Some("1")) || matches!(physical, Physical::Code(Code::Digit1))
}

fn is_digit_zero(value: Option<&str>, physical: &Physical) -> bool {
    matches!(value, Some("0"))
        || matches!(physical, Physical::Code(Code::Digit0))
        || matches!(physical, Physical::Code(Code::Numpad0))
}

fn is_minus(value: Option<&str>, physical: &Physical) -> bool {
    matches!(value, Some("-") | Some("_"))
        || matches!(physical, Physical::Code(Code::Minus))
        || matches!(physical, Physical::Code(Code::NumpadSubtract))
}

fn is_plus_or_equal(value: Option<&str>, physical: &Physical) -> bool {
    matches!(value, Some("+") | Some("="))
        || matches!(physical, Physical::Code(Code::Equal))
        || matches!(physical, Physical::Code(Code::NumpadAdd))
}

fn is_bracket_left(value: Option<&str>, physical: &Physical) -> bool {
    matches!(value, Some("[") | Some("{")) || matches!(physical, Physical::Code(Code::BracketLeft))
}

fn is_bracket_right(value: Option<&str>, physical: &Physical) -> bool {
    matches!(value, Some("]") | Some("}")) || matches!(physical, Physical::Code(Code::BracketRight))
}
