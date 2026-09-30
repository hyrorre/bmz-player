use std::collections::BTreeSet;
use winit::keyboard::KeyCode;

pub(super) fn physical_key(usage: u32) -> Option<winit::keyboard::PhysicalKey> {
    use winit::keyboard::{NativeKeyCode, PhysicalKey};
    // winit 0.30.13 leaves these JIS virtual keycodes unidentified. Preserve
    // existing Native:MacOS:* bindings rather than changing their names.
    let native = match usage {
        135 => Some(0x5e),
        144 => Some(0x68),
        145 => Some(0x66),
        _ => None,
    };
    native
        .map(|code| PhysicalKey::Unidentified(NativeKeyCode::MacOS(code)))
        .or_else(|| key(usage).map(PhysicalKey::Code))
}

/// Physical USB keyboard usages; names go through the existing winit mapping.
pub(super) fn key(usage: u32) -> Option<KeyCode> {
    use KeyCode::*;
    let letters = [
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO,
        KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    ];
    if (4..=29).contains(&usage) {
        return Some(letters[(usage - 4) as usize]);
    }
    let digits = [Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9, Digit0];
    if (30..=39).contains(&usage) {
        return Some(digits[(usage - 30) as usize]);
    }
    let function = [F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12];
    if (58..=69).contains(&usage) {
        return Some(function[(usage - 58) as usize]);
    }
    let keypad =
        [Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9, Numpad0];
    if (89..=98).contains(&usage) {
        return Some(keypad[(usage - 89) as usize]);
    }
    Some(match usage {
        40 => Enter,
        41 => Escape,
        42 => Backspace,
        43 => Tab,
        44 => Space,
        45 => Minus,
        46 => Equal,
        47 => BracketLeft,
        48 => BracketRight,
        49 => Backslash,
        50 => Backslash,
        51 => Semicolon,
        52 => Quote,
        53 => Backquote,
        54 => Comma,
        55 => Period,
        56 => Slash,
        57 => CapsLock,
        70 => PrintScreen,
        71 => ScrollLock,
        72 => Pause,
        73 => Insert,
        74 => Home,
        75 => PageUp,
        76 => Delete,
        77 => End,
        78 => PageDown,
        79 => ArrowRight,
        80 => ArrowLeft,
        81 => ArrowDown,
        82 => ArrowUp,
        83 => NumLock,
        84 => NumpadDivide,
        85 => NumpadMultiply,
        86 => NumpadSubtract,
        87 => NumpadAdd,
        88 => NumpadEnter,
        99 => NumpadDecimal,
        100 => IntlBackslash,
        101 => ContextMenu,
        102 => Power,
        103 => NumpadEqual,
        104 => F13,
        105 => F14,
        106 => F15,
        107 => F16,
        108 => F17,
        109 => F18,
        110 => F19,
        111 => F20,
        112 => F21,
        113 => F22,
        114 => F23,
        115 => F24,
        135 => IntlRo,
        136 => KanaMode,
        137 => IntlYen,
        138 => Convert,
        139 => NonConvert,
        144 => Lang1,
        145 => Lang2,
        146 => Lang3,
        147 => Lang4,
        148 => Lang5,
        224 => ControlLeft,
        225 => ShiftLeft,
        226 => AltLeft,
        227 => SuperLeft,
        228 => ControlRight,
        229 => ShiftRight,
        230 => AltRight,
        231 => SuperRight,
        _ => return None,
    })
}

#[derive(Default)]
pub(super) struct Holds(BTreeSet<(usize, u32)>);
impl Holds {
    /// Return a logical edge only when the aggregate changes. Duplicate reports
    /// and releasing one of two keyboards cannot release the logical key.
    pub(super) fn update(&mut self, device: usize, usage: u32, down: bool) -> bool {
        let before = self.0.iter().any(|(_, u)| *u == usage);
        if down {
            self.0.insert((device, usage));
        } else {
            self.0.remove(&(device, usage));
        }
        before != self.0.iter().any(|(_, u)| *u == usage)
    }
    pub(super) fn remove(&mut self, device: usize) -> Vec<u32> {
        let usages: Vec<_> = self.0.iter().filter(|(d, _)| *d == device).map(|(_, u)| *u).collect();
        usages.into_iter().filter(|u| self.update(device, *u, false)).collect()
    }
    pub(super) fn clear(&mut self) -> Vec<u32> {
        let usages: BTreeSet<_> = self.0.iter().map(|(_, u)| *u).collect();
        self.0.clear();
        usages.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiple_keyboards_repeat_and_removal() {
        let mut holds = Holds::default();
        assert!(holds.update(1, 4, true));
        assert!(!holds.update(1, 4, true));
        assert!(!holds.update(2, 4, true));
        assert!(!holds.update(1, 4, false));
        assert_eq!(holds.remove(2), vec![4]);
        assert!(!holds.update(2, 4, false));
        assert!(holds.update(1, 225, true));
        assert!(holds.update(1, 229, true));
        assert_eq!(holds.clear(), vec![225, 229]);
    }
    #[test]
    fn physical_layout_modifiers_and_jis() {
        assert_eq!(key(4), Some(KeyCode::KeyA));
        assert_eq!(key(225), Some(KeyCode::ShiftLeft));
        assert_eq!(key(229), Some(KeyCode::ShiftRight));
        assert_eq!(key(137), Some(KeyCode::IntlYen));
        assert_eq!(key(135), Some(KeyCode::IntlRo));
        assert_eq!(key(0), None);
    }
}
