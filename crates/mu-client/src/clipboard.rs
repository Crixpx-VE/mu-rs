//! Copy and paste in text boxes, like the Windows `edit` control the original uses underneath
//! (`CUITextInputBox`): Ctrl+C/Ctrl+Insert copies, Ctrl+X/Shift+Del cuts, Ctrl+V/Shift+Insert pastes
//! and Ctrl+A selects all. Passwords (`ES_PASSWORD`) cannot be copied or cut.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

/// The key for the text box: editing shortcuts become `Key::Copy`, `Key::Cut`,
/// `Key::Paste` and `Key::Select` (select all); everything else as is.
pub fn edit_key(ev: &KeyboardInput, ctrl: bool, shift: bool) -> Key {
    if ev.key_code == KeyCode::Insert {
        if shift && !ctrl {
            return Key::Paste;
        }
        if ctrl && !shift {
            return Key::Copy;
        }
    }
    if ev.key_code == KeyCode::Delete && shift && !ctrl {
        return Key::Cut;
    }
    if !ctrl {
        return ev.logical_key.clone();
    }
    // The letter per keyboard layout (with Ctrl, Windows may give the control character).
    let letter = match &ev.logical_key {
        Key::Character(s) => match s.chars().next() {
            Some('\u{1}') => Some('a'),
            Some('\u{3}') => Some('c'),
            Some('\u{16}') => Some('v'),
            Some('\u{18}') => Some('x'),
            Some(c) => Some(c.to_ascii_lowercase()),
            None => None,
        },
        _ => None,
    };
    let letter = letter.or(match ev.key_code {
        KeyCode::KeyA => Some('a'),
        KeyCode::KeyC => Some('c'),
        KeyCode::KeyV => Some('v'),
        KeyCode::KeyX => Some('x'),
        _ => None,
    });
    match letter {
        Some('a') => Key::Select,
        Some('c') => Key::Copy,
        Some('v') => Key::Paste,
        Some('x') => Key::Cut,
        _ => ev.logical_key.clone(),
    }
}

/// Keys pressed this frame for a text box (with repeat).
pub fn typed_keys(events: impl Iterator<Item = KeyboardInput>, keys: &ButtonInput<KeyCode>) -> Vec<Key> {
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    events.filter(|k| k.state == bevy::input::ButtonState::Pressed).map(|k| edit_key(&k, ctrl, shift)).collect()
}

/// What gets pasted into a single-line box: up to the first line break and without control
/// characters (tabs become spaces).
pub fn paste_text(text: &str) -> impl Iterator<Item = char> + '_ {
    text.lines().next().unwrap_or("").chars().map(|c| if c == '\t' { ' ' } else { c }).filter(|c| !c.is_control())
}

/// The Windows clipboard text (or nothing).
pub fn get() -> String {
    match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
        Ok(t) => t,
        Err(e) => {
            debug!("portapapeles: no se pudo leer ({e})");
            String::new()
        }
    }
}

/// Puts `text` on the Windows clipboard.
pub fn set(text: &str) {
    if text.is_empty() {
        return;
    }
    if let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())) {
        warn!("portapapeles: no se pudo copiar ({e})");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(key_code: KeyCode, logical: Key) -> KeyboardInput {
        KeyboardInput {
            key_code,
            logical_key: logical,
            state: bevy::input::ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        }
    }

    #[test]
    fn atajos() {
        let v = ev(KeyCode::KeyV, Key::Character("v".into()));
        assert_eq!(edit_key(&v, true, false), Key::Paste);
        assert_eq!(edit_key(&v, false, false), Key::Character("v".into()));
        // With Ctrl, Windows may give the control character.
        assert_eq!(edit_key(&ev(KeyCode::KeyC, Key::Character("\u{3}".into())), true, false), Key::Copy);
        assert_eq!(edit_key(&ev(KeyCode::KeyX, Key::Character("X".into())), true, true), Key::Cut);
        assert_eq!(edit_key(&ev(KeyCode::KeyA, Key::Character("a".into())), true, false), Key::Select);
        assert_eq!(edit_key(&ev(KeyCode::Insert, Key::Insert), false, true), Key::Paste);
        assert_eq!(edit_key(&ev(KeyCode::Insert, Key::Insert), true, false), Key::Copy);
        assert_eq!(edit_key(&ev(KeyCode::Delete, Key::Delete), false, true), Key::Cut);
        assert_eq!(edit_key(&ev(KeyCode::Delete, Key::Delete), false, false), Key::Delete);
    }

    #[test]
    fn pegar_una_linea() {
        assert_eq!(paste_text("hola\tmundo\r\notra").collect::<String>(), "hola mundo");
        assert_eq!(paste_text("").count(), 0);
    }
}
