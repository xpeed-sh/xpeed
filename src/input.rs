use eframe::egui::{Key, Modifiers};

pub fn encode_key(key: Key, modifiers: Modifiers, application_cursor: bool) -> Option<Vec<u8>> {
    if modifiers.ctrl && !modifiers.alt {
        let control = match key {
            Key::A => 1,
            Key::B => 2,
            Key::C => 3,
            Key::D => 4,
            Key::E => 5,
            Key::F => 6,
            Key::G => 7,
            Key::H => 8,
            Key::I => 9,
            Key::J => 10,
            Key::K => 11,
            Key::L => 12,
            Key::M => 13,
            Key::N => 14,
            Key::O => 15,
            Key::P => 16,
            Key::Q => 17,
            Key::R => 18,
            Key::S => 19,
            Key::T => 20,
            Key::U => 21,
            Key::V => 22,
            Key::W => 23,
            Key::X => 24,
            Key::Y => 25,
            Key::Z => 26,
            Key::Space => 0,
            _ => 255,
        };
        if control != 255 {
            return Some(vec![control]);
        }
    }
    let modifier =
        1 + u8::from(modifiers.shift) + 2 * u8::from(modifiers.alt) + 4 * u8::from(modifiers.ctrl);
    let arrow = match key {
        Key::ArrowUp => Some('A'),
        Key::ArrowDown => Some('B'),
        Key::ArrowRight => Some('C'),
        Key::ArrowLeft => Some('D'),
        Key::Home => Some('H'),
        Key::End => Some('F'),
        _ => None,
    };
    if let Some(letter) = arrow {
        return Some(if modifier > 1 {
            format!("\x1b[1;{modifier}{letter}").into_bytes()
        } else if application_cursor {
            format!("\x1bO{letter}").into_bytes()
        } else {
            format!("\x1b[{letter}").into_bytes()
        });
    }
    let sequence = match key {
        Key::Enter => "\r".to_string(),
        Key::Backspace => if modifiers.ctrl { "\x08" } else { "\x7f" }.to_string(),
        Key::Escape => "\x1b".to_string(),
        Key::Tab => if modifiers.shift { "\x1b[Z" } else { "\t" }.to_string(),
        Key::Insert
        | Key::Delete
        | Key::PageUp
        | Key::PageDown
        | Key::F5
        | Key::F6
        | Key::F7
        | Key::F8
        | Key::F9
        | Key::F10
        | Key::F11
        | Key::F12 => {
            let code = match key {
                Key::Insert => 2,
                Key::Delete => 3,
                Key::PageUp => 5,
                Key::PageDown => 6,
                Key::F5 => 15,
                Key::F6 => 17,
                Key::F7 => 18,
                Key::F8 => 19,
                Key::F9 => 20,
                Key::F10 => 21,
                Key::F11 => 23,
                _ => 24,
            };
            if modifier > 1 {
                format!("\x1b[{code};{modifier}~")
            } else {
                format!("\x1b[{code}~")
            }
        }
        Key::F1 | Key::F2 | Key::F3 | Key::F4 => {
            let letter = match key {
                Key::F1 => 'P',
                Key::F2 => 'Q',
                Key::F3 => 'R',
                _ => 'S',
            };
            if modifier > 1 {
                format!("\x1b[1;{modifier}{letter}")
            } else {
                format!("\x1bO{letter}")
            }
        }
        _ => return None,
    };
    let mut bytes = sequence.into_bytes();
    if modifiers.alt && matches!(key, Key::Enter | Key::Backspace) {
        bytes.insert(0, 27);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupt_remains_available_to_the_shell() {
        assert_eq!(encode_key(Key::C, Modifiers::CTRL, false), Some(vec![3]));
        assert_eq!(encode_key(Key::D, Modifiers::CTRL, false), Some(vec![4]));
    }
    #[test]
    fn terminal_application_cursor_and_modified_arrows() {
        assert_eq!(
            encode_key(Key::ArrowUp, Modifiers::NONE, true),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            encode_key(Key::ArrowLeft, Modifiers::CTRL, false),
            Some(b"\x1b[1;5D".to_vec())
        );
        assert_eq!(
            encode_key(Key::Tab, Modifiers::SHIFT, false),
            Some(b"\x1b[Z".to_vec())
        );
    }
}
